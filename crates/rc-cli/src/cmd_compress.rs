use std::io::Read;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use rc_compress::Level;
use rc_core::Config;

#[derive(Args)]
pub struct CompressArgs {
    /// Compression aggressiveness.
    #[arg(long, default_value = "full")]
    pub level: Level,
    /// Read from this file instead of stdin.
    pub file: Option<PathBuf>,
    /// Print a savings summary to stderr after the compressed text.
    #[arg(long)]
    pub stats: bool,
}

pub fn run(args: CompressArgs) -> Result<()> {
    let input = match &args.file {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            buf
        }
    };

    let result = rc_compress::compress(&input, args.level);
    print!("{}", result.text);
    if !result.text.ends_with('\n') {
        println!();
    }

    let cfg = Config::load();
    let _ = rc_core::stats::append(
        &cfg.resolved_stats_file(),
        &rc_core::stats::StatsRecord {
            timestamp: crate::cmd_stats::now_rfc3339(),
            command: format!("compress::{}", args.level),
            rule_name: format!("compress::{}", args.level),
            original_bytes: input.len(),
            compacted_bytes: result.text.len(),
        },
    );

    if args.stats {
        eprintln!(
            "compressed {} -> {} bytes ({:.1}% smaller)",
            input.len(),
            result.text.len(),
            result.reduction_pct()
        );
    }
    Ok(())
}
