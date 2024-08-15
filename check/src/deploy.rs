// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

#![allow(clippy::println_empty_string)]
use crate::{
    check::{self, ContractCheck},
    constants::ARB_WASM_H160,
    macros::*,
    DeployConfig,
};
use alloy_primitives::{Address, U256 as AU256};
use alloy_sol_macro::sol;
use alloy_sol_types::SolCall;
use cargo_stylus_util::{
    color::{Color, DebugColor},
    sys,
};
use ethers::{
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    prelude::*,
    providers::{Middleware, Provider},
    signers::Signer,
    types::{transaction::eip2718::TypedTransaction, Eip1559TransactionRequest, H160, U256, U64},
};
use eyre::{bail, eyre, Result, WrapErr};

sol! {
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

pub type SignerClient = SignerMiddleware<Provider<Http>, Wallet<SigningKey>>;

/// Deploys a stylus contract, activating if needed.
pub async fn deploy(cfg: DeployConfig) -> Result<()> {
    macro_rules! run {
        ($expr:expr) => {
            $expr.await?
        };
        ($expr:expr, $($msg:expr),+) => {
            $expr.await.wrap_err_with(|| eyre!($($msg),+))?
        };
    }

    let contract = run!(check::check(&cfg.check_config), "cargo stylus check failed");
    let verbose = cfg.check_config.common_cfg.verbose;

    let client = sys::new_provider(&cfg.check_config.common_cfg.endpoint)?;
    let chain_id = run!(client.get_chainid(), "failed to get chain id");

    let wallet = cfg.auth.wallet().wrap_err("failed to load wallet")?;
    let wallet = wallet.with_chain_id(chain_id.as_u64());
    let sender = wallet.address();
    let client = SignerMiddleware::new(client, wallet);

    if verbose {
        greyln!("sender address: {}", sender.debug_lavender());
    }

    let data_fee = contract.suggest_fee();

    if let ContractCheck::Ready { .. } = &contract {
        // check balance early
        let balance = run!(client.get_balance(sender, None), "failed to get balance");
        let balance = alloy_ethers_typecast::ethers_u256_to_alloy(balance);

        if balance < data_fee && !cfg.estimate_gas {
            bail!(
                "not enough funds in account {} to pay for data fee\n\
                 balance {} < {}\n\
                 please see the Quickstart guide for funding new accounts:\n{}",
                sender.red(),
                balance.red(),
                format!("{data_fee} wei").red(),
                "https://docs.arbitrum.io/stylus/stylus-quickstart".yellow(),
            );
        }
    }

    let contract_addr = cfg
        .deploy_contract(contract.code(), sender, &client)
        .await?;

    match contract {
        ContractCheck::Ready { .. } => {
            cfg.activate(sender, contract_addr, data_fee, &client)
                .await?
        }
        ContractCheck::Active { .. } => greyln!("wasm already activated!"),
    }
    Ok(())
}

impl DeployConfig {
    async fn deploy_contract(
        &self,
        code: &[u8],
        sender: H160,
        client: &SignerClient,
    ) -> Result<H160> {
        let init_code = contract_deployment_calldata(code);

        let tx = Eip1559TransactionRequest::new()
            .from(sender)
            .data(init_code);

        let verbose = self.check_config.common_cfg.verbose;
        let gas = client
            .estimate_gas(&TypedTransaction::Eip1559(tx.clone()), None)
            .await?;

        if self.check_config.common_cfg.verbose || self.estimate_gas {
            greyln!("deploy gas estimate: {}", format_gas(gas));
        }
        if self.estimate_gas {
            let nonce = client.get_transaction_count(sender, None).await?;
            return Ok(ethers::utils::get_contract_address(sender, nonce));
        }

        let receipt = run_tx(
            "deploy",
            tx,
            Some(gas),
            self.check_config.common_cfg.max_fee_per_gas_gwei,
            client,
            self.check_config.common_cfg.verbose,
        )
        .await?;
        let contract = receipt.contract_address.ok_or(eyre!("missing address"))?;
        let address = contract.debug_lavender();

        if verbose {
            let gas = format_gas(receipt.gas_used.unwrap_or_default());
            greyln!(
                "deployed code at address: {address} {} {gas}",
                "with".grey()
            );
        } else {
            greyln!("deployed code at address: {address}");
        }
        let tx_hash = receipt.transaction_hash.debug_lavender();
        greyln!("deployment tx hash: {tx_hash}");
        println!(
            r#"we recommend running cargo stylus cache --address={} to cache your activated contract in ArbOS.
Cached contracts benefit from cheaper calls. To read more about the Stylus contract cache, see
https://docs.arbitrum.io/stylus/concepts/stylus-cache-manager"#,
            hex::encode(contract)
        );
        Ok(contract)
    }

    async fn activate(
        &self,
        sender: H160,
        contract: H160,
        data_fee: AU256,
        client: &SignerClient,
    ) -> Result<()> {
        let verbose = self.check_config.common_cfg.verbose;
        let data_fee = alloy_ethers_typecast::alloy_u256_to_ethers(data_fee);
        let contract_addr: Address = contract.to_fixed_bytes().into();

        let data = ArbWasm::activateProgramCall {
            program: contract_addr,
        }
        .abi_encode();

        let tx = Eip1559TransactionRequest::new()
            .from(sender)
            .to(*ARB_WASM_H160)
            .data(data)
            .value(data_fee);

        let gas = client
            .estimate_gas(&TypedTransaction::Eip1559(tx.clone()), None)
            .await
            .map_err(|e| eyre!("did not estimate correctly: {e}"))?;

        if self.check_config.common_cfg.verbose || self.estimate_gas {
            greyln!("activation gas estimate: {}", format_gas(gas));
        }
        if self.estimate_gas {
            return Ok(());
        }

        let receipt = run_tx(
            "activate",
            tx,
            Some(gas),
            self.check_config.common_cfg.max_fee_per_gas_gwei,
            client,
            self.check_config.common_cfg.verbose,
        )
        .await?;

        if verbose {
            let gas = format_gas(receipt.gas_used.unwrap_or_default());
            greyln!("activated with {gas}");
        }
        greyln!(
            "contract activated and ready onchain with tx hash: {}",
            receipt.transaction_hash.debug_lavender()
        );
        Ok(())
    }
}

pub async fn run_tx(
    name: &str,
    tx: Eip1559TransactionRequest,
    gas: Option<U256>,
    max_fee_per_gas_gwei: Option<u128>,
    client: &SignerClient,
    verbose: bool,
) -> Result<TransactionReceipt> {
    let mut tx = tx;
    if let Some(gas) = gas {
        tx.gas = Some(gas);
    }
    if let Some(max_fee) = max_fee_per_gas_gwei {
        tx.max_fee_per_gas = Some(U256::from(gwei_to_wei(max_fee)?));
    }
    let tx = TypedTransaction::Eip1559(tx);
    let tx = client.send_transaction(tx, None).await?;
    let tx_hash = tx.tx_hash();
    if verbose {
        greyln!("sent {name} tx: {}", tx_hash.debug_lavender());
    }
    let Some(receipt) = tx.await.wrap_err("tx failed to complete")? else {
        bail!("failed to get receipt for tx {}", tx_hash.lavender());
    };
    if receipt.status != Some(U64::from(1)) {
        bail!("{name} tx reverted {}", tx_hash.debug_red());
    }
    Ok(receipt)
}

/// Prepares an EVM bytecode prelude for contract creation.
pub fn contract_deployment_calldata(code: &[u8]) -> Vec<u8> {
    let mut code_len = [0u8; 32];
    U256::from(code.len()).to_big_endian(&mut code_len);
    let mut deploy: Vec<u8> = vec![];
    deploy.push(0x7f); // PUSH32
    deploy.extend(code_len);
    deploy.push(0x80); // DUP1
    deploy.push(0x60); // PUSH1
    deploy.push(42 + 1); // prelude + version
    deploy.push(0x60); // PUSH1
    deploy.push(0x00);
    deploy.push(0x39); // CODECOPY
    deploy.push(0x60); // PUSH1
    deploy.push(0x00);
    deploy.push(0xf3); // RETURN
    deploy.push(0x00); // version
    deploy.extend(code);
    deploy
}

pub fn extract_contract_evm_deployment_prelude(calldata: &[u8]) -> Vec<u8> {
    // The length of the prelude, version part is 42 + 1 as per the code
    let metadata_length = 42 + 1;
    // Extract and return the metadata part
    calldata[0..metadata_length].to_vec()
}

pub fn extract_compressed_wasm(calldata: &[u8]) -> Vec<u8> {
    // The length of the prelude, version part is 42 + 1 as per the code
    let metadata_length = 42 + 1;
    // Extract and return the metadata part
    calldata[metadata_length..].to_vec()
}

pub fn format_gas(gas: U256) -> String {
    let gas: u64 = gas.try_into().unwrap_or(u64::MAX);
    let text = format!("{gas} gas");
    if gas <= 3_000_000 {
        text.mint()
    } else if gas <= 7_000_000 {
        text.yellow()
    } else {
        text.pink()
    }
}

pub fn gwei_to_wei(gwei: u128) -> Result<u128> {
    let wei_per_gwei: u128 = 10u128.pow(9);
    match gwei.checked_mul(wei_per_gwei) {
        Some(wei) => Ok(wei),
        None => bail!("overflow occurred while converting gwei to wei"),
    }
}
