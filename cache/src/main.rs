// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use alloy_primitives::FixedBytes;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
use cargo_stylus_util::sys;
use clap::{Args, Parser};
use ethers::middleware::Middleware;
use ethers::providers::{Http, Provider, ProviderError, RawCall};
use ethers::types::spoof::State;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Address, Eip1559TransactionRequest, U256};
use eyre::{bail, eyre, Context, ErrReport, Result};
use serde_json::Value;
use CacheManager::CacheManagerErrors;

#[derive(Parser, Clone, Debug)]
#[command(name = "cargo-stylus-cache")]
#[command(bin_name = "cargo stylus cache")]
#[command(author = "Offchain Labs, Inc.")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Cargo command for interacting with the Arbitrum Stylus cache manager", long_about = None)]
#[command(propagate_version = true)]
pub struct Opts {
    #[command(subcommand)]
    command: Subcommands,
}

#[derive(Parser, Debug, Clone)]
enum Subcommands {
    #[command(alias = "p")]
    Program(CacheArgs),
}

#[derive(Args, Clone, Debug)]
struct CacheArgs {
    /// RPC endpoint.
    #[arg(short, long, default_value = "https://sepolia-rollup.arbitrum.io/rpc")]
    endpoint: String,
    /// Address of the Stylus program to cache.
    #[arg(short, long)]
    address: Address,
    /// Bid, in wei, to place on the program cache.
    #[arg(short, long, hide(true))]
    bid: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Opts::parse();
    macro_rules! run {
        ($expr:expr, $($msg:expr),+) => {
            $expr.await.wrap_err_with(|| eyre!($($msg),+))
        };
    }
    match args.command {
        Subcommands::Program(args) => run!(
            self::cache_program(args),
            "failed to submit program cache request"
        ),
    }
}

sol! {
    interface CacheManager {
        function placeBid(bytes32 codehash) external payable;

        error NotChainOwner(address sender);
        error AsmTooLarge(uint256 asm, uint256 queueSize, uint256 cacheSize);
        error AlreadyCached(bytes32 codehash);
        error BidTooSmall(uint192 bid, uint192 min);
        error BidsArePaused();
        error MakeSpaceTooLarge(uint64 size, uint64 limit);
    }
}

async fn cache_program(args: CacheArgs) -> Result<()> {
    let provider: Provider<Http> = sys::new_provider(&args.endpoint)?;
    let chain_id = provider.get_chainid().await?;
    println!("Connected to chain {}", chain_id);

    let program_code = provider
        .get_code(args.address, None)
        .await
        .wrap_err("failed to fetch program code")?;
    let codehash = ethers::utils::keccak256(&program_code);
    let codehash = FixedBytes::<32>::from(codehash);

    let data = CacheManager::placeBidCall { codehash }.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .data(data)
        .value(U256::from(args.bid));
    if let Err(EthCallError { data, msg }) =
        eth_call(tx.clone(), State::default(), &provider).await?
    {
        let Ok(error) = CacheManagerErrors::abi_decode(&data, true) else {
            bail!("unknown CacheManager error: {msg}");
        };
        use CacheManagerErrors as C;
        match error {
            C::AsmTooLarge(_) => bail!("program too large"),
            _ => bail!("unexpected CacheManager error: {msg}"),
        }
    }

    // Otherwise, we are ready to send the tx data if our call passed.
    // TODO: Send.
    Ok(())
}

struct EthCallError {
    data: Vec<u8>,
    msg: String,
}

impl From<EthCallError> for ErrReport {
    fn from(value: EthCallError) -> Self {
        eyre!(value.msg)
    }
}

async fn eth_call(
    tx: Eip1559TransactionRequest,
    mut state: State,
    provider: &Provider<Http>,
) -> Result<Result<Vec<u8>, EthCallError>> {
    let tx = TypedTransaction::Eip1559(tx);
    state.account(Default::default()).balance = Some(ethers::types::U256::MAX); // infinite balance

    match provider.call_raw(&tx).state(&state).await {
        Ok(bytes) => Ok(Ok(bytes.to_vec())),
        Err(ProviderError::JsonRpcClientError(error)) => {
            let error = error
                .as_error_response()
                .ok_or_else(|| eyre!("json RPC failure: {error}"))?;

            let msg = error.message.clone();
            let data = match &error.data {
                Some(Value::String(data)) => cargo_stylus_util::text::decode0x(data)?.to_vec(),
                Some(value) => bail!("failed to decode RPC failure: {value}"),
                None => vec![],
            };
            Ok(Err(EthCallError { data, msg }))
        }
        Err(error) => Err(error.into()),
    }
}
