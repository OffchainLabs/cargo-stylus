// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::{
    constants::{
        BROTLI_COMPRESSION_LEVEL, EOF_PREFIX_NO_DICT, PROJECT_HASH_SECTION_NAME, RUST_TARGET,
        TOOLCHAIN_FILE_NAME,
    },
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
use toml::Value;

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

    let (wasm, code) =
        compress_wasm(&wasm_file_path, [0u8; 32]).wrap_err("failed to compress WASM")?;

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
                if !glob_paths.is_empty() {
                    for glob_path in glob_paths.iter() {
                        if glob_path == &path {
                            return true;
                        }
                    }
                    false
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

pub fn extract_toolchain_channel(toolchain_file_path: &PathBuf) -> Result<String> {
    let toolchain_file_contents = fs::read_to_string(toolchain_file_path).context(
        "expected to find a rust-toolchain.toml file in project directory \
        to specify your Rust toolchain for reproducible verification. The channel in your project's rust-toolchain.toml's \
        toolchain section must be a specific version e.g., '1.80.0' or 'nightly-YYYY-MM-DD'. \
        To ensure reproducibility, it cannot be a generic channel like 'stable', 'nightly', or 'beta'. Read more about \
        the toolchain file in https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file or see \
        the file in https://github.com/OffchainLabs/stylus-hello-world for an example",
    )?;

    let toolchain_toml: Value =
        toml::from_str(&toolchain_file_contents).context("failed to parse rust-toolchain.toml")?;

    // Extract the channel from the toolchain section
    let Some(toolchain) = toolchain_toml.get("toolchain") else {
        bail!("toolchain section not found in rust-toolchain.toml");
    };
    let Some(channel) = toolchain.get("channel") else {
        bail!("could not find channel in rust-toolchain.toml's toolchain section");
    };
    let Some(channel) = channel.as_str() else {
        bail!("channel in rust-toolchain.toml's toolchain section is not a string");
    };

    // Reject "stable" and "nightly" channels specified alone
    if channel == "stable" || channel == "nightly" || channel == "beta" {
        bail!("the channel in your project's rust-toolchain.toml's toolchain section must be a specific version e.g., '1.80.0' or 'nightly-YYYY-MM-DD'. \
        To ensure reproducibility, it cannot be a generic channel like 'stable', 'nightly', or 'beta'");
    }

    // Parse the Rust version from the toolchain project, only allowing alphanumeric chars and dashes.
    let channel = channel
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '.')
        .collect();

    Ok(channel)
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

    // Fetch the Rust toolchain toml file from the project root. Assert that it exists and add it to the
    // files in the directory to hash.
    let toolchain_file_path = PathBuf::from(".").as_path().join(TOOLCHAIN_FILE_NAME);
    let _ = std::fs::metadata(&toolchain_file_path).wrap_err(
        "expected to find a rust-toolchain.toml file in project directory \
         to specify your Rust toolchain for reproducible verification",
    )?;

    let mut paths = all_paths(PathBuf::from(".").as_path(), source_file_patterns)?;
    paths.push(toolchain_file_path);
    paths.sort();

    for filename in paths.iter() {
        greyln!(
            "File used for deployment hash: {}",
            filename.as_os_str().to_string_lossy()
        );
        hash_file(filename)?;
    }

    let mut hash = [0u8; 32];
    keccak.finalize(&mut hash);
    greyln!(
        "project metadata hash computed on deployment: {:?}",
        hex::encode(hash)
    );
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
pub fn compress_wasm(wasm: &PathBuf, project_hash: [u8; 32]) -> Result<(Vec<u8>, Vec<u8>)> {
    let wasm =
        fs::read(wasm).wrap_err_with(|| eyre!("failed to read Wasm {}", wasm.to_string_lossy()))?;

    let wasm = add_project_hash_to_wasm_file(&wasm, project_hash)
        .wrap_err("failed to add project hash to wasm file as custom section")?;
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

// Adds the hash of the project's source files to the wasm as a custom section
// if it does not already exist. This allows for reproducible builds by cargo stylus
// for all Rust stylus programs. See `cargo stylus verify --help` for more information.
fn add_project_hash_to_wasm_file(
    wasm_file_bytes: &[u8],
    project_hash: [u8; 32],
) -> Result<Vec<u8>> {
    let section_exists = has_project_hash_section(wasm_file_bytes)?;
    if section_exists {
        greyln!("Wasm file bytes already contains a custom section with a project hash, not overwriting'");
        return Ok(wasm_file_bytes.to_vec());
    }
    Ok(add_custom_section(wasm_file_bytes, project_hash))
}

pub fn has_project_hash_section(wasm_file_bytes: &[u8]) -> Result<bool> {
    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(wasm_file_bytes) {
        if let wasmparser::Payload::CustomSection(reader) = payload? {
            if reader.name() == PROJECT_HASH_SECTION_NAME {
                println!(
                    "Found the project hash custom section name {}",
                    hex::encode(reader.data())
                );
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn add_custom_section(wasm_file_bytes: &[u8], project_hash: [u8; 32]) -> Vec<u8> {
    let mut bytes = vec![];
    bytes.extend_from_slice(wasm_file_bytes);
    wasm_gen::write_custom_section(&mut bytes, PROJECT_HASH_SECTION_NAME, &project_hash);
    bytes
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_extract_toolchain_channel() -> Result<()> {
        let dir = tempdir()?;
        let dir_path = dir.path();

        let toolchain_file_path = dir_path.join(TOOLCHAIN_FILE_NAME);
        let toolchain_contents = r#"
            [toolchain]
        "#;
        std::fs::write(&toolchain_file_path, toolchain_contents)?;

        let channel = extract_toolchain_channel(&toolchain_file_path);
        let Err(err_details) = channel else {
            panic!("expected an error");
        };
        assert!(err_details.to_string().contains("could not find channel"),);

        let toolchain_contents = r#"
            [toolchain]
            channel = 32390293
        "#;
        std::fs::write(&toolchain_file_path, toolchain_contents)?;

        let channel = extract_toolchain_channel(&toolchain_file_path);
        let Err(err_details) = channel else {
            panic!("expected an error");
        };
        assert!(err_details.to_string().contains("is not a string"),);

        let toolchain_contents = r#"
            [toolchain]
            channel = "nightly-2020-07-10"
            components = [ "rustfmt", "rustc-dev" ]
            targets = [ "wasm32-unknown-unknown", "thumbv2-none-eabi" ]
            profile = "minimal"
        "#;
        std::fs::write(&toolchain_file_path, toolchain_contents)?;

        let channel = extract_toolchain_channel(&toolchain_file_path)?;
        assert_eq!(channel, "nightly-2020-07-10");
        Ok(())
    }

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
