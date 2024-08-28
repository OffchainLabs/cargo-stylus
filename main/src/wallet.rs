// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use crate::util::text::{self, decode0x};
use crate::AuthOpts;
use alloy_primitives::FixedBytes;
use alloy_provider::network::EthereumWallet;
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use ethers::signers::LocalWallet;
use eyre::{eyre, Context, Result};
use std::fs;

/// Loads a wallet for signing transactions.
impl AuthOpts {
    pub fn wallet(&self) -> Result<LocalWallet> {
        macro_rules! wallet {
            ($key:expr) => {{
                let key = text::decode0x($key).wrap_err("invalid private key")?;
                LocalWallet::from_bytes(&key).wrap_err("invalid private key")
            }};
        }

        if let Some(key) = &self.private_key {
            if key.is_empty() {
                return Err(eyre!("empty private key"));
            }
            return wallet!(key);
        }

        if let Some(file) = &self.private_key_path {
            let key = fs::read_to_string(file).wrap_err("could not open private key file")?;
            return wallet!(key);
        }

        let keystore = self.keystore_path.as_ref().ok_or(eyre!("no keystore"))?;
        let password = self
            .keystore_password_path
            .as_ref()
            .map(fs::read_to_string)
            .unwrap_or(Ok("".into()))?;

        LocalWallet::decrypt_keystore(keystore, password).wrap_err("could not decrypt keystore")
    }

    pub fn alloy_wallet(&self, chain_id: u64) -> Result<EthereumWallet> {
        if let Some(key) = &self.private_key {
            if key.is_empty() {
                return Err(eyre!("empty private key"));
            }
            let priv_key_bytes: FixedBytes<32> = FixedBytes::from_slice(decode0x(key)?.as_slice());
            let signer =
                PrivateKeySigner::from_bytes(&priv_key_bytes)?.with_chain_id(Some(chain_id));
            return Ok(EthereumWallet::new(signer));
        }

        if let Some(file) = &self.private_key_path {
            let key = fs::read_to_string(file).wrap_err("could not open private key file")?;
            let priv_key_bytes: FixedBytes<32> = FixedBytes::from_slice(decode0x(key)?.as_slice());
            let signer =
                PrivateKeySigner::from_bytes(&priv_key_bytes)?.with_chain_id(Some(chain_id));
            return Ok(EthereumWallet::new(signer));
        }

        let keystore = self.keystore_path.as_ref().ok_or(eyre!("no keystore"))?;
        let password = self
            .keystore_password_path
            .as_ref()
            .map(fs::read_to_string)
            .unwrap_or(Ok("".into()))?;

        let signer =
            PrivateKeySigner::decrypt_keystore(keystore, password)?.with_chain_id(Some(chain_id));
        Ok(EthereumWallet::new(signer))
    }
}
