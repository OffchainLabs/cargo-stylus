// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use eyre::{Context, Result};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

pub fn new_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    command
}

pub fn command_exists<S: AsRef<OsStr>>(program: S) -> bool {
    Command::new(program)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("--version")
        .output()
        .map(|x| x.status.success())
        .unwrap_or_default()
}

pub fn host_arch() -> Result<String> {
    rustc_host::from_cli().wrap_err_with(|| "failed to get host arch")
}

/// Opens a file for writing, or stdout.
pub fn file_or_stdout(path: Option<PathBuf>) -> Result<Box<dyn Write>> {
    Ok(match path {
        Some(file) => Box::new(File::create(file)?),
        None => Box::new(io::stdout().lock()),
    })
}
