use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
};

use eyre::{bail, eyre, Context, OptionExt, Result};

use crate::{util, ScriptNewConfig, ScriptRunConfig};

mod templates;

pub async fn new(config: ScriptNewConfig) -> Result<()> {
    println!("Adding new script: {:?}", config);

    let cwd = env::current_dir()?;
    let project_root: &Path =
        &util::discover_project_root_from_path(cwd)?.ok_or_eyre("Could not find Cargo.toml")?;
    println!("Found to project root: {}", project_root.display());

    let script_dir: &Path = &project_root.join("scripts").join(&config.path);

    fs::create_dir_all(script_dir)?;
    env::set_current_dir(script_dir)?;

    println!(
        "Moved down into project's script dir: {}",
        script_dir.display()
    );

    let template = templates::rust::basic_template(
        config
            .path
            .to_str()
            .ok_or_eyre("Could not convert path to string")?
            .to_string(),
        script_dir,
    )?;
    templates::realise(template)?;

    Ok(())
}

pub async fn run(config: ScriptRunConfig) -> Result<()> {
    println!("Running script {}", config.name);

    let cwd = env::current_dir()?;
    let project_root: &Path =
        &util::discover_project_root_from_path(cwd)?.ok_or_eyre("Could not find Cargo.toml")?;
    println!("Found to project root: {}", project_root.display());

    let script_dir: &Path = &project_root.join("scripts").join(&config.name);

    env::set_current_dir(script_dir).wrap_err(eyre!(
        "Could not find script {}, does it exist at {}?",
        &config.name,
        script_dir.display(),
    ))?;

    println!(
        "Moved down into project's script dir: {}",
        script_dir.display()
    );

    let mut cmd = Command::new("make");
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let output = cmd
        .output()
        .map_err(|e| eyre!("failed to execute `make`: {e}"))?;

    if !output.status.success() {
        bail!("Run script {} failed: {:?}", &config.name, output);
    }

    Ok(())
}
