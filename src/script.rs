use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
};

use eyre::{bail, eyre, OptionExt, Result};

use crate::{util, ScriptNewConfig};

pub async fn new(config: ScriptNewConfig) -> Result<()> {
    println!("Adding new script: {:?}", config);

    let cwd = env::current_dir()?;
    let project_root: &Path =
        &util::discover_project_root_from_path(cwd)?.ok_or_eyre("Could not find Cargo.toml")?;
    println!("Found to project root: {}", project_root.display());

    let script_dir: &Path = &project_root.join("scripts");

    fs::create_dir_all(script_dir)?;
    env::set_current_dir(script_dir)?;

    println!(
        "Moved down into project's script dir: {}",
        script_dir.display()
    );

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
