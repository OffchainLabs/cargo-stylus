use std::{
    env,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use eyre::{bail, eyre, Result};

use crate::ScriptNewConfig;

pub async fn new(config: ScriptNewConfig) -> Result<()> {
    println!("Adding new script: {:?}", config);

    // TODO: check if at project root

    let script_dir: &Path = &PathBuf::from("./scripts/");

    fs::create_dir_all(script_dir)?;
    env::set_current_dir(script_dir)?;

    let mut cmd = Command::new("cargo");
    cmd.stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .arg("new")
        .arg("--bin")
        .arg(config.path);

    let output = cmd
        .output()
        .map_err(|e| eyre!("failed to execute cargo new command: {e}"))?;

    if !output.status.success() {
        bail!("Create new script failed: {:?}", output);
    }

    Ok(())
}
