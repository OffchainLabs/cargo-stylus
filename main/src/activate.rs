// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use crate::check::check_activate;
use crate::constants::ARB_WASM_H160;
use crate::macros::greyln;
use crate::util::color::{Color, DebugColor};
use crate::util::sys;
use crate::ActivateConfig;
use alloy_primitives::Address;
use alloy_sol_macro::sol;
use alloy_sol_types::SolCall;
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::signers::Signer;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::Eip1559TransactionRequest;
use ethers::utils::format_units;
use eyre::{bail, Context, Result};

sol! {
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

/// Activates an already deployed Stylus contract by address.
pub async fn activate_contract(cfg: &ActivateConfig) -> Result<()> {
    let provider = sys::new_provider(&cfg.common_cfg.endpoint)?;
    let chain_id = provider
        .get_chainid()
        .await
        .wrap_err("failed to get chain id")?;

    let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    let client = SignerMiddleware::new(provider.clone(), wallet);

    let code = client.get_code(cfg.address, None).await?;
    let data_fee = check_activate(code, cfg.address, &cfg.data_fee, &provider).await?;

    let contract: Address = cfg.address.to_fixed_bytes().into();
    let data = ArbWasm::activateProgramCall { program: contract }.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .from(client.address())
        .to(*ARB_WASM_H160)
        .value(alloy_ethers_typecast::alloy_u256_to_ethers(data_fee))
        .data(data);
    let tx = TypedTransaction::Eip1559(tx);
    if cfg.estimate_gas {
        let gas = client.estimate_gas(&tx, None).await?;
        let gas_price = client.get_gas_price().await?;
        greyln!("estimates");
        greyln!("activation tx gas: {}", gas.debug_lavender());
        greyln!(
            "gas price: {} gwei",
            format_units(gas_price, "gwei")?.debug_lavender()
        );
        let total_cost = gas_price.checked_mul(gas).unwrap_or_default();
        let eth_estimate = format_units(total_cost, "ether")?;
        greyln!(
            "activation tx total cost: {} ETH",
            eth_estimate.debug_lavender()
        );
    }
    let tx = client.send_transaction(tx, None).await?;
    match tx.await? {
        Some(receipt) => {
            greyln!(
                "successfully activated contract 0x{} with tx {}",
                hex::encode(cfg.address),
                hex::encode(receipt.transaction_hash).debug_lavender()
            );
        }
        None => {
            bail!(
                "failed to fetch receipt for contract activation {}",
                cfg.address
            );
        }
    }
    Ok(())
}
