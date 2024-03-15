// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use alloy_primitives::TxHash;
use clap::{Args, Parser, ValueEnum};
use ethers::types::H160;
use eyre::{eyre, Context, Result};
use std::path::PathBuf;

pub mod c;
pub mod check;
pub mod color;
pub mod constants;
pub mod deploy;
pub mod export_abi;
pub mod new;
pub mod project;
pub mod replay;
pub mod tx;
pub mod script;
pub mod util;
pub mod wallet;

#[derive(Parser, Debug)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
pub enum CargoCli {
    Stylus(StylusArgs),
    CGen(CGenArgs), // not behind the stylus command, to hide it from rust-developers.
}

#[derive(Parser, Debug)]
#[command(name = "c_generate")]
pub struct CGenArgs {
    #[arg(required = true)]
    pub input: String,
    pub out_dir: String,
}

#[derive(Parser, Debug)]
#[command(name = "stylus")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(version = "0.0.1")]
#[command(about = "Cargo command for developing Arbitrum Stylus projects", long_about = None)]
#[command(propagate_version = true)]
pub struct StylusArgs {
    #[command(subcommand)]
    pub command: Subcommands,
}

#[derive(Parser, Debug, Clone)]
pub enum Subcommands {
    /// Create a new Rust project.
    New {
        /// Project name.
        #[arg(required = true)]
        name: String,
        /// Create a minimal program.
        #[arg(long)]
        minimal: bool,
    },
    /// Export a Solidity ABI.
    ExportAbi {
        /// Build in release mode.
        #[arg(long)]
        release: bool,
        /// The Output file (defaults to stdout).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Output a JSON ABI instead using solc. Requires solc.
        /// See https://docs.soliditylang.org/en/latest/installing-solidity.html
        #[arg(long)]
        json: bool,
    },
    /// Check that a contract can be activated onchain.
    #[command(alias = "c")]
    Check(CheckConfig),
    /// Deploy a stylus contract.
    #[command(alias = "d")]
    Deploy(DeployConfig),
    /// Replay a transaction in gdb.
    #[command(alias = "r")]
    Replay(ReplayConfig),
    /// Trace a transaction.
    #[command(alias = "t")]
    Trace(TraceConfig),
    /// Work with scripts.
    #[command(subcommand, alias = "s")]
    Script(ScriptArgs),
}

#[derive(Args, Clone, Debug)]
pub struct CheckConfig {
    /// RPC endpoint of the Stylus node to connect to.
    #[arg(short, long, default_value = "https://stylus-testnet.arbitrum.io/rpc")]
    pub endpoint: String,
    /// Specifies a WASM file instead of looking for one in the current directory.
    #[arg(long)]
    pub wasm_file_path: Option<String>,
    /// Specify the program address we want to check activation for. If unspecified, it will
    /// compute the next program address from the user's wallet address and nonce, which will require
    /// wallet-related flags to be specified.
    #[arg(long, default_value = "0x0000000000000000000000000000000000000000")]
    pub expected_program_address: H160,
    /// File path to a text file containing a private key.
    #[arg(long)]
    pub private_key_path: Option<String>,
    /// Private key 0x-prefixed hex string to use with the cargo stylus plugin. Warning: this exposes
    /// your private key secret in plaintext in your CLI history. We instead recommend using the
    /// --private-key-path flag or account keystore options.
    #[arg(long)]
    pub private_key: Option<String>,
    /// Wallet source to use with the cargo stylus plugin.
    #[command(flatten)]
    pub keystore_opts: KeystoreOpts,
    /// Whether to use Rust nightly.
    #[arg(long)]
    pub nightly: bool,
    /// Whether to skip the contract size check. In case the contract size is exceeded, a warning
    /// will be emitted, but the overall cargo stylus check command will not fail.
    #[arg(long)]
    pub skip_contract_size_check: bool,
}

#[derive(Args, Clone, Debug)]
pub struct DeployConfig {
    #[command(flatten)]
    pub check_cfg: CheckConfig,
    /// Estimates deployment gas costs.
    #[arg(long)]
    pub estimate_gas_only: bool,
    /// By default, submits two transactions to deploy and activate the program to Arbitrum.
    /// Otherwise, a user could choose to split up the deploy and activate steps into individual transactions.
    #[arg(long, value_enum)]
    pub mode: Option<DeployMode>,
    /// If only activating an already-deployed, onchain program, the address of the program to send an activation tx for.
    #[arg(long)]
    pub activate_program_address: Option<H160>,
    /// Configuration options for sending the deployment / activation txs through the Cargo stylus deploy command.
    #[command(flatten)]
    pub tx_sending_opts: TxSendingOpts,
}

#[derive(Args, Clone, Debug)]
pub struct ReplayConfig {
    /// RPC endpoint.
    #[arg(short, long, default_value = "http://localhost:8545")]
    pub endpoint: String,
    /// Tx to replay.
    #[arg(short, long)]
    pub tx: TxHash,
    /// Project path.
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,
    /// Whether to use stable Rust. Note that nightly is needed to expand macros.
    #[arg(short, long)]
    pub stable_rust: bool,
    /// Whether this process is the child of another.
    #[arg(short, long, hide(true))]
    pub child: bool,
}

#[derive(Args, Clone, Debug)]
pub struct TraceConfig {
    /// RPC endpoint.
    #[arg(short, long, default_value = "http://localhost:8545")]
    pub endpoint: String,
    /// Tx to replay.
    #[arg(short, long)]
    pub tx: TxHash,
    /// Project path.
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,
}

#[derive(Parser, Clone, Debug)]
pub enum ScriptArgs {
    /// Create a new script
    #[command(alias = "a")]
    New(ScriptNewConfig),
}

#[derive(Args, Clone, Debug)]
pub struct ScriptNewConfig {
    // language: Language,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum DeployMode {
    DeployOnly,
    ActivateOnly,
}

#[derive(Clone, Debug, Args)]
#[group(multiple = true)]
pub struct KeystoreOpts {
    /// Path to an Ethereum wallet keystore file, such as the one produced by wallets such as clef.
    #[arg(long)]
    pub keystore_path: Option<String>,
    /// Path to a text file containing a password to the specified wallet keystore file.
    #[arg(long)]
    pub keystore_password_path: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct TxSendingOpts {
    /// Prepares transactions to send onchain for deploying and activating a Stylus program,
    /// but does not send them. Instead, outputs the prepared tx data hex bytes to files in the directory
    /// specified by the --output-tx-data-to-dir flag. Useful for sending the deployment / activation
    /// txs via a user's preferred means instead of via the Cargo stylus tool. For example, Foundry's
    /// https://book.getfoundry.sh/cast/ CLI tool.
    #[arg(long)]
    pub dry_run: bool,
    /// Outputs the deployment / activation tx data as bytes to a specified directory.
    #[arg(long)]
    pub output_tx_data_to_dir: Option<PathBuf>,
}

pub async fn main_impl(args: StylusArgs) -> Result<()> {
    macro_rules! run {
        ($expr:expr, $($msg:expr),+) => {
            $expr.wrap_err_with(|| eyre!($($msg),+))?
        };
    }

    match args.command {
        Subcommands::New { name, minimal } => {
            run!(
                new::new_stylus_project(&name, minimal),
                "failed to create project"
            );
        }
        Subcommands::ExportAbi {
            release,
            json,
            output,
        } => match json {
            true => run!(
                export_abi::export_json_abi(release, output),
                "failed to export json"
            ),
            false => run!(
                export_abi::export_solidity_abi(release, output),
                "failed to export abi"
            ),
        },
        Subcommands::Check(config) => {
            run!(check::run_checks(config).await, "stylus checks failed");
        }
        Subcommands::Deploy(config) => {
            run!(deploy::deploy(config).await, "failed to deploy");
        }
        Subcommands::Replay(config) => {
            run!(replay::replay(config).await, "failed to replay tx");
        }
        Subcommands::Trace(config) => {
            run!(replay::trace(config).await, "failed to trace");
        }
        Subcommands::Script(script_args) => match script_args {
            ScriptArgs::New(config) => {
                run!(script::new(config).await, "failed to add script")
            }
        },
    }
    Ok(())
}

