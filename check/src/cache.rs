// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use alloy_primitives::Address;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
use cargo_stylus_util::color::{Color, DebugColor};
use cargo_stylus_util::sys;
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::signers::Signer;
use ethers::types::spoof::State;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Eip1559TransactionRequest, H160, U256};
use eyre::{bail, Context, Result};

use crate::check::{eth_call, EthCallError};
use crate::constants::ARB_WASM_CACHE_H160;
use crate::deploy::{format_gas, run_tx};
use crate::macros::greyln;
use crate::CacheConfig;

sol! {
    interface ArbWasmCache {
        function allCacheManagers() external view returns (address[] memory managers);
    }
    interface CacheManager {
        function placeBid(address program) external payable;

        error AsmTooLarge(uint256 asm, uint256 queueSize, uint256 cacheSize);
        error AlreadyCached(bytes32 codehash);
        error BidTooSmall(uint192 bid, uint192 min);
        error BidsArePaused();
        error ProgramNotActivated();
    }
}

pub async fn cache_contract(cfg: &CacheConfig) -> Result<()> {
    let provider = sys::new_provider(&cfg.common_cfg.endpoint)?;
    let chain_id = provider
        .get_chainid()
        .await
        .wrap_err("failed to get chain id")?;

    let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    let client = SignerMiddleware::new(provider.clone(), wallet);

    let data = ArbWasmCache::allCacheManagersCall {}.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .to(*ARB_WASM_CACHE_H160)
        .data(data);
    let tx = TypedTransaction::Eip1559(tx);
    let result = client.call(&tx, None).await?;
    let cache_managers_result =
        ArbWasmCache::allCacheManagersCall::abi_decode_returns(&result, true)?;
    let cache_manager_addrs = cache_managers_result.managers;
    if cache_manager_addrs.is_empty() {
        bail!("no cache managers found in ArbWasmCache, perhaps the Stylus cache is not yet enabled on this chain");
    }
    let cache_manager = *cache_manager_addrs.last().unwrap();
    let cache_manager = H160::from_slice(cache_manager.as_slice());

    let contract: Address = cfg.address.to_fixed_bytes().into();
    let data = CacheManager::placeBidCall { program: contract }.abi_encode();
    let mut tx = Eip1559TransactionRequest::new()
        .to(cache_manager)
        .data(data);

    // If a bid is set, specify it. Otherwise, a zero bid will be sent.
    if let Some(bid) = cfg.bid {
        tx = tx.value(U256::from(bid));
        greyln!("Setting bid value of {} wei", bid.debug_mint());
    }

    if let Err(EthCallError { data, msg }) =
        eth_call(tx.clone(), State::default(), &provider).await?
    {
        let error = match CacheManager::CacheManagerErrors::abi_decode(&data, true) {
            Ok(err) => err,
            Err(err_details) => bail!("unknown CacheManager error: {msg} and {:?}", err_details),
        };
        use CacheManager::CacheManagerErrors as C;
        match error {
            C::AsmTooLarge(_) => bail!("Stylus contract was too large to cache"),
            C::AlreadyCached(_) => bail!("Stylus contract is already cached"),
            C::BidsArePaused(_) => {
                bail!("Bidding is currently paused for the Stylus cache manager")
            }
            C::BidTooSmall(_) => {
                bail!("Bid amount {} (wei) too small", cfg.bid.unwrap_or_default())
            }
            C::ProgramNotActivated(_) => {
                bail!("Your Stylus contract {} is not yet activated. To activate it, use the `cargo stylus activate` subcommand", hex::encode(contract))
            }
        }
    }
    let verbose = cfg.common_cfg.verbose;
    let receipt = run_tx(
        "cache",
        tx,
        None,
        cfg.common_cfg.max_fee_per_gas_gwei,
        &client,
        verbose,
    )
    .await?;

    let address = cfg.address.debug_lavender();

    if verbose {
        let gas = format_gas(receipt.gas_used.unwrap_or_default());
        greyln!(
            "Successfully cached contract at address: {address} {} {gas}",
            "with".grey()
        );
    } else {
        greyln!("Successfully cached contract at address: {address}");
    }
    let tx_hash = receipt.transaction_hash.debug_lavender();
    greyln!("Sent Stylus cache tx with hash: {tx_hash}");
    Ok(())
}
