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
2. `rusty_compactor run` splits the command into segments on top-level
   `&&`/`||`/`;` (`rc_core::split_compound` — a lone `|`, and anything
   inside `(...)`/quotes/a `for`/`while`/`if`/`case` block, is never a split
   point) and executes each segment via `sh -c` in order, applying
   `&&`/`||`/`;` short-circuit semantics itself; see
   [ADR-0002](./docs/adr/0002-split-compound-commands-in-run-not-the-hook.md)
   for why this lives here rather than as a hook-time text rewrite.
3. Each segment's captured stdout+stderr goes through `rc_engine::compact`,
   which parses the command (`ParsedCommand`), tries a
   structured parser first (git status/diff, cargo build/test, npm install,
   pytest, jest, go test), then falls back to the generic rule-based
   drop/group/dedupe/truncate pipeline against the matched `CompiledRule`.
4. Each segment's compacted text is printed to stdout in order (what the
   agent actually sees) and its own `StatsRecord` is appended to the local
   JSONL log; the process exits with the last-executed segment's real exit
   code (matching normal shell `&&`/`||`/`;` chain semantics).
5. Independently, `rusty_compactor compress` runs the same segment-then-
   rewrite pipeline over prose text (`rc_compress`), protecting code/command/
   error spans before applying level-gated word/phrase rules.
6. `run --from-stdin` short-circuits step 2: instead of spawning a process,
   it reads already-captured output from stdin and feeds that straight into
   step 3 under the given command name. This is a real execution path (not
   test-only scaffolding) — it's what lets a saved log be replayed through
   the compactor, and it's how the golden-fixture tests in
   `crates/rc-cli/tests/` exercise real rule matching against captured
   cargo/git/pytest/npm/jest/go output with none of those tools installed.

## Key decisions
See [docs/adr/](./docs/adr/) for the record of individual decisions and their
tradeoffs:
- [ADR-0001](./docs/adr/0001-deterministic-rule-based-compaction.md) — why
  compaction is deterministic rule-based text processing rather than an LLM
  call.
- [ADR-0002](./docs/adr/0002-split-compound-commands-in-run-not-the-hook.md)
  — why compound commands (`cmd1 && cmd2`) are split and compacted per
  segment inside `run` itself, not via hook-time text rewriting.

## Non-goals
- Not an LLM-based summarizer — all compaction/compression is deterministic
  regex- and rule-based text processing, so behavior is reproducible and
  auditable.
- Not a general shell-command sandbox or security boundary — it wraps
  execution for output compaction, not for permission enforcement.
- Not trying to hand-write bespoke parsers for every one of the 190+ covered
  commands — only the highest-traffic ones get structured parsers; the long
  tail relies on the generic rule engine.
- Not a full POSIX shell parser — `rc_core::split_compound` tracks quotes,
  `(...)`/`$(...)` nesting, and `for`/`while`/`until`/`if`/`case` block
  depth well enough to split real `&&`/`||`/`;` chains correctly, but
  unusual constructs it doesn't specifically recognize fall back to being
  treated as one opaque segment rather than being (possibly incorrectly)
  split. See [ADR-0002](./docs/adr/0002-split-compound-commands-in-run-not-the-hook.md).
