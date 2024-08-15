// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use clap::{ArgGroup, Args, Parser, Subcommand};
use constants::DEFAULT_ENDPOINT;
use ethers::types::{H160, U256};
use eyre::{eyre, Context, Result};
use std::fmt;
use std::path::PathBuf;
use tokio::runtime::Builder;

mod activate;
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
        /// Create a minimal contract.
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
    /// Activate an already deployed contract.
    #[command(alias = "a")]
    Activate(ActivateConfig),
    #[command(subcommand)]
    /// Cache a contract using the Stylus CacheManager for Arbitrum chains.
    Cache(Cache),
    /// Check a contract.
    #[command(alias = "c")]
    Check(CheckConfig),
    /// Deploy a contract.
    #[command(alias = "d")]
    Deploy(DeployConfig),
    /// Verify the deployment of a Stylus contract.
    #[command(alias = "v")]
    Verify(VerifyConfig),
}

#[derive(Args, Clone, Debug)]
struct CommonConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
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

#[derive(Subcommand, Clone, Debug)]
enum Cache {
    /// Places a bid on a Stylus contract to cache it in the Arbitrum chain's wasm cache manager.
    #[command(alias = "b")]
    Bid(CacheBidConfig),
    /// Checks the status of a Stylus contract in the Arbitrum chain's wasm cache manager.
    #[command(alias = "s")]
    Status(CacheStatusConfig),
}

#[derive(Args, Clone, Debug)]
pub struct CacheBidConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
    /// Whether to print debug info.
    #[arg(long)]
    verbose: bool,
    /// Wallet source to use.
    #[command(flatten)]
    auth: AuthOpts,
    /// Deployed and activated contract address to cache.
    address: H160,
    /// Bid, in wei, to place on the desired contract to cache. A value of 0 is a valid bid.
    bid: u64,
    #[arg(long)]
    /// Optional max fee per gas in gwei units.
    max_fee_per_gas_gwei: Option<U256>,
}

#[derive(Args, Clone, Debug)]
pub struct CacheStatusConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
    /// Deployed and activated contract address to cache.
    address: H160,
}

#[derive(Args, Clone, Debug)]
pub struct ActivateConfig {
    #[command(flatten)]
    common_cfg: CommonConfig,
    /// Wallet source to use.
    #[command(flatten)]
    auth: AuthOpts,
    /// Deployed Stylus contract address to activate.
    #[arg(long)]
    address: H160,
    /// Percent to bump the estimated activation data fee by. Default of 20%
    #[arg(long, default_value = "20")]
    data_fee_bump_percent: u64,
}

#[derive(Args, Clone, Debug)]
pub struct CheckConfig {
    #[command(flatten)]
    common_cfg: CommonConfig,
    /// The WASM to check (defaults to any found in the current directory).
    #[arg(long)]
    wasm_file: Option<PathBuf>,
    /// Where to deploy and activate the contract (defaults to a random address).
    #[arg(long)]
    contract_address: Option<H160>,
    /// If specified, will not run the command in a reproducible docker container. Useful for local
    /// builds, but at the risk of not having a reproducible contract for verification purposes.
    #[arg(long)]
    no_verify: bool,
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
    #[arg(long)]
    /// If specified, will not run the command in a reproducible docker container. Useful for local
    /// builds, but at the risk of not having a reproducible contract for verification purposes.
    no_verify: bool,
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

impl fmt::Display for CommonConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Convert the vector of source files to a comma-separated string
        let mut source_files: String = "".to_string();
        if !self.source_files_for_project_hash.is_empty() {
            source_files = format!(
                "--source-files-for-project-hash={}",
                self.source_files_for_project_hash.join(", ")
            );
        }
        write!(
            f,
            "--endpoint={} {} {} {}",
            self.endpoint,
            match self.verbose {
                true => "--verbose",
                false => "",
            },
            source_files,
            match &self.max_fee_per_gas_gwei {
                Some(fee) => format!("--max-fee-per-gas-gwei {}", fee),
                None => "".to_string(),
            }
        )
    }
}

impl fmt::Display for CheckConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.common_cfg,
            match &self.wasm_file {
                Some(path) => format!("--wasm-file={}", path.display()),
                None => "".to_string(),
            },
            match &self.contract_address {
                Some(addr) => format!("--contract-address={:?}", addr),
                None => "".to_string(),
            },
            match self.no_verify {
                true => "--no-verify".to_string(),
                false => "".to_string(),
            },
        )
    }
}

impl fmt::Display for DeployConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            self.check_config,
            self.auth,
            match self.estimate_gas {
                true => "--estimate-gas".to_string(),
                false => "".to_string(),
            },
        )
    }
}

impl fmt::Display for AuthOpts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            match &self.private_key_path {
                Some(path) => format!("--private-key-path={}", path.display()),
                None => "".to_string(),
            },
            match &self.private_key {
                Some(key) => format!("--private-key={}", key.clone()),
                None => "".to_string(),
            },
            match &self.keystore_path {
                Some(path) => format!("--keystore-path={}", path.clone()),
                None => "".to_string(),
            },
            match &self.keystore_password_path {
                Some(path) => format!("--keystore-password-path={}", path.display()),
                None => "".to_string(),
            }
        )
    }
}

impl fmt::Display for VerifyConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} --deployment-tx={} {}",
            self.common_cfg,
            self.deployment_tx,
            match self.no_verify {
                true => "--no-verify".to_string(),
                false => "".to_string(),
            }
        )
    }
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
        Apis::Activate(config) => {
            run!(
                activate::activate_contract(&config).await,
                "stylus activate failed"
            );
        }
        Apis::Cache(subcommand) => match subcommand {
            Cache::Bid(config) => {
                run!(
                    cache::place_bid(&config).await,
                    "stylus cache place bid failed"
                );
            }
            Cache::Status(config) => {
                // run!(cache::cache_contract(&config).await, "stylus cache failed");
                todo!();
            }
        },
        Apis::Check(config) => {
            if config.no_verify {
                run!(check::check(&config).await, "stylus checks failed");
            } else {
                let mut commands: Vec<String> =
                    vec![String::from("check"), String::from("--no-verify")];
                let config_args = config
                    .to_string()
                    .split(' ')
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>();
                commands.extend(config_args);
                run!(
                    docker::run_reproducible(&commands),
                    "failed reproducible run"
                );
            }
        }
        Apis::Deploy(config) => {
            if config.check_config.no_verify {
                run!(deploy::deploy(config).await, "stylus deploy failed");
            } else {
                let mut commands: Vec<String> =
                    vec![String::from("deploy"), String::from("--no-verify")];
                let config_args = config
                    .to_string()
                    .split(' ')
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>();
                commands.extend(config_args);
                run!(
                    docker::run_reproducible(&commands),
                    "failed reproducible run"
                );
            }
        }
        Apis::Verify(config) => {
            if config.no_verify {
                run!(verify::verify(config).await, "failed to verify");
            } else {
                let mut commands: Vec<String> =
                    vec![String::from("verify"), String::from("--no-verify")];
                let config_args = config
                    .to_string()
                    .split(' ')
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>();
                commands.extend(config_args);
                run!(
                    docker::run_reproducible(&commands),
                    "failed reproducible run"
                );
            }
        }
    }
    Ok(())
}
