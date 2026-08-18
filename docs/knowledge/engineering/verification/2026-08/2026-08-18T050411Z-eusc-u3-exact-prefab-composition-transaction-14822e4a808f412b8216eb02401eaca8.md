---
type: "Verification Evidence"
title: "EUSC-U3 exact Prefab composition transaction"
description: "Verifies exact document and revision-bound Prefab structural transactions, failure-atomic ID allocation, history and selection publication, Host revalidation, gesture cancellation, and Canvas retirement."
timestamp: 2026-08-18T05:04:11Z
record_id: "14822e4a808f412b8216eb02401eaca8"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "278b986"
verified_by: "codex-root"
---

# Verification

EUSC-U3 was verified at commit `278b986` with Cargo commands run serially from the repository
root. Substantial builds used `CARGO_BUILD_JOBS=1`.

# Result

Passed for the focused unit gates. `nara_tooling` now owns one document- and revision-bound Prefab
composition transaction for Create, Move, Delete, and normalized NoOp. It prepares document,
inverse history, selection, content state, and allocator publication before committing them
together. A rejected or stale create consumes no entity identity and publishes no partial state.

The root Editor Host lowers structure only from the current `EditorUiAuthoringFrame`, then
revalidates the active document, source revision, complete source topology, catalog recipe,
generation-bound image choice, parent, and anchor before dispatch. A successful mutation cancels
the same-document gesture and retires the prior UI Canvas authority. Rejection and NoOp preserve
both. Same-batch document switches, stale Canvas picks, and post-mutation old-revision selection
are rejected before workspace mutation.

An independent static correctness review of `278b986` reported no remaining findings. Its earlier
active-document and exact-frame findings were closed before the commit.

# Evidence

- `cargo nextest run -p nara_tooling --test-threads=1` - 33 passed.
- `cargo nextest run --locked -p nara --lib --features
  'tooling,presentation-ui,render-wgpu,project-workflow,runtime-2d,serde' -E
  'test(editor_ui_authoring_frame_and_continuous_gesture_share_one_exact_prefab_revision)'
  --test-threads=1` - 1 passed.
- `cargo nextest run --locked -p nara --test editor_persistence --features
  'tooling,runtime-2d,serde' -E 'test(persistence_authority_surface_is_opaque_and_linear)'
  --test-threads=1` - 1 passed.
- The two focused stale-reservation tests passed, proving that a rejected attempt does not leak or
  consume a backend reservation.
- The owner tests cover exact Create allocation and failure atomicity, Move/Delete/NoOp and Undo,
  selection publication, cross-document command rejection, same-batch active-document handoff,
  stale pick/revision rejection, successful gesture cancellation, and Canvas retirement.
- `cargo fmt --all -- --check` and `git diff --check` passed before commit.

A broader root-library run advanced through 28 of 78 selected tests, then the existing Windows
`prepared_publication_unwind_keeps_the_host_slot_empty` test aborted with process code
`0xc0000409` after its intentional injected pre-publication panic. No EUSC-U3 assertion failed
before that abort. This run is not claimed as a complete root-library pass; the focused unit and
integration gates above are the evidence for this unit.

# Follow-up

EUSC-U4 owns the egui structural authoring workflow and Adapter parity. It must consume the exact
frame-lowered commands and current placement/catalog facts without owning entity allocation,
schema paths, structural validation, persistence, history, selection, or source data.

# Citations

- [Editor UI Prefab Structural Composition v1 plan](../../../../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
