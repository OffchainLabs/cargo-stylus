// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use ethers::{prelude::*, providers::Provider};
use eyre::{eyre, Context, Result};
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    time::Duration,
};

pub fn new_provider(url: &str) -> Result<Provider<Http>> {
    let mut provider =
        Provider::<Http>::try_from(url).wrap_err_with(|| eyre!("failed to init http provider"))?;

    provider.set_interval(Duration::from_millis(250));
    Ok(provider)
}

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
