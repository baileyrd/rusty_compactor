//! Lightweight shell-command parsing: just enough to identify the program,
//! subcommand, and arguments for rule matching. Not a full POSIX shell parser.

/// A command split into its invoked program, first non-flag argument
/// ("subcommand"), and the full argument list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub raw: String,
    pub program: String,
    pub subcommand: Option<String>,
    pub args: Vec<String>,
}

impl ParsedCommand {
    pub fn parse(raw: &str) -> Self {
        let tokens = split_words(raw);
        let program = tokens.first().map(|p| basename(p)).unwrap_or_default();
        let args: Vec<String> = tokens.iter().skip(1).cloned().collect();
        let subcommand = args.iter().find(|t| !t.starts_with('-')).cloned();
        ParsedCommand {
            raw: raw.to_string(),
            program,
            subcommand,
            args,
        }
    }

    /// Convenience: "git status" -> ("git", Some("status")).
    pub fn head(&self) -> (&str, Option<&str>) {
        (self.program.as_str(), self.subcommand.as_deref())
    }
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Minimal shell-word splitter: handles single/double quotes and backslash
/// escapes, but does not attempt full shell semantics (pipes, globs, etc.
/// are treated as literal tokens which is fine for rule-matching purposes).
fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = input.chars().peekable();
    let mut has_content = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                has_content = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_content = true;
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    has_content = true;
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_content {
                    words.push(std::mem::take(&mut current));
                    has_content = false;
                }
            }
            c => {
                current.push(c);
                has_content = true;
            }
        }
    }
    if has_content {
        words.push(current);
    }
    words
}

/// One piece of a top-level shell chain: either a literal command segment,
/// or the operator joining it to the next segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainLink {
    Cmd(String),
    /// `&&` — run the next segment only if this one succeeded.
    And,
    /// `||` — run the next segment only if this one failed.
    Or,
    /// `;` — run the next segment unconditionally.
    Then,
}

const BLOCK_OPENERS: &[&str] = &["for", "while", "until", "if", "case"];
const BLOCK_CLOSERS: &[&str] = &["done", "fi", "esac"];

fn end_word(word: &mut String, block_depth: &mut i32) {
    if BLOCK_OPENERS.contains(&word.as_str()) {
        *block_depth += 1;
    } else if BLOCK_CLOSERS.contains(&word.as_str()) {
        *block_depth = (*block_depth - 1).max(0);
    }
    word.clear();
}

fn flush_segment(result: &mut Vec<ChainLink>, current: &mut String) {
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        result.push(ChainLink::Cmd(trimmed.to_string()));
    }
    current.clear();
}

/// Splits a raw command line into segments at top-level `&&`, `||`, and `;`
/// boundaries, so each independent command can be executed and rule-matched
/// on its own instead of the whole chain being treated as one blob (which
/// would apply one command's rule to every other command's output too).
///
/// Deliberately conservative, not a full POSIX shell parser, but aware
/// enough to avoid the sharpest edges:
/// - a lone `|` (pipe) is never a split point, since compacting one side of
///   a pipe would change what the other side actually receives;
/// - anything inside `(...)`/`$(...)` or quotes is never split;
/// - anything inside a `for`/`while`/`until`/`if`/`case` block (tracked up
///   to its matching `done`/`fi`/`esac`) is never split either, since the
///   `;` separating a loop's clauses (`for x in y; do ...; done`) isn't an
///   independent-command boundary the way `cmd1; cmd2` is.
pub fn split_compound(raw: &str) -> Vec<ChainLink> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut word = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut paren_depth: i32 = 0;
    let mut block_depth: i32 = 0;
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if in_single {
            current.push(c);
            if c == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            current.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else if c == '"' {
                in_double = false;
            }
            continue;
        }

        let splittable = paren_depth == 0 && block_depth == 0;
        match c {
            '\'' => {
                in_single = true;
                current.push(c);
                end_word(&mut word, &mut block_depth);
            }
            '"' => {
                in_double = true;
                current.push(c);
                end_word(&mut word, &mut block_depth);
            }
            '\\' => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
                end_word(&mut word, &mut block_depth);
            }
            '(' => {
                paren_depth += 1;
                current.push(c);
                end_word(&mut word, &mut block_depth);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
                end_word(&mut word, &mut block_depth);
            }
            '&' if splittable && chars.peek() == Some(&'&') => {
                chars.next();
                end_word(&mut word, &mut block_depth);
                flush_segment(&mut result, &mut current);
                result.push(ChainLink::And);
            }
            '|' if splittable && chars.peek() == Some(&'|') => {
                chars.next();
                end_word(&mut word, &mut block_depth);
                flush_segment(&mut result, &mut current);
                result.push(ChainLink::Or);
            }
            ';' if splittable => {
                end_word(&mut word, &mut block_depth);
                flush_segment(&mut result, &mut current);
                result.push(ChainLink::Then);
            }
            // Reached only when not splittable (mid-block/paren) or for a
            // lone `&`/`|` (background job / pipe): keep it as literal text.
            '&' | '|' | ';' => {
                current.push(c);
                word.clear();
            }
            c if c.is_whitespace() => {
                end_word(&mut word, &mut block_depth);
                current.push(c);
            }
            c => {
                current.push(c);
                word.push(c);
            }
        }
    }
    end_word(&mut word, &mut block_depth);
    flush_segment(&mut result, &mut current);
    result
}

/// Captured output of an executed command, ready for compaction.
#[derive(Debug, Clone, Default)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl CommandOutput {
    pub fn combined(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_and_subcommand() {
        let p = ParsedCommand::parse("git status --short");
        assert_eq!(p.program, "git");
        assert_eq!(p.subcommand.as_deref(), Some("status"));
        assert_eq!(p.args, vec!["status", "--short"]);
    }

    #[test]
    fn strips_path_from_program() {
        let p = ParsedCommand::parse("/usr/bin/cargo test --release");
        assert_eq!(p.program, "cargo");
        assert_eq!(p.subcommand.as_deref(), Some("test"));
    }

    #[test]
    fn handles_quoted_args() {
        let p = ParsedCommand::parse(r#"grep -r "hello world" src"#);
        assert_eq!(p.program, "grep");
        assert_eq!(p.args, vec!["-r", "hello world", "src"]);
    }

    #[test]
    fn subcommand_skips_leading_flags() {
        let p = ParsedCommand::parse("npm --silent install express");
        assert_eq!(p.subcommand.as_deref(), Some("install"));
    }

    #[test]
    fn split_compound_single_command_is_one_segment() {
        let links = split_compound("git status");
        assert_eq!(links, vec![ChainLink::Cmd("git status".to_string())]);
    }

    #[test]
    fn split_compound_splits_on_and() {
        let links = split_compound("git status && npm test");
        assert_eq!(
            links,
            vec![
                ChainLink::Cmd("git status".to_string()),
                ChainLink::And,
                ChainLink::Cmd("npm test".to_string()),
            ]
        );
    }

    #[test]
    fn split_compound_splits_on_or_and_semicolon() {
        let links = split_compound("cmd1 || cmd2 ; cmd3");
        assert_eq!(
            links,
            vec![
                ChainLink::Cmd("cmd1".to_string()),
                ChainLink::Or,
                ChainLink::Cmd("cmd2".to_string()),
                ChainLink::Then,
                ChainLink::Cmd("cmd3".to_string()),
            ]
        );
    }

    #[test]
    fn split_compound_never_splits_on_a_lone_pipe() {
        let links = split_compound("cargo test 2>&1 | tail -20");
        assert_eq!(
            links,
            vec![ChainLink::Cmd("cargo test 2>&1 | tail -20".to_string())]
        );
    }

    #[test]
    fn split_compound_ignores_operators_inside_quotes() {
        let links = split_compound(r#"echo "a && b""#);
        assert_eq!(links, vec![ChainLink::Cmd(r#"echo "a && b""#.to_string())]);
    }

    #[test]
    fn split_compound_ignores_operators_inside_parens() {
        let links = split_compound("(echo a && echo b) || echo c");
        assert_eq!(
            links,
            vec![
                ChainLink::Cmd("(echo a && echo b)".to_string()),
                ChainLink::Or,
                ChainLink::Cmd("echo c".to_string()),
            ]
        );
    }

    #[test]
    fn split_compound_does_not_split_inside_a_for_loop() {
        let raw = "for i in $(seq 1 3); do echo same-line; done";
        let links = split_compound(raw);
        assert_eq!(links, vec![ChainLink::Cmd(raw.to_string())]);
    }

    #[test]
    fn split_compound_splits_after_a_for_loop_closes() {
        let links = split_compound("for i in 1 2; do echo $i; done && echo after");
        assert_eq!(
            links,
            vec![
                ChainLink::Cmd("for i in 1 2; do echo $i; done".to_string()),
                ChainLink::And,
                ChainLink::Cmd("echo after".to_string()),
            ]
        );
    }

    #[test]
    fn split_compound_does_not_split_inside_an_if_block() {
        let raw = "if grep -q foo file.txt; then echo yes; else echo no; fi";
        let links = split_compound(raw);
        assert_eq!(links, vec![ChainLink::Cmd(raw.to_string())]);
    }
}
