---
type: "Verification Evidence"
title: "EGV-U1 Game View presentation authority"
description: "Verifies inactive viewport admission, logical-window Runtime UI mapping, exact external outcome rendezvous, source handoff, and final composite authority."
timestamp: 2026-08-16T17:24:00Z
record_id: "ee246fd4862d4f0fb06423f9f6bbd9a0"
tags: ["editor", "game-view", "verification", "egv-u1"]
producer_id: "codex-root"
run_id: "goal-019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8942bbb"
verified_by: "codex-root"
---

# Verification

Commit `8942bbb` implements the EGV-U1 tracer and owner contracts selected by ADR 0112. The
verification is intentionally focused: it does not claim a live managed Play Runtime, gameplay
input, or the final Scene/Game product workflow.

# Result

- Runtime UI work can leave its owning Runtime only after backend-outcome capacity is reserved.
  Foreign runtimes cannot consume the move-only permit, and the owning Runtime accepts each
  terminal outcome once.
- The private Editor WGPU owner keeps Runtime Camera/UI identity on the logical primary window while
  lowering the admitted payload into one private image target. The image receipt retains exact
  source, source-handoff, target, backend, device, and render-transaction generations.
- Viewport allocation and egui texture registration remain inactive until a validated move-only
  activation is committed. Submission cannot create or replace a target implicitly.
- Runtime UI presentation reaches `Presented` only after the exact image is sampled by egui and the
  Editor window publishes the matching composite receipt. Deferred, content rejection, target
  fault, device fault, and supersession retain exact terminal authority.
- The synthetic owner-local start tracer rejects mismatched Runtime/target reservations without
  partial publication and commits a matching pair together. EGV-U2 must replace that proof with the
  real unpublished `RuntimeStartAttempt` and private placement.

# Evidence

- `cargo nextest run -p nara_ui_render host_external_backend_frame_returns_each_outcome_to_its_owning_runtime_once`
  passed: 1 test.
- `cargo nextest run -p nara_render_wgpu --features ui-submitter editor_` passed: 7 tests covering
  inactive admission, source handoff, logical-window mapping, terminal outcomes, and exact composite
  authority.
- `cargo nextest run -p nara_render_wgpu --features ui-submitter ui_outcomes_` passed: 2 existing
  backend-generation and fault-domain regressions after sharing the terminal mapper.
- `cargo nextest run -p nara --features presentation-ui host_external_backend_rendezvous_reexports_the_exact_deferred_retry`
  passed: the exact Deferred retry was reinstalled, re-exported, and Presented by its owning Runtime.
- `cargo check -p nara_render_wgpu` and
  `cargo check -p nara_render_wgpu --features sprite-submitter,ui-submitter` passed, proving the base
  feature ceiling does not read the optional Game View payload.
- `cargo check -p nara-editor`, `cargo fmt --all -- --check`, and `git diff --check` passed. The Editor
  check reported one pre-existing unused private composition helper outside this commit.

# Follow-up

Activate EGV-U2. Implement the root-private pre-required-slot Play placement, preserve the selected
profile/recipe/schema lineage, retain `RuntimeStartAttempt` unpublished until the matching native
target activation exists, and export one exact renderer-neutral Game candidate without installing a
second native or WGPU owner inside Play.

# Citations

- [Editor Game View v1 plan](../../../../plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md)
- [ADR 0112](../../../../architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md)
- `crates/nara_ui_render/src/integration.rs`
- `crates/nara_render_wgpu/src/editor_execution.rs`
- `crates/nara_render_wgpu/src/frame.rs`
- `editor/src/platform.rs`
- `src/ui_presentation/tests.rs`
