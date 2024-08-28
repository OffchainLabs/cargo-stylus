// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::constants::{GITHUB_TEMPLATE_REPO, GITHUB_TEMPLATE_REPO_MINIMAL};
use crate::util::{
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

pub fn init(minimal: bool) -> Result<()> {
    let current_dir = current_dir().wrap_err("no current dir")?;
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

    println!(
        "{GREY}initialized project in: {}",
        current_dir.to_string_lossy().mint()
    );
    Ok(())
}
