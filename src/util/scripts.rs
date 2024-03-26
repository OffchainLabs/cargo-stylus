use ethers::{
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer, Wallet},
    types::Address,
};
use eyre::{bail, eyre, OptionExt, Result};

use crate::util::{
    find_parent_project_root, make_absolute_relative_to, move_to_parent_project_root,
    read_and_trim_line_from_file,
};
use crate::{CheckConfig, DeployConfig, KeystoreOpts, TxSendingOpts};
use serde::Deserialize;
use tokio::fs;

use std::{collections::BTreeMap, str::FromStr, sync::Arc};

const STYLUS_CONFIG_FILENAME: &str = "Stylus.toml";

#[derive(Debug, Deserialize)]
pub enum PrivateKeySource {
    /// Private key literal
    #[serde(rename = "private_key")]
    Literal(String),

    /// Path to a private key, can be relative
    #[serde(rename = "private_key_path")]
    FilePath(String),
}

impl PrivateKeySource {
    pub fn load(self) -> Result<String> {
        let project_root = find_parent_project_root(None)?;

        Ok(match self {
            PrivateKeySource::Literal(private_key) => private_key,
            PrivateKeySource::FilePath(private_key_path) => {
                let pk_path = make_absolute_relative_to(private_key_path, project_root)?;
                read_and_trim_line_from_file(pk_path)?
            }
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfig {
    /// RPC URL
    pub rpc_url: String,

    /// Private key or path to one
    #[serde(flatten)]
    pub private_key_source: PrivateKeySource,

    /// Additional variables
    #[serde(flatten)]
    pub additional_variables: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylusConfig {
    /// Networks' configs
    pub networks: BTreeMap<String, NetworkConfig>,
}

#[derive(Debug)]
pub struct Config {
    pub client: Arc<SignerMiddleware<Provider<Http>, Wallet<SigningKey>>>,
    pub additional_variables: BTreeMap<String, String>,
}

/// Load bare Stylus config from `Stylus.toml` of the current Stylus project
pub async fn load_stylus_config() -> Result<StylusConfig> {
    let project_root = find_parent_project_root(None)?;
    let stylus_config_path = project_root.join(STYLUS_CONFIG_FILENAME);
    if !fs::try_exists(&stylus_config_path).await? {
        bail!(
            "{} not found at {}",
            STYLUS_CONFIG_FILENAME,
            project_root.display()
        );
    }
    let stylus_config_str = fs::read_to_string(&stylus_config_path).await?;
    Ok(toml::from_str(&stylus_config_str)?)
}

/// Load a specific network's bare config
/// from the Stylus config of the current Stylus project
pub async fn load_network_config_for(network: &str) -> Result<NetworkConfig> {
    let mut stylus_config: StylusConfig = load_stylus_config().await?;

    stylus_config
        .networks
        // NOTE: `.remove`-ing instead of `.get`-ing to avoid `.clone()`-ing
        .remove(&network.to_lowercase())
        .ok_or_eyre(format!("No configuration for network {}", network))
}

/// Load and prepare the ready-to-use config for a given network
pub async fn load_config_for(network: &str) -> Result<Config> {
    let NetworkConfig {
        rpc_url,
        private_key_source,
        additional_variables,
    } = load_network_config_for(network).await?;

    let private_key = private_key_source.load()?;

    // Prepare client
    let provider = Provider::<Http>::try_from(rpc_url.clone())?;
    let wallet = LocalWallet::from_str(&private_key)?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let client = Arc::new(SignerMiddleware::new(
        provider,
        wallet.clone().with_chain_id(chain_id),
    ));

    Ok(Config {
        client,
        additional_variables,
    })
}

/// Helper function, wrapping `cargo_stylus::deploy::deploy`,
/// used to deploy the main contract of the current Stylus project
pub async fn deploy(network: &str) -> Result<Address> {
    let NetworkConfig {
        rpc_url,
        private_key_source,
        additional_variables: _,
    } = load_network_config_for(network).await?;

    let private_key = private_key_source.load()?;

    // Prepare client
    let provider = Provider::<Http>::try_from(rpc_url.clone())?;
    let wallet = LocalWallet::from_str(&private_key)?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let client = Arc::new(SignerMiddleware::new(
        provider,
        wallet.clone().with_chain_id(chain_id),
    ));

    // Deploy and activate contract
    let addr = wallet.address();
    let nonce = client
        .get_transaction_count(addr, None)
        .await
        .map_err(|e| eyre!("could not get nonce for address {addr}: {e}"))?;

    let expected_program_address = ethers::utils::get_contract_address(wallet.address(), nonce);

    // NOTE: reusing `cargo_stylus::deploy::deploy`'s CLI arguments
    let cfg = DeployConfig {
        check_cfg: CheckConfig {
            endpoint: rpc_url,
            wasm_file_path: None,
            expected_program_address,
            private_key_path: None,
            private_key: Some(private_key),
            keystore_opts: KeystoreOpts {
                keystore_path: None,
                keystore_password_path: None,
            },
            nightly: false,
            skip_contract_size_check: false,
        },
        estimate_gas_only: false,
        mode: None,
        activate_program_address: None,
        tx_sending_opts: TxSendingOpts {
            dry_run: false,
            output_tx_data_to_dir: None,
        },
    };

    // NOTE: `cargo_stylus::deploy::deploy` uses cwd
    move_to_parent_project_root()?;
    crate::deploy::deploy(cfg).await?;

    Ok(expected_program_address)
}
