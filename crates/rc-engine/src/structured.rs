//! Bespoke structured parsers for the highest-traffic dev commands, going
//! beyond the generic drop/group/dedupe/truncate pipeline in
//! [`crate::strategies`]. Each parser understands the *shape* of its tool's
//! output (summary lines, failure markers, diff hunks) rather than just
//! pattern-matching lines in isolation. Returns `None` when the command
//! doesn't match or the output doesn't look like what the parser expects,
//! in which case the caller falls back to the generic rule engine.

use once_cell::sync::Lazy;
use regex::Regex;

use rc_core::{CompactedOutput, ParsedCommand};

pub fn try_compact(cmd: &ParsedCommand, raw: &str) -> Option<CompactedOutput> {
    let (program, sub) = cmd.head();
    match (program, sub) {
        ("git", Some("status")) => git_status(raw),
        ("git", Some("diff")) => git_diff(raw),
        ("cargo", Some("build")) | ("cargo", Some("check")) => cargo_build(raw),
        ("cargo", Some("test")) => cargo_test(raw),
        ("npm", Some("install"))
        | ("npm", Some("ci"))
        | ("npm", Some("i"))
        | ("yarn", _)
        | ("pnpm", Some("install"))
        | ("pnpm", Some("i")) => npm_install(raw),
        ("pytest", _) | ("py.test", _) | (_, Some("pytest")) => pytest(raw),
        ("jest", _) | ("vitest", _) => jest(raw),
        ("go", Some("test")) => go_test(raw),
        _ => None,
    }
    .map(|text| CompactedOutput::new(raw, text, &format!("structured::{program}")))
}

static BRANCH: Lazy<Regex> = Lazy::new(|| Regex::new(r"^On branch (\S+)").unwrap());
static STATUS_ENTRY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(modified|deleted|new file|renamed|copied):\s*(.+)$").unwrap());
static UNTRACKED_HEADER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Untracked files:").unwrap());
static STAGED_HEADER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Changes to be committed:").unwrap());
static UNSTAGED_HEADER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Changes not staged for commit:").unwrap());

fn git_status(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let branch = lines
        .iter()
        .find_map(|l| BRANCH.captures(l).map(|c| c[1].to_string()))?;

    if raw.contains("nothing to commit, working tree clean") {
        return Some(format!("On branch {branch} - clean, nothing to commit"));
    }

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    let mut section = 0u8; // 0 none, 1 staged, 2 unstaged, 3 untracked

    for line in &lines {
        if STAGED_HEADER.is_match(line) {
            section = 1;
            continue;
        }
        if UNSTAGED_HEADER.is_match(line) {
            section = 2;
            continue;
        }
        if UNTRACKED_HEADER.is_match(line) {
            section = 3;
            continue;
        }
        if line.trim().is_empty() || line.trim_start().starts_with('(') {
            continue;
        }
        match section {
            1 => {
                if let Some(c) = STATUS_ENTRY.captures(line) {
                    staged.push(c[2].trim().to_string());
                }
            }
            2 => {
                if let Some(c) = STATUS_ENTRY.captures(line) {
                    unstaged.push(c[2].trim().to_string());
                }
            }
            3 => {
                let f = line.trim();
                if !f.is_empty() {
                    untracked.push(f.to_string());
                }
            }
            _ => {}
        }
    }

    let mut out = vec![format!("On branch {branch}")];
    let render = |label: &str, files: &[String]| -> Option<String> {
        if files.is_empty() {
            return None;
        }
        Some(format!("{label} ({}): {}", files.len(), files.join(", ")))
    };
    if let Some(s) = render("Staged", &staged) {
        out.push(s);
    }
    if let Some(s) = render("Unstaged", &unstaged) {
        out.push(s);
    }
    if let Some(s) = render("Untracked", &untracked) {
        out.push(s);
    }
    if out.len() == 1 {
        return None; // didn't recognize the format; let the generic engine handle it
    }
    Some(out.join("\n"))
}

static HUNK_HEADER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^@@ .* @@").unwrap());
static DIFF_FILE_HEADER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^diff --git").unwrap());

/// Keeps file/hunk headers and every +/- change line verbatim, but collapses
/// runs of unchanged context lines longer than 3 into a single "..." marker.
fn git_diff(raw: &str) -> Option<String> {
    if !raw.contains("diff --git") {
        return None;
    }
    const CONTEXT_KEEP: usize = 2;
    let lines: Vec<&str> = raw.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut context_run: Vec<String> = Vec::new();

    let flush_context = |out: &mut Vec<String>, run: &mut Vec<String>| {
        if run.len() <= CONTEXT_KEEP * 2 {
            out.append(run);
            return;
        }
        let omitted = run.len() - CONTEXT_KEEP * 2;
        out.extend(run.drain(..CONTEXT_KEEP));
        out.push(format!("... {omitted} unchanged lines ..."));
        let tail_start = run.len() - CONTEXT_KEEP;
        out.extend(run.drain(tail_start..));
        run.clear();
    };

    for line in lines {
        let is_context = !line.starts_with('+')
            && !line.starts_with('-')
            && !DIFF_FILE_HEADER.is_match(line)
            && !HUNK_HEADER.is_match(line)
            && !line.starts_with("index ")
            && !line.starts_with("+++")
            && !line.starts_with("---");
        if is_context {
            context_run.push(line.to_string());
        } else {
            flush_context(&mut out, &mut context_run);
            if line.starts_with("index ") {
                continue; // low-signal noise
            }
            out.push(line.to_string());
        }
    }
    flush_context(&mut out, &mut context_run);
    Some(out.join("\n"))
}

static COMPILING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*Compiling (\S+)").unwrap());
static CARGO_ERROR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^error(\[|:)").unwrap());
static CARGO_FINISHED: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*Finished ").unwrap());

fn cargo_build(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let compiling_count = lines.iter().filter(|l| COMPILING.is_match(l)).count();
    if compiling_count == 0 && !lines.iter().any(|l| CARGO_ERROR.is_match(l)) {
        return None;
    }

    let mut out = Vec::new();
    if compiling_count > 0 {
        out.push(format!("Compiling {compiling_count} crate(s)"));
    }

    let mut in_error_block = false;
    for line in &lines {
        if CARGO_ERROR.is_match(line) || line.starts_with("warning:") {
            in_error_block = true;
            out.push(line.to_string());
        } else if in_error_block {
            if line.trim().is_empty() {
                in_error_block = false;
            } else {
                out.push(line.to_string());
            }
        } else if CARGO_FINISHED.is_match(line) {
            out.push(line.to_string());
        }
    }
    Some(out.join("\n"))
}

static CARGO_TEST_RESULT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed;").unwrap());
static CARGO_TEST_FAILED_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^test (.*) \.\.\. FAILED$").unwrap());
static PANIC_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^thread '.*' panicked at").unwrap());

fn cargo_test(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut total_passed = 0u64;
    let mut total_failed = 0u64;
    let mut saw_result = false;
    let mut failed_tests = Vec::new();
    let mut panics = Vec::new();

    for line in &lines {
        if let Some(c) = CARGO_TEST_RESULT.captures(line) {
            saw_result = true;
            total_passed += c[2].parse::<u64>().unwrap_or(0);
            total_failed += c[3].parse::<u64>().unwrap_or(0);
        }
        if let Some(c) = CARGO_TEST_FAILED_LINE.captures(line) {
            failed_tests.push(c[1].to_string());
        }
        if PANIC_LINE.is_match(line) {
            panics.push(line.to_string());
        }
    }
    if !saw_result {
        return None;
    }

    let mut out = vec![format!(
        "test result: {} passed, {} failed",
        total_passed, total_failed
    )];
    for t in &failed_tests {
        out.push(format!("FAILED: {t}"));
    }
    for p in panics.iter().take(20) {
        out.push(p.clone());
    }
    Some(out.join("\n"))
}

static NPM_ADDED: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(added|changed|removed) \d+ packages?").unwrap());
static YARN_DONE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(Done|Success) in ").unwrap());
static VULN_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\d+ vulnerabilit(y|ies)").unwrap());
static NPM_ERR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^npm ERR!").unwrap());

fn npm_install(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out = Vec::new();
    for line in &lines {
        if NPM_ADDED.is_match(line)
            || YARN_DONE.is_match(line)
            || VULN_LINE.is_match(line)
            || NPM_ERR.is_match(line)
        {
            out.push(line.trim().to_string());
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("\n"))
}

static PYTEST_SUMMARY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^=+ (.+ in [\d.]+s.*?) =+$").unwrap());
static PYTEST_FAILED: Lazy<Regex> = Lazy::new(|| Regex::new(r"^FAILED (\S+)").unwrap());
static PYTEST_ASSERT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^E\s").unwrap());

fn pytest(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let summary = lines
        .iter()
        .find_map(|l| PYTEST_SUMMARY.captures(l).map(|c| c[1].trim().to_string()))?;

    let mut out = vec![summary];
    for line in &lines {
        if let Some(c) = PYTEST_FAILED.captures(line) {
            out.push(format!("FAILED {}", &c[1]));
        }
    }
    let assert_lines: Vec<&str> = lines
        .iter()
        .filter(|l| PYTEST_ASSERT.is_match(l))
        .copied()
        .take(50)
        .collect();
    out.extend(assert_lines.into_iter().map(String::from));
    Some(out.join("\n"))
}

static JEST_TESTS_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(Tests|Test Suites|Snapshots):\s").unwrap());
static JEST_FAIL_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(FAIL|\s*\u{25cf})\s").unwrap());

fn jest(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut out = Vec::new();
    for line in &lines {
        if JEST_TESTS_LINE.is_match(line) || JEST_FAIL_LINE.is_match(line) {
            out.push(line.to_string());
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("\n"))
}

static GO_FAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(--- FAIL|FAIL\s)").unwrap());
static GO_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^ok\s+\S+").unwrap());

fn go_test(raw: &str) -> Option<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let ok_count = lines.iter().filter(|l| GO_OK.is_match(l)).count();
    let fail_lines: Vec<&str> = lines
        .iter()
        .filter(|l| GO_FAIL.is_match(l))
        .copied()
        .collect();
    if ok_count == 0 && fail_lines.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    if ok_count > 0 {
        out.push(format!("{ok_count} package(s) passed"));
    }
    out.extend(fail_lines.into_iter().map(String::from));
    Some(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_clean() {
        let raw = "On branch main\nYour branch is up to date with 'origin/main'.\n\nnothing to commit, working tree clean\n";
        let out = git_status(raw).unwrap();
        assert_eq!(out, "On branch main - clean, nothing to commit");
    }

    #[test]
    fn git_status_dirty() {
        let raw = "\
On branch main
Changes to be committed:
  (use \"git restore --staged <file>...\" to unstage)
        modified:   src/lib.rs

Changes not staged for commit:
  (use \"git add <file>...\" to update what will be committed)
        modified:   src/main.rs

Untracked files:
  (use \"git add <file>...\" to include in what will be committed)
        scratch.txt
";
        let out = git_status(raw).unwrap();
        assert!(out.contains("Staged (1): src/lib.rs"));
        assert!(out.contains("Unstaged (1): src/main.rs"));
        assert!(out.contains("Untracked (1): scratch.txt"));
    }

    #[test]
    fn cargo_test_summary() {
        let raw = "running 3 tests\ntest foo::bar ... ok\ntest foo::baz ... FAILED\n\nfailures:\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let out = cargo_test(raw).unwrap();
        assert!(out.contains("1 passed, 1 failed"));
        assert!(out.contains("FAILED: foo::baz"));
    }

    #[test]
    fn pytest_summary_extraction() {
        let raw = "collecting ...\nF.\nFAILED tests/test_x.py::test_one - assert 1 == 2\nE       assert 1 == 2\n===== 1 failed, 1 passed in 0.12s =====\n";
        let out = pytest(raw).unwrap();
        assert!(out.contains("1 failed, 1 passed in 0.12s"));
        assert!(out.contains("FAILED tests/test_x.py::test_one"));
    }

    #[test]
    fn diff_collapses_long_context() {
        let mut raw = String::from(
            "diff --git a/f b/f\nindex abc..def 100644\n--- a/f\n+++ b/f\n@@ -1,10 +1,10 @@\n",
        );
        for i in 0..10 {
            raw.push_str(&format!(" context line {i}\n"));
        }
        raw.push_str("-old\n+new\n");
        let out = git_diff(&raw).unwrap();
        assert!(out.contains("unchanged lines"));
        assert!(out.contains("-old"));
        assert!(out.contains("+new"));
    }
}
