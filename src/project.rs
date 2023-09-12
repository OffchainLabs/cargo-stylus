// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::env::current_dir;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use brotli2::read::BrotliEncoder;
use bytesize::ByteSize;
use eyre::{bail, eyre};

use crate::constants::{MAX_PRECOMPRESSED_WASM_SIZE, MAX_PROGRAM_SIZE};
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
    pub rebuild: bool,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum BuildError {
    #[error("could not find WASM in release dir ({path}). Hint: Do you have a main.rs?")]
    NoWasmFound { path: PathBuf },
    #[error(
        r#"compressed program size ({got}) exceeds max ({want}) despite --nightly flag. We recommend splitting up your program. 
We are actively working to reduce WASM program sizes that use the Stylus SDK.
To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#
    )]
    ExceedsMaxDespiteBestEffort { got: ByteSize, want: ByteSize },
    #[error(
        r#"Brotli-compressed WASM program size ({got}) is bigger than program size limit: ({want}). We recommend splitting up your program. 
We are actively working to reduce WASM program sizes that use the Stylus SDK.
To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#
    )]
    MaxCompressedSizeExceeded { got: ByteSize, want: ByteSize },
    #[error(
        r#"uncompressed WASM program size ({got}) is bigger than size limit: ({want}). We recommend splitting up your program. 
We are actively working to reduce WASM program sizes that use the Stylus SDK.
To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#)]
    MaxPrecompressedSizeExceeded { got: ByteSize, want: ByteSize },
}

/// Build a Rust project to WASM and return the path to the compiled WASM file.
pub fn build_project_to_wasm(cfg: BuildConfig) -> eyre::Result<PathBuf> {
    let cwd: PathBuf = current_dir().map_err(|e| eyre!("could not get current dir: {e}"))?;

    if cfg.rebuild {
        let mut cmd = Command::new("cargo");
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

        if cfg.nightly {
            cmd.arg("+nightly");
            let msg = "Warning:".to_string().yellow();
            println!("{} building with the Rust nightly toolchain, make sure you are aware of the security risks of doing so", msg);
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

        let output = cmd
            .arg("--release")
            .arg(format!("--target={}", RUST_TARGET))
            .output()
            .map_err(|e| eyre!("failed to execute cargo build: {e}"))?;

        if !output.status.success() {
            bail!("cargo build command failed");
        }
    }

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

    if let Err(e) = get_compressed_wasm_bytes(&wasm_file_path) {
        if let Some(BuildError::MaxCompressedSizeExceeded { got, .. }) = e.downcast_ref() {
            match cfg.opt_level {
                OptLevel::S => {
                    println!(
                        r#"Compressed program built with defaults had program size {} > max of 24Kb, 
rebuilding with optimizations. We are actively working to reduce WASM program sizes that are
using the Stylus SDK. To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#,
                        got.red(),
                    );
                    // Attempt to build again with a bumped-up optimization level.
                    return build_project_to_wasm(BuildConfig {
                        opt_level: OptLevel::Z,
                        nightly: cfg.nightly,
                        rebuild: true,
                    });
                }
                OptLevel::Z => {
                    if !cfg.nightly {
                        println!(
                            r#"Compressed program still exceeding max program size {} > max of 24Kb, 
rebuilding with optimizations. We are actively working to reduce WASM program sizes that are
using the Stylus SDK. To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#,
                            got.red(),
                        );
                        // Attempt to build again with the nightly flag enabled and extra optimizations
                        // only available with nightly compilation.
                        return build_project_to_wasm(BuildConfig {
                            opt_level: OptLevel::Z,
                            nightly: true,
                            rebuild: true,
                        });
                    }
                    return Err(BuildError::ExceedsMaxDespiteBestEffort {
                        got: *got,
                        want: MAX_PROGRAM_SIZE,
                    }
                    .into());
                }
            }
        }
        return Err(e);
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

    let precompressed_size = ByteSize::b(wasm_bytes.len() as u64);
    if precompressed_size > MAX_PRECOMPRESSED_WASM_SIZE {
        return Err(BuildError::MaxPrecompressedSizeExceeded {
            got: precompressed_size,
            want: MAX_PRECOMPRESSED_WASM_SIZE,
        }
        .into());
    }

    let compressed_size = ByteSize::b(deploy_ready_code.len() as u64);
    if compressed_size > MAX_PROGRAM_SIZE {
        return Err(BuildError::MaxCompressedSizeExceeded {
            got: compressed_size,
            want: MAX_PROGRAM_SIZE,
        }
        .into());
    }

    Ok((wasm_bytes.to_vec(), deploy_ready_code))
}
