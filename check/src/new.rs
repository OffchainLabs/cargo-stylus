// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::constants::{GITHUB_TEMPLATE_REPO, GITHUB_TEMPLATE_REPO_MINIMAL};
use cargo_stylus_util::{
    color::{Color, GREY},
    sys,
};
use eyre::{bail, Context, Result};
use std::{env::current_dir, path::Path};

/// Creates a new Stylus project in the current directory
pub fn new(name: &Path, minimal: bool) -> Result<()> {
    let repo = match minimal {
        true => GITHUB_TEMPLATE_REPO_MINIMAL,
        false => GITHUB_TEMPLATE_REPO,
    };
    let output = sys::new_command("git")
        .arg("clone")
        .arg(repo)
        .arg(name)
        .output()
        .wrap_err("git clone failed")?;

    if !output.status.success() {
        bail!("git clone command failed");
    }
    let path = current_dir().wrap_err("no current dir")?.join(name);
    println!("{GREY}new project at: {}", path.to_string_lossy().mint());
    Ok(())
}
