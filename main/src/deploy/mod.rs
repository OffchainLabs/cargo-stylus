// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

#![allow(clippy::println_empty_string)]
use crate::{
    check::{self, ContractCheck},
    constants::ARB_WASM_ADDRESS,
    export_abi,
    macros::*,
    util::color::{Color, DebugColor},
    DeployConfig, GasFeeConfig,
};
use alloy::{
    primitives::{utils::format_units, Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::{TransactionInput, TransactionReceipt, TransactionRequest},
    sol,
    sol_types::SolCall,
};
use eyre::{bail, eyre, Result, WrapErr};

mod deployer;

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

/// Deploys a stylus contract, activating if needed.
pub async fn deploy(cfg: DeployConfig) -> Result<()> {
    let contract = check::check(&cfg.check_config)
        .await
        .expect("cargo stylus check failed");
    let verbose = cfg.check_config.common_cfg.verbose;
    let use_wasm_file = cfg.check_config.wasm_file.is_some();

    let constructor = if use_wasm_file {
        None
    } else {
        export_abi::get_constructor_signature()?
    };

    let deployer_args = match constructor {
        Some(constructor) => {
            let args = deployer::parse_constructor_args(&cfg, &constructor, &contract).await?;
            Some(args)
        }
        None => None,
    };

    let provider = ProviderBuilder::new()
        .on_builtin(&cfg.check_config.common_cfg.endpoint)
        .await?;
    let chain_id = provider.get_chain_id().await?;
    let wallet = cfg.auth.alloy_wallet(chain_id)?;
    let from_address = wallet.default_signer().address();
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_builtin(&cfg.check_config.common_cfg.endpoint)
        .await?;

    if verbose {
        greyln!("sender address: {}", from_address.debug_lavender());
    }

    let data_fee = contract.suggest_fee() + cfg.experimental_constructor_value;

    if let ContractCheck::Ready { .. } = &contract {
        // check balance early
        let balance = provider
            .get_balance(from_address)
            .await
            .expect("failed to get balance");

        if balance < data_fee && !cfg.estimate_gas {
            bail!(
                "not enough funds in account {} to pay for data fee\n\
                 balance {} < {}\n\
                 please see the Quickstart guide for funding new accounts:\n{}",
                from_address.red(),
                balance.red(),
                format!("{data_fee} wei").red(),
                "https://docs.arbitrum.io/stylus/stylus-quickstart".yellow(),
            );
        }
    }

    if let Some(deployer_args) = deployer_args {
        return deployer::deploy(&cfg, deployer_args, from_address, &provider).await;
    }

    let contract_addr = cfg
        .deploy_contract(contract.code(), from_address, &provider)
        .await?;

    if cfg.estimate_gas {
        return Ok(());
    }

    match contract {
        ContractCheck::Ready { .. } => {
            if cfg.no_activate {
                mintln!(
                    r#"NOTE: You must activate the stylus contract before calling it. To do so, we recommend running:
cargo stylus activate --address {}"#,
                    hex::encode(contract_addr)
                );
            } else {
                cfg.activate(from_address, contract_addr, data_fee, &provider)
                    .await?
            }
        }
        ContractCheck::Active { .. } => greyln!("wasm already activated!"),
    }
    print_cache_notice(contract_addr);
    Ok(())
}

impl DeployConfig {
    async fn deploy_contract(
        &self,
        code: &[u8],
        sender: Address,
        provider: &impl Provider,
    ) -> Result<Address> {
        let init_code = contract_deployment_calldata(code);

        let tx = TransactionRequest::default()
            .from(sender)
            .input(TransactionInput::new(init_code.into()));

        let verbose = self.check_config.common_cfg.verbose;
        let gas = provider.estimate_gas(&tx).await?;

        let gas_price = provider.get_gas_price().await?;

        if self.check_config.common_cfg.verbose || self.estimate_gas {
            print_gas_estimate("deployment", gas, gas_price).await?;
        }
        if self.estimate_gas {
            let nonce = provider.get_transaction_count(sender).await?;
            return Ok(sender.create(nonce));
        }

        let fee_per_gas = calculate_fee_per_gas(&self.check_config.common_cfg, gas_price)?;

        let receipt = run_tx(
            "deploy",
            tx,
            Some(gas),
            fee_per_gas,
            provider,
            self.check_config.common_cfg.verbose,
        )
        .await?;
        let contract = receipt.contract_address.ok_or(eyre!("missing address"))?;
        let address = contract.debug_lavender();

        if verbose {
            let gas = format_gas(receipt.gas_used);
            greyln!(
                "deployed code at address: {address} {} {gas}",
                "with".grey()
            );
        } else {
            greyln!("deployed code at address: {address}");
        }
        let tx_hash = receipt.transaction_hash.debug_lavender();
        greyln!("deployment tx hash: {tx_hash}");
        Ok(contract)
    }

    async fn activate(
        &self,
        sender: Address,
        contract_addr: Address,
        data_fee: U256,
        client: &impl Provider,
    ) -> Result<()> {
        let verbose = self.check_config.common_cfg.verbose;

        let data = ArbWasm::activateProgramCall {
            program: contract_addr,
        }
        .abi_encode();

        let tx = TransactionRequest::default()
            .from(sender)
            .to(ARB_WASM_ADDRESS)
            .input(TransactionInput::new(data.into()))
            .value(data_fee);

        let gas = client
            .estimate_gas(&tx)
            .await
            .map_err(|e| eyre!("did not estimate correctly: {e}"))?;

        let gas_price = client.get_gas_price().await?;

        if self.check_config.common_cfg.verbose || self.estimate_gas {
            greyln!("activation gas estimate: {}", format_gas(gas));
        }

        let fee_per_gas = calculate_fee_per_gas(&self.check_config.common_cfg, gas_price)?;

        let receipt = run_tx(
            "activate",
            tx,
            Some(gas),
            fee_per_gas,
            client,
            self.check_config.common_cfg.verbose,
        )
        .await?;

        if verbose {
            let gas = format_gas(receipt.gas_used);
            greyln!("activated with {gas}");
        }
        greyln!(
            "contract activated and ready onchain with tx hash: {}",
            receipt.transaction_hash.debug_lavender()
        );
        Ok(())
    }
}

pub async fn print_gas_estimate(name: &str, gas: u64, gas_price: u128) -> Result<()> {
    greyln!("estimates");
    greyln!("{} tx gas: {}", name, gas.debug_lavender());
    greyln!(
        "gas price: {} gwei",
        format_units(gas_price, "gwei")?.debug_lavender()
    );
    let total_cost = gas_price.checked_mul(gas.into()).unwrap_or_default();
    let eth_estimate = format_units(total_cost, "ether")?;
    greyln!(
        "{} tx total cost: {} ETH",
        name,
        eth_estimate.debug_lavender()
    );
    Ok(())
}

pub fn print_cache_notice(contract_addr: Address) {
    let contract_addr = hex::encode(contract_addr);
    println!("");
    mintln!(
        r#"NOTE: We recommend running cargo stylus cache bid {contract_addr} 0 to cache your activated contract in ArbOS.
Cached contracts benefit from cheaper calls. To read more about the Stylus contract cache, see
https://docs.arbitrum.io/stylus/how-tos/caching-contracts"#
    );
}

pub async fn run_tx(
    name: &str,
    tx: TransactionRequest,
    gas: Option<u64>,
    max_fee_per_gas_wei: u128,
    provider: &impl Provider,
    verbose: bool,
) -> Result<TransactionReceipt> {
    let mut tx = tx;
    if let Some(gas) = gas {
        tx.gas = Some(gas);
    }

    tx.max_fee_per_gas = Some(max_fee_per_gas_wei);
    tx.max_priority_fee_per_gas = Some(0);

    let tx = provider.send_transaction(tx).await?;
    let tx_hash = *tx.tx_hash();
    if verbose {
        greyln!("sent {name} tx: {}", tx_hash.debug_lavender());
    }
    let receipt = tx.get_receipt().await.wrap_err("tx failed to complete")?;
    if !receipt.status() {
        bail!("{name} tx reverted {}", tx_hash.debug_red());
    }
    Ok(receipt)
}

/// Prepares an EVM bytecode prelude for contract creation.
pub fn contract_deployment_calldata(code: &[u8]) -> Vec<u8> {
    let code_len: [u8; 32] = U256::from(code.len()).to_be_bytes();
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

pub fn format_gas(gas: u64) -> String {
    let text = format!("{gas} gas");
    if gas <= 3_000_000 {
        text.mint()
    } else if gas <= 7_000_000 {
        text.yellow()
    } else {
        text.pink()
    }
}

pub fn calculate_fee_per_gas<T: GasFeeConfig>(config: &T, gas_price: u128) -> Result<u128> {
    let fee_per_gas = match config.get_max_fee_per_gas_wei()? {
        Some(wei) => wei,
        None => gas_price,
    };
    Ok(fee_per_gas)
}
