// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use eyre::eyre;
use std::{
    fs::File,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Exports the solidity ABI a Stylus Rust program in the current directory to stdout.
pub fn export_abi(release: bool, output: Option<PathBuf>) -> eyre::Result<()> {
    let target_host =
        rustc_host::from_cli().map_err(|e| eyre!("could not get host target architecture: {e}"))?;
    let mut cmd = Command::new("cargo");

    match output.as_ref() {
        Some(output_file_path) => {
            let output_file = File::create(output_file_path).map_err(|e| {
                eyre!(
                    "could not create output file to write ABI at path {}: {e}",
                    output_file_path.as_os_str().to_string_lossy()
                )
            })?;
            cmd.stdout(output_file);
        }
        None => {
            cmd.stdout(Stdio::inherit());
        }
    }

    cmd.stderr(Stdio::inherit())
        .arg("run")
        .arg("--features=export-abi")
        .arg(format!("--target={}", target_host));

    if release {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .map_err(|e| eyre!("failed to execute export abi command: {e}"))?;
    if !output.status.success() {
        return Err(eyre!("Export ABI command failed: {:?}", output));
    }
    Ok(())
}
