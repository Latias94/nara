---
type: "Verification Evidence"
title: "EGV-U5 embedded Game View product conformance"
description: "Verifies the generic starter trajectory, compiled Reference Game native Game View canary, bounded lifecycle recovery, and final in-process authority closure."
timestamp: 2026-08-17T10:08:47Z
record_id: "9d0b0a17beef4c39bea70a8d6f025455"
tags: ["editor", "game-view", "verification", "egv-u5"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "b5697f3"
verified_by: "codex-root"
---

# Verification

Commit `b5697f3` closes EGV-U5 over the verified EGV-U1 through EGV-U4 authority. It proves the
first complete embedded Game View workflow with two deliberately different consumers: the generic
`desktop-2d/v1` starter exercises the Editor-domain lifecycle without native presentation, while
the compiled Reference Game exercises the real native Winit/WGPU/egui composite with sprites,
Runtime UI, captured input, Pause, exact Step, resize, and Stop.

# Result

- The generic starter accepts one unsaved built-in Scene edit and starts Play from that exact
  revision. Scene preview and Game candidates observe the edit while the source file remains
  unchanged.
- Pause, paused resize, one exact fixed Step, Scene authoring while Paused, out-of-date feedback,
  Restart from the successor revision, Stop without write-back, and explicit Save/Reopen all use
  the existing Editor document, Play, history, persistence, and Game View authorities.
- The compiled Reference Game uses the explicit `desktop` profile plus
  `desktop_wave_recipe_or_panic()` exactly once. One native Editor window presents 129 sprite
  instances and at least one concrete Runtime UI paint packet, admits captured `D` input only after
  a compatible final composite receipt, visibly moves the tracked sprite, remains Paused across an
  ordinary resize, advances one visible fixed Step, and retires cleanly on Stop.
- Empty Runtime UI backend completion batches still drain terminal content outcomes. Bounded input
  overflow releases capture and publishes one fail-closed cleanup instead of panicking. Input
  rejection retains typed event and Runtime-scope diagnostics.
- The implementation remains private to the first-party Editor and compiled product canary. It
  adds no public image Surface endpoint, Viewport registry, RenderGraph, runtime-session trait,
  second device, async render worker, child-process placement, or live-edit mechanism.

# Evidence

- `cargo nextest run --locked -p nara-editor -E 'test(starter_game_view_preserves_unsaved_source_and_explicit_play_lifecycle)'`
  passed 1 focused test with 11 tests skipped.
- `cargo check --locked -p nara-editor` passed the private Editor package and feature closure.
- `cargo build --manifest-path reference-game/Cargo.toml --locked --features editor-game-view-canary --bin editor_game_view_canary`
  passed with one Cargo build job.
- `reference-game/target/debug/editor_game_view_canary.exe` passed on Windows and emitted one
  structured record with runtime generation 1, initial/input/paused/stepped frames 10/19/20/23,
  geometry generation 3, resized extent 1120x869, 129 sprite instances, concrete Runtime UI, and
  positive input/Step movement deltas.
- `cargo fmt --all` and `git diff --check` passed.
- Focused correctness validation confirmed the empty-outcome, concrete-UI-packet, input-overflow,
  and canary Step fixes. The reported missing Game-tab transition was rejected against the existing
  automatic Scene-to-Game reconciliation and the successful native canary.
- EGV-U1 through EGV-U4 immutable records retain the deterministic target deferral, stale-route,
  GPU-owner replacement, repeated-recovery, Runtime-fault, retry, and exact input-cleanup evidence;
  U5 did not rerun a native fault or platform matrix.

# Follow-up

Editor Game View v1 is complete and no successor implementation unit is active. OQ-039 remains
open for a second placement or measured failure of the private in-process model. OQ-040 remains
open for live document-to-runtime projection beyond the existing explicit bounded Apply Changes
path. Any child-process Play, live edit, multiple Game Views, detached windows, gamepad/relative
mouse, or embedded platform-accessibility work requires its own trigger and focused authority.

# Citations

- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- [OQ-039 and OQ-040](../../../../architecture/open-questions.md)
- `editor/tests/authoring_loop.rs`
- `editor/src/platform/canary.rs`
- `editor/src/platform/game_input.rs`
- `editor/src/platform/viewport.rs`
- `src/project_host/runtime/editor/game_view.rs`
- `reference-game/src/bin/editor_game_view_canary.rs`
