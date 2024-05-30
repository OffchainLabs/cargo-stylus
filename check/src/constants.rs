// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use alloy_primitives::{address, Address};
use ethers::types::{H160, U256};
use lazy_static::lazy_static;

/// EOF prefix used in Stylus compressed WASMs on-chain
pub const EOF_PREFIX_NO_DICT: &str = "EFF00000";

/// Maximum brotli compression level used for Stylus programs.
pub const BROTLI_COMPRESSION_LEVEL: u32 = 11;

lazy_static! {
    /// Address of the ArbWasm precompile.
    pub static ref ARB_WASM_H160: H160 = H160(*ARB_WASM_ADDRESS.0);
}

/// Address of the ArbWasm precompile.
pub const ARB_WASM_ADDRESS: Address = address!("0000000000000000000000000000000000000071");

/// Target for compiled WASM folder in a Rust project
pub const RUST_TARGET: &str = "wasm32-unknown-unknown";

/// The default repo to clone when creating new projects
pub const GITHUB_TEMPLATE_REPO: &str = "https://github.com/OffchainLabs/stylus-hello-world";

/// The minimal entrypoint repo
pub const GITHUB_TEMPLATE_REPO_MINIMAL: &str =
    "https://github.com/OffchainLabs/stylus-hello-world-minimal";

/// One ether in wei.
pub const ONE_ETH: U256 = U256([1000000000000000000, 0, 0, 0]);
