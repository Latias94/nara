---
type: "Verification"
title: "Compiled project tooling and Host integration pass all 198 focused tests"
description: "Verification for Compiled project tooling and Host integration pass all 198 focused tests."
timestamp: 2026-10-15T16:38:19Z
record_id: "6a8e83bd2e48414b9573ab094aa2240e"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
verified_by: "codex-root"
supersedes: "0454868035504e1cb6a6b75e6dc841c6"
---

# Summary

The second integrated U6/Host checkpoint passes all 198 tests, including actual compiled project
tooling, both built Export products, Repair, and durable custom component migration after a native
Rust type rename. U6 and U12 are not yet complete: two subsequently added regressions and the
Editor/native gates remain in progress.

# Details

- Command: `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --all-features --lib --bin nara
  --test project_workflow_cli --locked --offline --run-ignored all --test-threads=1
  --no-fail-fast --status-level fail --final-status-level fail`.
- Session `22655`; Nextest `1fb5cc88-7d9e-405e-bf31-69519ccc36d4`; exit 0; 198 passed,
  0 skipped, 3 slow; execution 1367.747 seconds, after 1m45s test compilation.
- The existing real fresh-project Export/Repair trajectory exercises two standalone exports and
  source products. The custom independent starter path checks before a lock file exists, runs
  migrated gameplay, renames its Rust type while retaining durable IDs, repairs the selected
  Scene, and exports with two exact product observations. Invalid tool declarations and an
  actual bootstrap compile error start no gameplay and publish no destination; the diagnostic
  assertion includes the actual compiler failure.
- Host external observations preserve the previous operation result on a quiet notification,
  report only real changes, and map failed physical close to the ordinary retry action.
- This is integrated worktree evidence. Pre-existing staged/unstaged filesystem, Host, and SPL
  changes remain preserved; an independently buildable committed HEAD is not claimed.
- `cargo fmt --all` passed after this checkpoint. Newly added selected-profile diagnostic and
  replacement-Closing quiet-frame regressions were not part of this already-built test run.
  Session `40860` is running those two pre-implementation regressions; its result is pending.

# Next Action

Observe and fix the two regressions, then verify affected Editor integrations, Reference products,
the native supervised-Play canary, and headless dependency direction. Commit only owned paths or
attributable hunks. Continue U7-U11/U13; do not mark the goal complete at this checkpoint.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T160315Z-compiled-custom-project-check-run-repair-export-passes-0454868035504e1cb6a6b75e6dc841c6.md
- tests/project_workflow_cli.rs
- src/project_tooling/export/trajectory.rs
