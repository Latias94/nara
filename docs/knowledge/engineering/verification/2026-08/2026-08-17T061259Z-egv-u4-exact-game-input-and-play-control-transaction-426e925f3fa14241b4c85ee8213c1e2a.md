---
type: "Verification Evidence"
title: "EGV-U4 exact Game input and Play control transaction"
description: "Verifies exact visible-generation Game input capture, balanced cleanup, visible Play controls, bounded Running cadence, and presentation-joined fixed Step behavior."
timestamp: 2026-08-17T06:12:59Z
record_id: "426e925f3fa14241b4c85ee8213c1e2a"
tags: ["editor", "game-view", "verification", "egv-u4"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "67199fa"
verified_by: "codex-root"
---

# Verification

Commit `67199fa` closes EGV-U4 against the corrected EGV-U3 baseline. The native Editor Host now
owns one bounded physical-input transaction and one explicit Play drive cadence without moving
window, GPU, runtime, or presentation authority into egui or the managed Play Runtime.

# Result

- A primary click inside the exact presented Game content enters one stable capture lease without
  forwarding the acquisition click. Keyboard, pointer, and mouse events use the existing normalized
  input drivers and retain an exact compatible presented stamp.
- Compatible composite progression preserves capture and held-input ownership. Stop, Restart,
  Escape, focus loss, modal activation, Scene handoff, resize, runtime fault, GPU retirement, and
  bounded input overflow fail closed with balanced endpoint cleanup.
- The fixed toolbar exposes Play, Pause, Resume, Step, Stop, Restart, and retry actions through the
  UI-neutral Editor action boundary, including Cancel during Starting and Stop during Stepping.
- Running Play advances on a bounded native cadence even when the Editor window has zero extent.
  Pause and suspension retire the cadence epoch, so Resume excludes hidden wall time.
- Step executes one fixed tick, remains visibly Stepping until an exact successor composite is
  accepted, survives transient presentation deferral through Paused redraws, and never repeats the
  tick.

# Evidence

- `cargo check --locked -p nara-editor` passed.
- `cargo nextest run --locked -p nara-editor` passed 10 tests; the existing native launch canary
  remained intentionally skipped.
- `cargo nextest run --locked -p nara --no-default-features --features desktop-winit,presentation-2d,presentation-ui,project-workflow,render-wgpu,runtime-ui,tooling-egui --test workspace_play_runtime -E 'test(editor_game_start_publishes_only_after_the_exact_native_activation)'` passed the focused Step presentation join.
- `cargo nextest run --locked -p nara_tooling_egui -E 'test(play_toolbar_emits_ui_neutral_pause_and_step_actions)'` passed the visible control transaction.
- `cargo check --locked -p nara --no-default-features --features runtime-2d,serde,tooling` passed the non-presenting Editor Host subset; existing unrelated dead-code warnings remain.
- `cargo fmt --all` and `git diff --check` passed.

# Follow-up

EGV-U5 is active under registration `668fed4a49b74cad8c80298445985c8f`. Use this commit as the
fixed baseline for the bounded starter trajectory, compiled Reference Game native canary,
resize/fault/recovery evidence, and final documentation closure. Do not reopen the input or cadence
contracts unless those product carriers expose a shared defect.

# Citations

- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- `editor/src/platform/game_input.rs`
- `editor/src/platform.rs`
- `src/project_host/runtime/editor/game_view.rs`
- `tests/workspace_play_runtime.rs`
