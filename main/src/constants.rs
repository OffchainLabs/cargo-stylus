// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use alloy::primitives::{address, Address, U256};

/// EOF prefix used in Stylus compressed WASMs on-chain
pub const EOF_PREFIX_NO_DICT: &str = "EFF00000";

/// Maximum brotli compression level used for Stylus contracts.
pub const BROTLI_COMPRESSION_LEVEL: u32 = 11;

/// Address of the ArbWasm precompile.
pub const ARB_WASM_ADDRESS: Address = address!("0000000000000000000000000000000000000071");

/// Address of the ArbWasmCache precompile.
pub const ARB_WASM_CACHE_ADDRESS: Address = address!("0000000000000000000000000000000000000072");

/// Target for compiled WASM folder in a Rust project
pub const RUST_TARGET: &str = "wasm32-unknown-unknown";

/// The default repo to clone when creating new projects
pub const GITHUB_TEMPLATE_REPO: &str = "https://github.com/OffchainLabs/stylus-hello-world";

/// The minimal entrypoint repo
pub const GITHUB_TEMPLATE_REPO_MINIMAL: &str =
    "https://github.com/OffchainLabs/stylus-hello-world-minimal";

/// Name of the custom wasm section that is added to contracts deployed with cargo stylus
/// to include a hash of the Rust project's source files for reproducible verification of builds.
pub const PROJECT_HASH_SECTION_NAME: &str = "project_hash";

/// Name of the toolchain file used to specify the Rust toolchain version for a project.
pub const TOOLCHAIN_FILE_NAME: &str = "rust-toolchain.toml";

/// The default endpoint for connections to a Stylus-enabled Arbitrum node.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:8547";
