// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use eyre::eyre;
use std::process::{Command, Stdio};

/// Exports the solidity ABI a Stylus Rust program in the current directory to stdout.
pub fn export_abi(release: bool) -> eyre::Result<()> {
    let target_host =
        rustc_host::from_cli().map_err(|e| eyre!("could not get host target architecture: {e}"))?;
    println!("Exporting Solidity ABI for Stylus Rust program in current directory");
    let mut cmd = Command::new("cargo");

    cmd.stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("run")
        .arg("--features=export-abi")
        .arg(format!("--target={}", target_host));

    if release {
        cmd.arg("--release");
    }

    cmd.output()
        .map_err(|e| eyre!("failed to execute export abi command: {e}"))?;
    Ok(())
}
