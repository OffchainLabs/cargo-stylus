// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::color::Color;
use eyre::{bail, eyre, Result};

use crate::constants::TOOLCHAIN_FILE_NAME;
use crate::macros::greyln;
use crate::project::extract_toolchain_channel;

fn image_name(cargo_stylus_version: &str, toolchain_version: &str) -> String {
    format!(
        "cargo-stylus:base-{}-toolchain-{}",
        cargo_stylus_version, toolchain_version
    )
}

fn image_exists(image_name: &str) -> Result<bool> {
    let output = Command::new("docker")
        .arg("images")
        .arg(image_name)
        .output()
        .map_err(|e| eyre!("failed to execute Docker command: {e}"))?;

    if !output.status.success() {
        let stderr = std::str::from_utf8(&output.stderr)
            .map_err(|e| eyre!("failed to read Docker command stderr: {e}"))?;
        if stderr.contains("Cannot connect to the Docker daemon") {
            println!(
                r#"Cargo stylus deploy|check|verify run in a Docker container by default to ensure deployments
are reproducible, but Docker is not found in your system. Please install Docker if you wish to create 
a reproducible deployment, or opt out by using the --no-verify flag for local builds"#
            );
            bail!("Docker not running");
        }
        bail!(stderr.to_string())
    }

    Ok(output.stdout.iter().filter(|c| **c == b'\n').count() > 1)
}

fn create_image(cargo_stylus_version: &str, toolchain_version: &str) -> Result<()> {
    let image_name = image_name(cargo_stylus_version, toolchain_version);
    if image_exists(&image_name)? {
        return Ok(());
    }
    println!(
        "Building Docker image for Rust toolchain {}",
        toolchain_version
    );
    let mut child = Command::new("docker")
        .arg("build")
        .arg("-t")
        .arg(image_name)
        .arg(".")
        .arg("-f-")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| eyre!("failed to execute Docker command: {e}"))?;
    write!(
        child.stdin.as_mut().unwrap(),
        "\
            ARG BUILD_PLATFORM=linux/amd64
            FROM --platform=${{BUILD_PLATFORM}} offchainlabs/cargo-stylus-base:{} AS base
            RUN rustup toolchain install {}-x86_64-unknown-linux-gnu 
            RUN rustup default {}-x86_64-unknown-linux-gnu
            RUN rustup target add wasm32-unknown-unknown
            RUN rustup component add rust-src --toolchain {}-x86_64-unknown-linux-gnu
        ",
        cargo_stylus_version,
        toolchain_version,
        toolchain_version,
        toolchain_version,
    )?;
    child.wait().map_err(|e| eyre!("wait failed: {e}"))?;
    Ok(())
}

fn run_in_docker_container(
    cargo_stylus_version: &str,
    toolchain_version: &str,
    command_line: &[&str],
) -> Result<()> {
    let image_name = image_name(cargo_stylus_version, toolchain_version);
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
        .arg(image_name)
        .args(command_line)
        .spawn()
        .map_err(|e| eyre!("failed to execute Docker command: {e}"))?
        .wait()
        .map_err(|e| eyre!("wait failed: {e}"))?;
    Ok(())
}

pub fn run_reproducible(
    cargo_stylus_version: Option<String>,
    command_line: &[String],
) -> Result<()> {
    verify_valid_host()?;
    let toolchain_file_path = PathBuf::from(".").as_path().join(TOOLCHAIN_FILE_NAME);
    let toolchain_channel = extract_toolchain_channel(&toolchain_file_path)?;
    greyln!(
        "Running reproducible Stylus command with toolchain {}",
        toolchain_channel.mint()
    );
    let cargo_stylus_version =
        cargo_stylus_version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let mut command = vec!["cargo", "stylus"];
    for s in command_line.iter() {
        command.push(s);
    }
    create_image(&cargo_stylus_version, &toolchain_channel)?;
    run_in_docker_container(&cargo_stylus_version, &toolchain_channel, &command)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_image_and_check_it_exists() {
        let toolchain_version = "1.80.0";
        let cargo_stylus_version = "0.5.3";
        let image_name = image_name(&cargo_stylus_version, toolchain_version);
        println!("image name: {}", image_name);

        // Remove existing docker image
        Command::new("docker")
            .arg("image")
            .arg("rm")
            .arg("-f")
            .arg(&image_name)
            .spawn()
            .expect("failed to spawn docker image rm")
            .wait()
            .expect("failed to run docker image rm");

        assert!(!image_exists(&image_name).unwrap());
        create_image(&cargo_stylus_version, toolchain_version).unwrap();
        assert!(image_exists(&image_name).unwrap());
    }
}
