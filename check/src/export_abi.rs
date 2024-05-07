// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use cargo_stylus_util::{color::Color, sys};
use eyre::{bail, Result, WrapErr};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Exports Solidity ABIs by running the program natively.
pub fn export_abi(file: Option<PathBuf>, json: bool) -> Result<()> {
    if json && !sys::command_exists("solc") {
        let link = "https://docs.soliditylang.org/en/latest/installing-solidity.html".red();
        bail!("solc not found. Please see\n{link}");
    }

    let target = format!("--target={}", sys::host_arch()?);
    let mut output = Command::new("cargo")
        .stderr(Stdio::inherit())
        .arg("run")
        .arg("--features=export-abi")
        .arg(target)
        .output()?;

    if !output.status.success() {
        let out = String::from_utf8_lossy(&output.stdout);
        bail!("failed to run program: {out}");
    }

    // convert the ABI to a JSON file via solc
    if json {
        let solc = Command::new("solc")
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit())
            .stdout(Stdio::piped())
            .arg("--abi")
            .arg("-")
            .spawn()
            .wrap_err("failed to run solc")?;

        let mut stdin = solc.stdin.as_ref().unwrap();
        stdin.write_all(&output.stdout)?;
        output = solc.wait_with_output()?;
    }

    let mut out = sys::file_or_stdout(file)?;
    out.write_all(&output.stdout)?;
    Ok(())
}
