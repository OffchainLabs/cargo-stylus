// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

#![allow(clippy::println_empty_string)]

use std::path::PathBuf;

use eyre::{bail, eyre};

use ethers::middleware::Middleware;
use ethers::types::{H160, H256};

use serde::{Deserialize, Serialize};

use cargo_stylus_check::project::BuildConfig;
use cargo_stylus_util::util;

#[derive(Debug, Deserialize, Serialize)]
struct RpcResult {
    input: String,
}

pub async fn verify(cfg: VerifyConfig) -> eyre::Result<()> {
    let provider = util::new_provider(&cfg.common_cfg.endpoint)?;
    let hash = hex::decode(
        cfg.deployment_tx
            .as_str()
            .strip_prefix("0x")
            .unwrap_or(&cfg.deployment_tx)
            .as_bytes(),
    )
    .map_err(|e| eyre!("Invalid hash: {e}"))?;
    if hash.len() != 32 {
        bail!("Invalid hash");
    }
    let Some(result) = provider
        .get_transaction(H256::from_slice(&hash))
        .await
        .map_err(|e| eyre!("RPC failed: {e}"))?
    else {
        bail!("No code at address");
    };

    let output = util::new_command("cargo")
        .arg("clean")
        .output()
        .map_err(|e| eyre!("failed to execute cargo clean: {e}"))?;
    if !output.status.success() {
        bail!("cargo clean command failed");
    }
    let check_cfg = CheckConfig {
        common_cfg: cfg.common_cfg.clone(),
        wasm_file_path: None,
        expected_program_address: H160::zero(),
    };
    check::run_checks(check_cfg)
        .await
        .map_err(|e| eyre!("Stylus checks failed: {e}"))?;
    let build_cfg = BuildConfig {
        opt_level: project::OptLevel::default(),
        nightly: cfg.common_cfg.nightly,
        rebuild: false,
        skip_contract_size_check: cfg.common_cfg.skip_contract_size_check,
    };
    let wasm_file_path: PathBuf = project::build_project_dylib(build_cfg)
        .map_err(|e| eyre!("could not build project to WASM: {e}"))?;
    let (_, init_code) =
        project::compress_wasm(&wasm_file_path, cfg.common_cfg.skip_contract_size_check)?;
    let hash = project::hash_files(build_cfg)?;
    let deployment_data = project::program_deployment_calldata(&init_code, &hash);

    if deployment_data == *result.input {
        println!("Verified - data matches!");
    } else {
        println!("Not verified - data does not match!");
    }

    Ok(())
}
