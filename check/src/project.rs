// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::{
    constants::{
        BROTLI_COMPRESSION_LEVEL, EOF_PREFIX_NO_DICT, MAX_PRECOMPRESSED_WASM_SIZE,
        MAX_PROGRAM_SIZE, RUST_TARGET,
    },
    macros::*,
};
use brotli2::read::BrotliEncoder;
use bytesize::ByteSize;
use cargo_stylus_util::{color::Color, sys};
use eyre::{bail, eyre, Result, WrapErr};
use std::{env::current_dir, fs, io::Read, path::PathBuf, process};

#[derive(Default, PartialEq)]
pub enum OptLevel {
    #[default]
    S,
    Z,
}

#[derive(Default)]
pub struct BuildConfig {
    pub opt_level: OptLevel,
    pub stable: bool,
    pub rebuild: bool,
}

impl BuildConfig {
    pub fn new(stable: bool) -> Self {
        Self {
            stable,
            ..Default::default()
        }
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum BuildError {
    #[error("could not find WASM in release dir ({path}).")]
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
pub fn build_dylib(cfg: BuildConfig) -> Result<PathBuf> {
    let cwd: PathBuf = current_dir().map_err(|e| eyre!("could not get current dir: {e}"))?;

    //if cfg.rebuild {
    let mut cmd = sys::new_command("cargo");

    if !cfg.stable {
        cmd.arg("+nightly");
    }

    cmd.arg("build");
    cmd.arg("--lib");

    if !cfg.stable {
        cmd.arg("-Z");
        cmd.arg("build-std=std,panic_abort");
        cmd.arg("-Z");
        cmd.arg("build-std-features=panic_immediate_abort");
    }

    if cfg.opt_level == OptLevel::Z {
        cmd.arg("--config");
        cmd.arg("profile.release.opt-level='z'");
    }

    let output = cmd
        .arg("--release")
        .arg(format!("--target={RUST_TARGET}"))
        .output()
        .wrap_err("failed to execute cargo build")?;

    if !output.status.success() {
        egreyln!("cargo build command failed");
        process::exit(1);
    }

    let release_path = cwd
        .join("target")
        .join(RUST_TARGET)
        .join("release")
        .join("deps");

    // Gets the files in the release folder.
    let release_files: Vec<PathBuf> = fs::read_dir(&release_path)
        .map_err(|e| eyre!("could not read deps dir: {e}"))?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|r| r.is_file())
        .collect();

    let wasm_file_path = release_files
        .into_iter()
        .find(|p| {
            if let Some(ext) = p.file_name() {
                return ext.to_string_lossy().contains(".wasm");
            }
            false
        })
        .ok_or(BuildError::NoWasmFound { path: release_path })?;

    if let Err(e) = compress_wasm(&wasm_file_path) {
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
                    return build_dylib(BuildConfig {
                        opt_level: OptLevel::Z,
                        stable: cfg.stable,
                        rebuild: true,
                    });
                }
                OptLevel::Z => {
                    if !cfg.stable {
                        println!(
                            r#"Compressed program still exceeding max program size {} > max of 24Kb, 
rebuilding with optimizations. We are actively working to reduce WASM program sizes that are
using the Stylus SDK. To see all available optimization options, see more in:
https://github.com/OffchainLabs/cargo-stylus/blob/main/OPTIMIZING_BINARIES.md"#,
                            got.red(),
                        );
                        // Attempt to build again with the nightly flag enabled and extra optimizations
                        // only available with nightly compilation.
                        return build_dylib(BuildConfig {
                            opt_level: OptLevel::Z,
                            stable: false,
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
pub fn compress_wasm(wasm: &PathBuf) -> Result<(Vec<u8>, Vec<u8>)> {
    let wasm =
        fs::read(wasm).wrap_err_with(|| eyre!("failed to read Wasm {}", wasm.to_string_lossy()))?;

    let wasm = wasmer::wat2wasm(&wasm).wrap_err("failed to parse Wasm")?;

    let mut compressor = BrotliEncoder::new(&*wasm, BROTLI_COMPRESSION_LEVEL);
    let mut compressed_bytes = vec![];
    compressor
        .read_to_end(&mut compressed_bytes)
        .wrap_err("failed to compress WASM bytes")?;

    let mut contract_code = hex::decode(EOF_PREFIX_NO_DICT).unwrap();
    contract_code.extend(compressed_bytes);

    Ok((wasm.to_vec(), contract_code))
}
