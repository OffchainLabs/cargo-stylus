// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::{
    check::ArbWasm::ArbWasmErrors,
    constants::ARB_WASM_ADDRESS,
    macros::*,
    project,
    util::color::{Color, GREY, LAVENDER, MINT, PINK, YELLOW},
    CheckConfig, DataFeeOpts,
};
use alloy::{
    contract::Error,
    primitives::{utils::parse_ether, Address, Bytes, B256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::state::{AccountOverride, StateOverride},
    sol,
};
use bytesize::ByteSize;
use eyre::{bail, eyre, ErrReport, Result, WrapErr};
use std::path::PathBuf;

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);

        function stylusVersion() external view returns (uint16 version);

        function codehashVersion(bytes32 codehash) external view returns (uint16 version);

        error ProgramNotWasm();
        error ProgramNotActivated();
        error ProgramNeedsUpgrade(uint16 version, uint16 stylusVersion);
        error ProgramExpired(uint64 ageInSeconds);
        error ProgramUpToDate();
        error ProgramKeepaliveTooSoon(uint64 ageInSeconds);
        error ProgramInsufficientValue(uint256 have, uint256 want);
    }
}

/// Checks that a contract is valid and can be deployed onchain.
/// Returns whether the WASM is already up-to-date and activated onchain, and the data fee.
pub async fn check(cfg: &CheckConfig) -> Result<ContractCheck> {
    if cfg.common_cfg.endpoint == "https://stylus-testnet.arbitrum.io/rpc" {
        let version = "cargo stylus version 0.2.1".to_string().red();
        bail!("The old Stylus testnet is no longer supported.\nPlease downgrade to {version}",);
    }

    let verbose = cfg.common_cfg.verbose;
    let (wasm, project_hash) = cfg.build_wasm().wrap_err("failed to build wasm")?;

    if verbose {
        greyln!("reading wasm file at {}", wasm.to_string_lossy().lavender());
    }

    let (wasm_file_bytes, code) =
        project::compress_wasm(&wasm, project_hash).wrap_err("failed to compress WASM")?;

    greyln!("contract size: {}", format_file_size(code.len(), 16, 24));

    if verbose {
        greyln!(
            "wasm size: {}",
            format_file_size(wasm_file_bytes.len(), 96, 128)
        );
        greyln!("connecting to RPC: {}", &cfg.common_cfg.endpoint.lavender());
    }

    // Check if the contract already exists.
    let provider = ProviderBuilder::new()
        .on_builtin(&cfg.common_cfg.endpoint)
        .await?;
    let codehash = alloy::primitives::keccak256(&code);

    if contract_exists(codehash, &provider).await? {
        return Ok(ContractCheck::Active { code });
    }

    let address = cfg.contract_address.unwrap_or(Address::random());
    let fee = check_activate(code.clone().into(), address, &cfg.data_fee, &provider).await?;
    Ok(ContractCheck::Ready { code, fee })
}

/// Whether a contract is active, or needs activation.
#[derive(PartialEq)]
pub enum ContractCheck {
    /// Contract already exists onchain.
    Active { code: Vec<u8> },
    /// Contract can be activated with the given data fee.
    Ready { code: Vec<u8>, fee: U256 },
}

impl ContractCheck {
    pub fn code(&self) -> &[u8] {
        match self {
            Self::Active { code, .. } => code,
            Self::Ready { code, .. } => code,
        }
    }
    pub fn suggest_fee(&self) -> U256 {
        match self {
            Self::Active { .. } => U256::default(),
            Self::Ready { fee, .. } => *fee,
        }
    }
}

impl CheckConfig {
    fn build_wasm(&self) -> Result<(PathBuf, [u8; 32])> {
        if let Some(wasm) = self.wasm_file.clone() {
            return Ok((wasm, [0u8; 32]));
        }
        project::build_wasm_from_features(
            self.common_cfg.features.clone(),
            self.common_cfg.source_files_for_project_hash.clone(),
        )
    }
}

/// Pretty-prints a file size based on its limits.
pub fn format_file_size(len: usize, mid: u64, max: u64) -> String {
    let len = ByteSize::b(len as u64);
    let mid = ByteSize::kib(mid);
    let max = ByteSize::kib(max);
    let color = if len <= mid {
        MINT
    } else if len <= max {
        YELLOW
    } else {
        PINK
    };
    format!("{color}{}{GREY} ({} bytes)", len, len.as_u64())
}

/// Pretty-prints a data fee.
fn format_data_fee(fee: U256) -> String {
    let Ok(fee): Result<u64, _> = (fee / U256::from(1e9)).try_into() else {
        return ("???").red();
    };
    let fee: f64 = fee as f64 / 1e9;
    let text = format!("{fee:.6} ETH");
    if fee <= 5e14 {
        text.mint()
    } else if fee <= 5e15 {
        text.yellow()
    } else {
        text.pink()
    }
}

pub struct EthCallError {
    pub data: Vec<u8>,
    pub msg: String,
}

impl From<EthCallError> for ErrReport {
    fn from(value: EthCallError) -> Self {
        eyre!(value.msg)
    }
}

/// Checks whether a contract has already been activated with the most recent version of Stylus.
async fn contract_exists(codehash: B256, provider: &impl Provider) -> Result<bool> {
    let arbwasm = ArbWasm::new(ARB_WASM_ADDRESS, provider);
    match arbwasm.codehashVersion(codehash).call().await {
        Ok(_) => Ok(true),
        Err(e) => {
            let Error::TransportError(tperr) = e else {
                bail!("failed to send cache bid tx: {:?}", e)
            };
            let Some(err_resp) = tperr.as_error_resp() else {
                bail!("no error payload received in response: {:?}", tperr)
            };
            let Some(errs) = err_resp.as_decoded_error::<ArbWasmErrors>(true) else {
                bail!("failed to decode CacheManager error: {:?}", err_resp)
            };
            use ArbWasmErrors as A;
            match errs {
                A::ProgramNotActivated(_) | A::ProgramNeedsUpgrade(_) | A::ProgramExpired(_) => {
                    Ok(false)
                }
                _ => bail!("unexpected ArbWasm error"),
            }
        }
    }
}

/// Checks contract activation, returning the data fee.
pub async fn check_activate(
    code: Bytes,
    address: Address,
    opts: &DataFeeOpts,
    provider: &impl Provider,
) -> Result<U256> {
    let arbwasm = ArbWasm::new(ARB_WASM_ADDRESS, provider);
    let random_sender_addr = Address::random();
    let spoofed_sender_account = AccountOverride::default().with_balance(U256::MAX);
    let spoofed_code = AccountOverride::default().with_code(code.clone());
    let mut state_override = StateOverride::default();
    state_override.insert(address, spoofed_code);
    state_override.insert(random_sender_addr, spoofed_sender_account);
    let active_call = arbwasm
        .activateProgram(address)
        .state(state_override)
        .from(random_sender_addr)
        .value(parse_ether("1").unwrap());

    let result = match active_call.call().await {
        Ok(result) => result,
        Err(e) => {
            if e.to_string().contains("pay_for_memory_grow") {
                bail!(
                    "Contract could not be activated as it is missing an entrypoint. \
                Please ensure that your contract has an #[entrypoint] defined on your main struct"
                );
            } else {
                return Err(e.into());
            }
        }
    };
    let ArbWasm::activateProgramReturn {
        dataFee: data_fee, ..
    } = result;

    let bump = opts.data_fee_bump_percent;
    let adjusted_data_fee = data_fee * U256::from(100 + bump) / U256::from(100);
    greyln!(
        "wasm data fee: {} {GREY}(originally {}{GREY} with {LAVENDER}{bump}%{GREY} bump)",
        format_data_fee(adjusted_data_fee),
        format_data_fee(data_fee)
    );

    Ok(adjusted_data_fee)
}
