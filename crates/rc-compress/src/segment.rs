//! Splits input text into prose (compressible) and verbatim (protected)
//! segments, so compression never touches code, commands, or error output.

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Prose(String),
    Verbatim(String),
}

static VERBATIM_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^\s*[$>]\s+\S+",                              // shell prompt: "$ cmd" / "> cmd"
        r"|^\s{4,}\S+",                                 // indented code block (Markdown-style)
        r"|^Traceback \(most recent",                   // Python traceback
        r#"|^\s*File "[^"]+", line"#,                   // Python traceback frame
        r"|^\s*at \S+\(",                               // JS/Java stack frame
        r"|^[A-Za-z_][A-Za-z0-9_.]*(Error|Exception):", // FooError: msg
        r"|^(error|Error|ERROR|fatal|Fatal|FATAL|panic|Panic|PANIC)[:\[]", // compiler/panic
        r"|^\s*-->\s",                                  // rustc location arrow
        r"|^[\w./-]+:\d+:\d+:",                         // file:line:col:
    ))
    .unwrap()
});

static FENCE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*```").unwrap());

/// Splits `input` into ordered segments. Fenced code blocks are kept whole;
/// individual lines that look like shell commands, stack traces, or
/// compiler/error output are protected line-by-line; everything else is
/// grouped into prose paragraphs.
pub fn segment(input: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut prose_buf: Vec<&str> = Vec::new();
    let mut lines = input.lines().peekable();

    let flush = |segments: &mut Vec<Segment>, buf: &mut Vec<&str>| {
        if !buf.is_empty() {
            segments.push(Segment::Prose(buf.join("\n")));
            buf.clear();
        }
    };

    while let Some(line) = lines.next() {
        if FENCE.is_match(line) {
            flush(&mut segments, &mut prose_buf);
            let mut block = vec![line.to_string()];
            for l in lines.by_ref() {
                block.push(l.to_string());
                if FENCE.is_match(l) {
                    break;
                }
            }
            segments.push(Segment::Verbatim(block.join("\n")));
        } else if VERBATIM_LINE.is_match(line) {
            flush(&mut segments, &mut prose_buf);
            segments.push(Segment::Verbatim(line.to_string()));
        } else {
            prose_buf.push(line);
        }
    }
    flush(&mut segments, &mut prose_buf);
    segments
}

const PLACEHOLDER_MARK: char = '\u{E000}';

/// Replaces inline `` `code` `` spans with placeholder tokens so word-level
/// compression rules never rewrite their contents, returning the rewritten
/// text plus the extracted spans (in order) for later restoration.
pub fn protect_inline_code(text: &str) -> (String, Vec<String>) {
    static INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`[^`\n]+`").unwrap());
    let mut protected = Vec::new();
    let out = INLINE_CODE
        .replace_all(text, |caps: &regex::Captures| {
            protected.push(caps[0].to_string());
            format!(
                "{PLACEHOLDER_MARK}{}{PLACEHOLDER_MARK}",
                protected.len() - 1
            )
        })
        .to_string();
    (out, protected)
}

pub fn restore_inline_code(text: &str, protected: &[String]) -> String {
    let re = Regex::new(&format!("{PLACEHOLDER_MARK}(\\d+){PLACEHOLDER_MARK}")).unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let idx: usize = caps[1].parse().unwrap_or(0);
        protected.get(idx).cloned().unwrap_or_default()
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_fenced_code_whole() {
        let input = "Here is code:\n```rust\nfn main() {}\n```\nDone explaining.";
        let segs = segment(input);
        assert_eq!(segs.len(), 3);
        assert!(matches!(&segs[0], Segment::Prose(p) if p == "Here is code:"));
        assert!(matches!(&segs[1], Segment::Verbatim(v) if v.contains("fn main()")));
        assert!(matches!(&segs[2], Segment::Prose(p) if p == "Done explaining."));
    }

    #[test]
    fn protects_shell_command_lines() {
        let input = "Run this:\n$ cargo test\nIt should pass.";
        let segs = segment(input);
        assert!(segs
            .iter()
            .any(|s| matches!(s, Segment::Verbatim(v) if v == "$ cargo test")));
    }

    #[test]
    fn inline_code_round_trips() {
        let text = "Set `max_lines` to 10 please.";
        let (placeholder, protected) = protect_inline_code(text);
        assert!(!placeholder.contains("max_lines"));
        let restored = restore_inline_code(&placeholder, &protected);
        assert_eq!(restored, text);
    }
}
