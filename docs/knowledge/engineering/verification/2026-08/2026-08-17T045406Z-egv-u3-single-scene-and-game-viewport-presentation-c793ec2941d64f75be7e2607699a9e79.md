---
type: "Verification Evidence"
title: "EGV-U3 single Scene and Game viewport presentation"
description: "Verifies mutually exclusive Scene and Game viewport ownership, exact Game composite completion, bounded successor scheduling, resize, and safe GPU recovery."
timestamp: 2026-08-17T04:54:06Z
record_id: "c793ec2941d64f75be7e2607699a9e79"
tags: ["editor", "game-view", "verification", "egv-u3"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "67c9638"
verified_by: "codex-root"
---

# Verification

Commit `67c9638` implements EGV-U3 against ADR 0112. This record verifies the single central
Scene/Game viewport, exact Game image and Runtime UI completion, source handoff, resize succession,
and bounded GPU-owner recovery. Ordinary Game input capture and the visible Pause/Step product loop
remain EGV-U4; the native starter-project canary remains EGV-U5.

# Result

- Scene Preview and managed Game Runtime now share one mutually exclusive private viewport slot and
  one Editor GPU authority. Source changes admit an explicit zero-target handoff and terminalize
  any buffered Game candidate before retiring its target.
- The Game path preserves the Runtime logical-window view while mapping only backend execution to
  the private image. egui must sample the exact registered texture before the final Editor-window
  composite receipt can complete the owning Runtime UI submission.
- Deferred WGPU work retains exact retry authority. Content rejection completes the rejected
  candidate once and schedules a bounded wait for a valid successor instead of parking it.
- Resizable Game profiles advance geometry generation, update the logical Runtime window through a
  typed driver port, retire the stale texture/composite generation, and admit the matching
  successor target.
- Device-owner replacement first suspends Game capture and reaches a Runtime safe pause. Scene
  authoring remains available after a faulted handoff, while explicit retry owns any second GPU
  replacement attempt.
- Native platform orchestration remains in `platform.rs`; target, texture, Scene presenter, and
  Game presenter internals moved into the private `platform/viewport.rs` deep module.

# Evidence

- `cargo check --locked -p nara-editor` passed after the private module split.
- `cargo nextest run --locked -p nara-editor` passed all 8 selected tests, with the existing
  platform launch canary intentionally skipped.
- `cargo nextest run --locked -p nara_tooling_egui` passed all 12 adapter tests.
- `cargo nextest run --locked -p nara_render_wgpu --features ui-submitter
  editor_ui_completion_maps_every_terminal_path_without_stranding_permits` passed the focused
  exact-terminal test.
- The focused root `editor_game_start_publishes_only_after_the_exact_native_activation` test
  passed with the Runtime/UI/Desktop/WGPU feature closure and now covers resize succession plus
  buffered-candidate retirement.
- `cargo fmt --all`, `git diff --check`, and the final staged-scope review passed. Independent
  maintainability re-review reported no remaining finding; the standards finding requiring the new
  viewport module to be tracked was closed by the exact U3 commit.

# Follow-up

EGV-U4 is active. Add Host-owned physical input capture and exact visible-generation routing,
surface ordinary Pause/Step/Stop/Restart controls, preserve balanced held-input cleanup, and add the
bounded drive cadence without changing Runtime fixed-tick semantics.

# Citations

- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- `crates/nara_render/src/lib.rs`
- `crates/nara_render_wgpu/src/editor_execution.rs`
- `src/project_host/runtime/editor/game_view.rs`
- `editor/src/platform.rs`
- `editor/src/platform/viewport.rs`
- `tests/workspace_play_runtime.rs`
