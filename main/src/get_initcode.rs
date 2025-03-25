// Copyright 2025, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/main/licenses/COPYRIGHT.md

use std::{fs::File, io::Write};
use eyre::{Result, WrapErr};
use crate::{deploy::contract_deployment_calldata, project, GetInitcodeConfig};

/// Build and print initcode for the given source
pub fn get_initcode(cfg: &GetInitcodeConfig) -> Result<()> {
    let (wasm, project_hash) = project::build_wasm_from_features(
        cfg.features.clone(),
        cfg.source_files_for_project_hash.clone(),
    )?;

    let (_, code) =
        project::compress_wasm(&wasm, project_hash).wrap_err("failed to compress WASM")?;

    let initcode = contract_deployment_calldata(&code);
    let hex_initcode = hex::encode(initcode);

    match &cfg.output {
        Some(path) => {
            let mut file = File::create(path).wrap_err("failed to create output file")?;
            file.write_all(hex_initcode.as_bytes())?;
        }
        None => {
            println!("{hex_initcode}");
        }
    }

    Ok(())
}