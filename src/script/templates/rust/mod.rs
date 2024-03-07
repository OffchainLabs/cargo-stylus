#![allow(clippy::useless_format)]

use std::path::Path;

use eyre::Result;
use indoc::indoc;

use super::Template;

pub fn basic_template(name: String, base: impl AsRef<Path>) -> Result<Template> {
    let base = base.as_ref();

    Ok(vec![
        (
            base.join("Cargo.toml"),
            format!(
                indoc! {r#"
                    [package]
                    name = "{}"
                    version = "0.1.0"
                    edition = "2021"

                    [dependencies]
                    alloy-primitives = "0.4.0"
                    eyre = "0.6.12"
                "#},
                name
            ),
        ),
        (
            base.join("src/main.rs"),
            format!(
                indoc! {r#"
                    use eyre::Result;

                    pub fn main() -> Result<()> {{
                        println!("{{}}", "{}");
                        Ok(())
                    }}
                "#},
                "Hello"
            ),
        ),
        (
            base.join("Makefile"),
            // NOTE: beware of the tab literals
            format!(
                indoc! {r#"
                    run: build
                    	cargo run

                    build:
                    	cargo build

                    love:
                    	@echo "not war"
                "#}
            ),
        ),
    ])
}
