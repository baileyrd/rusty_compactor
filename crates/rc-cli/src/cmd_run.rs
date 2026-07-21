use std::io::Read as _;
use std::process::Command as Process;

use anyhow::{Context, Result};
use clap::Args;
use rc_core::{ChainLink, CommandOutput, Config};
use rc_engine::RuleTable;

use crate::cmd_config::load_custom_rules;

#[derive(Args)]
pub struct RunArgs {
    /// Print what would be run and the compaction rule that would apply to
    /// each segment, without actually executing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// Skip compaction entirely; just execute and print raw output.
    #[arg(long)]
    pub no_compact: bool,
    /// Don't execute `command` — instead compact whatever is piped into
    /// stdin, matching rules as if it were that command's output. Useful
    /// for replaying a saved log, or for fixture-based tests that don't
    /// have the real tool installed. Not compound-aware: the whole command
    /// is treated as one unit, since stdin can't be split per segment.
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
        println!("command: {raw}");
        for link in rc_core::split_compound(&raw) {
            if let ChainLink::Cmd(cmd) = link {
                let parsed = rc_core::ParsedCommand::parse(&cmd);
                let rule = table.find(&parsed);
                println!("  {cmd:?} -> matched rule: {}", rule.name);
            }
        }
        return Ok(0);
    }

    if args.from_stdin {
        let mut stdout = String::new();
        std::io::stdin()
            .read_to_string(&mut stdout)
            .context("reading stdin")?;
        let command_output = CommandOutput {
            stdout,
            stderr: String::new(),
            exit_code: None,
        };
        if args.no_compact || !cfg.enabled {
            print!("{}", command_output.combined());
        } else {
            let compacted = rc_engine::compact(&raw, &command_output, &cfg, &table);
            print_with_trailing_newline(&compacted.text);
        }
        return Ok(0);
    }

    // Split on top-level &&/||/; so each independent command gets executed,
    // rule-matched, and compacted on its own — otherwise one command's rule
    // (or a structured parser's early-return, like git status's clean-tree
    // summary) can silently swallow every other command's output. A lone
    // pipe is never a split point (see rc_core::split_compound), so piped
    // chains still run as a single real pipeline.
    let mut exit_code = 0;
    let mut should_run = true;
    for link in rc_core::split_compound(&raw) {
        match link {
            ChainLink::Cmd(cmd) => {
                if !should_run {
                    should_run = true; // gate consumed; next operator re-evaluates fresh
                    continue;
                }
                exit_code = execute_and_print(&cmd, &cfg, &table, args.no_compact)?;
            }
            ChainLink::And => should_run = exit_code == 0,
            ChainLink::Or => should_run = exit_code != 0,
            ChainLink::Then => should_run = true,
        }
    }

    Ok(exit_code)
}

/// Executes one already-split command segment, prints its (optionally
/// compacted) output, logs stats, and returns its exit code.
fn execute_and_print(cmd: &str, cfg: &Config, table: &RuleTable, no_compact: bool) -> Result<i32> {
    let output = Process::new("sh").arg("-c").arg(cmd).output()?;
    let exit_code = output.status.code().unwrap_or(1);
    let command_output = CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
    };

    if no_compact || !cfg.enabled {
        print!("{}", command_output.combined());
        return Ok(exit_code);
    }

    let compacted = rc_engine::compact(cmd, &command_output, cfg, table);
    print_with_trailing_newline(&compacted.text);

    let _ = rc_core::stats::append(
        &cfg.resolved_stats_file(),
        &rc_core::stats::StatsRecord {
            timestamp: crate::cmd_stats::now_rfc3339(),
            command: cmd.to_string(),
            rule_name: compacted.rule_name,
            original_bytes: compacted.original_bytes,
            compacted_bytes: compacted.compacted_bytes,
        },
    );

    Ok(exit_code)
}

fn print_with_trailing_newline(text: &str) {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
}
