// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::constants::{ONE_ETH, PROGRAM_UP_TO_DATE_ERR};
use crate::{
    constants::ARB_WASM_ADDRESS,
    deploy::activation_calldata,
    project::{self, BuildConfig},
    wallet, CheckConfig,
};
use bytesize::ByteSize;
use cargo_stylus_util::{color::Color, sys};
use ethers::prelude::*;
use ethers::utils::get_contract_address;
use ethers::{
    providers::JsonRpcClient,
    types::{transaction::eip2718::TypedTransaction, Address},
};
use std::path::PathBuf;
use std::str::FromStr;

use ethers::types::Eip1559TransactionRequest;
use ethers::{
    core::types::spoof,
    providers::{Provider, RawCall},
};
use eyre::{bail, eyre};

/// Implements a custom wrapper for byte size that can be formatted with color
/// depending on the byte size. For example, file sizes that are greater than 24Kb
/// get formatted in pink as they are large, yellow for less than 24Kb, and mint for
/// WASMS less than 8Kb.
pub struct FileByteSize(ByteSize);

impl FileByteSize {
    fn new(len: u64) -> Self {
        Self(ByteSize::b(len))
    }
}

impl std::fmt::Display for FileByteSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            n if n <= ByteSize::kb(16) => {
                write!(f, "{}", n.mint())
            }
            n if n > ByteSize::kb(16) && n <= ByteSize::kb(24) => {
                write!(f, "{}", n.yellow())
            }
            n => {
                write!(f, "{}", n.pink())
            }
        }
    }
}

/// Runs a series of checks on the WASM program to ensure it is valid for compilation
/// and code size before being deployed and activated onchain. An optional list of checks
/// to disable can be specified.
///
/// Returns whether the WASM is already up-to-date and activated onchain, and the data fee.
pub async fn run_checks(cfg: CheckConfig) -> eyre::Result<(bool, Option<U256>)> {
    let wasm_file_path: PathBuf = match &cfg.wasm_file_path {
        Some(path) => PathBuf::from_str(path).unwrap(),
        None => project::build_project_dylib(BuildConfig {
            opt_level: project::OptLevel::default(),
            nightly: cfg.nightly,
            rebuild: true,
        })
        .map_err(|e| eyre!("failed to build project to WASM: {e}"))?,
    };
    println!("Reading WASM file at {}", wasm_file_path.display().grey());

    let (precompressed_bytes, init_code) = project::compress_wasm(&wasm_file_path)
        .map_err(|e| eyre!("failed to get compressed WASM bytes: {e}"))?;

    let precompressed_size = FileByteSize::new(precompressed_bytes.len() as u64);
    println!("Uncompressed WASM size: {precompressed_size}");

    let compressed_size = FileByteSize::new(init_code.len() as u64);
    println!("Compressed WASM size to be deployed onchain: {compressed_size}");

    println!(
        "Connecting to Stylus RPC endpoint: {}",
        &cfg.endpoint.mint()
    );

    let provider = sys::new_provider(&cfg.endpoint)?;

    let expected_program_addr = get_expected_program_addr(cfg.clone(), provider.clone())
        .await
        .map_err(|e| eyre!("could not get expected program address: {e}"))?;
    check_can_activate(provider, &expected_program_addr, init_code).await
}

async fn get_expected_program_addr<T>(
    cfg: CheckConfig,
    provider: Provider<T>,
) -> eyre::Result<Address>
where
    T: JsonRpcClient + Clone,
{
    let expected_program_addr = cfg.expected_program_address;
    if !expected_program_addr.is_zero() {
        return Ok(expected_program_addr);
    }
    // If there is no expected program address specified, compute it from the user's wallet.
    let wallet = wallet::load(&cfg)?;
    let chain_id = provider
        .get_chainid()
        .await
        .map_err(|e| eyre!("could not get chain id {e}"))?
        .as_u64();
    let client = SignerMiddleware::new(provider.clone(), wallet.clone().with_chain_id(chain_id));

    let addr = wallet.address();
    let nonce = client
        .get_transaction_count(addr, None)
        .await
        .map_err(|e| eyre!("could not get nonce {addr}: {e}"))?;

    Ok(get_contract_address(wallet.address(), nonce))
}

/// Checks if a program can be successfully activated onchain before it is deployed
/// by using an eth_call override that injects the program's code at a specified address.
/// This ensures an activation call is correct and will succeed if sent as a transaction with the
/// appropriate gas.
///
/// Returns whether the program's code is up-to-date and activated onchain, and the data fee.
pub async fn check_can_activate<T>(
    client: Provider<T>,
    expected_program_address: &Address,
    compressed_wasm: Vec<u8>,
) -> eyre::Result<(bool, Option<U256>)>
where
    T: JsonRpcClient,
{
    let calldata = activation_calldata(expected_program_address);
    let to = hex::decode(ARB_WASM_ADDRESS).unwrap();
    let to = Address::from_slice(&to);

    let tx = Eip1559TransactionRequest::new()
        .to(to)
        .data(calldata)
        .value(ONE_ETH);
    let tx = TypedTransaction::Eip1559(tx);

    // pretend the program already exists at the specified address via an eth_call override
    let mut state = spoof::code(
        Address::from_slice(expected_program_address.as_bytes()),
        compressed_wasm.into(),
    );

    // spoof the deployer's balance
    state.account(Default::default()).balance = Some(U256::MAX);

    let (response, program_up_to_date) = match client.call_raw(&tx).state(&state).await {
        Ok(response) => (response, false),
        Err(e) => {
            // TODO: Improve this check by instead calling ArbWasm to check if a program is up to date
            // once the feature is available and exposed onchain.
            if e.to_string().contains(PROGRAM_UP_TO_DATE_ERR) {
                (Bytes::new(), true)
            } else {
                bail!(
                    "program predeployment check failed when checking against ARB_WASM_ADDRESS {ARB_WASM_ADDRESS}: {e}"
                );
            }
        }
    };

    if program_up_to_date {
        let msg = "already activated";
        println!(
            "Stylus program with same WASM code is {} onchain",
            msg.mint()
        );
        return Ok((true, None));
    }

    // TODO: switch to alloy
    if response.len() != 64 {
        bail!("unexpected ArbWasm result: {}", hex::encode(&response));
    }
    let version = u16::from_be_bytes(response[30..32].try_into().unwrap());
    let data_fee = U256::from_big_endian(&response[32..]);

    println!("Program succeeded Stylus onchain activation checks with Stylus version: {version}");
    Ok((false, Some(data_fee)))
}

#[cfg(test)]
mod tests {
    use crate::KeystoreOpts;

    use super::*;

    #[tokio::test]
    async fn test_get_expected_program_addr() {
        let (provider, mock) = Provider::mocked();
        let chain_id = U64::from(1);
        let nonce = U256::from(1);
        mock.push(U64::from(chain_id)).unwrap();
        mock.push(U256::from(nonce)).unwrap();

        let wallet_address = Address::from_str("A72efc67beCA9786C46c01788B0303520614809c").unwrap();

        // We check that if we specify an expected program addr, that we get the same result back.
        let want = Address::from_str("e444E89f4A0CcC659b727e15F1f388DbBdCf4550").unwrap();
        let mut cfg = CheckConfig {
            expected_program_address: want,
            endpoint: "http://localhost:8545".to_string(),
            nightly: false,
            wasm_file_path: None,
            private_key: Some(
                "ff99561e3edc649a575b8706667f9acc500e818df97b78c29b54b522be7c89ac".to_string(),
            ),
            private_key_path: None,
            keystore_opts: KeystoreOpts {
                keystore_path: None,
                keystore_password_path: None,
            },
        };
        let got = get_expected_program_addr(cfg.clone(), provider.clone())
            .await
            .unwrap();
        assert_eq!(want, got);

        // Otherwise, if we specify the zero address, we should be computing the expected
        // from the account and nonce.
        cfg.expected_program_address = Address::zero();
        let got = get_expected_program_addr(cfg.clone(), provider)
            .await
            .unwrap();
        assert_eq!(get_contract_address(wallet_address, nonce), got);
    }
}
