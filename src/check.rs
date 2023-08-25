// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use bytesize::ByteSize;
use std::path::PathBuf;
use std::str::FromStr;

use crate::{
    color::Color,
    constants::{ARB_WASM_ADDRESS, MAX_PROGRAM_SIZE},
    deploy::activation_calldata,
    project::{self, BuildConfig},
    wallet, CheckConfig,
};
use ethers::prelude::*;
use ethers::utils::get_contract_address;
use ethers::{
    providers::JsonRpcClient,
    types::{transaction::eip2718::TypedTransaction, Address},
};

use ethers::types::Eip1559TransactionRequest;
use ethers::{
    core::types::spoof,
    providers::{Provider, RawCall},
};

/// Runs a series of checks on the WASM program to ensure it is valid for compilation
/// and code size before being deployed and activated onchain. An optional list of checks
/// to disable can be specified.
pub async fn run_checks(cfg: CheckConfig) -> eyre::Result<(), String> {
    let wasm_file_path: PathBuf = match cfg.wasm_file_path {
        Some(path) => PathBuf::from_str(&path).unwrap(),
        None => project::build_project_to_wasm(BuildConfig {
            opt_level: project::OptLevel::default(),
            nightly: cfg.nightly,
            clean: true,
        })
        .map_err(|e| format!("failed to build project to WASM: {e}"))?,
    };
    println!("Reading WASM file at {}", wasm_file_path.display().grey());

    let (_, deploy_ready_code) = project::get_compressed_wasm_bytes(&wasm_file_path)
        .map_err(|e| format!("failed to get compressed WASM bytes: {e}"))?;

    let compressed_size = ByteSize::b(deploy_ready_code.len() as u64);
    if compressed_size > MAX_PROGRAM_SIZE {
        return Err(format!(
            "brotli-compressed WASM size {} is bigger than program size limit: {}",
            compressed_size.to_string().red(),
            MAX_PROGRAM_SIZE,
        ));
    }

    println!(
        "Compressed WASM size: {}",
        compressed_size.to_string().mint(),
    );

    let provider = Provider::<Http>::try_from(&cfg.endpoint)
        .map_err(|e| format!("could not initialize provider from http: {e}"))?;

    let mut expected_program_addr = cfg.expected_program_address;

    // If there is no expected program address specified, compute it from the user's wallet.
    if expected_program_addr != H160::zero() {
        let wallet = wallet::load(cfg.private_key_path, cfg.keystore_opts)?;
        let chain_id = provider
            .get_chainid()
            .await
            .map_err(|e| format!("could not get chain id {e}"))?
            .as_u64();
        let client =
            SignerMiddleware::new(provider.clone(), wallet.clone().with_chain_id(chain_id));

        let addr = wallet.address();
        let nonce = client
            .get_transaction_count(addr, None)
            .await
            .map_err(|e| format!("could not get nonce {addr}: {e}"))?;

        expected_program_addr = get_contract_address(wallet.address(), nonce);
    }
    check_can_activate(provider, &expected_program_addr, deploy_ready_code).await
}

/// Checks if a program can be successfully activated onchain before it is deployed
/// by using an eth_call override that injects the program's code at a specified address.
/// This allows for verifying an activation call is correct and will succeed if sent
/// as a transaction with the appropriate gas.
pub async fn check_can_activate<T>(
    client: Provider<T>,
    expected_program_address: &Address,
    compressed_wasm: Vec<u8>,
) -> eyre::Result<(), String>
where
    T: JsonRpcClient + Send + Sync,
{
    let calldata = activation_calldata(expected_program_address);
    let to = hex::decode(ARB_WASM_ADDRESS).unwrap();
    let to = Address::from_slice(&to);

    let tx_request = Eip1559TransactionRequest::new().to(to).data(calldata);
    let tx = TypedTransaction::Eip1559(tx_request);

    // Spoof the state as if the program already exists at the specified address
    // using an eth_call override.
    let state = spoof::code(
        Address::from_slice(expected_program_address.as_bytes()),
        compressed_wasm.into(),
    );
    let response = client.call_raw(&tx).state(&state).await.map_err(|e| {
        format!(
            "program predeployment check failed when checking against ARB_WASM_ADDRESS {to}: {e}"
        )
    })?;

    if response.len() < 2 {
        return Err(format!(
            "Stylus version bytes response too short, expected at least 2 bytes but got: {}",
            hex::encode(&response)
        ));
    }
    let n = response.len();
    let version_bytes: [u8; 2] = response[n - 2..]
        .try_into()
        .map_err(|e| format!("could not parse Stylus version bytes: {e}"))?;
    let version = u16::from_be_bytes(version_bytes);
    println!("Program succeeded Stylus onchain activation checks with Stylus version: {version}");
    Ok(())
}
