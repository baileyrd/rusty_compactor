# rusty_compactor

[![CI](https://github.com/baileyrd/rusty_compactor/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/baileyrd/rusty_compactor/actions/workflows/ci.yml)

A Rust CLI that cuts LLM token usage for AI coding agents (Claude Code and
friends), combining the ideas of two projects into one binary:

- **[rtk](https://github.com/rtk-ai/rtk)** — compacts *command output* (git,
  cargo, npm, docker, ...) via filtering, grouping, deduplication, and
  truncation, installed as a shell-command-rewriting hook.
- **[caveman](https://github.com/JuliusBrussee/caveman)** — compresses
  *agent prose* into a terser style while preserving code, commands, and
  error messages byte-for-byte.

`rusty_compactor` implements both as two independent engines behind one
binary: `run` compacts a command's output, `compress` shrinks prose text.
Everything is deterministic Rust — no LLM calls, no network, no telemetry.

## Install

### Prerequisites
A Rust toolchain (stable) — install via [rustup](https://rustup.rs/) if you
don't have one; `rustc --version` should print something. There's no
crates.io publish or prebuilt binary release yet, so building from source is
currently the only way to get it.

### Build and install

```sh
git clone https://github.com/baileyrd/rusty_compactor.git
cd rusty_compactor
cargo install --path crates/rc-cli
```

`cargo install` puts the `rusty_compactor` binary in `~/.cargo/bin`, which
needs to be on your `PATH` (rustup's installer adds this automatically for
most shells; if `rusty_compactor --version` doesn't work after installing,
check `echo $PATH` includes `~/.cargo/bin`).

Prefer not to install it globally? Build and run it directly from the repo:

```sh
cargo build --release
./target/release/rusty_compactor --help
```

### Set up the Claude Code hook

Installing the binary doesn't wire it into Claude Code — that's a separate
step, since it edits Claude Code's own settings file:

```sh
rusty_compactor hook install          # writes ./.claude/settings.json (project-local, shareable)
rusty_compactor hook install --user   # writes ~/.claude/settings.json (applies to every project)
```

This merges a `PreToolUse` hook entry for the `Bash` tool into that file
(existing keys/hooks are left alone — `hook install` only ever touches its
own entry, and re-running it is safe/idempotent):

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "/home/you/.cargo/bin/rusty_compactor hook exec" }
        ]
      }
    ]
  }
}
```

**Restart Claude Code (or start a new session) after installing** — hooks
are only read at session start. Confirm it took effect:

```sh
rusty_compactor hook status   # "Hook installed in ..." / "Hook NOT installed in ..."
```

To remove it, `rusty_compactor hook uninstall` (same `--user` flag). See
[Claude Code hook](#claude-code-hook) below for what `hook exec` actually
does at runtime once it's wired in.

## Workspace layout

| Crate         | Purpose                                                                 |
|---------------|--------------------------------------------------------------------------|
| `rc-core`     | Shared types: command parsing, config, stats log, compaction result.      |
| `rc-engine`   | The rtk-style engine: generic drop/group/dedupe/truncate pipeline, a 190+-key built-in rule table, and bespoke structured parsers for the highest-traffic commands (git status/diff, cargo build/test, npm/yarn install, pytest, jest, go test). |
| `rc-compress` | The caveman-style engine: a reusable library that segments text into prose vs. protected (code/commands/errors) spans and compresses prose across four levels. |
| `rc-cli`      | The `rusty_compactor` binary tying both engines together, plus the Claude Code hook installer. |

## Usage

### Compact a command's output

```sh
rusty_compactor run -- git status
rusty_compactor run -- cargo test
rusty_compactor run --dry-run -- docker ps    # show which rule would match, don't execute
rusty_compactor run --no-compact -- npm test  # execute normally, skip compaction
```

`run` executes the command (via `sh -c`), captures stdout+stderr, and prints
the compacted result — propagating the original exit code so scripting
against it still works.

To compact already-captured output (a saved log, a fixture file) without
executing anything, pipe it in with `--from-stdin`:

```sh
rusty_compactor run --from-stdin -- cargo test < saved_output.txt
```

Rule matching is still based on the `command` you pass (`cargo test` here),
it's just the process spawn that's skipped — this is also how the golden
tests under `crates/rc-cli/tests/` work without needing the real tools
installed (see [Testing](#testing) below).

### Compress prose

```sh
echo "Please note that this function does not, in order to be safe, mutate the shared configuration." \
  | rusty_compactor compress --level ultra
# -> "This function doesn't, to be safe, mutate shared config."
```

Levels, each a strict superset of the one before:

| Level    | What it does |
|----------|--------------|
| `lite`   | Strips filler/hedge phrases ("basically", "please note that"), collapses whitespace. No grammar changes. |
| `full`   | *(default)* + contractions (`do not` → `don't`) + drops articles (a/an/the). |
| `ultra`  | + drops sentence-initial discourse markers ("However,") + dev-jargon abbreviations (`configuration` → `config`). |
| `wenyan` | + collapses wordy connectors (`for example` → `e.g.`) + intensifiers (`very`, `quite`). |

Fenced code blocks, inline `` `code` ``, shell command lines, stack traces,
and compiler/error output are always detected and passed through unchanged —
compression only ever touches prose, and never deletes negation words
(`not`, `never`, `without`, `cannot`, ...).

### Claude Code hook

Install a `PreToolUse` hook that transparently rewrites every Bash tool call
to run through `rusty_compactor run`, so compaction happens automatically:

```sh
rusty_compactor hook install          # project-local .claude/settings.json
rusty_compactor hook install --user   # user-global ~/.claude/settings.json
rusty_compactor hook status
rusty_compactor hook uninstall
```

Restart Claude Code after installing for the hook to take effect.
`hook exec` is the entrypoint the hook actually invokes (reads the
`PreToolUse` event JSON on stdin, emits an `updatedInput.command` that
reroutes execution through `rusty_compactor run`); you shouldn't need to run
it by hand.

### Stats

Every `run` and `compress` invocation appends a record to a local JSONL log
(no network, no telemetry):

```sh
rusty_compactor stats
# rusty_compactor stats (42 events, /home/you/.rusty_compactor/stats.jsonl)
#   original:  128000 bytes (~32000 tokens)
#   compacted: 19200 bytes (~4800 tokens)
#   saved:     108800 bytes (~27200 tokens), 85.0% reduction

rusty_compactor stats --reset   # clear the log
```

### Config

```sh
rusty_compactor config show            # print resolved config
rusty_compactor config init            # write ./.rusty_compactor.toml
rusty_compactor config init --user     # write ~/.rusty_compactor/config.toml
```

Config is loaded from `./.rusty_compactor.toml` (project) or
`~/.rusty_compactor/config.toml` (user), falling back to built-in defaults.
Key fields: `enabled`, `max_output_lines`, `head_lines`, `tail_lines`,
`dedupe_min_repeats`, `stats_file`, `custom_rules_file`.

`custom_rules_file` points to a TOML file that adds or overrides command
rules on top of the built-ins, e.g.:

```toml
[[rule]]
name = "my_tool"
match = ["my_tool:*"]
drop = ["^DEBUG:"]
keep = ["(?i)error"]
group = [["^Processing item \\d+", "processed {n} items"]]
max_lines = 100
```

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

### Testing

Each crate has unit tests for its own logic (rule matching, the compaction
pipeline, structured parsers, prose compression). `crates/rc-cli/tests/cli.rs`
adds a black-box integration suite that spawns the actual `rusty_compactor`
binary via [`assert_cmd`](https://docs.rs/assert_cmd) — every test runs in its
own temp dir with `$HOME` pointed at it too, so hook installs, config, and the
stats log never touch your real environment or interfere with each other.

It also includes golden-fixture tests: realistic captured tool output (a
failing `pytest` run, a dirty `git status`, a flaky `jest` suite, ...) lives
under `crates/rc-cli/tests/fixtures/` and gets piped through
`rusty_compactor run --from-stdin -- <command>` — a mode that compacts
whatever's on stdin as if it were that command's output, without executing
anything, so these tests need no real cargo/npm/pytest/etc. installed.
Results are pinned with [`insta`](https://docs.rs/insta) snapshots
(`crates/rc-cli/tests/snapshots/`); a rule-table change that silently breaks
one of these shows up as a diff instead of passing unnoticed. To add a case:
drop a new fixture file, add a test calling it, then run
`INSTA_UPDATE=always cargo test -p rc-cli --test cli`, review the generated
`.snap` file, and commit it alongside the fixture.
