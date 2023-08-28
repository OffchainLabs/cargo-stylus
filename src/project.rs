// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::env::current_dir;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use brotli2::read::BrotliEncoder;
use bytesize::ByteSize;
use eyre::eyre;

use crate::constants::MAX_PROGRAM_SIZE;
use crate::{
    color::Color,
    constants::{BROTLI_COMPRESSION_LEVEL, EOF_PREFIX, RUST_TARGET},
};

#[derive(Default, PartialEq)]
pub enum OptLevel {
    #[default]
    S,
    Z,
}

pub struct BuildConfig {
    pub opt_level: OptLevel,
    pub nightly: bool,
    pub clean: bool,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum BuildError {
    #[error("Could not find WASM in release dir ({path})")]
    NoWasmFound { path: PathBuf },
    #[error(
        "program size exceeds max despite --nightly flag. We recommend splitting up your program"
    )]
    ExceedsMaxDespiteBestEffort,
}

/// Build a Rust project to WASM and return the path to the compiled WASM file.
pub fn build_project_to_wasm(cfg: BuildConfig) -> eyre::Result<PathBuf> {
    let cwd: PathBuf = current_dir().map_err(|e| eyre!("could not get current dir: {e}"))?;

    if cfg.clean {
        // Clean the cargo project for fresh checks each time.
        Command::new("cargo")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .arg("clean")
            .output()
            .map_err(|e| eyre!("failed to execute cargo clean: {e}"))?;
    }

    let mut cmd = Command::new("cargo");
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    if cfg.nightly {
        cmd.arg("+nightly");
        let msg = "Warning:".to_string().yellow();
        if cfg.clean {
            println!("{} building with the Rust nightly toolchain, make sure you are aware of the security risks of doing so", msg);
        }
    }

    cmd.arg("build");

    if cfg.nightly {
        cmd.arg("-Z");
        cmd.arg("build-std=std,panic_abort");
        cmd.arg("-Z");
        cmd.arg("build-std-features=panic_immediate_abort");
    }

    if matches!(cfg.opt_level, OptLevel::Z) {
        cmd.arg("--config");
        cmd.arg("profile.release.opt-level='z'");
    }

    cmd.arg("--release")
        .arg(format!("--target={}", RUST_TARGET))
        .output()
        .map_err(|e| eyre!("failed to execute cargo build: {e}"))?;

    let release_path = cwd.join("target").join(RUST_TARGET).join("release");

    // Gets the files in the release folder.
    let release_files: Vec<PathBuf> = std::fs::read_dir(&release_path)
        .map_err(|e| eyre!("could not read release dir: {e}"))?
        .filter(|r| r.is_ok())
        .map(|r| r.unwrap().path())
        .filter(|r| r.is_file())
        .collect();

    let wasm_file_path = release_files
        .into_iter()
        .find(|p| {
            if let Some(ext) = p.file_name() {
                return ext.to_str().unwrap_or("").contains(".wasm");
            }
            false
        })
        .ok_or(BuildError::NoWasmFound { path: release_path })?;

    let (_, compressed_wasm_code) = get_compressed_wasm_bytes(&wasm_file_path)?;
    let compressed_size = ByteSize::b(compressed_wasm_code.len() as u64);
    if compressed_size > MAX_PROGRAM_SIZE {
        match cfg.opt_level {
            OptLevel::S => {
                println!(
                    "Compressed program built with defaults had program size {} > max of 24Kb, rebuilding with optimizations", 
                    compressed_size.red(),
                );
                // Attempt to build again with a bumped-up optimization level.
                return build_project_to_wasm(BuildConfig {
                    opt_level: OptLevel::Z,
                    nightly: cfg.nightly,
                    clean: false,
                });
            }
            OptLevel::Z => {
                if !cfg.nightly {
                    let msg = eyre!(
                        r#"WASM program size {} > max size of 24Kb after applying optimizations. We recommend
reducing the codesize or attempting to build again with the --nightly flag. However, this flag can pose a security risk if used liberally"#,
                        compressed_size.red(),
                    );
                    return Err(msg);
                }
                return Err(BuildError::ExceedsMaxDespiteBestEffort.into());
            }
        }
    }

    Ok(wasm_file_path)
}

/// Reads a WASM file at a specified path and returns its brotli compressed bytes.
pub fn get_compressed_wasm_bytes(wasm_path: &PathBuf) -> eyre::Result<(Vec<u8>, Vec<u8>)> {
    let wasm_file_bytes = std::fs::read(wasm_path).map_err(|e| {
        eyre!(
            "could not read WASM file at target path {}: {e}",
            wasm_path.as_os_str().to_string_lossy(),
        )
    })?;

    let wasm_bytes = wasmer::wat2wasm(&wasm_file_bytes)
        .map_err(|e| eyre!("could not parse wasm file bytes: {e}"))?;
    let wasm_bytes = &*wasm_bytes;

    let mut compressor = BrotliEncoder::new(wasm_bytes, BROTLI_COMPRESSION_LEVEL);
    let mut compressed_bytes = vec![];
    compressor
        .read_to_end(&mut compressed_bytes)
        .map_err(|e| eyre!("could not Brotli compress WASM bytes: {e}"))?;
    let mut deploy_ready_code = hex::decode(EOF_PREFIX).unwrap();
    deploy_ready_code.extend(compressed_bytes);
    Ok((wasm_bytes.to_vec(), deploy_ready_code))
}
