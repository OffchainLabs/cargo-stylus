// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use cargo_stylus_util::color::Color;
use eyre::{bail, eyre, Result};

use crate::constants::{RUST_BASE_IMAGE_VERSION, TOOLCHAIN_FILE_NAME};
use crate::macros::greyln;
use crate::project::extract_toolchain_channel;

fn version_to_image_name(version: &str) -> String {
    format!("cargo-stylus-{}", version)
}

fn image_exists(name: &str) -> Result<bool> {
    let output = Command::new("docker")
        .arg("images")
        .arg(name)
        .output()
        .map_err(|e| eyre!("failed to execute Docker command: {e}"))?;
    Ok(output.stdout.iter().filter(|c| **c == b'\n').count() > 1)
}

fn create_image() -> Result<()> {
    let version = "1.79".to_string();
    let name = version_to_image_name(&version);
    if image_exists(&name)? {
        return Ok(());
    }
    let toolchain_file_path = PathBuf::from(".").as_path().join(TOOLCHAIN_FILE_NAME);
    let toolchain_channel = extract_toolchain_channel(&toolchain_file_path)?;
    // let rust_version = extract_toolchain_version(&toolchain_file_path)?;
    let mut child = Command::new("docker")
        .arg("build")
        .arg("-t")
        .arg(name)
        .arg(".")
        .arg("-f-")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| eyre!("failed to execure Docker command: {e}"))?;
    write!(
        child.stdin.as_mut().unwrap(),
        "\
            FROM --platform=linux/amd64 rust:{} as builder\n\
            RUN rustup toolchain install {}-x86_64-unknown-linux-gnu 
            RUN rustup default {}-x86_64-unknown-linux-gnu
            RUN rustup target add wasm32-unknown-unknown
            RUN rustup target add wasm32-wasi
            RUN rustup target add x86_64-unknown-linux-gnu
            RUN apt-get update && apt-get install -y git
            RUN git clone https://github.com/offchainlabs/cargo-stylus.git
            WORKDIR /cargo-stylus
            RUN git checkout docker-changes
            RUN cargo install --path check
            RUN cargo install --path main
        ",
        RUST_BASE_IMAGE_VERSION,
        toolchain_channel,
        toolchain_channel,
    )?;
    child.wait().map_err(|e| eyre!("wait failed: {e}"))?;
    Ok(())
}

fn run_in_docker_container(version: &str, command_line: &[&str]) -> Result<()> {
    let name = version_to_image_name(version);
    if !image_exists(&name)? {
        bail!("Docker image {name} doesn't exist");
    }
    let dir =
        std::env::current_dir().map_err(|e| eyre!("failed to find current directory: {e}"))?;
    Command::new("docker")
        .arg("run")
        .arg("--network")
        .arg("host")
        .arg("-w")
        .arg("/source")
        .arg("-v")
        .arg(format!("{}:/source", dir.as_os_str().to_str().unwrap()))
        .arg(name)
        .args(command_line)
        .spawn()
        .map_err(|e| eyre!("failed to execute Docker command: {e}"))?
        .wait()
        .map_err(|e| eyre!("wait failed: {e}"))?;
    Ok(())
}

pub fn run_reproducible(version: &str, command_line: &[String]) -> Result<()> {
    verify_valid_host()?;
    let version: String = version
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == ':' || *c == '-')
        .collect();
    greyln!(
        "Running reproducible Stylus command with Rust Docker image tag {}",
        version.mint()
    );
    let mut command = vec!["cargo", "stylus"];
    for s in command_line.iter() {
        command.push(s);
    }
    create_image()?;
    run_in_docker_container(&version, &command)
}

fn verify_valid_host() -> Result<()> {
    let Ok(os_type) = sys_info::os_type() else {
        bail!("unable to determine host OS type");
    };
    if os_type == "Windows" {
        // Check for WSL environment
        let Ok(kernel_version) = sys_info::os_release() else {
            bail!("unable to determine kernel version");
        };
        if kernel_version.contains("microsoft") || kernel_version.contains("WSL") {
            greyln!("Detected Windows Linux Subsystem host");
        } else {
            bail!(
                "Reproducible cargo stylus commands on Windows are only supported \
            in Windows Linux Subsystem (WSL). Please install within WSL. \
            To instead opt out of reproducible builds, add the --no-verify \
            flag to your commands."
            );
        }
    }
    Ok(())
}
