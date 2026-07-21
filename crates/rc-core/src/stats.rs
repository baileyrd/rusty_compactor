use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// One row of the JSONL stats log: a single compaction event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsRecord {
    pub timestamp: String,
    pub command: String,
    pub rule_name: String,
    pub original_bytes: usize,
    pub compacted_bytes: usize,
}

/// Rough token estimate (chars / 4), matching the heuristic commonly used
/// by CLI token-budgeting tools when an exact tokenizer isn't available.
pub fn approx_tokens(bytes: usize) -> usize {
    ((bytes as f64) / 4.0).ceil() as usize
}

pub fn append(path: &Path, record: &StatsRecord) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(record)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn read_all(path: &Path) -> anyhow::Result<Vec<StatsRecord>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let records = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    Ok(records)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StatsSummary {
    pub events: usize,
    pub original_bytes: usize,
    pub compacted_bytes: usize,
}

impl StatsSummary {
    pub fn bytes_saved(&self) -> usize {
        self.original_bytes.saturating_sub(self.compacted_bytes)
    }

    pub fn reduction_pct(&self) -> f64 {
        if self.original_bytes == 0 {
            return 0.0;
        }
        (self.bytes_saved() as f64 / self.original_bytes as f64) * 100.0
    }

    pub fn approx_tokens_saved(&self) -> usize {
        approx_tokens(self.bytes_saved())
    }
}

pub fn summarize(records: &[StatsRecord]) -> StatsSummary {
    let mut summary = StatsSummary::default();
    for r in records {
        summary.events += 1;
        summary.original_bytes += r.original_bytes;
        summary.compacted_bytes += r.compacted_bytes;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_savings() {
        let records = vec![
            StatsRecord {
                timestamp: "t1".into(),
                command: "git status".into(),
                rule_name: "git::status".into(),
                original_bytes: 1000,
                compacted_bytes: 200,
            },
            StatsRecord {
                timestamp: "t2".into(),
                command: "cargo test".into(),
                rule_name: "cargo::test".into(),
                original_bytes: 500,
                compacted_bytes: 100,
            },
        ];
        let summary = summarize(&records);
        assert_eq!(summary.events, 2);
        assert_eq!(summary.original_bytes, 1500);
        assert_eq!(summary.compacted_bytes, 300);
        assert_eq!(summary.bytes_saved(), 1200);
        assert!((summary.reduction_pct() - 80.0).abs() < 0.01);
    }
}
