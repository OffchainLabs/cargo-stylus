use std::{fs, path::PathBuf};

use eyre::{Context, OptionExt, Result};

pub mod rust;

/// Type alias for templates - file names paired up with their respective content
pub type Template = Vec<(PathBuf, String)>;

/// Bring a template to life in a given directory
pub fn realise(template: Template) -> Result<()> {
    template.into_iter().try_for_each(|(path, contents)| {
        fs::create_dir_all(
            path.parent()
                .ok_or_eyre(format!("Could not get parent of {}", path.display()))?,
        )
        .wrap_err(format!(
            "Could not create needed directories for {}",
            path.display()
        ))?;
        fs::write(&path, contents).wrap_err(format!("Failed to write file {}", path.display()))?;
        println!("Created file {}", path.display());
        Ok(())
    })
}
