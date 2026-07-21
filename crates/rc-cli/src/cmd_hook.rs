use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};

const HOOK_MARKER: &str = "rusty_compactor";

#[derive(Subcommand)]
pub enum HookCommand {
    /// Register the PreToolUse Bash hook in a Claude Code settings.json.
    Install(InstallArgs),
    /// Remove the hook entry this tool previously installed.
    Uninstall(InstallArgs),
    /// Show whether the hook is currently installed and where.
    Status(InstallArgs),
    /// Hook entrypoint: reads a PreToolUse event on stdin, prints the
    /// rewritten-command JSON on stdout. Not meant to be run by hand.
    Exec,
}

#[derive(Args)]
pub struct InstallArgs {
    /// Install into the user-global ~/.claude/settings.json instead of the
    /// project-local .claude/settings.json.
    #[arg(long)]
    pub user: bool,
}

pub fn run(cmd: HookCommand) -> Result<()> {
    match cmd {
        HookCommand::Install(args) => install(args),
        HookCommand::Uninstall(args) => uninstall(args),
        HookCommand::Status(args) => status(args),
        HookCommand::Exec => exec(),
    }
}

fn settings_path(args: &InstallArgs) -> PathBuf {
    if args.user {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("settings.json")
    } else {
        PathBuf::from(".claude").join("settings.json")
    }
}

fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).with_context(|| format!("parsing {} as JSON", path.display()))
}

fn write_settings(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text + "\n").with_context(|| format!("writing {}", path.display()))
}

fn bin_path() -> Result<String> {
    let exe = std::env::current_exe().context("resolving current executable path")?;
    Ok(exe.to_string_lossy().into_owned())
}

fn install(args: InstallArgs) -> Result<()> {
    let path = settings_path(&args);
    let mut settings = read_settings(&path)?;
    let bin = bin_path()?;
    let hook_command = format!("{bin} hook exec");

    let hooks = settings
        .as_object_mut()
        .context("settings.json root is not a JSON object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let pre_tool_use = hooks
        .as_object_mut()
        .context("`hooks` key is not a JSON object")?
        .entry("PreToolUse")
        .or_insert_with(|| json!([]));
    let matchers = pre_tool_use
        .as_array_mut()
        .context("`hooks.PreToolUse` is not a JSON array")?;

    // Remove any matcher group this tool previously installed, then add the
    // current one back, so re-running `install` is idempotent and always
    // reflects the current binary path.
    matchers.retain(|m| !is_our_matcher(m));
    matchers.push(json!({
        "matcher": "Bash",
        "hooks": [
            { "type": "command", "command": hook_command }
        ]
    }));

    write_settings(&path, &settings)?;
    println!("Installed PreToolUse Bash hook in {}", path.display());
    println!("  {hook_command}");
    println!("Restart Claude Code (or start a new session) for the hook to take effect.");
    Ok(())
}

fn uninstall(args: InstallArgs) -> Result<()> {
    let path = settings_path(&args);
    let mut settings = read_settings(&path)?;
    let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        println!(
            "No hooks section found in {}; nothing to do.",
            path.display()
        );
        return Ok(());
    };
    let Some(pre_tool_use) = hooks.get_mut("PreToolUse").and_then(|p| p.as_array_mut()) else {
        println!(
            "No PreToolUse hooks found in {}; nothing to do.",
            path.display()
        );
        return Ok(());
    };
    let before = pre_tool_use.len();
    pre_tool_use.retain(|m| !is_our_matcher(m));
    let removed = before - pre_tool_use.len();

    write_settings(&path, &settings)?;
    println!("Removed {removed} matcher group(s) from {}", path.display());
    Ok(())
}

fn status(args: InstallArgs) -> Result<()> {
    let path = settings_path(&args);
    let settings = read_settings(&path)?;
    let installed = settings
        .get("hooks")
        .and_then(|h| h.get("PreToolUse"))
        .and_then(|p| p.as_array())
        .map(|arr| arr.iter().any(is_our_matcher))
        .unwrap_or(false);

    if installed {
        println!("Hook installed in {}", path.display());
    } else {
        println!("Hook NOT installed in {}", path.display());
    }
    Ok(())
}

fn is_our_matcher(matcher: &Value) -> bool {
    matcher
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(HOOK_MARKER))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// The actual hook entrypoint invoked by Claude Code for every Bash tool
/// call. Reads the PreToolUse event JSON from stdin and, unless disabled or
/// already wrapped, prints an `updatedInput` that reroutes execution through
/// `rusty_compactor run`.
fn exec() -> Result<()> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("reading hook event from stdin")?;
    let event: Value = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));

    let no_decision = json!({});
    let Some(command) = event
        .get("tool_name")
        .and_then(|t| t.as_str())
        .filter(|t| *t == "Bash")
        .and_then(|_| event.get("tool_input"))
        .and_then(|i| i.get("command"))
        .and_then(|c| c.as_str())
    else {
        println!("{no_decision}");
        return Ok(());
    };

    let cfg = rc_core::Config::load();
    if !cfg.enabled || command.contains(HOOK_MARKER) {
        // Already wrapped (avoid double-wrapping) or compaction disabled.
        println!("{no_decision}");
        return Ok(());
    }

    let bin = bin_path()?;
    let rewritten = format!("{bin} run -- {}", shell_single_quote(command));
    let response = json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "updatedInput": { "command": rewritten }
        }
    });
    println!("{response}");
    Ok(())
}

fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quote_escapes_embedded_quotes() {
        let quoted = shell_single_quote("echo 'hi'");
        assert_eq!(quoted, "'echo '\\''hi'\\'''");
    }

    #[test]
    fn is_our_matcher_detects_installed_entry() {
        let matcher = json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "/usr/local/bin/rusty_compactor hook exec"}]
        });
        assert!(is_our_matcher(&matcher));
        let other = json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "/usr/local/bin/some-other-tool"}]
        });
        assert!(!is_our_matcher(&other));
    }
}
