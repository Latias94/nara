---
type: "Verification Evidence"
title: "EGV-U2 managed Game View runtime placement"
description: "Verifies pre-required-slot Editor Play placement, coordinated unpublished Runtime and target activation, and exact move-only Game candidate ownership."
timestamp: 2026-08-17T01:48:19Z
record_id: "4012c0efb4d046c78031b9ad6d30a2c3"
tags: ["editor", "game-view", "verification", "egv-u2"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "66a1433"
verified_by: "codex-root"
---

# Verification

Commit `66a1433` implements EGV-U2 against the placement and ownership boundary accepted by ADR
0112. This record verifies the private Play placement and candidate handoff only; final Game image
submission, Editor-window composite publication, gameplay input, and the native product trajectory
remain EGV-U3 through EGV-U5 work.

# Result

- Editor Play now lowers the selected project profile before ordinary native/backend required-slot
  materialization. The Play Runtime retains the logical Window plus renderer-neutral 2D/UI
  producers while WGPU, submitters, Winit, and platform accessibility remain Editor-owned.
- `ProjectHost` can prepare an unpublished managed Runtime, then either atomically publish it with
  the matching native target activation or abort it into bounded retirement. No runnable Runtime is
  visible before the coordinated commit.
- The private Runtime bridge exports one move-only candidate carrying the exact runtime generation,
  logical-window render packet, optional sprite batches, and optional Runtime UI presentation work.
- Runtime UI backend permits return through the owning Runtime transaction before another candidate,
  Stop, Restart, or retirement can proceed. Foreign starts and foreign completions are rejected.
- The generic Editor keeps its empty recipe, while the private launch-with-intent entry preserves an
  already compiled product recipe without adding a dynamic loader or second composition authority.

# Evidence

- `cargo nextest run --locked --no-default-features --features
  "runtime-2d,serde,tooling,presentation-2d,presentation-ui,desktop-winit,render-wgpu" --test
  workspace_play_runtime` passed all 17 tests, including coordinated start, foreign activation,
  candidate completion, Stop, and abort retirement.
- `cargo nextest run --locked --no-default-features --features
  "runtime-2d,runtime-ui,serde,tooling,presentation-2d,presentation-ui,desktop-winit,render-wgpu"
  --lib -E
  "test(project_host::runtime::tests::editor_play_profile_keeps_logical_producers_without_native_gpu_contributions)"`
  passed the focused profile-placement contract.
- `cargo nextest run -p nara_sprite_render` passed all 20 tests after changing retained sprite
  batches to an immutable shared slice for move-only candidate transfer.
- The focused `nara_ui_render` Host-external outcome test passed after reusing a retained completion
  scratch set instead of allocating one per frame.
- `cargo check --locked -p nara-editor`, plus focused sprite-only and Runtime-UI-only root feature
  checks, passed. `cargo fmt --all` and `git diff --check` passed.

# Follow-up

Activate EGV-U3. Consume the candidate through the existing single Editor WGPU authority, enforce a
zero-target Scene/Game handoff, publish one exact final composite stamp, and return the matching UI
terminal result to the Runtime. Do not add gameplay input or the bounded drive cadence yet; those
remain EGV-U4.

# Citations

- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- `src/product/profile.rs`
- `src/project_host/runtime.rs`
- `src/project_host/runtime/editor/game_view.rs`
- `editor/src/platform.rs`
- `tests/workspace_play_runtime.rs`
