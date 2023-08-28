// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use crate::color::Color;

use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::Eip1559TransactionRequest;
use ethers::{middleware::SignerMiddleware, providers::Middleware, signers::Signer};
use eyre::eyre;

/// Submits a tx to a client given a data payload and a
/// transaction request to sign and send. If estimate_only is true, only a call to
/// estimate gas will occur and the actual tx will not be submitted.
pub async fn submit_signed_tx<M, S>(
    client: &SignerMiddleware<M, S>,
    tx_kind: &str,
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
        .ok_or(eyre!("no block found"))?;
    let base_fee = block
        .base_fee_per_gas
        .ok_or(eyre!("no base fee found for block"))?;

    if !(estimate_only) {
        tx_request.max_fee_per_gas = Some(base_fee);
        tx_request.max_priority_fee_per_gas = Some(base_fee);
    }

    let typed = TypedTransaction::Eip1559(tx_request.clone());
    let estimated = client
        .estimate_gas(&typed, None)
        .await
        .map_err(|e| eyre!("not estimate gas {e}"))?;

    println!("Estimated gas for {tx_kind}: {}", estimated.pink());

    if estimate_only {
        return Ok(());
    }

    println!("Submitting {tx_kind} tx...");

    let pending_tx = client
        .send_transaction(typed, None)
        .await
        .map_err(|e| eyre!("could not send tx: {e}"))?;

    let receipt = pending_tx
        .await
        .map_err(|e| eyre!("could not get receipt: {e}"))?
        .ok_or(eyre!("no receipt found"))?;

    match receipt.status {
        None => Err(eyre!(
            "{tx_kind} tx with hash {} reverted",
            receipt.transaction_hash,
        )),
        Some(_) => {
            let tx_hash = receipt.transaction_hash;
            let gas_used = receipt.gas_used.unwrap();
            println!(
                "Confirmed {tx_kind} tx {}, gas used {}",
                hex::encode(tx_hash.as_bytes()).pink(),
                gas_used.mint()
            );
            Ok(())
        }
    }
}
