// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

//! This crate provides an example cargo stylus extension named `cargo-stylus-example`.
//! Normally, the command `cargo stylus example` would return an error saying it's unknown.
//! Installing this crate will cause `cargo stylus` to run this subcommand.
//!
//! ```sh
//!     cargo install --path example          # use this when developing locally.
//!     cargo install cargo-stylus-example    # use this once you've published the crate.
//!     cargo stylus example --help           # see that execution passes here.
//! ```

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "example")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Your name / company")]
#[command(about = "Short description of custom command.", long_about = None)]
#[command(propagate_version = true)]
#[command(version)]
struct Opts {
    #[command(subcommand)]
    command: Apis,
}

#[derive(Parser, Debug, Clone)]
enum Apis {
    /// Short description of custom command.
    #[command()]
    Example {
        /// Description of this arg.
        /// The missing `#[arg()]` annotation means its unnamed.
        my_file: PathBuf,

        /// Description of this arg.
        /// The `#[arg(long)]` means you have to pass `--my-flag`.
        #[arg(long)]
        my_flag: bool,
    },
}

fn main() {
    let args = Opts::parse();
    let Apis::Example { my_file, my_flag } = args.command;

    // do something with your args
    println!("example: {} {}", my_file.to_string_lossy(), my_flag);
}
