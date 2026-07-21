# Changelog

All notable changes to this repo are documented here.
Format: Added / Changed / Deprecated / Removed / Fixed / Security, newest first.

## [Unreleased]
### Added
- Initial `rusty_compactor` workspace: rtk-style command-output compaction
  (192-key rule table + structured parsers for the top ~10 commands) and
  caveman-style prose compression (four levels), unified behind one binary.
- `hook install/uninstall/status/exec` — a Claude Code `PreToolUse` hook
  that reroutes Bash tool calls through `rusty_compactor run`.
- `run --from-stdin` — compacts piped-in text against a named command's
  rule without executing anything.
- CI workflow (fmt, clippy, build, test) on push to `main` and on PRs.
- Black-box integration test suite (`assert_cmd`) plus golden-fixture
  snapshot tests (`insta`) covering the top structured parsers.

### Fixed
- The structured `git status` parser no longer swallows the trailing
  "no changes added to commit ..." summary line as if it were an
  untracked filename; section entries must now be indented, matching
  git's own output convention.

See [RELEASE_NOTES.md](./RELEASE_NOTES.md) for the narrative, per-commit
version of this history with reasoning and known limitations.

<!-- ## [0.1.0] - YYYY-MM-DD
### Added
- Initial release -->
