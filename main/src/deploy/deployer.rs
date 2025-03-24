// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::{
    check::ContractCheck,
    deploy::calculate_fee_per_gas,
    macros::*,
    util::color::{Color, DebugColor, GREY},
    DeployConfig,
};
use alloy::{
    dyn_abi::{DynSolValue, JsonAbiExt, Specifier},
    json_abi::{Constructor, StateMutability},
    primitives::{utils::format_ether, Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::{TransactionInput, TransactionReceipt, TransactionRequest},
    sol,
    sol_types::{SolCall, SolEvent},
};
use eyre::{bail, eyre, Context, Result};

sol! {
    #[sol(rpc)]
    interface StylusDeployer {
        event ContractDeployed(address deployedContract);

        function deploy(
            bytes calldata bytecode,
            bytes calldata initData,
            uint256 initValue,
            bytes32 salt
        ) public payable returns (address);
    }

    function stylus_constructor();
}

pub struct DeployerArgs {
    /// Factory address
    address: Address,
    /// Value to be sent in the tx
    tx_value: U256,
    /// Calldata to be sent in the tx
    tx_calldata: Vec<u8>,
}

/// Parses the constructor arguments and returns the data to deploy the contract using the deployer.
pub async fn parse_constructor_args(
    cfg: &DeployConfig,
    constructor: &Constructor,
    contract: &ContractCheck,
) -> Result<DeployerArgs> {
    let Some(address) = cfg.experimental_deployer_address else {
        bail!("this contract has a constructor so it requires the deployer address for deployment");
    };

    if !cfg.experimental_constructor_value.is_zero() {
        greyln!(
            "value sent to the constructor: {} {GREY}Ether",
            format_ether(cfg.experimental_constructor_value).debug_lavender()
        );
    }
    let constructor_value = cfg.experimental_constructor_value;
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
    let provider = ProviderBuilder::new()
        .on_builtin(&cfg.check_config.common_cfg.endpoint)
        .await?;
    let deployer = StylusDeployer::new(Address::ZERO, provider);
    let deploy_call = deployer.deploy(
        bytecode.into(),
        constructor_calldata.into(),
        constructor_value,
        cfg.experimental_deployer_salt,
    );

    let tx_calldata = deploy_call.calldata().to_vec();
    Ok(DeployerArgs {
        address,
        tx_value,
        tx_calldata,
    })
}

/// Deploys, activates, and initializes the contract using the Stylus deployer.
pub async fn deploy(
    cfg: &DeployConfig,
    deployer: DeployerArgs,
    sender: Address,
    provider: &impl Provider,
) -> Result<()> {
    if cfg.check_config.common_cfg.verbose {
        greyln!(
            "deploying contract using deployer at address: {}",
            deployer.address.debug_lavender()
        );
    }
    let tx = TransactionRequest::default()
        .to(deployer.address)
        .from(sender)
        .value(deployer.tx_value)
        .input(TransactionInput::new(deployer.tx_calldata.into()));

    let gas = provider
        .estimate_gas(&tx)
        .await
        .wrap_err("deployment failed during gas estimation")?;

    let gas_price = provider.get_gas_price().await?;

    if cfg.check_config.common_cfg.verbose || cfg.estimate_gas {
        super::print_gas_estimate("deployer deploy, activate, and init", gas, gas_price).await?;
    }
    if cfg.estimate_gas {
        return Ok(());
    }

    let fee_per_gas = calculate_fee_per_gas(&cfg.check_config.common_cfg, gas_price)?;

    let receipt = super::run_tx(
        "deploy_activate_init",
        tx,
        Some(gas),
        fee_per_gas,
        provider,
        cfg.check_config.common_cfg.verbose,
    )
    .await?;
    let contract = get_address_from_receipt(&receipt)?;
    let address = contract.debug_lavender();

    if cfg.check_config.common_cfg.verbose {
        let gas = super::format_gas(receipt.gas_used);
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

/// Gets the Stylus-contract address that was deployed using the deployer.
fn get_address_from_receipt(receipt: &TransactionReceipt) -> Result<Address> {
    let receipt = receipt.clone().into_inner();
    for log in receipt.logs().iter() {
        if let Some(topic) = log.topics().first() {
            if topic.0 == StylusDeployer::ContractDeployed::SIGNATURE_HASH {
                if log.data().data.len() != 32 {
                    bail!("address missing from ContractDeployed log");
                }
                return Ok(Address::from_slice(&log.data().data[12..32]));
            }
        }
    }
    Err(eyre!("contract address not found in receipt"))
}
