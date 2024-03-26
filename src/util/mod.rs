// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::{
    env,
    ffi::OsStr,
    fs, io,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use ethers::{prelude::*, providers::Provider};
use eyre::{eyre, Context, OptionExt, Result};

pub mod scripts;

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

/// Climb each parent directory from a given starting directory until we find `.git`
pub fn discover_project_root_from_path(start_from: impl AsRef<Path>) -> Result<Option<PathBuf>> {
    discover_file_up_from_path(start_from, |path| -> Option<PathBuf> {
        path.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name == ".git")
            .then(|| {
                path.parent()
                    .expect(".git is sure to be inside another directory")
                    .into()
            })
    })
}

/// Climb each parent directory from a given starting directory until we find a file, matching the given predicate
pub fn discover_file_up_from_path<T>(
    start_from: impl AsRef<Path>,
    predicate: impl Fn(PathBuf) -> Option<T>,
) -> Result<Option<T>> {
    let mut cwd_opt = Some(start_from.as_ref());

    while let Some(cwd) = cwd_opt {
        #[rustfmt::skip]
        let paths = fs::read_dir(cwd)
            .wrap_err(format!("Error reading the directory with path: {}", cwd.display()))?;

        let result = paths
            .collect::<Result<Vec<_>, io::Error>>()
            .wrap_err(format!("Could not read entries in {}", cwd.display()))?
            .into_iter()
            .map(|entry| entry.path())
            .find_map(&predicate);

        if let Some(p) = result {
            return Ok(Some(p));
        }

        cwd_opt = cwd.parent();
    }

    Ok(None)
}

/// Reads and trims a line from a filepath
pub fn read_and_trim_line_from_file(fpath: impl AsRef<Path>) -> eyre::Result<String> {
    let f = std::fs::File::open(fpath)?;
    let mut buf_reader = BufReader::new(f);
    let mut secret = String::new();
    buf_reader.read_line(&mut secret)?;
    Ok(secret.trim().to_string())
}

/// Find and return the Stylus project root (characterized by `.git`),
/// relative to cwd or a given directory
pub fn find_parent_project_root(start_from: Option<PathBuf>) -> Result<PathBuf> {
    let start_from = start_from.unwrap_or(env::current_dir()?);

    //  NOTE: search upwards for `.git`
    crate::util::discover_project_root_from_path(start_from)?
        .ok_or_eyre("Could not find project root")
}

/// Set cwd to the current Stylus project root
pub fn move_to_parent_project_root() -> Result<()> {
    let parent_project_root = &find_parent_project_root(None)?;

    env::set_current_dir(parent_project_root)?;
    println!("Set cwd to {}", parent_project_root.display());

    Ok(())
}

/// Convert (maybe) relative paths to absolute ones,
/// relative to another path
pub fn make_absolute_relative_to(
    path: impl AsRef<Path>,
    relative_to: impl AsRef<Path>,
) -> Result<PathBuf> {
    let mut path: PathBuf = path.as_ref().to_path_buf();
    let relative_to = relative_to.as_ref();

    if !path.is_absolute() {
        path = relative_to.join(path);
    }

    path.canonicalize()
        .wrap_err(format!("Could not canonicalize {}", path.display()))
}
