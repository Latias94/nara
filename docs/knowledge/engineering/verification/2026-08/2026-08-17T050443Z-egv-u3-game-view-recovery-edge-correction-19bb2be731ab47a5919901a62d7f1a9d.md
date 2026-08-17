---
type: "Verification Evidence"
title: "EGV-U3 Game View recovery edge correction"
description: "Rebinds EGV-U3 to the correction that makes inactive-lifecycle resize non-fatal and resets GPU recovery only after an exact viewport composite."
timestamp: 2026-08-17T05:04:43Z
record_id: "19bb2be731ab47a5919901a62d7f1a9d"
tags: ["editor", "game-view", "verification", "egv-u3", "correction"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4ad3bd2"
verified_by: "codex-root"
---

# Verification

Commit `4ad3bd2` is a focused successor to the EGV-U3 implementation commit `67c9638`. It
closes two post-review recovery edges without broadening the Game View contract or changing the
active EGV-U4 scope.

# Result

- Game resize now returns a non-fatal no-op unless the managed Runtime is Running or Paused. Faulted,
  Stepping, Stopping, and close/retirement states retain their observable lifecycle instead of
  disabling capture and failing the Editor.
- The automatic GPU-replacement budget resets only after a matching viewport composite is
  published. Presenting the Editor shell alone no longer fabricates Game/Scene recovery or permits
  an unbounded second automatic replacement.

# Evidence

- `cargo check --locked -p nara-editor` passed.
- `cargo nextest run --locked -p nara-editor` passed all 8 selected tests, with the existing
  platform launch canary intentionally skipped.
- `cargo fmt --all` and `git diff --check` passed.
- The correction is limited to `EditorProjectSession::resize_game_view` lifecycle admission and
  `EditorGpuRecovery` composite-success accounting.

# Follow-up

Continue EGV-U4 from commit `4ad3bd2`. Physical input capture must use the exact compatible Game
composite and must preserve this corrected lifecycle/recovery behavior.

# Citations

- [EGV-U3 primary verification](2026-08-17T045406Z-egv-u3-single-scene-and-game-viewport-presentation-c793ec2941d64f75be7e2607699a9e79.md)
- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- `editor/src/platform.rs`
- `src/project_host/runtime/editor.rs`
