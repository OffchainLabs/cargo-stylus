// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use crate::trace::Trace;
use alloy_primitives::TxHash;
use cargo_stylus_util::{color::Color, sys};
use clap::{Args, Parser};
use eyre::{bail, eyre, Context, Result};
// Conditional import for Unix-specific `CommandExt`
#[cfg(unix)]
use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
};

// Conditional import for Windows
#[cfg(windows)]
use std::{
    env,
    path::{Path, PathBuf},
};
use tokio::runtime::Builder;

mod hostio;
mod trace;

#[derive(Parser, Clone, Debug)]
#[command(name = "cargo-stylus-replay")]
#[command(bin_name = "cargo stylus replay")]
#[command(author = "Offchain Labs, Inc.")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Cargo command for replaying Arbitrum Stylus transactions", long_about = None)]
#[command(propagate_version = true)]
pub struct Opts {
    #[command(subcommand)]
    command: Subcommands,
}

#[derive(Parser, Debug, Clone)]
enum Subcommands {
    /// Replay a transaction in gdb.
    #[command(alias = "r")]
    Replay(ReplayArgs),
    /// Trace a transaction.
    #[command(alias = "t")]
    Trace(TraceArgs),
}

#[derive(Args, Clone, Debug)]
struct ReplayArgs {
    /// RPC endpoint.
    #[arg(short, long, default_value = "http://localhost:8547")]
    endpoint: String,
    /// Tx to replay.
    #[arg(short, long)]
    tx: TxHash,
    /// Project path.
    #[arg(short, long, default_value = ".")]
    project: PathBuf,
    /// Whether to use stable Rust. Note that nightly is needed to expand macros.
    #[arg(short, long)]
    stable_rust: bool,
    /// Whether this process is the child of another.
    #[arg(short, long, hide(true))]
    child: bool,
}

#[derive(Args, Clone, Debug)]
struct TraceArgs {
    /// RPC endpoint.
    #[arg(short, long, default_value = "http://localhost:8547")]
    endpoint: String,
    /// Tx to replay.
    #[arg(short, long)]
    tx: TxHash,
    /// Project path.
    #[arg(short, long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Opts::parse();

    // use the current thread for replay
    let mut runtime = match args.command {
        Subcommands::Trace(_) => Builder::new_multi_thread(),
        Subcommands::Replay(_) => Builder::new_current_thread(),
    };

    let runtime = runtime.enable_all().build()?;
    runtime.block_on(main_impl(args))
}

async fn main_impl(args: Opts) -> Result<()> {
    macro_rules! run {
        ($expr:expr, $($msg:expr),+) => {
            $expr.await.wrap_err_with(|| eyre!($($msg),+))
        };
    }

    match args.command {
        Subcommands::Trace(args) => run!(self::trace(args), "failed to trace tx"),
        Subcommands::Replay(args) => run!(self::replay(args), "failed to replay tx"),
    }
}

async fn trace(args: TraceArgs) -> Result<()> {
    let provider = sys::new_provider(&args.endpoint)?;
    let trace = Trace::new(provider, args.tx).await?;
    println!("{}", trace.json);
    Ok(())
}

async fn replay(args: ReplayArgs) -> Result<()> {
    if !args.child {
        let rust_gdb = sys::command_exists("rust-gdb");
        if !rust_gdb {
            println!(
                "{} not installed, falling back to {}",
                "rust-gdb".red(),
                "gdb".red()
            );
        }

        let mut cmd = match rust_gdb {
            true => sys::new_command("rust-gdb"),
            false => sys::new_command("gdb"),
        };
        cmd.arg("--quiet");
        cmd.arg("-ex=set breakpoint pending on");
        cmd.arg("-ex=b user_entrypoint");
        cmd.arg("-ex=r");
        cmd.arg("--args");

        for arg in std::env::args() {
            cmd.arg(arg);
        }
        cmd.arg("--child");
        #[cfg(unix)]
        let err = cmd.exec();
        #[cfg(windows)]
        let err = cmd.status();

        bail!("failed to exec gdb {:?}", err);
    }

    let provider = sys::new_provider(&args.endpoint)?;
    let trace = Trace::new(provider, args.tx).await?;

    build_so(&args.project, args.stable_rust)?;
    let so = find_so(&args.project)?;

    // TODO: don't assume the contract is top-level
    let args_len = trace.tx.input.len();

    unsafe {
        *hostio::FRAME.lock() = Some(trace.reader());

        type Entrypoint = unsafe extern "C" fn(usize) -> usize;
        let lib = libloading::Library::new(so)?;
        let main: libloading::Symbol<Entrypoint> = lib.get(b"user_entrypoint")?;

        match main(args_len) {
            0 => println!("call completed successfully"),
            1 => println!("call reverted"),
            x => println!("call exited with unknown status code: {}", x.red()),
        }
    }
    Ok(())
}

pub fn build_so(path: &Path, stable: bool) -> Result<()> {
    let mut cargo = sys::new_command("cargo");

    if !stable {
        cargo.arg("+nightly");
    }
    cargo
        .current_dir(path)
        .arg("build")
        .arg("--lib")
        .arg("--target")
        .arg(rustc_host::from_cli()?)
        .output()?;
    Ok(())
}

pub fn find_so(project: &Path) -> Result<PathBuf> {
    let triple = rustc_host::from_cli()?;
    let so_dir = project.join(format!("target/{triple}/debug/"));
    let so_dir = std::fs::read_dir(&so_dir)
        .map_err(|e| eyre!("failed to open {}: {e}", so_dir.to_string_lossy()))?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|r| r.is_file());

    let mut file: Option<PathBuf> = None;
    for entry in so_dir {
        let Some(ext) = entry.file_name() else {
            continue;
        };
        let ext = ext.to_string_lossy();

        if ext.contains(".so") {
            if let Some(other) = file {
                let other = other.file_name().unwrap().to_string_lossy();
                bail!("more than one .so found: {ext} and {other}",);
            }
            file = Some(entry);
        }
    }
    let Some(file) = file else {
        bail!("failed to find .so");
    };
    Ok(file)
}
