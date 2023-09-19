// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use std::time::Duration;

use ethers::{prelude::*, providers::Provider};
use eyre::{eyre, Context, Result};

pub fn new_provider(url: &str) -> Result<Provider<Http>> {
    let mut provider =
        Provider::<Http>::try_from(url).wrap_err_with(|| eyre!("failed to init http provider"))?;

    provider.set_interval(Duration::from_millis(250));
    Ok(provider)
}
