// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

use clap::Parser;
use eyre::Result;
use tokio::runtime::Builder;

use cargo_stylus::{c, main_impl, CargoCli, Subcommands};

fn main() -> Result<()> {
    let args = match CargoCli::parse() {
        CargoCli::Stylus(args) => args,
        CargoCli::CGen(args) => {
            return c::gen::c_gen(args.input, args.out_dir);
        }
    };

    // use the current thread for replay
    let mut runtime = match &args.command {
        Subcommands::Replay(_) => Builder::new_current_thread(),
        _ => Builder::new_multi_thread(),
    };

    let runtime = runtime.enable_all().build()?;
    runtime.block_on(main_impl(args))
}
