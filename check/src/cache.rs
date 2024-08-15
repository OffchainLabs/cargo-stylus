// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use std::cmp::min;

use alloy_primitives::{address, keccak256, Address};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
use bytesize::ByteSize;
use cargo_stylus_util::color::{Color, DebugColor};
use cargo_stylus_util::sys;
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::signers::Signer;
use ethers::types::spoof::State;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Eip1559TransactionRequest, H160, U256};
use eyre::{bail, Context, Result};

use crate::check::{eth_call, EthCallError};
use crate::constants::{ARB_WASM_ADDRESS, ARB_WASM_CACHE_ADDRESS, ARB_WASM_CACHE_H160};
use crate::deploy::{format_gas, run_tx};
use crate::macros::greyln;
use crate::{CacheBidConfig, CacheStatusConfig, CacheSuggestionsConfig};

sol! {
    #[sol(rpc)]
    interface ArbWasmCache {
        function allCacheManagers() external view returns (address[] memory managers);
        function codehashIsCached(bytes32 codehash) external view returns (bool);
    }
    #[sol(rpc)]
    interface CacheManager {
        function cacheSize() external view returns (uint64);
        function queueSize() external view returns (uint64);
        function decay() external view returns (uint64);
        function isPaused() external view returns (bool);
        function placeBid(address program) external payable;
        function getMinBid(address program) external view returns (uint192 min);
        function getMinBid(uint64 size) public view returns (uint192 min);

        error AsmTooLarge(uint256 asm, uint256 queueSize, uint256 cacheSize);
        error AlreadyCached(bytes32 codehash);
        error BidTooSmall(uint192 bid, uint192 min);
        error BidsArePaused();
        error ProgramNotActivated();
    }
}

pub async fn suggest_bid(cfg: &CacheSuggestionsConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .on_builtin(&cfg.endpoint)
        .await?;
    let cache_manager_addr = get_cache_manager_address(provider.clone()).await?;
    let cache_manager = CacheManager::new(cache_manager_addr, provider.clone());
    let CacheManager::getMinBid_0Return { min: min_bid } = cache_manager
        .getMinBid_0(cfg.address.to_fixed_bytes().into())
        .call()
        .await?;
    greyln!(
        "Minimum bid for contract {}: {} wei",
        cfg.address,
        min_bid.debug_mint()
    );
    Ok(())
}

pub async fn check_status(cfg: &CacheStatusConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .on_builtin(&cfg.endpoint)
        .await?;
    let arb_wasm_cache = ArbWasmCache::new(ARB_WASM_CACHE_ADDRESS, provider.clone());
    let cache_manager_addr = get_cache_manager_address(provider.clone()).await?;
    let cache_manager = CacheManager::new(cache_manager_addr, provider.clone());
    let CacheManager::isPausedReturn { _0: is_paused } = cache_manager.isPaused().call().await?;
    let CacheManager::queueSizeReturn { _0: queue_size } = cache_manager.queueSize().call().await?;
    let CacheManager::cacheSizeReturn { _0: cache_size } = cache_manager.cacheSize().call().await?;
    let CacheManager::getMinBid_1Return { min: min_bid_smol } = cache_manager
        .getMinBid_1(ByteSize::kb(8).as_u64())
        .call()
        .await?;
    let CacheManager::getMinBid_1Return { min: min_bid_med } = cache_manager
        .getMinBid_1(ByteSize::kb(16).as_u64())
        .call()
        .await?;
    let CacheManager::getMinBid_1Return { min: min_bid_big } = cache_manager
        .getMinBid_1(ByteSize::kb(24).as_u64())
        .call()
        .await?;

    greyln!(
        "Cache manager address: {}",
        cache_manager_addr.debug_lavender()
    );
    greyln!(
        "Cache manager status: {}",
        if is_paused {
            "paused".debug_red()
        } else {
            "active".debug_mint()
        }
    );
    let cache_size = ByteSize::b(cache_size);
    let queue_size = ByteSize::b(queue_size);
    greyln!("Cache size: {}", cache_size.debug_grey());
    greyln!("Queue size: {}", queue_size.debug_grey());
    greyln!(
        "Minimum bid for {} contract: {}",
        "8kb".debug_mint(),
        min_bid_smol.debug_lavender()
    );
    greyln!(
        "Minimum bid for {} contract: {}",
        "16kb".debug_yellow(),
        min_bid_med.debug_lavender()
    );
    greyln!(
        "Minimum bid for {} contract: {}",
        "24kb".debug_red(),
        min_bid_big.debug_lavender()
    );
    if queue_size < cache_size {
        greyln!("Cache is not yet at capacity, so bids of size 0 are accepted");
    } else {
        greyln!("Cache is at capacity, bids must be >= 0 to be accepted");
    }
    if let Some(address) = cfg.address {
        let code = provider
            .get_code_at(address.to_fixed_bytes().into())
            .await?;
        let codehash = keccak256(code);
        let ArbWasmCache::codehashIsCachedReturn { _0: is_cached } =
            arb_wasm_cache.codehashIsCached(codehash).call().await?;
        greyln!(
            "Contract at address {} {}",
            address.debug_lavender(),
            if is_cached {
                "is cached".debug_mint()
            } else {
                "is not cached".debug_red()
            }
        );
    }
    Ok(())
}

pub async fn place_bid(cfg: &CacheBidConfig) -> Result<()> {
    // let provider = sys::new_provider(&cfg.endpoint)?;
    // let chain_id = provider
    //     .get_chainid()
    //     .await
    //     .wrap_err("failed to get chain id")?;

    // let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    // let wallet = wallet.with_chain_id(chain_id.as_u64());
    // let client = SignerMiddleware::new(provider.clone(), wallet);

    // let data = ArbWasmCache::allCacheManagersCall {}.abi_encode();
    // let tx = Eip1559TransactionRequest::new()
    //     .to(*ARB_WASM_CACHE_H160)
    //     .data(data);
    // let tx = TypedTransaction::Eip1559(tx);
    // let result = client.call(&tx, None).await?;
    // let cache_managers_result =
    //     ArbWasmCache::allCacheManagersCall::abi_decode_returns(&result, true)?;
    // let cache_manager_addrs = cache_managers_result.managers;
    // if cache_manager_addrs.is_empty() {
    //     bail!("no cache managers found in ArbWasmCache, perhaps the Stylus cache is not yet enabled on this chain");
    // }
    // let cache_manager = *cache_manager_addrs.last().unwrap();
    // let cache_manager = H160::from_slice(cache_manager.as_slice());

    // greyln!("Setting bid value of {} wei", cfg.bid.debug_mint());
    // let contract: Address = cfg.address.to_fixed_bytes().into();
    // let data = CacheManager::placeBidCall { program: contract }.abi_encode();
    // let tx = Eip1559TransactionRequest::new()
    //     .to(cache_manager)
    //     .value(U256::from(cfg.bid))
    //     .data(data);

    // if let Err(EthCallError { data, msg }) =
    //     eth_call(tx.clone(), State::default(), &provider).await?
    // {
    //     let error = match CacheManager::CacheManagerErrors::abi_decode(&data, true) {
    //         Ok(err) => err,
    //         Err(err_details) => bail!("unknown CacheManager error: {msg} and {:?}", err_details),
    //     };
    //     use CacheManager::CacheManagerErrors as C;
    //     match error {
    //         C::AsmTooLarge(_) => bail!("Stylus contract was too large to cache"),
    //         C::AlreadyCached(_) => bail!("Stylus contract is already cached"),
    //         C::BidsArePaused(_) => {
    //             bail!("Bidding is currently paused for the Stylus cache manager")
    //         }
    //         C::BidTooSmall(_) => {
    //             bail!("Bid amount {} (wei) too small", cfg.bid)
    //         }
    //         C::ProgramNotActivated(_) => {
    //             bail!("Your Stylus contract {} is not yet activated. To activate it, use the `cargo stylus activate` subcommand", hex::encode(contract))
    //         }
    //     }
    // }
    // let verbose = cfg.verbose;
    // let receipt = run_tx(
    //     "place_bid",
    //     tx,
    //     None,
    //     cfg.max_fee_per_gas_gwei,
    //     &client,
    //     verbose,
    // )
    // .await?;

    // let address = cfg.address.debug_lavender();

    // if verbose {
    //     let gas = format_gas(receipt.gas_used.unwrap_or_default());
    //     greyln!(
    //         "Successfully cached contract at address: {address} {} {gas}",
    //         "with".grey()
    //     );
    // } else {
    //     greyln!("Successfully cached contract at address: {address}");
    // }
    // let tx_hash = receipt.transaction_hash.debug_lavender();
    // greyln!("Sent Stylus cache tx with hash: {tx_hash}");
    Ok(())
}

async fn get_cache_manager_address<P>(provider: P) -> Result<Address>
where
    P: Provider + Clone + Send + Sync,
{
    let arb_wasm_cache = ArbWasmCache::new(ARB_WASM_CACHE_ADDRESS, provider.clone());
    let result = arb_wasm_cache.allCacheManagers().call().await?;
    if result.managers.is_empty() {
        bail!("no cache managers found in ArbWasmCache, perhaps the Stylus cache is not yet enabled on this chain");
    }
    Ok(*result.managers.last().unwrap())
}
