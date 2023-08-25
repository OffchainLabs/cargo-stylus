// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::{
    env::current_dir,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::{color::Color, constants::GITHUB_TEMPLATE_REPOSITORY};

/// Initializes a new Stylus project in the current directory by git cloning
/// the stylus-hello-world template repository and renaming
/// it to the user's choosing.
pub fn new_stylus_project(name: &str, minimal: bool) -> Result<(), String> {
    if name.is_empty() {
        return Err("cannot have an empty project name".to_string());
    }
    let cwd: PathBuf = current_dir().map_err(|e| format!("could not get current dir: {e}"))?;
    if minimal {
        Command::new("cargo")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .arg("new")
            .arg("--bin")
            .arg(name)
            .output()
            .map_err(|e| format!("failed to execute cargo new: {e}"))?;

        let cargo_config_dir_path = cwd.join(name).join(".cargo");
        std::fs::create_dir_all(&cargo_config_dir_path)
            .map_err(|e| format!("could not create .cargo folder in new project: {e}"))?;
        let cargo_config_path = cargo_config_dir_path.join("config");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(cargo_config_path)
            .map_err(|e| format!("could not open config file: {e}"))?;
        f.write_all(cargo_config().as_bytes())
            .map_err(|e| format!("could not write to file: {e}"))?;
        f.flush()
            .map_err(|e| format!("could not write to file: {e}"))?;

        let main_path = cwd.join(name).join("src").join("main.rs");

        // Overwrite the default main.rs file with the Stylus entrypoint.
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(main_path)
            .map_err(|e| format!("could not open main.rs file: {e}"))?;
        f.write_all(basic_entrypoint().as_bytes())
            .map_err(|e| format!("could not write to file: {e}"))?;
        f.flush()
            .map_err(|e| format!("could not write to file: {e}"))?;

        // Overwrite the default Cargo.toml file.
        let cargo_path = cwd.join(name).join("Cargo.toml");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(cargo_path)
            .map_err(|e| format!("could not open Cargo.toml file: {e}"))?;
        f.write_all(minimal_cargo_toml(name).as_bytes())
            .map_err(|e| format!("could not write to file: {e}"))?;
        f.flush()
            .map_err(|e| format!("could not write to file: {e}"))?;
        return Ok(());
    }
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

fn basic_entrypoint() -> &'static str {
    r#"#![no_main]

stylus_sdk::entrypoint!(user_main);

fn user_main(input: Vec<u8>) -> Result<Vec<u8>, Vec<u8>> {
    Ok(input)
}
"#
}

fn cargo_config() -> &'static str {
    r#"[build]
target = "wasm32-unknown-unknown"
    "#
}

fn minimal_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
stylus-sdk = {{ git = "https://github.com/OffchainLabs/stylus-sdk-rs" }}

[features]
export-abi = ["stylus-sdk/export-abi"]

[profile.release]
codegen-units = 1
strip = true
lto = true
panic = "abort"
opt-level = "s"

[workspace]
"#,
        name
    )
}
