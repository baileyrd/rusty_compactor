mod cmd_compress;
mod cmd_config;
mod cmd_hook;
mod cmd_run;
mod cmd_stats;

use clap::{Parser, Subcommand};

/// rusty_compactor: cuts LLM token usage for AI coding agents by compacting
/// command output (à la rtk) and compressing prose responses (à la caveman).
#[derive(Parser)]
#[command(name = "rusty_compactor", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a command and print its compacted output (drop-in wrapper).
    Run(cmd_run::RunArgs),
    /// Compress text into terse prose while preserving code/commands verbatim.
    Compress(cmd_compress::CompressArgs),
    /// Manage the Claude Code PreToolUse hook that auto-rewrites Bash commands.
    #[command(subcommand)]
    Hook(cmd_hook::HookCommand),
    /// Show aggregated token/byte savings from past runs.
    Stats(cmd_stats::StatsArgs),
    /// Manage the TOML config file.
    #[command(subcommand)]
    Config(cmd_config::ConfigCommand),
}

fn main() {
    let cli = Cli::parse();
    // `run` propagates the wrapped command's own exit code; every other
    // subcommand exits 0 on success so scripting against them is predictable.
    let result: anyhow::Result<i32> = match cli.command {
        Command::Run(args) => cmd_run::run(args),
        Command::Compress(args) => cmd_compress::run(args).map(|_| 0),
        Command::Hook(cmd) => cmd_hook::run(cmd).map(|_| 0),
        Command::Stats(args) => cmd_stats::run(args).map(|_| 0),
        Command::Config(cmd) => cmd_config::run(cmd).map(|_| 0),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("rusty_compactor: {e:#}");
            std::process::exit(1);
        }
    }
}
