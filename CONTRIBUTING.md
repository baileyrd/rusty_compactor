# Contributing

## Before you start
- Match surrounding conventions when editing existing code.
- Keep diffs focused — one logical change per PR.
- For large or hard-to-reverse changes (schema/data migrations, public API changes,
  deletions, dependency/toolchain bumps), open an issue or draft PR to discuss first.

## Workflow
1. Branch off the default branch.
2. Make your change. State the *why* in commit messages or PR description for any
   non-obvious decision.
3. Add tests for non-trivial logic — happy path and at least one failure/boundary case.
   Spikes/prototypes are exempt but should say so in the PR. A rule-table or parser
   change to `rc-engine` should usually come with (or update) a golden fixture under
   `crates/rc-cli/tests/fixtures/` — see the README's Testing section. If your change
   intentionally alters compacted output, regenerate the affected `.snap` file with
   `INSTA_UPDATE=always cargo test -p rc-cli --test cli` and review the diff before
   committing it; an unreviewed snapshot update defeats the point of the check.
4. Add or update docstrings on any public surface you touched.
5. Open a PR — pick the template that matches (feature / bug fix / docs / chore).

## Code style
- Explicit over implicit; type hints/annotations always.
- Flat control flow — guard clauses, early returns, avoid >3 levels of nesting.
- Short, single-purpose functions.
- Minimal dependencies — justify any new third-party one in the PR description.
- Never commit or log secrets/credentials. Validate external input at the boundary.
- Never silently swallow exceptions — handle, propagate with context, or log.

## Review
- CI must pass before merge.
- At least one approval required (see CODEOWNERS if present).
- Reviewers: check for scope creep, missing tests, and unexplained non-obvious decisions.
