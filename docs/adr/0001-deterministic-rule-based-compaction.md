# ADR-0001: Deterministic rule-based compaction instead of an LLM call

Status: Accepted
Date: 2026-07-21

## Context
The whole point of `rusty_compactor` is to shrink what an AI coding agent
reads (command output, its own prose) before it consumes tokens. The most
obvious way to "summarize command output" or "rewrite prose tersely" is to
hand it to an LLM. We needed to decide, up front, whether the compaction
engine itself should be an LLM call or a plain deterministic program.

## Decision
Every compaction/compression path — the rtk-style command-output engine
(`rc-engine`) and the caveman-style prose engine (`rc-compress`) — is plain
Rust: regex-based line filtering/grouping/deduplication/truncation for
output, and phrase-substitution/segment-protection rules for prose. No
network calls, no model invocations, anywhere in the binary.

## Alternatives considered
- **Call an LLM to summarize captured output.** Rejected: adds latency, a
  network dependency, a cost per invocation, and non-determinism — the same
  `cargo test` output could compact differently run to run, which defeats
  debuggability and makes the golden-fixture test harness (see the
  `insta`-snapshotted tests under `crates/rc-cli/tests/`) meaningless, since
  its whole premise is that the same input always compacts to the same
  output.
- **Call an LLM to rewrite prose tersely.** Same problems, plus a much
  sharper risk: an LLM rewrite of the agent's own explanation could subtly
  change technical meaning (drop a caveat, invert a negation) in ways that
  are hard to catch. A fixed rule table that only ever touches a known,
  reviewed set of filler phrases/articles/contractions is auditable —
  every possible rewrite is visible in `rc-compress/src/rules.rs` — an LLM
  rewrite is not.
- **Hybrid: rules first, LLM fallback for anything unmatched.** Rejected
  for v1 as unnecessary complexity — the generic drop/group/dedupe/truncate
  pipeline already provides a reasonable fallback for commands without a
  bespoke structured parser (see `ARCHITECTURE.md`'s Non-goals), so there
  was no gap that actually needed an LLM to fill.

## Consequences
- **Fast and free at runtime**: compaction is regex matching over already-
  captured text, microseconds, no API key, no cost per call.
- **Reproducible and testable**: the same input always produces the same
  output, which is what makes the golden-fixture/snapshot test harness
  possible at all — a rule-table regression shows up as an exact diff.
- **Bounded coverage**: quality depends on the rule table and structured
  parsers actually covering the command/phrase in question. Unusual tools
  or unusual prose patterns fall back to the generic engine, which is
  weaker than a bespoke parser or an LLM would be. This is an accepted,
  explicit tradeoff (see `ARCHITECTURE.md`'s Non-goals), not an oversight —
  the mitigation is that the rule table and structured parsers are cheap
  to extend as gaps are found (as happened with the `git status` bug fixed
  alongside the test harness — see RELEASE_NOTES.md).
- **Forecloses** using this binary as a general-purpose "ask an LLM to
  summarize anything" tool. If that's ever wanted, it should be a different
  tool (or an opt-in mode with its own ADR), not folded into this one.
