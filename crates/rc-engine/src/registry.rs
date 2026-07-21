use std::collections::HashMap;

use rc_core::{CommandOutput, CompactedOutput, Config, ParsedCommand};

use crate::defaults::DEFAULT_RULES;
use crate::rules::{generic_fallback_rule, CompiledRule, UserRuleFile};
use crate::structured;

/// Lookup table of compiled rules, keyed by exact `"program:subcommand"` and
/// by bare-`program` wildcard. Built once at startup from the built-in
/// defaults plus an optional user-supplied custom rules file (which take
/// priority on key collisions).
pub struct RuleTable {
    exact: HashMap<String, CompiledRule>,
    wildcard: HashMap<String, CompiledRule>,
}

impl RuleTable {
    pub fn build(custom: Option<&UserRuleFile>) -> Self {
        let mut table = RuleTable {
            exact: HashMap::new(),
            wildcard: HashMap::new(),
        };
        for def in DEFAULT_RULES {
            let compiled = def.compile();
            for m in def.matches {
                table.insert_key(m, compiled.clone());
            }
        }
        if let Some(file) = custom {
            for def in &file.rules {
                let compiled = def.compile();
                for m in &def.matches {
                    table.insert_key(m, compiled.clone());
                }
            }
        }
        table
    }

    fn insert_key(&mut self, key: &str, rule: CompiledRule) {
        if let Some(prog) = key.strip_suffix(":*") {
            self.wildcard.insert(prog.to_string(), rule);
        } else if key.contains(':') {
            self.exact.insert(key.to_string(), rule);
        } else {
            self.wildcard.insert(key.to_string(), rule);
        }
    }

    pub fn find(&self, cmd: &ParsedCommand) -> CompiledRule {
        let exact_key = format!(
            "{}:{}",
            cmd.program,
            cmd.subcommand.as_deref().unwrap_or("")
        );
        if let Some(r) = self.exact.get(&exact_key) {
            return r.clone();
        }
        if let Some(r) = self.wildcard.get(&cmd.program) {
            return r.clone();
        }
        generic_fallback_rule()
    }

    pub fn len(&self) -> usize {
        self.exact.len() + self.wildcard.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.wildcard.is_empty()
    }
}

impl Default for RuleTable {
    fn default() -> Self {
        Self::build(None)
    }
}

/// Compacts a command's captured output: tries a bespoke structured parser
/// first (see [`structured`]), then falls back to the generic rule-based
/// pipeline (see [`crate::strategies`]).
pub fn compact(
    command: &str,
    output: &CommandOutput,
    cfg: &Config,
    table: &RuleTable,
) -> CompactedOutput {
    let combined = output.combined();
    if !cfg.enabled {
        return CompactedOutput::passthrough(&combined, "disabled");
    }
    let parsed = ParsedCommand::parse(command);
    if let Some(structured) = structured::try_compact(&parsed, &combined) {
        return structured;
    }
    let rule = table.find(&parsed);
    crate::strategies::compact(&combined, &rule, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_100_plus_command_keys() {
        let table = RuleTable::build(None);
        assert!(
            table.len() >= 100,
            "expected 100+ distinct command keys, found {}",
            table.len()
        );
    }

    #[test]
    fn finds_exact_before_wildcard() {
        let table = RuleTable::build(None);
        let cmd = ParsedCommand::parse("git status");
        let rule = table.find(&cmd);
        assert_eq!(rule.name, "git::status");
    }

    #[test]
    fn falls_back_to_generic_for_unknown_program() {
        let table = RuleTable::build(None);
        let cmd = ParsedCommand::parse("totally-unknown-tool --flag");
        let rule = table.find(&cmd);
        assert_eq!(rule.name, "generic");
    }

    #[test]
    fn end_to_end_compacts_cargo_test_output() {
        let table = RuleTable::build(None);
        let cfg = Config::default();
        let output = CommandOutput {
            stdout: "running 1 test\ntest it_works ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
        };
        let out = compact("cargo test", &output, &cfg, &table);
        assert!(out.compacted_bytes < out.original_bytes);
        assert!(out.text.contains("1 passed, 0 failed"));
    }
}
