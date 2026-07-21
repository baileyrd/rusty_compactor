# ADR-0002: Split compound commands inside `run`, not at the hook-rewrite level

Status: Accepted
Date: 2026-07-21

## Context
Comparing `rusty_compactor`'s Claude Code hook against rtk's surfaced a real
gap: rtk's hook rewrites *each command in a shell chain individually*
(`cargo fmt --all && cargo test` becomes `rtk cargo fmt --all && rtk cargo
test`), while `rusty_compactor`'s hook wrapped the *entire* raw command as
one argument to `run --`. That meant a compound command was executed as one
`sh -c` call and rule-matched as if it were a single command — using only
the first command's program/subcommand to pick a rule.

This wasn't just a quality difference, it was a data-loss bug: `git status
&& echo plain-output-line` in a clean repo compacted down to just `On
branch main - clean, nothing to commit` — the structured `git status`
parser's clean-tree early return (`raw.contains("nothing to commit...")`)
matched against the *combined* output of both commands and silently
discarded `echo`'s output entirely. Confirmed live before writing the fix.

## Decision
Compound commands are split into segments at top-level `&&`, `||`, and `;`
by `rc_core::split_compound`, and `rusty_compactor run` executes, rule-
matches, and compacts each segment independently, applying `&&`/`||`/`;`
short-circuit semantics itself and returning the last-executed segment's
exit code. This lives inside `run`'s own execution path, not as a hook-time
text rewrite.

## Alternatives considered
- **Rewrite at the hook level, like rtk** (split the raw command, wrap each
  segment as its own `rusty_compactor run -- '<segment>'`, rejoin with the
  original real operators, let the outer shell handle short-circuiting).
  This works and would fix the bug too. Rejected as the *primary*
  mechanism because `rusty_compactor run` is documented and tested as a
  standalone tool (`rusty_compactor run -- git status && echo x`) — if the
  splitting only happened in the hook's text rewrite, direct CLI usage
  would still have the bug. Keeping the smart logic inside `run` fixes it
  for both entry points from one implementation; the hook itself stays
  unchanged (still wraps the whole raw string once).
- **Leave it as a documented limitation.** Rejected once the concrete
  failure mode was confirmed live: this isn't "occasionally suboptimal
  compaction," it's an agent's chained command silently losing real output
  it needs to see (e.g. a test run tacked on after a `git status`).
- **A full POSIX shell grammar parser** (e.g. reaching for a shell-parsing
  crate). Rejected as disproportionate to the problem: a lightweight,
  unit-tested splitter that tracks quotes, `(...)`/`$(...)` nesting, and
  `for`/`while`/`until`/`if`/`case` ... `done`/`fi`/`esac` block depth
  covers the realistic cases an agent actually issues, without a new
  dependency or the maintenance burden of a real grammar.

## Consequences
- Fixes the data-loss bug for the common case (`cmd1 && cmd2`, `cmd1 ||
  cmd2`, `cmd1; cmd2`) regardless of whether `run` is invoked via the hook
  or directly.
- A lone `|` (pipe) is deliberately never a split point — compacting one
  side of a pipe would change what the other side actually receives (e.g.
  `cargo test 2>&1 | tail -20` needs `tail` to see the real output) — so
  piped chains still run as a single real pipeline, same as before.
- Not a full shell parser: unusual constructs this doesn't specifically
  track (e.g. custom function/alias-defined block keywords, here-docs)
  fall back to being treated as one opaque segment — the same behavior as
  before this fix, never worse, just not further split. Covered by tests
  in `rc-core`'s `split_compound` suite and `rc-cli`'s integration suite
  (`compound_command_compacts_each_segment_independently`,
  `and_operator_short_circuits_on_failure`, a `for` loop with internal
  `;` staying intact, a piped chain staying intact).
