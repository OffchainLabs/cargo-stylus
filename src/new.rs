// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::{
    env::current_dir,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::{color::Color, constants::GITHUB_TEMPLATE_REPOSITORY};

/// Initializes a new Stylus project in the current directory by git cloning
/// the stylus-hello-world template repository and renaming
/// it to the user's choosing.
pub fn new_stylus_project(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("cannot have an empty project name".to_string());
    }
    let cwd: PathBuf = current_dir().map_err(|e| format!("could not get current dir: {e}"))?;
    Command::new("git")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("clone")
        .arg(GITHUB_TEMPLATE_REPOSITORY)
        .arg(name)
        .output()
        .map_err(|e| format!("failed to execute git clone: {e}"))?;

    let project_path = cwd.join(name);
    println!(
        "Initialized Stylus project at: {}",
        project_path.as_os_str().to_string_lossy().mint()
    );
    Ok(())
}
