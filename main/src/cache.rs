// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use crate::util::color::{Color, DebugColor};
use alloy_contract::Error;
use alloy_primitives::{keccak256, Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_sol_macro::sol;
use bytesize::ByteSize;
use eyre::{bail, Result};
use CacheManager::CacheManagerErrors;

use crate::constants::ARB_WASM_CACHE_ADDRESS;
use crate::deploy::gwei_to_wei;
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

/// Recommends a minimum bid to the user for caching a Stylus program by address. If the program
/// has not yet been activated, the user will be informed.
pub async fn suggest_bid(cfg: &CacheSuggestionsConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .on_builtin(&cfg.endpoint)
        .await?;
    let cache_manager_addr = get_cache_manager_address(provider.clone()).await?;
    let cache_manager = CacheManager::new(cache_manager_addr, provider.clone());
    match cache_manager
        .getMinBid_0(cfg.address.to_fixed_bytes().into())
        .call()
        .await
    {
        Ok(CacheManager::getMinBid_0Return { min: min_bid }) => {
            greyln!(
                "Minimum bid for contract {}: {} wei",
                cfg.address,
                min_bid.debug_mint()
            );
            Ok(())
        }
        Err(e) => {
            let Error::TransportError(tperr) = e else {
                bail!("failed to send cache bid tx: {:?}", e)
            };
            let Some(err_resp) = tperr.as_error_resp() else {
                bail!("no error payload received in response: {:?}", tperr)
            };
            let Some(errs) = err_resp.as_decoded_error::<CacheManagerErrors>(true) else {
                bail!("failed to decode CacheManager error: {:?}", err_resp)
            };
            handle_cache_manager_error(errs)
        }
    }
}

/// Checks the status of the Stylus cache manager, including the cache size, queue size, and minimum bid
/// for different contract sizes as reference points. It also checks if a specified Stylus contract address
/// is currently cached.
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
                "is not yet cached".debug_red() + " please use cargo stylus cache bid to cache it"
            }
        );
    }
    Ok(())
}

/// Attempts to cache a Stylus contract by address by placing a bid by sending a tx to the network.
/// It will handle the different cache manager errors that can be encountered along the way and
/// print friendlier errors if failed.
pub async fn place_bid(cfg: &CacheBidConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .on_builtin(&cfg.endpoint)
        .await?;
    let chain_id = provider.get_chain_id().await?;
    let wallet = cfg.auth.alloy_wallet(chain_id)?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_builtin(&cfg.endpoint)
        .await?;
    let cache_manager_addr = get_cache_manager_address(provider.clone()).await?;
    let cache_manager = CacheManager::new(cache_manager_addr, provider.clone());
    let addr = cfg.address.to_fixed_bytes().into();
    let mut place_bid_call = cache_manager.placeBid(addr).value(U256::from(cfg.bid));
    if let Some(max_fee) = cfg.max_fee_per_gas_gwei {
        place_bid_call = place_bid_call.max_fee_per_gas(gwei_to_wei(max_fee)?);
    };

    greyln!("Checking if contract can be cached...");

    let raw_output = place_bid_call.clone().call().await;
    if let Err(e) = raw_output {
        let Error::TransportError(tperr) = e else {
            bail!("failed to send cache bid tx: {:?}", e)
        };
        let Some(err_resp) = tperr.as_error_resp() else {
            bail!("no error payload received in response: {:?}", tperr)
        };
        let Some(errs) = err_resp.as_decoded_error::<CacheManagerErrors>(true) else {
            bail!("failed to decode CacheManager error: {:?}", err_resp)
        };
        handle_cache_manager_error(errs)?;
    }
    greyln!("Sending cache bid tx...");
    let pending_tx = place_bid_call.send().await?;
    let receipt = pending_tx.get_receipt().await?;
    if cfg.verbose {
        let gas = format_gas(receipt.gas_used);
        greyln!(
            "Successfully cached contract at address: {addr} {} {gas} gas used",
            "with".grey()
        );
    } else {
        greyln!("Successfully cached contract at address: {addr}");
    }
    let tx_hash = receipt.transaction_hash.debug_lavender();
    greyln!("Sent Stylus cache bid tx with hash: {tx_hash}");
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

fn format_gas(gas: u128) -> String {
    let gas: u128 = gas.try_into().unwrap_or(u128::MAX);
    let text = format!("{gas} gas");
    if gas <= 3_000_000 {
        text.mint()
    } else if gas <= 7_000_000 {
        text.yellow()
    } else {
        text.pink()
    }
}

fn handle_cache_manager_error(err: CacheManagerErrors) -> Result<()> {
    use CacheManager::CacheManagerErrors as C;
    match err {
        C::AsmTooLarge(_) => bail!("Stylus contract was too large to cache"),
        C::AlreadyCached(_) => bail!("Stylus contract is already cached"),
        C::BidsArePaused(_) => {
            bail!("Bidding is currently paused for the Stylus cache manager")
        }
        C::BidTooSmall(_) => {
            bail!("Bid amount (wei) too small");
        }
        C::ProgramNotActivated(_) => {
            bail!("Your Stylus contract is not yet activated. To activate it, use the `cargo stylus activate` subcommand");
        }
    }
}
