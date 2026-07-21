# rusty_compactor

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

```sh
cargo install --path crates/rc-cli
# binary name: rusty_compactor
```

Or build in place:

```sh
cargo build --release
./target/release/rusty_compactor --help
```

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
```

`run` executes the command (via `sh -c`), captures stdout+stderr, and prints
the compacted result — propagating the original exit code so scripting
against it still works.

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
