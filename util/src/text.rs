// Copyright 2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use eyre::Result;

pub fn decode0x<T: AsRef<str>>(text: T) -> Result<Vec<u8>> {
    let text = text.as_ref();
    let text = text.strip_prefix("0x").unwrap_or(text);
    Ok(hex::decode(text)?)
}
