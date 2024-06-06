// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use clap::{Args, Parser};
use eyre::{eyre, Context};

mod verify;

#[derive(Parser, Debug)]
#[command(name = "verify")]
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
    /// Verify a Stylus program deployment.
    #[command()]
    Verify(VerifyConfig),
}

#[derive(Args, Clone, Debug)]
pub struct VerifyConfig {
    /// Hash of the deployment transaction.
    #[arg(long)]
    deployment_tx: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Opts::parse();

    match args.command {
        Apis::Verify(config) => verify::verify(config)
            .await
            .wrap_err_with(|| eyre!("Failed to verify")),
    }
}
