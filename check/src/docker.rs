// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::io::Write;
use std::process::{Command, Stdio};

use eyre::{bail, eyre, Result};

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

fn create_image(version: &str) -> Result<()> {
    let name = version_to_image_name(version);
    if image_exists(&name)? {
        return Ok(());
    }
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
            FROM rust:{} as builder\n\
            RUN rustup target add wasm32-unknown-unknown
            RUN rustup target add wasm32-wasi
            RUN rustup target add aarch64-unknown-linux-gnu
            RUN rustup toolchain install nightly
            RUN rustup toolchain install nightly-aarch64-apple-darwin
            RUN rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
            RUN rustup component add rust-src --toolchain nightly-aarch64-apple-darwin
            RUN cargo install cargo-stylus
            RUN cargo install --force cargo-stylus-check
            RUN cargo install --force cargo-stylus-replay
            RUN cargo install --force cargo-stylus-cgen
        ",
        version
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
        .map_err(|e| eyre!("failed to execure Docker command: {e}"))?
        .wait()
        .map_err(|e| eyre!("wait failed: {e}"))?;
    Ok(())
}

pub fn run_reproducible(version: &str, command_line: &[String]) -> Result<()> {
    let mut command = vec!["cargo", "stylus"];
    for s in command_line.iter() {
        command.push(s);
    }
    create_image(version)?;
    run_in_docker_container(version, &command)
}
