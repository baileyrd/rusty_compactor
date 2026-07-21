# Release Notes

<!--
Two variants, pick the one that fits this repo's actual unit of change:

1. No version tags yet (pre-1.0, nothing published) — track by PR instead, same way
   AISF does it: one entry per merged PR against main, reverse chronological, each
   linking to its PR and (where one exists) to the doc that covers the change in full
   detail. Use "## PR #N — <summary>" headers.

2. Actual version tags exist — use "## vX.Y.Z - YYYY-MM-DD" headers instead, each
   linking to the PRs it shipped and a compare link to the previous tag. Add an
   "### Upgrade notes" subsection under any entry with a breaking change.

Either way, keep the tone AISF's file uses: bolded category tags inline in the
bullet (**Added:** / **Changed:** / **Fixed:**), not separate subheaders per
category — and state known limitations or deliberate scope cuts plainly instead of
leaving them implied.
-->

No PRs opened yet — this repo isn't tag-versioned, so entries track by commit
against `main` instead, reverse chronological.

---

## Fix compound-command handling in run (data-loss bug)
**2026-07-21** · [ea44029](https://github.com/baileyrd/rusty_compactor/commit/ea440290718db553a18c83d30bb504405ff2333e)

- **Fixed:** compound commands (`cmd1 && cmd2`) used to be executed as one
  `sh -c` call and rule-matched as a single command — using only the first
  command's rule for the whole combined output. This wasn't just a quality
  issue: `git status && echo plain-output-line` in a clean repo compacted
  down to just the git status summary, because the structured parser's
  clean-tree early return matched against the *combined* output and
  silently discarded the second command's output entirely. Confirmed live
  before writing the fix, by comparing this project's hook approach
  against rtk's and finding the two diverge exactly here.
- **Added:** `rc_core::split_compound` splits a raw command on top-level
  `&&`/`||`/`;`, while correctly leaving a lone `|` (pipe), anything inside
  `(...)`/`$(...)`/quotes, and anything inside a `for`/`while`/`until`/
  `if`/`case` block untouched. `run` now executes each segment
  independently, applying `&&`/`||`/`;` short-circuit semantics itself and
  returning the last-executed segment's exit code; `--dry-run` shows the
  matched rule per segment.
- **Known limitation, stated plainly:** not a full POSIX shell parser —
  see [ADR-0002](./docs/adr/0002-split-compound-commands-in-run-not-the-hook.md)
  for the reasoning and what's deliberately out of scope. `--from-stdin`
  is intentionally left compound-unaware (a single piped blob can't be
  attributed to multiple segments).
- 15 new tests (9 unit in `rc-core` covering the splitter's edge cases —
  including catching, before it shipped, that the existing
  `for i in $(seq 1 50); do echo x; done` test's internal `;`s aren't
  command boundaries — plus 6 integration tests in `rc-cli` covering the
  bug fix itself, short-circuit semantics, and piped chains staying
  intact). 78 tests passing across the workspace (up from 63); clippy/fmt
  clean.

## Add integration test harness with golden-fixture snapshots
**2026-07-21** · [da7c6b0](https://github.com/baileyrd/rusty_compactor/commit/da7c6b05306bc0a351cdd9f587db4c3aa95f1712)

- **Added:** a black-box integration suite (`crates/rc-cli/tests/cli.rs`)
  that spawns the real binary via `assert_cmd` — `run`, `compress`, `hook
  install/uninstall/status/exec`, `config`, `stats` — each isolated to its
  own temp dir + `$HOME`.
- **Added:** a `run --from-stdin` mode that compacts piped-in text against
  a named command's rule without executing anything, plus golden-fixture
  tests (realistic captured cargo/git/pytest/npm/jest/go output under
  `crates/rc-cli/tests/fixtures/`) pinned with `insta` snapshots, so a
  rule-table regression shows up as a diff instead of passing unnoticed.
- **Fixed:** the structured `git status` parser never left the "Untracked
  files" section, so it swallowed git's trailing "no changes added to
  commit ..." summary line as if it were a filename. Found by building the
  `git_status_dirty` fixture from real captured output rather than a
  synthetic snippet — exactly the class of bug this harness exists to
  catch. Fixed by requiring section entries to be indented (matching
  git's own convention), with both a unit regression test and the golden
  fixture locking it in.
- 63 tests passing across the workspace (up from 34); `cargo clippy
  --workspace --all-targets -- -D warnings` and `cargo fmt --check` clean.

## Add basic CI workflow
**2026-07-21** · [83f6b4b](https://github.com/baileyrd/rusty_compactor/commit/83f6b4b7207cd412e85dfd298958f3c7753ccec5)

- **Added:** `.github/workflows/ci.yml`, running on push to `main` and on
  every pull request: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo build --workspace --all-targets`,
  `cargo test --workspace`, with `Swatinem/rust-cache` for cargo caching.
- All four steps were run locally before pushing to confirm they pass as
  written, including clippy with warnings promoted to errors (stricter than
  the plain `cargo clippy` used during development).
- **Known limitation:** single job, single OS/toolchain (`ubuntu-latest` +
  stable) — no matrix across OSes or MSRV pinning yet.

## Initial implementation — Rust reimplementation of rtk + caveman
**2026-07-21** · [d52423d](https://github.com/baileyrd/rusty_compactor/commit/d52423d1dd3ca9006609df124a18599d54a66388)

- **Added:** the full `rusty_compactor` workspace (`rc-core`, `rc-engine`,
  `rc-compress`, `rc-cli`) combining two prior-art token-saving tools into
  one Rust binary: rtk-style command-output compaction (192-key rule table
  plus structured parsers for the ~10 highest-traffic commands) and
  caveman-style prose compression (four levels, code/command/error spans
  always protected).
- **Added:** a Claude Code `PreToolUse` hook (`hook install/uninstall/
  status/exec`) that rewrites Bash tool calls to route through
  `rusty_compactor run`, verified against the actual Claude Code hooks docs
  rather than assumed.
- **Known limitation, stated plainly:** the long tail of the 192 covered
  commands relies on the generic drop/group/dedupe/truncate pipeline rather
  than bespoke parsing — only git/cargo/npm/pytest/jest/go get hand-tuned
  structured parsers. The prose compressor is a deterministic rule-based
  text transform (word/phrase substitution + article dropping), not
  grammar-aware, so occasional minor phrasing artifacts (e.g. dropped
  articles inside idioms like "a lot of") are expected at higher
  compression levels.
- 34 unit/integration tests passing across the workspace; `cargo clippy
  --workspace --all-targets` clean.
