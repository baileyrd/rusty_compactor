use anyhow::Result;
use clap::Args;
use rc_core::{stats, Config};

#[derive(Args)]
pub struct StatsArgs {
    /// Clear the stats log after printing the summary.
    #[arg(long)]
    pub reset: bool,
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

pub fn run(args: StatsArgs) -> Result<()> {
    let cfg = Config::load();
    let path = cfg.resolved_stats_file();
    let records = stats::read_all(&path)?;
    let summary = stats::summarize(&records);

    if summary.events == 0 {
        println!("No compaction events recorded yet at {}", path.display());
    } else {
        println!(
            "rusty_compactor stats ({} events, {})",
            summary.events,
            path.display()
        );
        println!(
            "  original:  {} bytes (~{} tokens)",
            summary.original_bytes,
            stats::approx_tokens(summary.original_bytes)
        );
        println!(
            "  compacted: {} bytes (~{} tokens)",
            summary.compacted_bytes,
            stats::approx_tokens(summary.compacted_bytes)
        );
        println!(
            "  saved:     {} bytes (~{} tokens), {:.1}% reduction",
            summary.bytes_saved(),
            summary.approx_tokens_saved(),
            summary.reduction_pct()
        );
    }

    if args.reset {
        std::fs::write(&path, "")?;
        println!("Cleared stats log at {}", path.display());
    }
    Ok(())
}
