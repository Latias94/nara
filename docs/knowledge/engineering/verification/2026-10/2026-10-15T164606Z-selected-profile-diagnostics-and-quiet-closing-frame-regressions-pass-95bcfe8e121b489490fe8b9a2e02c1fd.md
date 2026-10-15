---
type: "Verification Evidence"
title: "Selected-profile diagnostics and quiet Closing frame regressions pass"
description: "Verification Evidence for Selected-profile diagnostics and quiet Closing frame regressions pass."
timestamp: 2026-10-15T16:46:06Z
record_id: "95bcfe8e121b489490fe8b9a2e02c1fd"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
verified_by: "codex-root"
supersedes: "329374ac720c4dc88d6a77013d26f16f"
---

# Verification

Run `9fc43229-7be5-408b-9f5d-b383114dde50`, session `63751`, with one Cargo job and test thread:
`cargo nextest run -p nara -p nara-editor --all-features --lib --locked --offline
-E 'test(project_tooling::) | package(nara-editor)' --test-threads=1 --no-fail-fast
--status-level fail --final-status-level fail`.

# Result

100 passed, 182 filtered/ignored; execution 7.982 seconds after 38 seconds compilation. This
includes all 92 Editor library tests and the eight selected project-tooling tests. The actual
nested Export trajectory already passed in the preceding 198-test checkpoint and was not rerun
for these local fixes. `cargo fmt --all` passed before this run.

# Evidence

Both selected regressions failed at their intended assertions before correction in run
`62f82fb3-969c-40b8-bd54-a138a3c72aaf`. The profile hint now reads the existing issue fact; the
Closing effects path preserves the displayed Arc on a quiet wake. The latter test physically
finishes replacement and exit before its final assertions.

The existing owned preparation/cancellation, process deadlines, spawn cancellation, close retry,
and current-frame tests pass with the simplification changes. One duplicate poll test was removed
when its now-unused runtime-availability argument was removed; Running and CloseIncomplete still
have explicit no-paint-poll assertions. Native process exit observation remains independently
covered and unchanged.

# Follow-up

Session `57167` is checking Editor application/authoring/recovery and real generated-starter Play.
Reference all-target checks, native canary, affected static checks, and precise commits remain.
This is integrated Windows worktree evidence, not native Linux/macOS coverage or final U1-U13
completion. The follow-up focused review is source-only and cannot substitute for full final review.

# Citations

- docs/knowledge/engineering/2026-10/2026-10-15T164359Z-u6-and-u12-regressions-reproduced-before-the-selected-profile-and-quiet-closing-fixes-329374ac720c4dc88d6a77013d26f16f.md
- editor/src/app.rs
- editor/src/supervised_play/process/spawn.rs
- editor/src/platform.rs
- src/project_tooling/mod.rs
