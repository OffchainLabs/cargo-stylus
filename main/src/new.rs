// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::constants::{GITHUB_TEMPLATE_REPO, GITHUB_TEMPLATE_REPO_MINIMAL};
use crate::util::{
    color::{Color, GREY},
    sys,
};
use eyre::{bail, Context, Result};
use std::{env, fs, path::Path};

/// Creates a new directory given the path and then initialize a stylus project.
pub fn new(path: &Path, minimal: bool) -> Result<()> {
    fs::create_dir_all(path).wrap_err("failed to create project dir")?;
    env::set_current_dir(path).wrap_err("failed to set project dir")?;
    init(minimal)
}

/// Creates a new Stylus project in the current directory.
pub fn init(minimal: bool) -> Result<()> {
    let current_dir = env::current_dir().wrap_err("no current dir")?;
    let repo = if minimal {
        GITHUB_TEMPLATE_REPO_MINIMAL
    } else {
        GITHUB_TEMPLATE_REPO
    };

    let output = sys::new_command("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(repo)
        .arg(".")
        .output()
        .wrap_err("git clone failed")?;

    if !output.status.success() {
        bail!("git clone command failed");
    }

    let output = sys::new_command("git")
        .arg("remote")
        .arg("remove")
        .arg("origin")
        .output()
        .wrap_err("git remote remove failed")?;

    if !output.status.success() {
        bail!("git remote remove command failed");
    }

    println!(
        "{GREY}initialized project in: {}",
        current_dir.to_string_lossy().mint()
    );
    Ok(())
}
