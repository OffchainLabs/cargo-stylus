// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use super::SignerClient;
use crate::{
    check::ContractCheck,
    macros::*,
    util::color::{Color, DebugColor},
    DeployConfig,
};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt, Specifier};
use alloy_json_abi::{Constructor, StateMutability};
use alloy_primitives::U256;
use alloy_sol_macro::sol;
use alloy_sol_types::{SolCall, SolEvent};
use ethers::{
    providers::Middleware,
    types::{
        transaction::eip2718::TypedTransaction, Eip1559TransactionRequest, TransactionReceipt, H160,
    },
};
use eyre::{bail, eyre, Context, Result};

sol! {
    interface CargoStylusFactory {
        event ContractDeployed(address indexed deployedContract, address indexed deployer);

        function deployActivateInit(
            bytes calldata bytecode,
            bytes calldata constructorCalldata,
            uint256 constructorValue
        ) public payable returns (address);

        function deployInit(
            bytes calldata bytecode,
            bytes calldata constructorCalldata
        ) public payable returns (address);
    }

    function stylus_constructor();
}

pub struct FactoryArgs {
    /// Factory address
    address: H160,
    /// Value to be sent in the tx
    tx_value: U256,
    /// Calldata to be sent in the tx
    tx_calldata: Vec<u8>,
}

/// Parses the constructor arguments and returns the data to deploy the contract using the factory.
pub fn parse_constructor_args(
    cfg: &DeployConfig,
    constructor: &Constructor,
    contract: &ContractCheck,
) -> Result<FactoryArgs> {
    let Some(address) = cfg.experimental_factory_address else {
        bail!("missing factory address");
    };

    let constructor_value =
        alloy_ethers_typecast::ethers_u256_to_alloy(cfg.experimental_constructor_value);
    if constructor.state_mutability != StateMutability::Payable && !constructor_value.is_zero() {
        bail!("attempting to send Ether to non-payable constructor");
    }
    let tx_value = contract.suggest_fee() + constructor_value;

    let args = &cfg.experimental_constructor_args;
    let params = &constructor.inputs;
    if args.len() != params.len() {
        bail!(
            "mismatch number of constructor arguments (want {}; got {})",
            params.len(),
            args.len()
        );
    }

    let mut arg_values = Vec::<DynSolValue>::with_capacity(args.len());
    for (arg, param) in args.iter().zip(params) {
        let ty = param
            .resolve()
            .wrap_err_with(|| format!("could not resolve constructor arg: {param}"))?;
        let value = ty
            .coerce_str(arg)
            .wrap_err_with(|| format!("could not parse constructor arg: {param}"))?;
        arg_values.push(value);
    }
    let calldata_args = constructor.abi_encode_input_raw(&arg_values)?;

    let mut constructor_calldata = Vec::from(stylus_constructorCall::SELECTOR);
    constructor_calldata.extend(calldata_args);

    let bytecode = super::contract_deployment_calldata(contract.code());
    let tx_calldata = if contract.suggest_fee().is_zero() {
        CargoStylusFactory::deployInitCall {
            bytecode: bytecode.into(),
            constructorCalldata: constructor_calldata.into(),
        }
        .abi_encode()
    } else {
        CargoStylusFactory::deployActivateInitCall {
            bytecode: bytecode.into(),
            constructorCalldata: constructor_calldata.into(),
            constructorValue: constructor_value,
        }
        .abi_encode()
    };

    Ok(FactoryArgs {
        address,
        tx_value,
        tx_calldata,
    })
}

/// Deploys, activates, and initializes the contract using the Stylus factory.
pub async fn deploy(
    cfg: &DeployConfig,
    factory: FactoryArgs,
    sender: H160,
    client: &SignerClient,
) -> Result<()> {
    if cfg.check_config.common_cfg.verbose {
        greyln!(
            "deploying contract using factory at address: {}",
            factory.address.debug_lavender()
        );
    }

    let tx = Eip1559TransactionRequest::new()
        .to(factory.address)
        .from(sender)
        .value(alloy_ethers_typecast::alloy_u256_to_ethers(
            factory.tx_value,
        ))
        .data(factory.tx_calldata);

    let gas = client
        .estimate_gas(&TypedTransaction::Eip1559(tx.clone()), None)
        .await?;
    if cfg.check_config.common_cfg.verbose || cfg.estimate_gas {
        super::print_gas_estimate("factory deploy, activate, and init", client, gas).await?;
    }
    if cfg.estimate_gas {
        return Ok(());
    }

    let receipt = super::run_tx(
        "deploy_activate_init",
        tx,
        Some(gas),
        cfg.check_config.common_cfg.max_fee_per_gas_gwei,
        client,
        cfg.check_config.common_cfg.verbose,
    )
    .await?;
    let contract = get_address_from_receipt(&receipt)?;
    let address = contract.debug_lavender();

    if cfg.check_config.common_cfg.verbose {
        let gas = super::format_gas(receipt.gas_used.unwrap_or_default());
        greyln!(
            "deployed code at address: {address} {} {gas}",
            "with".grey()
        );
    } else {
        greyln!("deployed code at address: {address}");
    }
    let tx_hash = receipt.transaction_hash.debug_lavender();
    greyln!("deployment tx hash: {tx_hash}");
    super::print_cache_notice(contract);
    Ok(())
}

/// Gets the Stylus-contract address that was deployed using the factory.
fn get_address_from_receipt(receipt: &TransactionReceipt) -> Result<H160> {
    for log in receipt.logs.iter() {
        if let Some(topic) = log.topics.first() {
            if topic.0 == CargoStylusFactory::ContractDeployed::SIGNATURE_HASH {
                let address = log
                    .topics
                    .get(1)
                    .ok_or(eyre!("address missing from ContractDeployed log"))?;
                return Ok(ethers::types::Address::from_slice(
                    &address.as_bytes()[12..32],
                ));
            }
        }
    }
    Err(eyre!("contract address not found in receipt"))
}
