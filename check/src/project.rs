// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::{
    constants::{BROTLI_COMPRESSION_LEVEL, EOF_PREFIX_NO_DICT, RUST_TARGET},
    macros::*,
};
use brotli2::read::BrotliEncoder;
use cargo_stylus_util::{color::Color, sys};
use eyre::{bail, eyre, Result, WrapErr};
use glob::glob;
use std::process::Command;
use std::{
    env::current_dir,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process,
};
use tiny_keccak::{Hasher, Keccak};

#[derive(Default, Clone, PartialEq)]
pub enum OptLevel {
    #[default]
    S,
    Z,
}

#[derive(Default, Clone)]
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
}

/// Build a Rust project to WASM and return the path to the compiled WASM file.
pub fn build_dylib(cfg: BuildConfig) -> Result<PathBuf> {
    let cwd: PathBuf = current_dir().map_err(|e| eyre!("could not get current dir: {e}"))?;

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

    let (wasm, code) = compress_wasm(&wasm_file_path).wrap_err("failed to compress WASM")?;

    greyln!(
        "contract size: {}",
        crate::check::format_file_size(code.len(), 16, 24)
    );
    greyln!(
        "wasm size: {}",
        crate::check::format_file_size(wasm.len(), 96, 128)
    );
    Ok(wasm_file_path)
}

fn all_paths(root_dir: &Path, source_file_patterns: Vec<String>) -> Result<Vec<PathBuf>> {
    let mut files = Vec::<PathBuf>::new();
    let mut directories = Vec::<PathBuf>::new();
    directories.push(root_dir.to_path_buf()); // Using `from` directly

    let glob_paths = expand_glob_patterns(source_file_patterns)?;

    while let Some(dir) = directories.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|e| eyre!("Unable to read directory {}: {e}", dir.display()))?
        {
            let entry = entry.map_err(|e| eyre!("Error finding file in {}: {e}", dir.display()))?;
            let path = entry.path();

            if path.is_dir() {
                if path.ends_with("target") || path.ends_with(".git") {
                    continue; // Skip "target" and ".git" directories
                }
                directories.push(path);
            } else if path.file_name().map_or(false, |f| {
                // If the user has has specified a list of source file patterns, check if the file
                // matches the pattern.
                if glob_paths.len() > 0 {
                    for glob_path in glob_paths.iter() {
                        if glob_path == &path {
                            return true;
                        }
                    }
                    return false;
                } else {
                    // Otherwise, by default include all rust files, Cargo.toml and Cargo.lock files.
                    f == "Cargo.toml" || f == "Cargo.lock" || f.to_string_lossy().ends_with(".rs")
                }
            }) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

pub fn hash_files(source_file_patterns: Vec<String>, cfg: BuildConfig) -> Result<[u8; 32]> {
    let mut keccak = Keccak::v256();
    let mut cmd = Command::new("cargo");
    if !cfg.stable {
        cmd.arg("+nightly");
    }
    cmd.arg("--version");
    let output = cmd
        .output()
        .map_err(|e| eyre!("failed to execute cargo command: {e}"))?;
    if !output.status.success() {
        bail!("cargo version command failed");
    }
    keccak.update(&output.stdout);
    if cfg.opt_level == OptLevel::Z {
        keccak.update(&[0]);
    } else {
        keccak.update(&[1]);
    }

    let mut buf = vec![0u8; 0x100000];

    let mut hash_file = |filename: &Path| -> Result<()> {
        keccak.update(&(filename.as_os_str().len() as u64).to_be_bytes());
        keccak.update(filename.as_os_str().as_encoded_bytes());
        let mut file = std::fs::File::open(filename)
            .map_err(|e| eyre!("failed to open file {}: {e}", filename.display()))?;
        keccak.update(&file.metadata().unwrap().len().to_be_bytes());
        loop {
            let bytes_read = file
                .read(&mut buf)
                .map_err(|e| eyre!("Unable to read file {}: {e}", filename.display()))?;
            if bytes_read == 0 {
                break;
            }
            keccak.update(&buf[..bytes_read]);
        }
        Ok(())
    };

    let mut paths = all_paths(PathBuf::from(".").as_path(), source_file_patterns)?;
    paths.sort();

    for filename in paths.iter() {
        hash_file(filename)?;
    }

    let mut hash = [0u8; 32];
    keccak.finalize(&mut hash);
    Ok(hash)
}

fn expand_glob_patterns(patterns: Vec<String>) -> Result<Vec<PathBuf>> {
    let mut files_to_include = Vec::new();
    for pattern in patterns {
        let paths = glob(&pattern)
            .map_err(|e| eyre!("Failed to read glob pattern '{}': {}", pattern, e))?;
        for path_result in paths {
            let path = path_result.map_err(|e| eyre!("Error processing path: {}", e))?;
            files_to_include.push(path);
        }
    }
    Ok(files_to_include)
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

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_all_paths() -> Result<()> {
        let dir = tempdir()?;
        let dir_path = dir.path();

        let files = vec!["file.rs", "ignore.me", "Cargo.toml", "Cargo.lock"];
        for file in files.iter() {
            let file_path = dir_path.join(file);
            let mut file = File::create(&file_path)?;
            writeln!(file, "Test content")?;
        }

        let dirs = vec!["nested", ".git", "target"];
        for d in dirs.iter() {
            let subdir_path = dir_path.join(d);
            if !subdir_path.exists() {
                fs::create_dir(&subdir_path)?;
            }
        }

        let nested_dir = dir_path.join("nested");
        let nested_file = nested_dir.join("nested.rs");
        if !nested_file.exists() {
            File::create(&nested_file)?;
        }

        let found_files = all_paths(
            dir_path,
            vec![format!(
                "{}/{}",
                dir_path.as_os_str().to_string_lossy(),
                "**/*.rs"
            )],
        )?;

        // Check that the correct files are included
        assert!(found_files.contains(&dir_path.join("file.rs")));
        assert!(found_files.contains(&nested_dir.join("nested.rs")));
        assert!(!found_files.contains(&dir_path.join("ignore.me")));
        assert!(!found_files.contains(&dir_path.join("Cargo.toml"))); // Not matching *.rs
        assert_eq!(found_files.len(), 2, "Should only find 2 Rust files.");

        Ok(())
    }
}
