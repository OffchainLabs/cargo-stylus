// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use alloy_primitives::Address;
use alloy_sol_macro::sol;
use alloy_sol_types::SolCall;
use cargo_stylus_util::color::{Color, DebugColor};
use cargo_stylus_util::sys;
use ethers::middleware::{Middleware, SignerMiddleware};
use ethers::signers::Signer;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Eip1559TransactionRequest, U256};
use ethers::utils::format_units;
use eyre::{bail, Context, Result};

use crate::check::check_activate;
use crate::constants::ARB_WASM_H160;
use crate::macros::greyln;

use crate::ActivateConfig;

sol! {
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

/// Activates an already deployed Stylus program by address.
pub async fn activate_program(cfg: &ActivateConfig) -> Result<()> {
    let provider = sys::new_provider(&cfg.common_cfg.endpoint)?;
    let chain_id = provider
        .get_chainid()
        .await
        .wrap_err("failed to get chain id")?;

    let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    let client = SignerMiddleware::new(provider.clone(), wallet);

    let code = client.get_code(cfg.address, None).await?;
    let data_fee = check_activate(code, cfg.address, &provider).await?;
    let mut data_fee = alloy_ethers_typecast::alloy_u256_to_ethers(data_fee);

    greyln!(
        "obtained estimated activation data fee {}",
        format_units(data_fee, "ether")?.debug_lavender()
    );
    if let Some(bump_percent) = cfg.data_fee_bump_percent {
        greyln!(
            "bumping estimated activation data fee by {}%",
            bump_percent.debug_lavender()
        );
        data_fee = bump_data_fee(data_fee, bump_percent);
    }

    let program: Address = cfg.address.to_fixed_bytes().into();
    let data = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .from(client.address())
        .to(*ARB_WASM_H160)
        .value(data_fee)
        .data(data);
    let tx = TypedTransaction::Eip1559(tx);
    let tx = client.send_transaction(tx, None).await?;
    match tx.await? {
        Some(receipt) => {
            greyln!(
                "successfully activated program 0x{} with tx {}",
                hex::encode(cfg.address),
                hex::encode(receipt.transaction_hash).debug_lavender()
            );
        }
        None => {
            bail!(
                "failed to fetch receipt for program activation {}",
                cfg.address
            );
        }
    }
    Ok(())
}

fn bump_data_fee(fee: U256, pct: u64) -> U256 {
    let num = 100 + pct;
    fee * U256::from(num) / U256::from(100)
}
