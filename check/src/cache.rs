// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use std::str::FromStr;
use std::sync::Arc;

use alloy_primitives::FixedBytes;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolInterface};
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
use hex::FromHex;
use serde_json::Value;

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
    println!("Program code: {:?}", hex::encode(&program_code));
    // let codehash = ethers::utils::keccak256(&program_code);
    // println!("Program codehash: {:#x}", &codehash);

    let raw_data = hex::decode("").unwrap();
    // if program_code != raw_data {
    //     bail!(
    //         "program code mismatch, got {} vs {}",
    //         hex::encode(program_code),
    //         hex::encode(raw_data)
    //     );
    // }
    println!("got codehash {:?}", hex::encode(keccak256(&raw_data)));
    let codehash = FixedBytes::<32>::from(keccak256(&raw_data));

    let data = CacheManager::placeBidCall { codehash }.abi_encode();
    let to = H160::from_slice(
        hex::decode("d1bbd579988f394a26d6ec16e77b3fa8a5e8fcee")
            .unwrap()
            .as_slice(),
    );
    let tx = Eip1559TransactionRequest::new()
        .to(NameOrAddress::Address(to))
        .data(data)
        .value(U256::from(args.bid));

    // let privkey = "93690ac9d039285ed00f874a2694d951c1777ac3a165732f36ea773f16179a89".to_string();
    // let wallet = LocalWallet::from_str(&privkey)?;
    // let chain_id = provider.get_chainid().await?.as_u64();
    // let client = Arc::new(SignerMiddleware::new(
    //     provider,
    //     wallet.clone().with_chain_id(chain_id),
    // ));
    // let pending_tx = client.send_transaction(tx, None).await?;
    // let receipt = pending_tx.await?;
    // match receipt {
    //     Some(receipt) => {
    //         println!("Receipt: {:?}", receipt);
    //     }
    //     None => {
    //         bail!("failed to cache program");
    //     }
    // }

    if let Err(EthCallError { data, msg }) =
        eth_call(tx.clone(), State::default(), &provider).await?
    {
        println!("Got data {}, msg {:?}", hex::encode(&data), msg);
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

    println!("Succeeded cache call");
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
