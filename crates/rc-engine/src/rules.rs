use regex::Regex;
use serde::Deserialize;

/// A rule after its regex patterns have been compiled, ready for use by
/// [`crate::strategies::compact`].
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub name: String,
    pub drop: Vec<Regex>,
    pub keep: Vec<Regex>,
    pub group: Vec<(Regex, String)>,
    pub max_lines: Option<usize>,
    pub head_lines: Option<usize>,
    pub tail_lines: Option<usize>,
    pub dedupe_min_repeats: Option<usize>,
}

/// Source form of a built-in rule: plain string patterns, defined as Rust
/// constants in [`crate::defaults`] so they're type-checked and diffable.
#[derive(Debug, Clone, Copy)]
pub struct RuleDef {
    pub name: &'static str,
    /// Keys this rule matches against, e.g. `"git:status"`, `"cargo:*"`, or
    /// `"*"` for the global fallback.
    pub matches: &'static [&'static str],
    pub drop: &'static [&'static str],
    pub keep: &'static [&'static str],
    /// `(pattern, label)` pairs; `label` may contain a literal `{n}` token
    /// that gets replaced with the group's occurrence count.
    pub group: &'static [(&'static str, &'static str)],
    pub max_lines: Option<usize>,
    pub head_lines: Option<usize>,
    pub tail_lines: Option<usize>,
    pub dedupe_min_repeats: Option<usize>,
}

/// User-supplied rule loaded from a TOML custom-rules file. Shape mirrors
/// [`RuleDef`] but with owned `String`s since it comes from parsed input.
#[derive(Debug, Clone, Deserialize)]
pub struct UserRuleDef {
    pub name: String,
    #[serde(rename = "match")]
    pub matches: Vec<String>,
    #[serde(default)]
    pub drop: Vec<String>,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub group: Vec<(String, String)>,
    pub max_lines: Option<usize>,
    pub head_lines: Option<usize>,
    pub tail_lines: Option<usize>,
    pub dedupe_min_repeats: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserRuleFile {
    #[serde(default, rename = "rule")]
    pub rules: Vec<UserRuleDef>,
}

fn compile_patterns(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| match Regex::new(p) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("rusty_compactor: invalid built-in pattern {p:?}: {e}");
                None
            }
        })
        .collect()
}

impl RuleDef {
    pub fn compile(&self) -> CompiledRule {
        CompiledRule {
            name: self.name.to_string(),
            drop: compile_patterns(self.drop),
            keep: compile_patterns(self.keep),
            group: self
                .group
                .iter()
                .filter_map(|(p, label)| Regex::new(p).ok().map(|r| (r, label.to_string())))
                .collect(),
            max_lines: self.max_lines,
            head_lines: self.head_lines,
            tail_lines: self.tail_lines,
            dedupe_min_repeats: self.dedupe_min_repeats,
        }
    }
}

impl UserRuleDef {
    pub fn compile(&self) -> CompiledRule {
        let drop_refs: Vec<&str> = self.drop.iter().map(String::as_str).collect();
        let keep_refs: Vec<&str> = self.keep.iter().map(String::as_str).collect();
        CompiledRule {
            name: self.name.clone(),
            drop: compile_patterns(&drop_refs),
            keep: compile_patterns(&keep_refs),
            group: self
                .group
                .iter()
                .filter_map(|(p, label)| Regex::new(p).ok().map(|r| (r, label.clone())))
                .collect(),
            max_lines: self.max_lines,
            head_lines: self.head_lines,
            tail_lines: self.tail_lines,
            dedupe_min_repeats: self.dedupe_min_repeats,
        }
    }
}

pub fn generic_fallback_rule() -> CompiledRule {
    CompiledRule {
        name: "generic".into(),
        drop: Vec::new(),
        keep: vec![
            Regex::new(r"(?i)\berror\b").unwrap(),
            Regex::new(r"(?i)\bfatal\b").unwrap(),
            Regex::new(r"(?i)\bpanic").unwrap(),
            Regex::new(r"(?i)\bfailed\b").unwrap(),
            Regex::new(r"(?i)\bexception\b").unwrap(),
        ],
        group: Vec::new(),
        max_lines: None,
        head_lines: None,
        tail_lines: None,
        dedupe_min_repeats: None,
    }
}
