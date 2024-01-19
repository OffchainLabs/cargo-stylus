// Copyright 2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use cargo_stylus_util::sys;
use eyre::{bail, Result};
use std::os::unix::process::CommandExt;

pub fn replay() -> Result<()> {
    println!("\nabout to replay\n");
    check_exists()
}

pub fn trace() -> Result<()> {
    println!("\nabout to trace\n");
    check_exists()?;

    let mut cmd = sys::new_command("cargo-stylus-replay");
    cmd.arg("trace");

    for arg in std::env::args().skip_while(|x| x != "trace").skip(1) {
        cmd.arg(arg);
    }

    Err(cmd.exec().into())
}

fn check_exists() -> Result<()> {
    if !sys::command_exists("cargo-stylus-replay") {
        bail!("cargo-stylus-replay not installed");
    }
    Ok(())
}
