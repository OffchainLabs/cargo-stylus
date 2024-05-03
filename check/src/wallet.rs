// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md
use std::io::{BufRead, BufReader};
use std::str::FromStr;

use ethers::signers::LocalWallet;
use eyre::{bail, eyre};

use crate::CheckConfig;

/// Loads a wallet for signing transactions either from a private key file path.
/// or a keystore along with a keystore password file.
pub fn load(cfg: &CheckConfig) -> eyre::Result<LocalWallet> {
    let CheckConfig {
        private_key_path,
        keystore_opts,
        private_key,
        ..
    } = cfg;
    if private_key.is_some() && private_key_path.is_some() {
        bail!("cannot provide both --private-key and --private-key-path");
    }
    let priv_key_flag_set = private_key.is_some() || private_key_path.is_some();
    if priv_key_flag_set
        && (keystore_opts.keystore_password_path.is_some() && keystore_opts.keystore_path.is_some())
    {
        bail!("must provide either (--private-key-path or --private-key) or (--keystore-path and --keystore-password-path)");
    }

    match (private_key.as_ref(), private_key_path.as_ref()) {
        (Some(privkey), None) => {
            return LocalWallet::from_str(privkey)
                .map_err(|e| eyre!("could not parse private key: {e}"));
        }
        (None, Some(priv_key_path)) => {
            let privkey = read_secret_from_file(priv_key_path)?;
            return LocalWallet::from_str(&privkey)
                .map_err(|e| eyre!("could not parse private key: {e}"));
        }
        (None, None) => {}
        _ => unreachable!(),
    }
    let keystore_password_path = keystore_opts
        .keystore_password_path
        .as_ref()
        .ok_or(eyre!("no keystore password path provided"))?;
    let keystore_pass = read_secret_from_file(keystore_password_path)?;
    let keystore_path = keystore_opts
        .keystore_path
        .as_ref()
        .ok_or(eyre!("no keystore path provided"))?;
    LocalWallet::decrypt_keystore(keystore_path, keystore_pass)
        .map_err(|e| eyre!("could not decrypt keystore: {e}"))
}

fn read_secret_from_file(fpath: &str) -> eyre::Result<String> {
    let f = std::fs::File::open(fpath)
        .map_err(|e| eyre!("could not open file at path: {fpath}: {e}"))?;
    let mut buf_reader = BufReader::new(f);
    let mut secret = String::new();
    buf_reader
        .read_line(&mut secret)
        .map_err(|e| eyre!("could not read secret from file at path {fpath}: {e}"))?;
    Ok(secret.trim().to_string())
}
