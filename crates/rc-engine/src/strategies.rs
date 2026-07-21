//! Generic, tool-agnostic compaction strategies: noise filtering, similar-line
//! grouping, deduplication with occurrence counts, and head/tail truncation.
//! These are combined into a single pipeline in [`crate::registry::compact`].

use std::collections::HashMap;

use rc_core::{CompactedOutput, Config};

use crate::rules::CompiledRule;

/// One line of output as it flows through the pipeline.
#[derive(Debug, Clone)]
struct Item {
    idx: usize,
    text: String,
    /// Matched a `keep` pattern: exempt from grouping/dedup and protected
    /// from truncation.
    protected: bool,
}

/// Counts occurrences of a key while remembering the index of first sight,
/// so output can be rendered in original order with a "(×N)" suffix.
struct OrderedCounter {
    order: Vec<String>,
    counts: HashMap<String, usize>,
    first_index: HashMap<String, usize>,
}

impl OrderedCounter {
    fn new() -> Self {
        OrderedCounter {
            order: Vec::new(),
            counts: HashMap::new(),
            first_index: HashMap::new(),
        }
    }

    fn add(&mut self, key: String, idx: usize) {
        if !self.counts.contains_key(&key) {
            self.order.push(key.clone());
            self.first_index.insert(key.clone(), idx);
        }
        *self.counts.entry(key).or_insert(0) += 1;
    }

    fn into_entries(self) -> Vec<(String, usize, usize)> {
        self.order
            .into_iter()
            .map(|k| {
                let count = self.counts[&k];
                let idx = self.first_index[&k];
                (k, count, idx)
            })
            .collect()
    }
}

/// Runs the full drop -> group -> dedupe -> truncate pipeline over raw text.
pub fn compact(raw: &str, rule: &CompiledRule, cfg: &Config) -> CompactedOutput {
    if raw.is_empty() {
        return CompactedOutput::passthrough(raw, &rule.name);
    }
    let lines: Vec<&str> = raw.lines().collect();

    let mut protected: Vec<Item> = Vec::new();
    let mut candidates: Vec<(usize, String)> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if rule.keep.iter().any(|r| r.is_match(line)) {
            protected.push(Item {
                idx,
                text: line.to_string(),
                protected: true,
            });
        } else if rule.drop.iter().any(|r| r.is_match(line)) {
            // dropped: noise, contributes nothing to output
        } else {
            candidates.push((idx, line.to_string()));
        }
    }

    let mut group_counter = OrderedCounter::new();
    let mut ungrouped: Vec<(usize, String)> = Vec::new();
    'lines: for (idx, line) in candidates {
        for (pattern, label) in &rule.group {
            if pattern.is_match(&line) {
                group_counter.add(label.clone(), idx);
                continue 'lines;
            }
        }
        ungrouped.push((idx, line));
    }

    let min_repeats = rule
        .dedupe_min_repeats
        .unwrap_or(cfg.dedupe_min_repeats)
        .max(2);
    let mut dedupe_counter = OrderedCounter::new();
    for (idx, line) in ungrouped {
        dedupe_counter.add(line, idx);
    }

    let mut unprotected: Vec<Item> = Vec::new();
    for (label, count, idx) in group_counter.into_entries() {
        let text = label.replace("{n}", &count.to_string());
        unprotected.push(Item {
            idx,
            text,
            protected: false,
        });
    }
    for (line, count, idx) in dedupe_counter.into_entries() {
        let text = if count >= min_repeats {
            format!("{line} (\u{d7}{count})")
        } else {
            line
        };
        unprotected.push(Item {
            idx,
            text,
            protected: false,
        });
    }
    unprotected.sort_by_key(|i| i.idx);

    let head = rule.head_lines.unwrap_or(cfg.head_lines);
    let tail = rule.tail_lines.unwrap_or(cfg.tail_lines);
    let max_lines = rule.max_lines.unwrap_or(cfg.max_output_lines);

    let mut all: Vec<Item> = protected;
    all.append(&mut unprotected);
    all.sort_by_key(|i| i.idx);

    let final_lines = truncate(all, max_lines, head, tail);
    let text = final_lines.join("\n");
    CompactedOutput::new(raw, text, &rule.name)
}

/// Truncates to `max_lines`, always preserving `protected` items regardless
/// of position, and keeping `head`/`tail` lines from the remaining pool.
fn truncate(items: Vec<Item>, max_lines: usize, head: usize, tail: usize) -> Vec<String> {
    if items.len() <= max_lines {
        return items.into_iter().map(|i| i.text).collect();
    }

    let (protected, unprotected): (Vec<Item>, Vec<Item>) =
        items.into_iter().partition(|i| i.protected);

    let head_n = head.min(unprotected.len());
    let tail_n = tail.min(unprotected.len().saturating_sub(head_n));
    let omitted = unprotected.len().saturating_sub(head_n + tail_n);

    let mut kept: Vec<(usize, String)> = Vec::new();
    kept.extend(
        unprotected[..head_n]
            .iter()
            .map(|i| (i.idx, i.text.clone())),
    );
    if omitted > 0 {
        let marker_idx = unprotected[head_n].idx;
        kept.push((marker_idx, format!("... {omitted} lines omitted ...")));
    }
    kept.extend(
        unprotected[unprotected.len() - tail_n..]
            .iter()
            .map(|i| (i.idx, i.text.clone())),
    );
    kept.extend(protected.into_iter().map(|i| (i.idx, i.text)));
    kept.sort_by_key(|(idx, _)| *idx);
    kept.into_iter().map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::CompiledRule;
    use regex::Regex;

    fn rule() -> CompiledRule {
        CompiledRule {
            name: "test".into(),
            drop: vec![Regex::new(r"^noise$").unwrap()],
            keep: vec![Regex::new(r"^ERROR").unwrap()],
            group: vec![(
                Regex::new(r"^Compiling \S+").unwrap(),
                "Compiling {n} crates".into(),
            )],
            max_lines: None,
            head_lines: None,
            tail_lines: None,
            dedupe_min_repeats: None,
        }
    }

    #[test]
    fn drops_noise_lines() {
        let cfg = Config::default();
        let out = compact("noise\nkeep me\nnoise", &rule(), &cfg);
        assert_eq!(out.text, "keep me");
    }

    #[test]
    fn groups_similar_lines_with_count() {
        let cfg = Config::default();
        let raw = "Compiling foo\nCompiling bar\nCompiling baz";
        let out = compact(raw, &rule(), &cfg);
        assert_eq!(out.text, "Compiling 3 crates");
    }

    #[test]
    fn dedupes_repeated_lines() {
        let cfg = Config {
            dedupe_min_repeats: 2,
            ..Config::default()
        };
        let raw = "same\nsame\nsame\nunique";
        let out = compact(raw, &rule(), &cfg);
        assert!(out.text.contains("same (\u{d7}3)"));
        assert!(out.text.contains("unique"));
    }

    #[test]
    fn protects_keep_lines_from_truncation() {
        let cfg = Config {
            max_output_lines: 5,
            head_lines: 1,
            tail_lines: 1,
            ..Config::default()
        };
        let mut raw_lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
        raw_lines.insert(10, "ERROR something broke".to_string());
        let raw = raw_lines.join("\n");
        let out = compact(&raw, &rule(), &cfg);
        assert!(out.text.contains("ERROR something broke"));
        assert!(out.text.contains("lines omitted"));
    }

    #[test]
    fn passthrough_when_within_budget() {
        let cfg = Config::default();
        let out = compact("just one line", &rule(), &cfg);
        assert_eq!(out.text, "just one line");
        assert_eq!(out.original_bytes, out.compacted_bytes);
    }
}
