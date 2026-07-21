//! Black-box integration tests: spawn the real `rusty_compactor` binary and
//! assert on its actual stdout/stderr/exit code, rather than calling library
//! functions directly (see the unit tests inside each crate for that). Every
//! test runs in its own temp dir with `HOME` pointed at it too, so config,
//! stats, and hook installs never touch the real developer environment or
//! interfere with each other when tests run in parallel.

use assert_cmd::Command;
use tempfile::TempDir;

/// A `rusty_compactor` invocation isolated to a fresh temp dir used as both
/// the working directory and `$HOME`. Keep the returned `TempDir` alive for
/// as long as the command needs to run.
fn isolated() -> (Command, TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let mut cmd = Command::cargo_bin("rusty_compactor").expect("find rusty_compactor binary");
    cmd.current_dir(dir.path());
    cmd.env("HOME", dir.path());
    (cmd, dir)
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading fixture {path}: {e}"))
}

mod run_tests {
    use super::*;

    #[test]
    fn dedupes_repeated_output_from_a_real_process() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("for i in $(seq 1 50); do echo same-line; done")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(
            stdout.contains("same-line (\u{d7}50)"),
            "expected deduped count, got: {stdout:?}"
        );
    }

    #[test]
    fn no_compact_flag_bypasses_compaction() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--no-compact")
            .arg("--")
            .arg("for i in $(seq 1 5); do echo same-line; done")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert_eq!(stdout.matches("same-line").count(), 5);
        assert!(!stdout.contains('\u{d7}'));
    }

    #[test]
    fn propagates_the_wrapped_commands_exit_code() {
        let (mut cmd, _dir) = isolated();
        cmd.arg("run").arg("--").arg("exit 7").assert().code(7);
    }

    #[test]
    fn dry_run_shows_matched_rule_without_executing() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--dry-run")
            .arg("--")
            .arg("git")
            .arg("status")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("matched rule: git::status"), "{stdout}");
    }

    #[test]
    fn real_git_status_in_a_clean_repo() {
        let (mut cmd, dir) = isolated();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            let status = std::process::Command::new("git")
                .args(&args)
                .current_dir(dir.path())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }

        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("git")
            .arg("status")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(
            stdout.contains("clean, nothing to commit"),
            "expected the structured git::status parser to fire, got: {stdout:?}"
        );
    }

    /// Regression test: a compound command used to be treated as one blob,
    /// rule-matched only on its first command. Since `git status`'s
    /// structured parser short-circuits on a clean tree (`raw.contains(...)`
    /// over the *combined* output), a second chained command's real output
    /// was silently swallowed whenever a real `git status` happened to be
    /// first in the chain and the tree was clean.
    #[test]
    fn compound_command_compacts_each_segment_independently() {
        let (mut cmd, dir) = isolated();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            let status = std::process::Command::new("git")
                .args(&args)
                .current_dir(dir.path())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }

        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("git status && echo plain-output-line")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(
            stdout.contains("clean, nothing to commit"),
            "first segment's structured parser should still fire: {stdout:?}"
        );
        assert!(
            stdout.contains("plain-output-line"),
            "second segment's output must survive, not be swallowed by the first: {stdout:?}"
        );
    }

    #[test]
    fn and_operator_short_circuits_on_failure() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("false && echo should-not-run")
            .assert()
            .code(1);
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(!stdout.contains("should-not-run"), "{stdout:?}");
    }

    #[test]
    fn or_operator_runs_next_only_on_failure() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("false || echo should-run")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("should-run"), "{stdout:?}");
    }

    #[test]
    fn semicolon_runs_next_unconditionally() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("false ; echo runs-anyway")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("runs-anyway"), "{stdout:?}");
    }

    #[test]
    fn a_for_loop_with_internal_semicolons_is_not_split() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("for i in $(seq 1 3); do echo loop-line; done")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(
            stdout.contains("loop-line (\u{d7}3)"),
            "loop should run as one command, not fragment on the internal ';': {stdout:?}"
        );
    }

    #[test]
    fn a_lone_pipe_still_runs_as_one_real_pipeline() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--")
            .arg("printf 'a\\nb\\nc\\n' | tail -2")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert_eq!(stdout.trim(), "b\nc");
    }

    #[test]
    fn from_stdin_compacts_without_executing_anything() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("run")
            .arg("--from-stdin")
            .arg("--")
            .arg("cargo")
            .arg("test")
            .write_stdin(fixture("cargo_test_pass.txt"))
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("14 passed, 0 failed"), "{stdout}");
        // Nothing was executed, so exit code is always 0 regardless of the
        // fixture's content.
    }
}

mod compress_tests {
    use super::*;

    #[test]
    fn shrinks_prose_but_preserves_fenced_code_byte_for_byte() {
        let (mut cmd, _dir) = isolated();
        let input = "It should be noted that this is basically just a simple fix. \
Here is the exact change:\n```rust\nlet a_value = the_thing();\n```\nThat is all.";
        let assert = cmd
            .arg("compress")
            .arg("--level")
            .arg("ultra")
            .write_stdin(input)
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("let a_value = the_thing();"));
        assert!(stdout.len() < input.len());
    }

    #[test]
    fn default_level_is_full_and_applies_contractions() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd
            .arg("compress")
            .write_stdin("The function does not return the value.")
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.to_lowercase().contains("doesn't"), "{stdout}");
    }
}

mod hook_tests {
    use super::*;

    #[test]
    fn install_writes_a_bash_pretooluse_hook() {
        let (mut cmd, dir) = isolated();
        cmd.arg("hook").arg("install").assert().success();

        let settings = std::fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&settings).unwrap();
        let matchers = json["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0]["matcher"], "Bash");
        assert!(matchers[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook exec"));
    }

    #[test]
    fn install_is_idempotent() {
        let (mut cmd, dir) = isolated();
        cmd.arg("hook").arg("install").assert().success();
        // Re-run with a fresh Command against the same dir/HOME.
        let mut cmd2 = Command::cargo_bin("rusty_compactor").unwrap();
        cmd2.current_dir(dir.path());
        cmd2.env("HOME", dir.path());
        cmd2.arg("hook").arg("install").assert().success();

        let settings = std::fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&settings).unwrap();
        let matchers = json["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(matchers.len(), 1, "install should not duplicate entries");
    }

    #[test]
    fn status_reflects_install_and_uninstall() {
        let (mut cmd, dir) = isolated();
        let assert = cmd.arg("hook").arg("status").assert().success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("NOT installed"), "{stdout}");

        let mut install = Command::cargo_bin("rusty_compactor").unwrap();
        install.current_dir(dir.path()).env("HOME", dir.path());
        install.arg("hook").arg("install").assert().success();

        let mut status2 = Command::cargo_bin("rusty_compactor").unwrap();
        status2.current_dir(dir.path()).env("HOME", dir.path());
        let assert2 = status2.arg("hook").arg("status").assert().success();
        let stdout2 = String::from_utf8(assert2.get_output().stdout.clone()).unwrap();
        assert!(stdout2.contains("Hook installed"), "{stdout2}");

        let mut uninstall = Command::cargo_bin("rusty_compactor").unwrap();
        uninstall.current_dir(dir.path()).env("HOME", dir.path());
        uninstall.arg("hook").arg("uninstall").assert().success();

        let mut status3 = Command::cargo_bin("rusty_compactor").unwrap();
        status3.current_dir(dir.path()).env("HOME", dir.path());
        let assert3 = status3.arg("hook").arg("status").assert().success();
        let stdout3 = String::from_utf8(assert3.get_output().stdout.clone()).unwrap();
        assert!(stdout3.contains("NOT installed"), "{stdout3}");
    }

    #[test]
    fn exec_rewrites_a_bash_command_to_route_through_run() {
        let (mut cmd, _dir) = isolated();
        let event = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": "git status" }
        });
        let assert = cmd
            .arg("hook")
            .arg("exec")
            .write_stdin(event.to_string())
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        let response: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
        let updated = response["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .expect("updatedInput.command present");
        assert!(updated.contains("run -- 'git status'"), "{updated}");
    }

    #[test]
    fn exec_ignores_non_bash_tools() {
        let (mut cmd, _dir) = isolated();
        let event = serde_json::json!({
            "tool_name": "Read",
            "tool_input": { "file_path": "/etc/hosts" }
        });
        let assert = cmd
            .arg("hook")
            .arg("exec")
            .write_stdin(event.to_string())
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert_eq!(stdout.trim(), "{}");
    }

    #[test]
    fn exec_does_not_double_wrap_an_already_wrapped_command() {
        let (mut cmd, _dir) = isolated();
        let event = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": "/usr/local/bin/rusty_compactor run -- 'git status'" }
        });
        let assert = cmd
            .arg("hook")
            .arg("exec")
            .write_stdin(event.to_string())
            .assert()
            .success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert_eq!(stdout.trim(), "{}");
    }
}

mod config_tests {
    use super::*;

    #[test]
    fn init_writes_a_default_config_file() {
        let (mut cmd, dir) = isolated();
        cmd.arg("config").arg("init").assert().success();
        let contents = std::fs::read_to_string(dir.path().join(".rusty_compactor.toml")).unwrap();
        assert!(contents.contains("enabled = true"), "{contents}");
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let (mut cmd, dir) = isolated();
        cmd.arg("config").arg("init").assert().success();

        let mut second = Command::cargo_bin("rusty_compactor").unwrap();
        second.current_dir(dir.path()).env("HOME", dir.path());
        second
            .arg("config")
            .arg("init")
            .assert()
            .failure()
            .stderr(predicates::str::contains("already exists"));
    }

    #[test]
    fn init_force_overwrites() {
        let (mut cmd, dir) = isolated();
        cmd.arg("config").arg("init").assert().success();

        let mut second = Command::cargo_bin("rusty_compactor").unwrap();
        second.current_dir(dir.path()).env("HOME", dir.path());
        second
            .arg("config")
            .arg("init")
            .arg("--force")
            .assert()
            .success();
    }

    #[test]
    fn show_prints_valid_toml() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd.arg("config").arg("show").assert().success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        let parsed: toml::Value = toml::from_str(&stdout).expect("valid TOML");
        assert!(parsed.get("enabled").is_some());
    }
}

mod stats_tests {
    use super::*;

    #[test]
    fn reports_no_events_initially() {
        let (mut cmd, _dir) = isolated();
        let assert = cmd.arg("stats").assert().success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("No compaction events recorded"), "{stdout}");
    }

    #[test]
    fn accumulates_events_and_reset_clears_them() {
        let (mut cmd, dir) = isolated();
        cmd.arg("compress")
            .write_stdin("This is basically just a very simple test of the stats pipeline.")
            .assert()
            .success();

        let mut stats_cmd = Command::cargo_bin("rusty_compactor").unwrap();
        stats_cmd.current_dir(dir.path()).env("HOME", dir.path());
        let assert = stats_cmd.arg("stats").assert().success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        assert!(stdout.contains("1 events"), "{stdout}");

        let mut reset_cmd = Command::cargo_bin("rusty_compactor").unwrap();
        reset_cmd.current_dir(dir.path()).env("HOME", dir.path());
        reset_cmd.arg("stats").arg("--reset").assert().success();

        let mut final_cmd = Command::cargo_bin("rusty_compactor").unwrap();
        final_cmd.current_dir(dir.path()).env("HOME", dir.path());
        let assert2 = final_cmd.arg("stats").assert().success();
        let stdout2 = String::from_utf8(assert2.get_output().stdout.clone()).unwrap();
        assert!(
            stdout2.contains("No compaction events recorded"),
            "{stdout2}"
        );
    }
}

/// Golden-fixture tests: pipe a realistic captured tool output through
/// `run --from-stdin` and snapshot the compacted result with `insta`. A
/// rule-table change that silently breaks one of these shows up as a diff
/// against the committed `.snap` file instead of passing unnoticed.
mod golden_fixture_tests {
    use super::*;

    fn compact_fixture(command: &[&str], fixture_file: &str) -> String {
        let (mut cmd, _dir) = isolated();
        cmd.arg("run").arg("--from-stdin").arg("--");
        for part in command {
            cmd.arg(part);
        }
        let assert = cmd.write_stdin(fixture(fixture_file)).assert().success();
        String::from_utf8(assert.get_output().stdout.clone()).unwrap()
    }

    #[test]
    fn cargo_test_failure() {
        insta::assert_snapshot!(compact_fixture(
            &["cargo", "test"],
            "cargo_test_failure.txt"
        ));
    }

    #[test]
    fn cargo_test_pass() {
        insta::assert_snapshot!(compact_fixture(&["cargo", "test"], "cargo_test_pass.txt"));
    }

    #[test]
    fn git_status_dirty() {
        insta::assert_snapshot!(compact_fixture(&["git", "status"], "git_status_dirty.txt"));
    }

    #[test]
    fn pytest_failure() {
        insta::assert_snapshot!(compact_fixture(&["pytest"], "pytest_failure.txt"));
    }

    #[test]
    fn npm_install() {
        insta::assert_snapshot!(compact_fixture(&["npm", "install"], "npm_install.txt"));
    }

    #[test]
    fn jest_failure() {
        insta::assert_snapshot!(compact_fixture(&["jest"], "jest_failure.txt"));
    }

    #[test]
    fn go_test_failure() {
        insta::assert_snapshot!(compact_fixture(
            &["go", "test", "./..."],
            "go_test_failure.txt"
        ));
    }

    #[test]
    fn generic_unknown_tool_dedupes_and_protects_errors() {
        insta::assert_snapshot!(compact_fixture(
            &["widgetctl", "status"],
            "generic_noisy_log.txt"
        ));
    }
}
