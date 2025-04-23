// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

#![allow(clippy::println_empty_string)]

use alloy::{
    consensus::Transaction,
    dyn_abi::JsonAbiExt,
    primitives::{Address, TxHash},
    providers::{Provider, ProviderBuilder},
};
use eyre::{bail, eyre, Result};
use serde::{Deserialize, Serialize};

use crate::{
    check,
    deploy::{self, deployer, extract_compressed_wasm, extract_contract_evm_deployment_prelude},
    export_abi,
    macros::greyln,
    util::{
        color::{Color, GREY, MINT},
        sys,
    },
    CheckConfig, DataFeeOpts, VerifyConfig,
};

#[derive(Debug, Deserialize, Serialize)]
struct RpcResult {
    input: String,
}

pub async fn verify(cfg: VerifyConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .on_builtin(&cfg.common_cfg.endpoint)
        .await?;

    let hash = crate::util::text::decode0x(cfg.deployment_tx)?;
    if hash.len() != 32 {
        bail!("Invalid hash");
    }
    let hash = TxHash::from_slice(&hash);
    let Some(tx) = provider
        .get_transaction_by_hash(hash)
        .await
        .map_err(|e| eyre!("RPC failed: {e}"))?
    else {
        bail!("No code at address");
    };
    let output = sys::new_command("cargo")
        .arg("clean")
        .output()
        .map_err(|e| eyre!("failed to execute cargo clean: {e}"))?;
    if !output.status.success() {
        bail!("cargo clean command failed");
    }
    let check_cfg = CheckConfig {
        common_cfg: cfg.common_cfg.clone(),
        data_fee: DataFeeOpts {
            data_fee_bump_percent: 20,
        },
        wasm_file: None,
        contract_address: None,
    };
    let contract_check = check::check(&check_cfg)
        .await
        .map_err(|e| eyre!("Stylus checks failed: {e}"))?;
    let deployment_data = deploy::contract_deployment_calldata(&contract_check.code());
    let calldata = tx.input();
    if let Some(deployer_address) = tx.to() {
        verify_constructor_deployment(deployer_address, calldata, &deployment_data)
    } else {
        verify_create_deployment(calldata, &deployment_data)
    }
}

fn verify_constructor_deployment(
    deployer_address: Address,
    calldata: &[u8],
    deployment_data: &[u8],
) -> Result<()> {
    let Some(constructor) = export_abi::get_constructor_signature()? else {
        bail!("Deployment transaction uses constructor but the local project doesn't have one");
    };
    let call = deployer::decode_deploy_call(calldata)?;
    if &call.bytecode != deployment_data {
        bail!("Mismatch between deployed bytecode and local project's bytecode");
    }
    if call.initData.len() < 4 {
        bail!("Invalid init data length");
    }
    let constructor_args = constructor.abi_decode_input(&call.initData[4..], true)?;
    greyln!("{MINT}VERIFIED{GREY} - contract with constructor matches local project's file hashes");
    greyln!("Deployer address: {}", deployer_address);
    greyln!("Value: {}", call.initValue);
    greyln!("Salt: {}", call.salt);
    greyln!("Constructor params:");
    for (param, value) in constructor.inputs.iter().zip(constructor_args) {
        greyln!(" * {}: {:?}", param, value);
    }
    Ok(())
}

fn verify_create_deployment(calldata: &[u8], deployment_data: &[u8]) -> Result<()> {
    if deployment_data == calldata {
        greyln!("{MINT}VERIFIED{GREY} - contract matches local project's file hashes");
    } else {
        let tx_prelude = extract_contract_evm_deployment_prelude(calldata);
        let reconstructed_prelude = extract_contract_evm_deployment_prelude(deployment_data);
        println!(
            "{} - contract deployment did not verify against local project's file hashes",
            "FAILED".red()
        );
        if tx_prelude != reconstructed_prelude {
            println!("Prelude mismatch");
            println!("Deployment tx prelude {}", hex::encode(tx_prelude));
            println!(
                "Reconstructed prelude {}",
                hex::encode(reconstructed_prelude)
            );
        } else {
            println!("Compressed WASM bytecode mismatch");
        }
        println!(
            "Compressed code length of locally reconstructed {}",
            extract_compressed_wasm(deployment_data).len()
        );
        println!(
            "Compressed code length of deployment tx {}",
            extract_compressed_wasm(calldata).len()
        );
    }
    Ok(())
}
