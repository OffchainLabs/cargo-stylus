// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use clap::{ArgGroup, Args, Parser};
use ethers::types::{H160, U256};
use eyre::{eyre, Context, Result};
use std::path::PathBuf;
use tokio::runtime::Builder;

mod cache;
mod check;
mod constants;
mod deploy;
mod docker;
mod export_abi;
mod macros;
mod new;
mod project;
mod verify;
mod wallet;

#[derive(Parser, Debug)]
#[command(name = "check")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(propagate_version = true)]
#[command(version)]
struct Opts {
    #[command(subcommand)]
    command: Apis,
}

#[derive(Parser, Debug, Clone)]
enum Apis {
    /// Create a new Rust project.
    New {
        /// Project name.
        name: PathBuf,
        /// Create a minimal program.
        #[arg(long)]
        minimal: bool,
    },
    /// Export a Solidity ABI.
    ExportAbi {
        /// The output file (defaults to stdout).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Write a JSON ABI instead using solc. Requires solc.
        #[arg(long)]
        json: bool,
    },
    /// Cache a contract using the Stylus CacheManager for Arbitrum chains.
    Cache(CacheConfig),
    /// Check a contract.
    #[command(alias = "c")]
    Check(CheckConfig),
    /// Deploy a contract.
    #[command(alias = "d")]
    Deploy(DeployConfig),
    /// Build in a Docker container to ensure reproducibility.
    ///
    /// Specify the Rust version to use, followed by the cargo stylus subcommand.
    /// Example: `cargo stylus reproducible 1.77 check`
    Reproducible {
        /// Rust version to use.
        #[arg()]
        rust_version: String,

        /// Stylus subcommand.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        stylus: Vec<String>,
    },
    /// Verify the deployment of a Stylus program.
    #[command(alias = "v")]
    Verify(VerifyConfig),
}

#[derive(Args, Clone, Debug)]
struct CommonConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = "https://sepolia-rollup.arbitrum.io/rpc")]
    endpoint: String,
    /// Whether to use stable Rust.
    #[arg(long)]
    rust_stable: bool,
    /// Whether to print debug info.
    #[arg(long)]
    verbose: bool,
    /// The path to source files to include in the project hash, which
    /// is included in the contract deployment init code transaction
    /// to be used for verification of deployment integrity.
    /// If not provided, all .rs files and Cargo.toml and Cargo.lock files
    /// in project's directory tree are included.
    #[arg(long)]
    source_files_for_project_hash: Vec<String>,
    #[arg(long)]
    /// Optional max fee per gas in gwei units.
    max_fee_per_gas_gwei: Option<U256>,
}

#[derive(Args, Clone, Debug)]
pub struct CacheConfig {
    #[command(flatten)]
    common_cfg: CommonConfig,
    /// Wallet source to use.
    #[command(flatten)]
    auth: AuthOpts,
    /// Deployed and activated program address to cache.
    #[arg(long)]
    address: H160,
    /// Address of the Stylus program cache manager on Arbitrum chains.
    #[arg(long, default_value = "0c9043d042ab52cfa8d0207459260040cca54253")]
    cache_manager_address: H160,
    /// Bid, in wei, to place on the desired program to cache
    #[arg(short, long, hide(true))]
    bid: Option<u64>,
}

#[derive(Args, Clone, Debug)]
pub struct CheckConfig {
    #[command(flatten)]
    common_cfg: CommonConfig,
    /// The WASM to check (defaults to any found in the current directory).
    #[arg(long)]
    wasm_file: Option<PathBuf>,
    /// Where to deploy and activate the program (defaults to a random address).
    #[arg(long)]
    program_address: Option<H160>,
}

#[derive(Args, Clone, Debug)]
struct DeployConfig {
    #[command(flatten)]
    check_config: CheckConfig,
    /// Wallet source to use.
    #[command(flatten)]
    auth: AuthOpts,
    /// Only perform gas estimation.
    #[arg(long)]
    estimate_gas: bool,
}

#[derive(Args, Clone, Debug)]
pub struct VerifyConfig {
    #[command(flatten)]
    common_cfg: CommonConfig,

    /// Hash of the deployment transaction.
    #[arg(long)]
    deployment_tx: String,
}

#[derive(Clone, Debug, Args)]
#[clap(group(ArgGroup::new("key").required(true).args(&["private_key_path", "private_key", "keystore_path"])))]
struct AuthOpts {
    /// File path to a text file containing a hex-encoded private key.
    #[arg(long)]
    private_key_path: Option<PathBuf>,
    /// Private key as a hex string. Warning: this exposes your key to shell history.
    #[arg(long)]
    private_key: Option<String>,
    /// Path to an Ethereum wallet keystore file (e.g. clef).
    #[arg(long)]
    keystore_path: Option<String>,
    /// Keystore password file.
    #[arg(long)]
    keystore_password_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Opts::parse();
    let runtime = Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(main_impl(args))
}

async fn main_impl(args: Opts) -> Result<()> {
    macro_rules! run {
        ($expr:expr, $($msg:expr),+) => {
            $expr.wrap_err_with(|| eyre!($($msg),+))?
        };
    }

    match args.command {
        Apis::New { name, minimal } => {
            run!(new::new(&name, minimal), "failed to open new project");
        }
        Apis::ExportAbi { json, output } => {
            run!(export_abi::export_abi(output, json), "failed to export abi");
        }
        Apis::Cache(config) => {
            run!(cache::cache_program(&config).await, "stylus cache failed");
        }
        Apis::Check(config) => {
            run!(check::check(&config).await, "stylus checks failed");
        }
        Apis::Deploy(config) => {
            run!(deploy::deploy(config).await, "failed to deploy");
        }
        Apis::Reproducible {
            rust_version,
            stylus,
        } => {
            run!(
                docker::run_reproducible(&rust_version, &stylus),
                "failed reproducible run"
            );
        }
        Apis::Verify(config) => {
            run!(verify::verify(config).await, "failed to verify");
        }
    }
    Ok(())
}
