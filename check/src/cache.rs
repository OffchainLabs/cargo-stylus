// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use std::str::FromStr;
use std::sync::Arc;

use alloy_primitives::FixedBytes;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
use cargo_stylus_util::color::{Color, DebugColor};
use cargo_stylus_util::sys;
use clap::{Args, Parser};
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::providers::{Http, Provider, ProviderError, RawCall};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::spoof::State;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Address, Eip1559TransactionRequest, NameOrAddress, H160, U256};
use ethers::utils::keccak256;
use eyre::{bail, eyre, Context, ErrReport, Result};
use serde_json::Value;

use crate::check::{eth_call, EthCallError};
use crate::constants::{CACHE_MANAGER_ADDRESS, CACHE_MANAGER_H160, EOF_PREFIX_NO_DICT};
use crate::deploy::{format_gas, run_tx};
use crate::macros::greyln;
use crate::CacheConfig;

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

pub async fn cache_program(cfg: &CacheConfig) -> Result<()> {
    let provider = sys::new_provider(&cfg.common_cfg.endpoint)?;
    let chain_id = provider
        .get_chainid()
        .await
        .wrap_err("failed to get chain id")?;

    let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    let client = SignerMiddleware::new(provider.clone(), wallet);

    let program_code = client
        .get_code(cfg.program_address, None)
        .await
        .wrap_err("failed to fetch program code")?;

    greyln!(
        "Program code length at address {}: {}",
        cfg.program_address.debug_lavender(),
        program_code.len()
    );
    if !program_code.starts_with(hex::decode(EOF_PREFIX_NO_DICT).unwrap().as_slice()) {
        bail!(
            "program code does not start with Stylus prefix {}",
            EOF_PREFIX_NO_DICT
        );
    }
    let codehash = FixedBytes::<32>::from(keccak256(&program_code));
    greyln!(
        "Program codehash {}",
        hex::encode(codehash).debug_lavender()
    );
    let codehash = FixedBytes::<32>::from(keccak256(&program_code));

    let data = CacheManager::placeBidCall { codehash }.abi_encode();
    let mut tx = Eip1559TransactionRequest::new()
        .to(*CACHE_MANAGER_H160)
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
            C::AsmTooLarge(_) => bail!("program too large"),
            _ => bail!("unexpected CacheManager error: {msg}"),
        }
    }
    let verbose = cfg.common_cfg.verbose;
    let receipt = run_tx("cache", tx, None, &client, verbose).await?;

    let address = cfg.program_address.debug_lavender();

    if verbose {
        let gas = format_gas(receipt.gas_used.unwrap_or_default());
        greyln!(
            "Deployed code at address: {address} {} {gas}",
            "with".grey()
        );
    } else {
        greyln!("Deployed code at address: {address}");
    }
    let tx_hash = receipt.transaction_hash.debug_lavender();
    greyln!("Sent Stylus cache tx with hash: {tx_hash}");
    Ok(())
}
