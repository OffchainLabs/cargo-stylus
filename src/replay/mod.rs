// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

use crate::{color::Color, util, ReplayConfig, TraceConfig};
use eyre::{bail, eyre, Result};
use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
};

use trace::Trace;

mod hostio;
mod trace;

pub async fn replay(config: ReplayConfig) -> Result<()> {
    if !config.child {
        let mut cmd = util::new_command("rust-gdb"); // TODO: fall back to gdb
        cmd.arg("-ex=set breakpoint pending on");
        cmd.arg("-ex=b user_entrypoint");
        cmd.arg("-ex=r");
        cmd.arg("--args");

        for arg in std::env::args() {
            cmd.arg(arg);
        }
        cmd.arg("--child");
        let err = cmd.exec();

        bail!("failed to exec gdb {}", err);
    }

    let provider = util::new_provider(&config.endpoint)?;
    let trace = Trace::new(provider, config.tx).await?;

    build_so(&config.project, config.stable_rust)?;
    let so = find_so(&config.project)?;

    // TODO: don't assume the contract is top-level
    let args_len = trace.tx.input.len();

    unsafe {
        *hostio::FRAME.lock() = Some(trace.reader());

        type Entrypoint = unsafe extern "C" fn(usize) -> usize;
        let lib = libloading::Library::new(so)?;
        let main: libloading::Symbol<Entrypoint> = lib.get(b"user_entrypoint")?;

        match main(args_len) {
            0 => println!("call completed successfully"),
            1 => println!("call reverted"),
            x => println!("call exited with unknown status code: {}", x.red()),
        }
    }
    Ok(())
}

pub async fn trace(config: TraceConfig) -> Result<()> {
    let provider = util::new_provider(&config.endpoint)?;
    let trace = Trace::new(provider, config.tx).await?;
    println!("{}", trace.json);
    Ok(())
}

pub fn build_so(path: &Path, stable: bool) -> Result<()> {
    let mut cargo = util::new_command("cargo");

    if !stable {
        cargo.arg("+nightly");
    }
    cargo
        .current_dir(path)
        .arg("build")
        .arg("--lib")
        .arg("--target")
        .arg(rustc_host::from_cli()?)
        .output()?;
    Ok(())
}

pub fn find_so(project: &Path) -> Result<PathBuf> {
    let triple = rustc_host::from_cli()?;
    let so_dir = project.join(format!("target/{triple}/debug/"));
    let so_dir = std::fs::read_dir(&so_dir)
        .map_err(|e| eyre!("failed to open {}: {e}", so_dir.to_string_lossy()))?
        .filter_map(|r| r.ok())
        .map(|r| r.path())
        .filter(|r| r.is_file());

    let mut file: Option<PathBuf> = None;
    for entry in so_dir {
        let Some(ext) = entry.file_name() else {
            continue;
        };
        let ext = ext.to_string_lossy();

        if ext.contains(".so") {
            if let Some(other) = file {
                let other = other.file_name().unwrap().to_string_lossy();
                bail!("more than one .so found: {ext} and {other}",);
            }
            file = Some(entry);
        }
    }
    let Some(file) = file else {
        bail!("failed to find .so");
    };
    Ok(file)
}
