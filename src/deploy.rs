// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

use ethers::types::{Eip1559TransactionRequest, H160, U256};
use ethers::utils::get_contract_address;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::Signer,
};
use eyre::eyre;

use crate::project::BuildConfig;
use crate::{check, color::Color, constants, project, tx, wallet, DeployConfig, DeployMode};

/// The transaction kind for using the Cargo stylus tool with Stylus programs.
/// Stylus programs can be deployed and activated onchain, and this enum represents
/// these two variants.
pub enum TxKind {
    Deployment,
    Activation,
}

impl std::fmt::Display for TxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            &TxKind::Deployment => write!(f, "deployment"),
            &TxKind::Activation => write!(f, "activation"),
        }
    }
}

/// Performs one of three different modes for a Stylus program:
/// DeployOnly: Sends a signed tx to deploy a Stylus program to a new address.
/// ActivateOnly: Sends a signed tx to activate a Stylus program at a specified address.
/// DeployAndActivate (default): Sends both transactions above.
pub async fn deploy(cfg: DeployConfig) -> eyre::Result<()> {
    // Run stylus checks before deployment.
    let program_is_up_to_date = check::run_checks(cfg.check_cfg.clone())
        .await
        .map_err(|e| eyre!("Stylus checks failed: {e}"))?;
    let wallet = wallet::load(cfg.check_cfg.private_key_path, cfg.check_cfg.keystore_opts)
        .map_err(|e| eyre!("could not load wallet: {e}"))?;

    let provider = Provider::<Http>::try_from(&cfg.check_cfg.endpoint).map_err(|e| {
        eyre!(
            "could not initialize provider from http endpoint: {}: {e}",
            &cfg.check_cfg.endpoint
        )
    })?;
    let chain_id = provider
        .get_chainid()
        .await
        .map_err(|e| eyre!("could not get chain id: {e}"))?
        .as_u64();
    let client = SignerMiddleware::new(provider, wallet.clone().with_chain_id(chain_id));

    let addr = wallet.address();
    let nonce = client
        .get_transaction_count(addr, None)
        .await
        .map_err(|e| eyre!("could not get nonce for address {addr}: {e}"))?;

    let expected_program_addr = get_contract_address(wallet.address(), nonce);

    let (deploy, activate) = match cfg.mode {
        Some(DeployMode::DeployOnly) => (true, false),
        Some(DeployMode::ActivateOnly) => (false, true),
        // Default mode is to deploy and activate
        None => {
            if cfg.estimate_gas_only && cfg.activate_program_address.is_none() {
                // cannot activate if not really deploying
                println!(
                    r#"Only estimating gas for deployment tx. To estimate gas for activation, 
run with --mode=activate-only and specify --activate-program-address. The program must have been deployed
already for estimating activation gas to work. To send individual txs for deployment and activation, see more
on the --mode flag under cargo stylus deploy --help"#
                );
                (true, false)
            } else {
                (true, true)
            }
        }
    };

    // Whether or not to send the transactions to the endpoint.
    let dry_run = cfg.tx_sending_opts.dry_run;

    // The folder at which to output the transaction data bytes.
    let output_dir = cfg.tx_sending_opts.output_tx_data_to_dir.as_ref();

    if dry_run && output_dir.is_none() {
        return Err(eyre!(
            "using the --dry-run flag requires specifying the --output-tx-data-to-dir flag as well"
        ));
    }

    if deploy {
        let wasm_file_path: PathBuf = match &cfg.check_cfg.wasm_file_path {
            Some(path) => PathBuf::from_str(path).unwrap(),
            None => project::build_project_to_wasm(BuildConfig {
                opt_level: project::OptLevel::default(),
                nightly: cfg.check_cfg.nightly,
                rebuild: false, // The check step at the start of this command rebuilt.
            })
            .map_err(|e| eyre!("could not build project to WASM: {e}"))?,
        };
        let (_, deploy_ready_code) = project::get_compressed_wasm_bytes(&wasm_file_path)?;
        println!(
            "Deploying program to address 0x{}",
            hex::encode(expected_program_addr).mint()
        );
        let deployment_calldata = program_deployment_calldata(&deploy_ready_code);

        // Output the tx data to a user's specified directory if desired.
        if let Some(tx_data_output_dir) = output_dir {
            write_tx_data(TxKind::Deployment, tx_data_output_dir, &deployment_calldata)?;
        }

        if !dry_run {
            let mut tx_request = Eip1559TransactionRequest::new()
                .from(wallet.address())
                .data(deployment_calldata);
            tx::submit_signed_tx(
                &client,
                TxKind::Deployment,
                cfg.estimate_gas_only,
                &mut tx_request,
            )
            .await
            .map_err(|e| eyre!("could not submit signed deployment tx: {e}"))?;
        }
    }
    if activate {
        // If program is up-to-date, there is no need for an activation transaction.
        if program_is_up_to_date {
            return Ok(());
        }
        let program_addr = cfg
            .activate_program_address
            .unwrap_or(expected_program_addr);
        println!(
            "Activating program at address {}",
            hex::encode(program_addr).mint()
        );
        let activate_calldata = activation_calldata(&program_addr);

        let to = hex::decode(constants::ARB_WASM_ADDRESS).unwrap();
        let to = H160::from_slice(&to);

        // Output the tx data to a user's specified directory if desired.
        if let Some(tx_data_output_dir) = output_dir {
            write_tx_data(TxKind::Activation, tx_data_output_dir, &activate_calldata)?;
        }

        if !dry_run {
            let mut tx_request = Eip1559TransactionRequest::new()
                .from(wallet.address())
                .to(to)
                .data(activate_calldata);
            tx::submit_signed_tx(
                &client,
                TxKind::Activation,
                cfg.estimate_gas_only,
                &mut tx_request,
            )
            .await
            .map_err(|e| eyre!("could not submit signed activation tx: {e}"))?;
        }
    }
    Ok(())
}

pub fn activation_calldata(program_addr: &H160) -> Vec<u8> {
    let mut activate_calldata = vec![];
    let activate_method_hash = hex::decode(constants::ARBWASM_ACTIVATE_METHOD_HASH).unwrap();
    activate_calldata.extend(activate_method_hash);
    let mut extension = [0u8; 32];
    // Next, we add the address to the last 20 bytes of extension
    extension[12..32].copy_from_slice(program_addr.as_bytes());
    activate_calldata.extend(extension);
    activate_calldata
}

/// Prepares an EVM bytecode prelude for contract creation.
pub fn program_deployment_calldata(code: &[u8]) -> Vec<u8> {
    let mut code_len = [0u8; 32];
    U256::from(code.len()).to_big_endian(&mut code_len);
    let mut deploy: Vec<u8> = vec![];
    deploy.push(0x7f); // PUSH32
    deploy.extend(code_len);
    deploy.push(0x80); // DUP1
    deploy.push(0x60); // PUSH1
    deploy.push(0x2a); // 42 the prelude length
    deploy.push(0x60); // PUSH1
    deploy.push(0x00);
    deploy.push(0x39); // CODECOPY
    deploy.push(0x60); // PUSH1
    deploy.push(0x00);
    deploy.push(0xf3); // RETURN
    deploy.extend(code);
    deploy
}

fn write_tx_data(tx_kind: TxKind, path: &PathBuf, data: &[u8]) -> eyre::Result<()> {
    let file_name = format!("{tx_kind}_tx_data");
    let path = path.join(file_name);
    let path_str = path.as_os_str().to_string_lossy();
    println!(
        "Writing {tx_kind} tx data bytes of size {} to path {}",
        data.len().mint(),
        path_str.grey(),
    );
    let mut f = std::fs::File::create(&path)
        .map_err(|e| eyre!("could not create file to write tx data to path {path_str}: {e}",))?;
    f.write_all(data)
        .map_err(|e| eyre!("could not write tx data as bytes to file to path {path_str}: {e}"))
}
