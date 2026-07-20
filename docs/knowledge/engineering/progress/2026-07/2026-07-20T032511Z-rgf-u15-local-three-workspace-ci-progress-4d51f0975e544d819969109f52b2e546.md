---
type: "Work Progress"
title: "RGF-U15 local three-workspace CI implementation"
description: "Commit 188a493 defines and locally verifies bounded Windows/Linux feedback; hosted execution remains pending."
timestamp: 2026-07-20T03:25:11Z
record_id: "4d51f0975e544d819969109f52b2e546"
resource: "nara CI"
tags: ["rgf-u15", "ci", "windows", "linux", "policy"]
status: "active"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "188a493db69be89692f0f89e4a10b5e59ff27a94"
---

# Summary

RGF-U15 now has an implementation commit, but it is not complete. Commit `188a493` adds a
read-only GitHub Actions workflow for the locked root, reference-game, and module-consumer
workspaces on disposable Windows and Linux runners. Local equivalents and policy mutation tests
pass; no GitHub-hosted run has yet supplied the cross-platform evidence required by the unit.

# Implemented Boundary

- The workflow defines exactly six matrix jobs: three independent workspaces across
  `ubuntu-latest` and `windows-latest`.
- Every job has a 45-minute timeout, one Cargo build job, disabled incremental compilation, and a
  ref-scoped concurrency group with cancellation.
- Workflow permissions are exactly `contents: read`; checkout credentials are not persisted; no
  secret, OIDC, cache, artifact, self-hosted runner, packaging, or publication path is present.
- Checkout and nextest installation use full commit SHAs. Rust `1.95.0` and nextest `0.9.138` are
  explicit inputs.
- `tests/ci_policy.rs` parses the workflow and independent manifests. It rejects unknown workflow
  structure, skipped jobs or steps, ignored/masked failures, matrix expansion or duplication,
  mutable or unexpected actions, credentials, write authority, secret expression variants, OIDC,
  persistent runners, shared caches, lockfile removal, workspace inheritance, patch overrides,
  and forbidden dependency edges.

# Local Evidence

- `actionlint .github/workflows/ci.yml` passed.
- Root locked workspace check passed; the exact focused root nextest command passed 16 tests.
- Reference-game locked all-target check passed; the exact focused nextest command passed 10
  tests.
- Module-consumer locked all-target check passed; its nextest command passed 1 test.
- The focused CI policy suite passed 7 tests, including all hostile mutations.
- The policy target passed strict Clippy with only documented pre-existing repository allowances.
- `cargo +1.95.0 check --locked -p nara --test ci_policy` passed.
- Architecture documentation tests passed 7 tests and engineering-memory validation passed with
  legacy warnings only.
- Formatting, diff validation, exact staged-scope review, and an independent read-only policy review
  passed with no remaining P0, P1, or P2 finding.
- Derived `current-state.md` and `log.md` remain stale because they contain concurrent working-tree
  edits; this unit does not overwrite those shared rollups.

# Remaining Gate

The branch has not been pushed by this session, so GitHub has not executed the six hosted jobs.
Local evidence cannot establish Windows/Linux runner behavior. Keep RGF-U15 active until one
reviewed PR or protected push run is green on both operating systems. Only then create final
Verification Evidence and a completed registration; packaging and artifact consumption remain
RGF-U7.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u15-establish-minimum-three-workspace-ci-feedback`
- `.github/workflows/ci.yml`
- `tests/ci_policy.rs`
- Commit `188a493`
