// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use ethers::{prelude::*, providers::Provider};
use eyre::{eyre, Context, Result};

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

/// Climb each parent directory from a given starting directory until we find Cargo.toml
pub fn discover_project_root_from_path(start_from: impl AsRef<Path>) -> Result<Option<PathBuf>> {
    discover_file_up_from_path(start_from, |path| {
        if path
            .file_name().and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name == "Cargo.toml")
        {
            path.parent().map(PathBuf::from)
        } else {
            None
        }
    })
}

/// Climb each parent directory from a given starting directory until we find a file, matching the
/// given predicate
pub fn discover_file_up_from_path<T>(
    start_from: impl AsRef<Path>,
    predicate: impl Fn(PathBuf) -> Option<T>,
) -> Result<Option<T>> {
    let mut cwd_opt = Some(start_from.as_ref());

    while let Some(cwd) = cwd_opt {
        let paths = fs::read_dir(cwd)
            .with_context(|| format!("Error reading the directory with path: {}", cwd.display()))?;

        let result = paths
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find_map(&predicate);

        if let Some(p) = result {
            return Ok(Some(p));
        }

        cwd_opt = cwd.parent();
    }

    Ok(None)
}
