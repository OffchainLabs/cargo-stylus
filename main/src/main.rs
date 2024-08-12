// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use cargo_stylus_util::{color::Color, sys};
use clap::{CommandFactory, Parser};
use eyre::{bail, Result};

// Conditional import for Unix-specific `CommandExt`
#[cfg(unix)]
use std::{env, os::unix::process::CommandExt};

// Conditional import for Windows
#[cfg(windows)]
use std::env;

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
    /// Cache a contract.
    Cache,
    /// Check a contract.
    #[command(alias = "c")]
    Check,
    /// Activate an already deployed contract
    #[command(alias = "a")]
    Activate,
    /// Deploy a contract.
    #[command(alias = "d")]
    Deploy,
    /// Replay a transaction in gdb.
    #[command(alias = "r")]
    Replay,
    /// Trace a transaction.
    #[command()]
    Trace,
    /// Verify the deployment of a Stylus contract against a local project.
    #[command(alias = "v")]
    Verify,
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
        apis: &[
            "new",
            "export-abi",
            "cache",
            "check",
            "deploy",
            "verify",
            "a",
            "n",
            "x",
            "c",
            "d",
            "v",
        ],
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

// prints help message and exits
fn exit_with_help_msg() -> ! {
    Opts::command().print_help().unwrap();
    std::process::exit(0);
}

// prints version information and exits
fn exit_with_version() -> ! {
    println!("{}", Opts::command().render_version());
    std::process::exit(0);
}

fn main() -> Result<()> {
    // skip the starting arguments passed from the OS and/or cargo.
    let mut args =
        env::args().skip_while(|x| x == "cargo" || x == "stylus" || x.contains("cargo-stylus"));

    let Some(arg) = args.next() else {
        exit_with_help_msg();
    };

    // perform any builtins
    match arg.as_str() {
        "--help" | "-h" => exit_with_help_msg(),
        "--version" | "-V" => exit_with_version(),
        _ => {}
    };

    let Some(bin) = COMMANDS.iter().find(|x| x.apis.contains(&arg.as_str())) else {
        // see if custom extension exists
        let custom = format!("cargo-stylus-{arg}");
        if sys::command_exists(&custom) {
            let mut command = sys::new_command(&custom);
            command.arg(arg).args(args);

            // Execute command conditionally based on the platform
            #[cfg(unix)]
            let err = command.exec(); // Unix-specific execution
            #[cfg(windows)]
            let err = command.status(); // Windows-specific execution
            bail!("failed to invoke {:?}: {:?}", custom.red(), err);
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
    let mut command = sys::new_command(name);
    command.arg(arg).args(args);

    // Execute command conditionally based on the platform
    #[cfg(unix)]
    let err = command.exec(); // Unix-specific execution
    #[cfg(windows)]
    let err = command.status(); // Windows-specific execution
    bail!("failed to invoke {:?}: {:?}", name.red(), err);
}
