use std::io::Read as _;
use std::process::Command as Process;

use anyhow::{Context, Result};
use clap::Args;
use rc_core::{CommandOutput, Config};
use rc_engine::RuleTable;

use crate::cmd_config::load_custom_rules;

#[derive(Args)]
pub struct RunArgs {
    /// Print what would be run and the compaction rule that would apply,
    /// without actually executing the command.
    #[arg(long)]
    pub dry_run: bool,
    /// Skip compaction entirely; just execute and print raw output.
    #[arg(long)]
    pub no_compact: bool,
    /// Don't execute `command` — instead compact whatever is piped into
    /// stdin, matching rules as if it were that command's output. Useful
    /// for replaying a saved log, or for fixture-based tests that don't
    /// have the real tool installed.
    #[arg(long)]
    pub from_stdin: bool,
    /// The command to run. Pass it as a single quoted string to preserve
    /// shell operators (pipes, &&, redirects); multiple bare words are
    /// joined with spaces and executed the same way.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    pub command: Vec<String>,
}

pub fn run(args: RunArgs) -> Result<i32> {
    let raw = args.command.join(" ");
    let cfg = Config::load();
    let table = RuleTable::build(load_custom_rules(&cfg).as_ref());

    if args.dry_run {
        let parsed = rc_core::ParsedCommand::parse(&raw);
        let rule = table.find(&parsed);
        println!("command: {raw}");
        println!("matched rule: {}", rule.name);
        return Ok(0);
    }

    let (command_output, exit_code) = if args.from_stdin {
        let mut stdout = String::new();
        std::io::stdin()
            .read_to_string(&mut stdout)
            .context("reading stdin")?;
        (
            CommandOutput {
                stdout,
                stderr: String::new(),
                exit_code: None,
            },
            0,
        )
    } else {
        let output = Process::new("sh").arg("-c").arg(&raw).output()?;
        let exit_code = output.status.code().unwrap_or(1);
        (
            CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                exit_code: output.status.code(),
            },
            exit_code,
        )
    };

    if args.no_compact || !cfg.enabled {
        print!("{}", command_output.combined());
        return Ok(exit_code);
    }

    let compacted = rc_engine::compact(&raw, &command_output, &cfg, &table);
    print!("{}", compacted.text);
    if !compacted.text.ends_with('\n') {
        println!();
    }

    let _ = rc_core::stats::append(
        &cfg.resolved_stats_file(),
        &rc_core::stats::StatsRecord {
            timestamp: crate::cmd_stats::now_rfc3339(),
            command: raw,
            rule_name: compacted.rule_name,
            original_bytes: compacted.original_bytes,
            compacted_bytes: compacted.compacted_bytes,
        },
    );

    Ok(exit_code)
}
