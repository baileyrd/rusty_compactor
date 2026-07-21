use once_cell::sync::Lazy;
use regex::Regex;

use crate::level::Level;
use crate::rules::{
    ABBREVIATIONS, CONTRACTIONS, DISCOURSE_MARKERS, FILLER_PHRASES, WENYAN_CONNECTORS,
};
use crate::segment::{protect_inline_code, restore_inline_code, segment, Segment};

#[derive(Debug, Clone)]
pub struct CompressResult {
    pub text: String,
    pub level: Level,
    pub original_chars: usize,
    pub compressed_chars: usize,
}

impl CompressResult {
    pub fn reduction_pct(&self) -> f64 {
        if self.original_chars == 0 {
            return 0.0;
        }
        let saved = self.original_chars.saturating_sub(self.compressed_chars);
        (saved as f64 / self.original_chars as f64) * 100.0
    }
}

/// Compresses `input` at the given level: code/commands/errors (see
/// [`crate::segment`]) pass through byte-for-byte, prose is rewritten.
pub fn compress(input: &str, level: Level) -> CompressResult {
    let segments = segment(input);
    let mut out = String::with_capacity(input.len());
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match seg {
            Segment::Verbatim(v) => out.push_str(v),
            Segment::Prose(p) => out.push_str(&compress_prose(p, level)),
        }
    }
    CompressResult {
        original_chars: input.chars().count(),
        compressed_chars: out.chars().count(),
        text: out,
        level,
    }
}

/// Compresses a single prose block (inline code within it is protected too).
pub fn compress_prose(text: &str, level: Level) -> String {
    if text.trim().is_empty() {
        return text.to_string();
    }
    let (mut working, protected) = protect_inline_code(text);

    working = apply_pairs(&working, &FILLER);
    if level >= Level::Full {
        working = apply_pairs(&working, &CONTRACTIONS_RULES);
        working = ARTICLE_RE.replace_all(&working, "").to_string();
    }
    if level >= Level::Ultra {
        working = apply_pairs(&working, &DISCOURSE_RULES);
        working = apply_pairs(&working, &ABBREV_RULES);
    }
    if level == Level::Wenyan {
        working = apply_pairs(&working, &WENYAN_RULES);
        working = apply_pairs(&working, &INTENSIFIER_RULES);
    }

    working = normalize_whitespace(&working);
    working = recapitalize_sentences(&working);
    restore_inline_code(&working, &protected)
}

fn phrase_regex(phrase: &str) -> Regex {
    let escaped = regex::escape(phrase);
    let starts_word = phrase.chars().next().is_some_and(|c| c.is_alphanumeric());
    let ends_word = phrase.chars().last().is_some_and(|c| c.is_alphanumeric());
    let pattern = format!(
        "(?i){}{escaped}{}",
        if starts_word { r"\b" } else { "" },
        if ends_word { r"\b" } else { "" }
    );
    Regex::new(&pattern).unwrap_or_else(|_| Regex::new(&regex::escape(phrase)).unwrap())
}

static FILLER: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    FILLER_PHRASES
        .iter()
        .map(|(p, r)| (phrase_regex(p), *r))
        .collect()
});

static CONTRACTIONS_RULES: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    CONTRACTIONS
        .iter()
        .map(|(p, r)| (phrase_regex(p), *r))
        .collect()
});

static DISCOURSE_RULES: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    DISCOURSE_MARKERS
        .iter()
        .map(|p| (phrase_regex(p), ""))
        .collect()
});

static ABBREV_RULES: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    ABBREVIATIONS
        .iter()
        .map(|(p, r)| (phrase_regex(p), *r))
        .collect()
});

static WENYAN_RULES: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    WENYAN_CONNECTORS
        .iter()
        .map(|(p, r)| (phrase_regex(p), *r))
        .collect()
});

const INTENSIFIERS: &[&str] = &["very", "quite", "rather", "fairly", "somewhat"];

static INTENSIFIER_RULES: Lazy<Vec<(Regex, &'static str)>> =
    Lazy::new(|| INTENSIFIERS.iter().map(|p| (phrase_regex(p), "")).collect());

static ARTICLE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b(a|an|the)\b").unwrap());

fn apply_pairs(text: &str, pairs: &[(Regex, &str)]) -> String {
    let mut out = text.to_string();
    for (re, rep) in pairs {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out
}

fn normalize_whitespace(text: &str) -> String {
    static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());
    static SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]+([,.!?;:])").unwrap());

    let mut result = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        let collapsed = MULTI_SPACE.replace_all(line, " ");
        let fixed = SPACE_BEFORE_PUNCT.replace_all(&collapsed, "$1");
        result.push_str(fixed.trim());
    }
    result
}

/// Re-capitalizes the first letter of each sentence/line after word removal
/// may have left a lowercase word at a boundary (e.g. removing "The " from
/// a sentence start). Boundary punctuation only triggers capitalization if
/// followed by whitespace, so abbreviations like "e.g." aren't mistaken for
/// a sentence break.
fn recapitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut want_capital = true;
    let mut after_boundary_punct = false;
    for c in text.chars() {
        if want_capital && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            want_capital = false;
            after_boundary_punct = false;
            continue;
        }
        out.push(c);
        if matches!(c, '.' | '!' | '?') {
            after_boundary_punct = true;
        } else if c.is_whitespace() {
            if after_boundary_punct {
                want_capital = true;
            }
        } else {
            after_boundary_punct = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_strips_filler_without_grammar_changes() {
        let out = compress_prose("This is basically just a simple fix.", Level::Lite);
        assert_eq!(out, "This is a simple fix.");
    }

    #[test]
    fn full_drops_articles_and_contracts() {
        let out = compress_prose("The function does not return the value.", Level::Full);
        assert_eq!(out, "Function doesn't return value.");
    }

    #[test]
    fn ultra_drops_discourse_markers_and_abbreviates() {
        let out = compress_prose(
            "However, the configuration file has a lot of information in it.",
            Level::Ultra,
        );
        let lower = out.to_lowercase();
        assert!(!lower.contains("however"));
        assert!(lower.contains("config"));
        assert!(lower.contains("info"));
    }

    #[test]
    fn wenyan_collapses_connectors() {
        let out = compress_prose("For example, this is very useful.", Level::Wenyan);
        assert!(out.starts_with("E.g."));
        assert!(!out.to_lowercase().contains("very"));
    }

    #[test]
    fn code_blocks_survive_byte_for_byte() {
        let input =
            "Fix the bug in this code:\n```rust\nlet a_value = the_thing();\n```\nThe fix is done.";
        let result = compress(input, Level::Wenyan);
        assert!(result.text.contains("let a_value = the_thing();"));
    }

    #[test]
    fn inline_code_and_negation_preserved() {
        let out = compress_prose("Do not delete `main.rs`, it is required.", Level::Wenyan);
        assert!(out.contains("`main.rs`"));
        let lower = out.to_lowercase();
        assert!(lower.contains("not") || lower.contains("don't"));
    }

    #[test]
    fn shell_commands_are_never_rewritten() {
        let input = "Run the tests:\n$ cargo test --the-flag\nThat is all.";
        let result = compress(input, Level::Wenyan);
        assert!(result.text.contains("$ cargo test --the-flag"));
    }

    #[test]
    fn reduction_pct_is_nonzero_for_wordy_input() {
        let result = compress(
            "It should be noted that this is essentially just a very basic example.",
            Level::Ultra,
        );
        assert!(result.reduction_pct() > 0.0);
    }
}
