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
}
