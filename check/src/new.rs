// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::constants::{GITHUB_TEMPLATE_REPO, GITHUB_TEMPLATE_REPO_MINIMAL};
use cargo_stylus_util::{color::Color, sys};
use eyre::{bail, eyre, Context};
use std::{env::current_dir, path::Path};

/// Creates a new Stylus project in the current directory
pub fn new_stylus_project(name: &Path, minimal: bool) -> eyre::Result<()> {
    let cwd = current_dir().wrap_err_with(|| eyre!("failed to get current dir"))?;

    let repo = match minimal {
        true => GITHUB_TEMPLATE_REPO_MINIMAL,
        false => GITHUB_TEMPLATE_REPO,
    };
    let output = sys::new_command("git")
        .arg("clone")
        .arg(repo)
        .arg(name)
        .output()
        .wrap_err_with(|| eyre!("git clone failed"))?;

    if !output.status.success() {
        bail!("git clone command failed");
    }
    let project_path = cwd.join(name);
    println!(
        "Initialized Stylus project at: {}",
        project_path.to_string_lossy().mint()
    );
    Ok(())
}
