// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use eyre::{bail, eyre, Result};
use std::{
    fs::File,
    io::{stdout, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

/// Exports the Solidity ABI for a Stylus Rust program in the current directory to stdout.
pub fn export_solidity_abi(release: bool, output_file: Option<PathBuf>) -> Result<()> {
    let target_host =
        rustc_host::from_cli().map_err(|e| eyre!("could not get host target architecture: {e}"))?;
    let mut cmd = Command::new("cargo");

    cmd.stderr(Stdio::inherit())
        .arg("run")
        .arg("--features=export-abi")
        .arg(format!("--target={}", target_host));

    if release {
        cmd.arg("--release");
    }

    match output_file.as_ref() {
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

    let output = cmd
        .output()
        .map_err(|e| eyre!("failed to execute export abi command: {e}"))?;
    if !output.status.success() {
        bail!("Export ABI command failed: {:?}", output);
    }
    Ok(())
}

/// Exports the Solidity JSON ABI output from solc given a Stylus Rust project in the current directory.
/// The solc binary must be installed for this command to succeed.
pub fn export_json_abi(release: bool, output_file: Option<PathBuf>) -> Result<()> {
    // We first check if solc is installed.
    let output = Command::new("solc")
        .arg("--version")
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .map_err(|e| eyre!("could not check solc version: {e}"))?;
    if !output.status.success() {
        bail!(
            r#"The Solidity compiler's solc command failed, perhaps you do not have solc installed.
Please see https://docs.soliditylang.org/en/latest/installing-solidity.html on how to install it"#,
        );
    }

    let target_host =
        rustc_host::from_cli().map_err(|e| eyre!("could not get host target architecture: {e}"))?;
    let mut exportcmd = Command::new("cargo");

    // We spawn the export command as a child process so we can pipe its output later.
    exportcmd
        .stderr(Stdio::inherit())
        .arg("run")
        .arg("--features=export-abi")
        .stdout(Stdio::piped())
        .arg(format!("--target={}", target_host));

    if release {
        exportcmd.arg("--release");
    }

    let child_proc = exportcmd
        .spawn()
        .map_err(|e| eyre!("failed to execute export abi command: {e}"))?;

    let mut cmd = Command::new("solc");

    cmd.stdin(Stdio::from(child_proc.stdout.unwrap()))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let solcout = cmd
        .arg("--abi")
        .arg("-")
        .output()
        .map_err(|e| eyre!("failed to execute solc command: {e}"))?;

    if !solcout.status.success() {
        bail!("Export ABI JSON command using solc failed: {:?}", solcout);
    }

    let mut output_file: Box<dyn Write> = match output_file.as_ref() {
        Some(output_file_path) => Box::new(File::create(output_file_path).map_err(|e| {
            eyre!(
                "could not create output file to write ABI at path {}: {e}",
                output_file_path.as_os_str().to_string_lossy()
            )
        })?),
        None => Box::new(stdout()),
    };

    // NOTE: filter out first three lines of output
    //
    // ```
    // 
    // ======= <stdin>:IName =======
    // Contract JSON ABI
    // ```
    let solcstdout = BufReader::new(&solcout.stdout[..]);
    solcstdout
        .lines()
        .skip(3)
        .try_for_each(|line| writeln!(output_file, "{}", line?))?;

    Ok(())
}
