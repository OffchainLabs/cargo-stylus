// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
#![allow(clippy::println_empty_string)]

use crate::color::Color;
use crate::deploy::TxKind;

use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::{Eip1559TransactionRequest, H256, U256};
use ethers::utils::{format_ether, format_units};
use ethers::{middleware::SignerMiddleware, providers::Middleware, signers::Signer};
use eyre::eyre;

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum TxError {
    #[error("no head block found")]
    NoHeadBlock,
    #[error("no base fee found for block")]
    NoBaseFee,
    #[error("no receipt found for tx hash ({tx_hash})")]
    NoReceiptFound { tx_hash: H256 },
    #[error("({tx_kind}) got reverted with hash ({tx_hash})")]
    Reverted { tx_kind: String, tx_hash: H256 },
}

/// Submits a tx to a client given a data payload and a
/// transaction request to sign and send. If estimate_only is true, only a call to
/// estimate gas will occur and the actual tx will not be submitted.
pub async fn submit_signed_tx<M, S>(
    client: &SignerMiddleware<M, S>,
    tx_kind: TxKind,
    estimate_only: bool,
    tx_request: &mut Eip1559TransactionRequest,
) -> eyre::Result<()>
where
    M: Middleware,
    S: Signer,
{
    let block_num = client
        .get_block_number()
        .await
        .map_err(|e| eyre!("could not get block number: {e}"))?;
    let block = client
        .get_block(block_num)
        .await
        .map_err(|e| eyre!("could not get block: {e}"))?
        .ok_or(TxError::NoHeadBlock)?;
    let base_fee = block.base_fee_per_gas.ok_or(TxError::NoBaseFee)?;

    let base_fee_gwei = format_units(base_fee, "gwei")
        .map_err(|e| eyre!("could not format base fee as gwei: {e}"))?;
    println!("Base fee: {} gwei", base_fee_gwei.grey());
    if !(estimate_only) {
        tx_request.max_fee_per_gas = Some(base_fee);
        tx_request.max_priority_fee_per_gas = Some(base_fee);
    }

    let typed = TypedTransaction::Eip1559(tx_request.clone());
    let estimated = client
        .estimate_gas(&typed, None)
        .await
        .map_err(|e| eyre!("could not estimate gas {e}"))?;

    println!(
        "Estimated gas for {tx_kind}: {} gas units",
        estimated.mint()
    );

    if estimate_only {
        return Ok(());
    }

    println!("Submitting {tx_kind} tx...");

    let pending_tx = client
        .send_transaction(typed, None)
        .await
        .map_err(|e| eyre!("could not send tx: {e}"))?;

    let tx_hash = pending_tx.tx_hash();

    let receipt = pending_tx
        .await
        .map_err(|e| eyre!("could not get receipt: {e}"))?
        .ok_or(TxError::NoReceiptFound { tx_hash })?;

    match receipt.status {
        None => Err(TxError::Reverted {
            tx_hash,
            tx_kind: tx_kind.to_string(),
        }
        .into()),
        Some(_) => {
            let tx_hash = receipt.transaction_hash;
            let gas_used = receipt.gas_used.unwrap();
            let effective_price = receipt.effective_gas_price.unwrap_or(U256::zero());
            let effective_price_gwei = format_units(effective_price, "gwei")
                .map_err(|e| eyre!("could not format effective gas price: {e}"))?;
            println!(
                "Confirmed {tx_kind} tx {}{}",
                "0x".mint(),
                hex::encode(tx_hash.as_bytes()).mint(),
            );
            println!(
                "Gas units used {}, effective gas price {} gwei",
                gas_used.mint(),
                effective_price_gwei.grey(),
            );
            println!(
                "Transaction fee: {} ETH",
                format_ether(gas_used * effective_price).mint()
            );
            Ok(())
        }
    }
}
