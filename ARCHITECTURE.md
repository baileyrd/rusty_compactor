# Architecture

## Overview
`rusty_compactor` is a Rust CLI that cuts LLM token usage for AI coding agents
by compacting command output (rtk-style) and compressing prose responses
(caveman-style). It's a single static binary with no network calls, no
telemetry, and no external services — all state is local flat files (TOML
config, a JSONL stats log).

Not: an LLM itself, a proxy server, or anything that inspects/modifies model
output — the prose compressor is a deterministic text transform, not a model
call.

## Boundaries
Domain logic (parsing, rule matching, text transforms) is plain, I/O-free
Rust in `rc-core`/`rc-engine`/`rc-compress`; all I/O (process execution,
filesystem, JSON hook protocol) lives in `rc-cli`.

| Port | Adapter(s) | Notes |
| ---- | ---------- | ----- |
| Command execution | `std::process::Command` (`sh -c`) in `rc-cli::cmd_run` | Only adapter; domain code (`rc_engine::compact`) takes already-captured stdout/stderr and never shells out itself |
| Rule source | Built-in `DEFAULT_RULES` (compiled into the binary) + optional user TOML via `custom_rules_file` (`rc_engine::rules::UserRuleFile`) | Same `CompiledRule` shape either way; user rules override on key collision |
| Config store | Project `.rusty_compactor.toml` / user `~/.rusty_compactor/config.toml` (`rc_core::config::Config`) | Falls back to in-code defaults if neither file exists |
| Stats sink | Local JSONL append (`rc_core::stats`) | No remote backend; `telemetry` config flag exists but is intentionally unused (matches caveman's no-phone-home stance) |
| Agent integration | Claude Code `PreToolUse` hook JSON over stdin/stdout (`rc-cli::cmd_hook`) | Only integration point; the hook rewrites `tool_input.command` to route through `rusty_compactor run` |

## Structure
Modular monolith: four crates in one Cargo workspace, one binary. No
component has crossed the line into a separate service — there's no
concurrent-access or independent-scaling forcing function here, just a CLI
invoked per command.

## Data flow
1. Claude Code's Bash tool is about to run a command → `PreToolUse` hook
   fires → `rusty_compactor hook exec` reads the event JSON, emits
   `updatedInput.command` wrapping it as `rusty_compactor run -- '<cmd>'`.
2. `rusty_compactor run` executes the real command via `sh -c`, captures
   stdout+stderr+exit code.
3. `rc_engine::compact` parses the command (`ParsedCommand`), tries a
   structured parser first (git status/diff, cargo build/test, npm install,
   pytest, jest, go test), then falls back to the generic rule-based
   drop/group/dedupe/truncate pipeline against the matched `CompiledRule`.
4. Compacted text is printed to stdout (what the agent actually sees) and a
   `StatsRecord` is appended to the local JSONL log; the process exits with
   the wrapped command's real exit code.
5. Independently, `rusty_compactor compress` runs the same segment-then-
   rewrite pipeline over prose text (`rc_compress`), protecting code/command/
   error spans before applying level-gated word/phrase rules.

## Key decisions
See [docs/adr/](./docs/adr/) for the record of individual decisions and their tradeoffs.

## Non-goals
- Not an LLM-based summarizer — all compaction/compression is deterministic
  regex- and rule-based text processing, so behavior is reproducible and
  auditable.
- Not a general shell-command sandbox or security boundary — it wraps
  execution for output compaction, not for permission enforcement.
- Not trying to hand-write bespoke parsers for every one of the 190+ covered
  commands — only the highest-traffic ones get structured parsers; the long
  tail relies on the generic rule engine.
