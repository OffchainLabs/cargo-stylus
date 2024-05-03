// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use cargo_stylus_util::{color::Color, sys};
use clap::Parser;
use eyre::{bail, Result};
use std::{env, os::unix::process::CommandExt};

#[derive(Parser, Debug)]
#[command(name = "stylus")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(about = "Cargo subcommand for developing Stylus projects", long_about = None)]
#[command(propagate_version = true)]
#[command(version)]
struct Opts {
    #[command(subcommand)]
    command: Subcommands,
}

#[derive(Parser, Debug, Clone)]
enum Subcommands {
    #[command(alias = "n")]
    /// Create a new Rust project.
    New,
    #[command(alias = "x")]
    /// Export a Solidity ABI.
    ExportAbi,
    /// Check a contract.
    #[command(alias = "c")]
    Check,
    /// Deploy a contract.
    #[command(alias = "d")]
    Deploy,
    /// Replay a transaction in gdb.
    #[command(alias = "r")]
    Replay,
    /// Trace a transaction.
    #[command()]
    Trace,
    /// Generate C code.
    #[command()]
    CGen,
}

struct Binary<'a> {
    name: &'a str,
    apis: &'a [&'a str],
    rust_flags: Option<&'a str>,
}

const COMMANDS: &[Binary] = &[
    Binary {
        name: "cargo-stylus-check",
        apis: &["new", "export-abi", "check", "deploy", "n", "x", "c", "d"],
        rust_flags: None,
    },
    Binary {
        name: "cargo-stylus-cgen",
        apis: &["cgen"],
        rust_flags: None,
    },
    Binary {
        name: "cargo-stylus-replay",
        apis: &["trace", "replay", "r"],
        rust_flags: None,
    },
    Binary {
        name: "cargo-stylus-test",
        apis: &["test", "t"],
        rust_flags: Some(r#"RUSTFLAGS="-C link-args=-rdynamic""#),
    },
];

fn exit_with_help_msg() -> ! {
    Opts::parse_from(["--help"]);
    unreachable!()
}

fn main() -> Result<()> {
    // skip the starting arguments passed from the OS and/or cargo.
    let mut args =
        env::args().skip_while(|x| x == "cargo" || x == "stylus" || x.contains("cargo-stylus"));

    let Some(arg) = args.next() else {
        exit_with_help_msg();
    };

    if arg == "--help" {
        exit_with_help_msg();
    }

    let Some(bin) = COMMANDS.iter().find(|x| x.apis.contains(&arg.as_str())) else {
        // see if custom extension exists
        let custom = format!("cargo-stylus-{arg}");
        if sys::command_exists(&custom) {
            let err = sys::new_command(&custom).arg(arg).args(args).exec();
            bail!("failed to invoke {}: {err}", custom.red());
        }

        eprintln!("Unknown subcommand {}.", arg.red());
        eprintln!();
        exit_with_help_msg();
    };

    let name = bin.name;

    // not all subcommands are shipped with `cargo-stylus`.
    if !sys::command_exists(name) {
        let flags = bin.rust_flags.map(|x| format!("{x} ")).unwrap_or_default();
        let install = format!("    {flags}cargo install --force {name}");

        eprintln!("{} {}{}", "missing".grey(), name.red(), ".".grey());
        eprintln!();
        eprintln!("{}", "to install it, run".grey());
        eprintln!("{}", install.yellow());
        return Ok(());
    }

    // should never return
    let err = sys::new_command(name).arg(arg).args(args).exec();
    bail!("failed to invoke {}: {err}", name.red());
}
