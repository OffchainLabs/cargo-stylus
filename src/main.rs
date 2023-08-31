use std::path::PathBuf;

// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use clap::{Args, Parser, ValueEnum};
use color::Color;
use ethers::types::H160;

mod check;
mod color;
mod constants;
mod deploy;
mod export_abi;
mod new;
mod project;
mod tx;
mod wallet;

#[derive(Parser, Debug)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
enum CargoCli {
    Stylus(StylusArgs),
}

#[derive(Parser, Debug)]
#[command(name = "stylus")]
#[command(bin_name = "cargo stylus")]
#[command(author = "Offchain Labs, Inc.")]
#[command(version = "0.0.1")]
#[command(about = "Cargo command for developing Arbitrum Stylus projects", long_about = None)]
#[command(propagate_version = true)]
struct StylusArgs {
    #[command(subcommand)]
    command: StylusSubcommands,
}

#[derive(Parser, Debug, Clone)]
enum StylusSubcommands {
    /// Initialize a Stylus Rust project using the https://github.com/OffchainLabs/stylus-hello-world template.
    New {
        /// Name of the Stylus project.
        #[arg(required = true)]
        name: String,
        /// Initializes a minimal version of a Stylus program, with just a barebones entrypoint and the Stylus SDK.
        #[arg(long)]
        minimal: bool,
    },
    /// Export the Solidity ABI for a Stylus project directly using the cargo stylus tool.
    ExportAbi {
        /// Build in release mode.
        #[arg(long)]
        release: bool,
    },
    /// Instrument a Rust project using Stylus.
    /// This command runs compiled WASM code through Stylus instrumentation checks and reports any failures.
    #[command(alias = "c")]
    Check(CheckConfig),
    /// Instruments a Rust project using Stylus and by outputting its brotli-compressed WASM code.
    /// Then, it submits two transactions: the first deploys the WASM
    /// program to an address and the second triggers an activation onchain
    /// Developers can choose to split up the deploy and activate steps via this command as desired.
    #[command(alias = "d")]
    Deploy(DeployConfig),
}

#[derive(Debug, Args, Clone)]
pub struct CheckConfig {
    /// The endpoint of the L2 node to connect to. See https://docs.arbitrum.io/stylus/reference/testnet-information
    /// for latest Stylus testnet information including public endpoints. Defaults
    /// to the current Stylus testnet RPC endpoint.
    #[arg(short, long, default_value = "https://stylus-testnet.arbitrum.io/rpc")]
    endpoint: String,
    /// If desired, it loads a WASM file from a specified path. If not provided, it will try to find
    /// a WASM file under the current working directory's Rust target release directory and use its
    /// contents for the deploy command.
    #[arg(long)]
    wasm_file_path: Option<String>,
    /// Specify the program address we want to check activation for. If unspecified, it will
    /// compute the next program address from the user's wallet address and nonce, which will require
    /// wallet-related flags to be specified.
    #[arg(long, default_value = "0x0000000000000000000000000000000000000000")]
    expected_program_address: H160,
    /// Privkey source to use with the cargo stylus plugin.
    #[arg(long)]
    private_key_path: Option<String>,
    /// Wallet source to use with the cargo stylus plugin.
    #[command(flatten)]
    keystore_opts: KeystoreOpts,
    /// Whether or not to compile the Rust program using the nightly Rust version. Nightly can help
    /// with reducing compressed WASM sizes, however, can be a security risk if used liberally.
    #[arg(long)]
    nightly: bool,
}

#[derive(Debug, Args, Clone)]
pub struct DeployConfig {
    #[command(flatten)]
    check_cfg: CheckConfig,
    /// Does not submit a transaction, but instead estimates the gas required
    /// to complete the operation.
    #[arg(long)]
    estimate_gas_only: bool,
    /// By default, submits two transactions to deploy and activate the program to Arbitrum.
    /// Otherwise, a user could choose to split up the deploy and activate steps into individual transactions.
    #[arg(long, value_enum)]
    mode: Option<DeployMode>,
    /// If only activating an already-deployed, onchain program, the address of the program to send an activation tx for.
    #[arg(long)]
    activate_program_address: Option<H160>,
    /// Configuration options for sending the deployment / activation txs through the Cargo stylus deploy command.
    #[command(flatten)]
    tx_sending_opts: TxSendingOpts,
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
    keystore_path: Option<String>,
    /// Path to a text file containing a password to the specified wallet keystore file.
    #[arg(long)]
    keystore_password_path: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct TxSendingOpts {
    /// Prepares transactions to send onchain for deploying and activating a Stylus program,
    /// but does not send them. Instead, outputs the prepared tx data hex bytes to files in the directory
    /// specified by the --output-tx-data-to-dir flag. Useful for sending the deployment / activation
    /// txs via a user's preferred means instead of via the Cargo stylus tool. For example, Foundry's
    /// https://book.getfoundry.sh/cast/ CLI tool.
    #[arg(long)]
    dry_run: bool,
    /// Outputs the deployment / activation tx data as bytes to a specified directory.
    #[arg(long)]
    output_tx_data_to_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let CargoCli::Stylus(args) = CargoCli::parse();

    match args.command {
        StylusSubcommands::New { name, minimal } => {
            if let Err(e) = new::new_stylus_project(&name, minimal) {
                println!(
                    "Could not create new stylus project with name {name}: {}",
                    e.red()
                );
            };
        }
        StylusSubcommands::ExportAbi { release } => {
            if let Err(e) = export_abi::export_abi(release) {
                println!("Could not export Stylus program Solidity ABI: {}", e.red());
            };
        }
        StylusSubcommands::Check(cfg) => {
            if let Err(e) = check::run_checks(cfg).await {
                println!("Stylus checks failed: {}", e.red());
            };
        }
        StylusSubcommands::Deploy(cfg) => {
            if let Err(e) = deploy::deploy(cfg).await {
                println!("Deploy / activation command failed: {}", e.red());
            };
        }
    }
    Ok(())
}
