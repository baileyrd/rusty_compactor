use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use rc_core::Config;
use rc_engine::UserRuleFile;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print the resolved configuration (defaults merged with any found file).
    Show,
    /// Write a default config file.
    Init(InitArgs),
}

#[derive(Args)]
pub struct InitArgs {
    /// Write to ~/.rusty_compactor/config.toml instead of ./.rusty_compactor.toml.
    #[arg(long)]
    pub user: bool,
    /// Overwrite an existing config file.
    #[arg(long)]
    pub force: bool,
}

pub fn run(cmd: ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Show => show(),
        ConfigCommand::Init(args) => init(args),
    }
}

fn show() -> Result<()> {
    let cfg = Config::load();
    print!("{}", toml::to_string_pretty(&cfg)?);
    Ok(())
}

fn init(args: InitArgs) -> Result<()> {
    let path: PathBuf = if args.user {
        Config::user_config_path()
    } else {
        Config::project_config_path()
    };
    if path.exists() && !args.force {
        anyhow::bail!(
            "{} already exists (pass --force to overwrite)",
            path.display()
        );
    }
    Config::default().save_to(&path)?;
    println!("Wrote default config to {}", path.display());
    Ok(())
}

/// Loads the user-configured custom rules file (if any), for merging into
/// the built-in [`rc_engine::RuleTable`].
pub fn load_custom_rules(cfg: &Config) -> Option<UserRuleFile> {
    let path = cfg.custom_rules_file.as_ref()?;
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading custom rules file {path}"))
        .ok()?;
    toml::from_str(&text)
        .with_context(|| format!("parsing custom rules file {path}"))
        .ok()
}
