// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use crate::check::check_activate;
use crate::constants::ARB_WASM_ADDRESS;
use crate::macros::greyln;
use crate::util::color::{Color, DebugColor};
use crate::ActivateConfig;
use alloy::primitives::utils::format_units;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol;
use eyre::Result;

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

/// Activates an already deployed Stylus contract by address.
pub async fn activate_contract(cfg: &ActivateConfig) -> Result<()> {
    let provider = ProviderBuilder::new()
        .connect(&cfg.common_cfg.endpoint)
        .await?;
    let chain_id = provider.get_chain_id().await?;
    let wallet = cfg.auth.alloy_wallet(chain_id)?;
    let from_address = wallet.default_signer().address();
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect(&cfg.common_cfg.endpoint)
        .await?;

    let code = provider.get_code_at(cfg.address).await?;
    let data_fee = check_activate(code, cfg.address, &cfg.data_fee, &provider).await?;

    let arbwasm = ArbWasm::new(ARB_WASM_ADDRESS, &provider);
    let activate_call = arbwasm
        .activateProgram(cfg.address)
        .from(from_address)
        .value(data_fee);

    if cfg.estimate_gas {
        let gas = activate_call.estimate_gas().await?;
        let gas_price = provider.get_gas_price().await?;
        greyln!("estimates");
        greyln!("activation tx gas: {}", gas.debug_lavender());
        greyln!(
            "gas price: {} gwei",
            format_units(gas_price, "gwei")?.debug_lavender()
        );
        let total_cost = gas_price.checked_mul(gas.into()).unwrap_or_default();
        let eth_estimate = format_units(total_cost, "ether")?;
        greyln!(
            "activation tx total cost: {} ETH",
            eth_estimate.debug_lavender()
        );
    }
    let tx = activate_call.send().await?;
    let receipt = tx.get_receipt().await?;
    greyln!(
        "successfully activated contract 0x{} with tx {}",
        hex::encode(cfg.address),
        hex::encode(receipt.transaction_hash).debug_lavender()
    );
    Ok(())
}
