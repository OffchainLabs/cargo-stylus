// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use eyre::{bail, eyre};
use std::{env::current_dir, path::PathBuf};

use crate::{
    color::Color,
    constants::{GITHUB_TEMPLATE_REPO, GITHUB_TEMPLATE_REPO_MINIMAL},
    util,
};

/// Creates a new Stylus project in the current directory
pub fn new_stylus_project(name: &str, minimal: bool) -> eyre::Result<()> {
    if name.is_empty() {
        bail!("cannot have an empty project name");
    }
    let cwd: PathBuf = current_dir().map_err(|e| eyre!("could not get current dir: {e}"))?;

    let repo = match minimal {
        true => GITHUB_TEMPLATE_REPO_MINIMAL,
        false => GITHUB_TEMPLATE_REPO,
    };
    let output = util::new_command("git")
        .arg("clone")
        .arg(repo)
        .arg(name)
        .output()
        .map_err(|e| eyre!("failed to execute git clone: {e}"))?;

    if !output.status.success() {
        bail!("git clone command failed");
    }
    let project_path = cwd.join(name);
    println!(
        "Initialized Stylus project at: {}",
        project_path.as_os_str().to_string_lossy().mint()
    );
    Ok(())
}
