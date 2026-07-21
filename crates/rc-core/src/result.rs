use serde::Serialize;

/// The outcome of compacting a command's output: the rewritten text plus
/// enough metadata to report savings.
#[derive(Debug, Clone, Serialize)]
pub struct CompactedOutput {
    pub text: String,
    pub rule_name: String,
    pub original_bytes: usize,
    pub compacted_bytes: usize,
    pub original_lines: usize,
    pub compacted_lines: usize,
}

impl CompactedOutput {
    pub fn passthrough(text: &str, rule_name: &str) -> Self {
        CompactedOutput {
            text: text.to_string(),
            rule_name: rule_name.to_string(),
            original_bytes: text.len(),
            compacted_bytes: text.len(),
            original_lines: text.lines().count(),
            compacted_lines: text.lines().count(),
        }
    }

    pub fn new(original: &str, compacted: String, rule_name: &str) -> Self {
        CompactedOutput {
            original_bytes: original.len(),
            compacted_bytes: compacted.len(),
            original_lines: original.lines().count(),
            compacted_lines: compacted.lines().count(),
            text: compacted,
            rule_name: rule_name.to_string(),
        }
    }

    pub fn reduction_pct(&self) -> f64 {
        if self.original_bytes == 0 {
            return 0.0;
        }
        let saved = self.original_bytes.saturating_sub(self.compacted_bytes);
        (saved as f64 / self.original_bytes as f64) * 100.0
    }
}
