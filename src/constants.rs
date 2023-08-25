// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use bytesize::ByteSize;

/// EOF prefix used in Stylus compressed WASMs on-chain
pub const EOF_PREFIX: &str = "EFF000";
/// Maximum brotli compression level used for Stylus programs.
pub const BROTLI_COMPRESSION_LEVEL: u32 = 11;
/// Address of the Arbitrum WASM precompile on L2.
pub const ARB_WASM_ADDRESS: &str = "0000000000000000000000000000000000000071";
/// Maximum allowed size of a program on Arbitrum (and Ethereum).
pub const MAX_PROGRAM_SIZE: ByteSize = ByteSize::kb(24);
/// 4 bytes method selector for the activate method of ArbWasm.
pub const ARBWASM_ACTIVATE_METHOD_HASH: &str = "58c780c2";
/// Target for compiled WASM folder in a Rust project
pub const RUST_TARGET: &str = "wasm32-unknown-unknown";
pub const GITHUB_TEMPLATE_REPOSITORY: &str = "https://github.com/OffchainLabs/stylus-hello-world";
