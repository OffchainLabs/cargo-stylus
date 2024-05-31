// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use clap::Parser;
use eyre::Result;
use std::path::PathBuf;

mod gen;

#[derive(Parser, Debug)]
#[command(name = "cgen")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(about = "Generate C code for Stylus ABI bindings.", long_about = None)]
#[command(propagate_version = true)]
#[command(version)]
struct Opts {
    #[command(subcommand)]
    command: Apis,
}

#[derive(Parser, Debug, Clone)]
enum Apis {
    /// Generate C code.
    #[command()]
    Cgen { input: PathBuf, out_dir: PathBuf },
}

fn main() -> Result<()> {
    let args = Opts::parse();
    match args.command {
        Apis::Cgen { input, out_dir } => gen::c_gen(&input, &out_dir),
    }
}
