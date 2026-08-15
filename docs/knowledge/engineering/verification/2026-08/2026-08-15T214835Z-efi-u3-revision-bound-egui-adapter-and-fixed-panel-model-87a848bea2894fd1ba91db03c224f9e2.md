---
type: "Verification Evidence"
title: "EFI-U3 revision-bound egui Adapter and fixed panel model"
description: "Verifies the exact-frame egui Adapter, revision-bound transient drafts, ordered actions, fixed panel model, and exact persistence retry identity."
timestamp: 2026-08-15T21:48:35Z
record_id: "87a848bea2894fd1ba91db03c224f9e2"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u3", "egui"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3e112b4"
verified_by: "focused-nextest-check-code-review"
---

# Verification

EFI-U3 is implemented by commit `3e112b4`. `nara_tooling_egui` now consumes exactly one immutable
`EditorFrameModel` and returns exactly one UI-neutral `EditorActionBatch`. The Adapter no longer
owns document, selection, persistence, Play, or Host mutation state, and its former grouped
response types have been removed rather than retained as a compatibility surface.

Inspector drafts and egui identities are bound to the exact document, source revision, entity,
component, and field shown by the originating frame. Reopen, document replacement, revision
change, or target removal retires obsolete transient state before it can emit an action. The fixed
toolbar, canonical Scene Tree, Inspector, and Diagnostics consume the same frame generation.

Persistence failure results now retain the exact Save or Reopen request kind and document. Only a
matching retry is admitted while the failure remains visible; unrelated operations remain blocked,
and retry admission does not erase the prior failure before completion.

# Result

- The public egui surface is one `EguiSceneEditorPanel::show` operation over an immutable frame; it
  is marked `must_use` because discarding the ordered action batch would lose user intent.
- Field buffers are created only for edited or invalid values and cannot survive a revision or
  target change under a stale egui ID.
- Toolbar actions preserve Undo, Redo, Save, and Reopen ordering and derive enabled state from the
  frame model rather than live Host reads.
- The fixed panel model gives Scene Tree, Inspector, and Diagnostics bounded independent scrolling
  without adding docking, Viewport, multi-window, or a persistent layout graph.
- No egui type enters `nara_tooling`; the later custom-drawn Nara UI panel can consume the same
  frame and action vocabulary.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling_egui`
  - passed: 7 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling runtime-2d serde" --test editor_persistence`
  - passed: 12 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling runtime-2d serde" --lib failed_and_uncertain_saves_do_not_advance_workspace_persistence_state`
  - passed: 1 focused regression.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features "tooling-egui runtime-2d serde" --test scene_play_mode`
  - passed: 2 tests before the persistence-only refinement; the refinement did not touch Play.
- `CARGO_BUILD_JOBS=1 cargo check -p nara_tooling_egui`
  - passed.
- `CARGO_BUILD_JOBS=1 cargo check --features tooling-egui`
  - passed with only unrelated existing warnings.
- `git diff --check`
  - passed apart from repository line-ending notices.
- Final API, correctness, and simplification reviews found no remaining EFI-U3 P0 or P1. The Save
  retry identity issue found during review was corrected and covered by focused persistence gates.

# Follow-up

Activate EFI-U4 only. Add the private `publish = false` native Editor application, keep native
window/event-loop/egui renderer ownership inside that product package, and close the first visible
field-validation and stale-action feedback loop without introducing docking, Viewport, a public
Editor application framework, or a generic binding system.

# Citations

- `crates/nara_tooling_egui/src/lib.rs`
- `crates/nara_tooling_egui/src/editor.rs`
- `crates/nara_tooling_egui/src/inspector.rs`
- `crates/nara_tooling_egui/src/fields.rs`
- `crates/nara_tooling_egui/src/tests.rs`
- `crates/nara_tooling/src/persistence.rs`
- `src/project_host/runtime/editor/persistence.rs`
- `tests/editor_persistence.rs`
- Commit `3e112b4`
