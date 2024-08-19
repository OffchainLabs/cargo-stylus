// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use alloy_primitives::TxHash;
use clap::{ArgGroup, Args, CommandFactory, Parser, Subcommand};
use constants::DEFAULT_ENDPOINT;
use ethers::types::H160;
use eyre::{bail, eyre, Context, Result};
use std::path::PathBuf;
use std::{fmt, path::Path};
use tokio::runtime::Builder;
use trace::Trace;
use util::{color::Color, sys};

// Conditional import for Unix-specific `CommandExt`
#[cfg(unix)]
use std::{env, os::unix::process::CommandExt};

// Conditional import for Windows
#[cfg(windows)]
use std::env;

mod activate;
mod cache;
mod check;
mod constants;
mod deploy;
mod docker;
mod export_abi;
mod gen;
mod hostio;
mod macros;
mod new;
mod project;
mod trace;
mod util;
mod verify;
mod wallet;

#[derive(Parser, Debug)]
#[command(name = "stylus")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(about = "Cargo subcommand for developing Stylus projects", long_about = None)]
#[command(propagate_version = true)]
#[command(version)]
struct Opts {
    #[command(subcommand)]
    command: Apis,
}

#[derive(Parser, Debug, Clone)]
enum Apis {
    /// Create a new Stylus project.
    New {
        /// Project name.
        name: PathBuf,
        /// Create a minimal contract.
        #[arg(long)]
        minimal: bool,
    },
    /// Initializes a Stylus project in the current directory.
    Init {
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
    #[command(visible_alias = "a")]
    Activate(ActivateConfig),
    #[command(subcommand)]
    /// Cache a contract using the Stylus CacheManager for Arbitrum chains.
    Cache(Cache),
    /// Check a contract.
    #[command(visible_alias = "c")]
    Check(CheckConfig),
    /// Deploy a contract.
    #[command(visible_alias = "d")]
    Deploy(DeployConfig),
    /// Verify the deployment of a Stylus contract.
    #[command(visible_alias = "v")]
    Verify(VerifyConfig),
    /// Generate c code bindings for a Stylus contract.
    Cgen { input: PathBuf, out_dir: PathBuf },
    /// Replay a transaction in gdb.
    #[command(visible_alias = "r")]
    Replay(ReplayArgs),
    /// Trace a transaction.
    #[command(visible_alias = "t")]
    Trace(TraceArgs),
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
    max_fee_per_gas_gwei: Option<u128>,
}

#[derive(Subcommand, Clone, Debug)]
enum Cache {
    /// Places a bid on a Stylus contract to cache it in the Arbitrum chain's wasm cache manager.
    #[command(visible_alias = "b")]
    Bid(CacheBidConfig),
    /// Checks the status of a Stylus contract in the Arbitrum chain's wasm cache manager.
    #[command(visible_alias = "s")]
    Status(CacheStatusConfig),
    /// Checks the status of a Stylus contract in the Arbitrum chain's wasm cache manager.
    #[command()]
    SuggestBid(CacheSuggestionsConfig),
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
    max_fee_per_gas_gwei: Option<u128>,
}

#[derive(Args, Clone, Debug)]
pub struct CacheStatusConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
    /// Stylus contract address to check status in the cache manager.
    #[arg(long)]
    address: Option<H160>,
}

#[derive(Args, Clone, Debug)]
pub struct CacheSuggestionsConfig {
    /// Arbitrum RPC endpoint.
    #[arg(short, long, default_value = DEFAULT_ENDPOINT)]
    endpoint: String,
    /// Stylus contract address to suggest a minimum bid for in the cache manager.
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
    /// Whether or not to just estimate gas without sending a tx.
    #[arg(long)]
    estimate_gas: bool,
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
    /// If specified, will not run the command in a reproducible docker container. Useful for local
    /// builds, but at the risk of not having a reproducible contract for verification purposes.
    #[arg(long)]
    no_verify: bool,
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
            "{} {} {}",
            self.common_cfg,
            match &self.wasm_file {
                Some(path) => format!("--wasm-file={}", path.display()),
                None => "".to_string(),
            },
            match &self.contract_address {
                Some(addr) => format!("--contract-address={:?}", addr),
                None => "".to_string(),
            },
        )
    }
}

impl fmt::Display for DeployConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.check_config,
            self.auth,
            match self.estimate_gas {
                true => "--estimate-gas".to_string(),
                false => "".to_string(),
            },
            match self.no_verify {
                true => "--no-verify".to_string(),
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

    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = Opts::parse_from(args);
    // use the current thread for replay.
    let mut runtime = match opts.command {
        Apis::Replay(_) => Builder::new_current_thread(),
        _ => Builder::new_multi_thread(),
    };
    let runtime = runtime.enable_all().build()?;
    runtime.block_on(main_impl(opts))
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
        Apis::Init { minimal } => {
            run!(new::init(minimal), "failed to initialize project");
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
        Apis::Cgen { input, out_dir } => {
            run!(gen::c_gen(&input, &out_dir), "failed to generate c code");
        }
        Apis::Trace(args) => run!(trace(args).await, "failed to trace tx"),
        Apis::Replay(args) => run!(replay(args).await, "failed to replay tx"),
        Apis::Cache(subcommand) => match subcommand {
            Cache::Bid(config) => {
                run!(
                    cache::place_bid(&config).await,
                    "stylus cache place bid failed"
                );
            }
            Cache::SuggestBid(config) => {
                run!(
                    cache::suggest_bid(&config).await,
                    "stylus cache suggest-bid failed"
                );
            }
            Cache::Status(config) => {
                run!(
                    cache::check_status(&config).await,
                    "stylus cache status failed"
                );
            }
        },
        Apis::Check(config) => {
            run!(check::check(&config).await, "stylus checks failed");
        }
        Apis::Deploy(config) => {
            if config.no_verify {
                run!(deploy::deploy(config).await, "stylus deploy failed");
            } else {
                println!(
                    "Running in a Docker container for reproducibility, this may take a while",
                );
                println!("NOTE: You can opt out by doing --no-verify");
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
                println!(
                    "Running in a Docker container for reproducibility, this may take a while",
                );
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

    build_so(&args.project)?;
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

pub fn build_so(path: &Path) -> Result<()> {
    let mut cargo = sys::new_command("cargo");

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
