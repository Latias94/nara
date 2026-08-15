---
type: "Verification Evidence"
title: "EFI-U1 exact bounded authoring conformance"
description: "Verifies exact document and revision authority, single-owner selection, canonical Scene Tree projection, bounded branch-aware history, and receipt-bound clean-state recovery."
timestamp: 2026-08-15T18:50:31Z
record_id: "b8d1206722cd47a1931e34c05258c221"
resource: "editor-authoring-loop-v1"
tags: ["editor", "authoring", "efi-u1", "history", "persistence"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6f1444b"
verified_by: "focused-nextest-rust-1.95-trybuild-code-review"
---

# Verification

EFI-U1 is implemented by commit `6f1444b`. `SceneAuthoringSession` now publishes one exact,
monotonic authoring revision together with a branch-safe content-state identity, admits the
maximum cost of each forward/inverse pair before publication, and bounds combined Undo and Redo
history by records, structural items, value nodes, and logical bytes. Compound inverse payloads
charge embedded entity records, child edges, component records, nested operations, and stable
identifier bytes, so a component-free removed subtree cannot bypass the retained-history limits.

`EditorWorkspace` is the sole selection owner and projects the Scene Tree from canonical roots and
ordered children. Mutation, Undo, Redo, external reload, and explicit Reopen publication require
one non-optional document plus the exact captured `SceneAuthoringRevision`. Persistence checkpoints
bind the exact document, revision, content-state identity, and Scene snapshot; only a matching
verified receipt advances the saved marker.

# Result

- Exact document and revision mismatches reject before document, history, selection, dirty state,
  or persistence state changes.
- Undo can return to the saved content-state marker while revision remains monotonic; Redo or a
  divergent branch becomes dirty again, and history eviction cannot synthesize a clean state.
- Total record, structural-item, value-node, and logical-byte pressure evicts only the oldest Undo
  entries. A single over-limit inverse, including a large empty subtree, rejects before publication.
- A Reopen requested for an existing revision rejects as stale when a later accepted edit advances
  the document before Host publication; the later edit remains present and dirty.
- The independent Reference Game Editor journey consumes the exact command and persistence seams
  without becoming an architecture authority.
- The review-deferred egui field-buffer document/revision binding remains EFI-U3 scope; it was not
  pulled into this Scene/Tooling unit.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_scene -p nara_tooling`
  - passed: 134 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --features project-workflow,tooling-egui --test scene_authoring_session --test editor_persistence`
  - passed: 26 tests, including aggregate history pressure, large empty-subtree rejection, exact
    Reopen publication, old-receipt handling, and persistence capability opacity.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling_egui`
  - passed: 12 adapter compatibility tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --features editor --test editor_journey`
  - passed: 1 independent product journey.
- `CARGO_BUILD_JOBS=1 cargo +1.95.0 test --features project-workflow,tooling-egui --test editor_persistence persistence_authority_surface_is_opaque_and_linear -- --nocapture`
  - passed: the pinned Rust 1.95 compile-fail surface remains exact and private.
- `cargo fmt --all -- --check`
  - passed.
- `git diff --check`
  - passed before the implementation commit.
- A nine-persona code review covered correctness, project standards, testing, maintainability,
  performance, API contract, reliability, agent-native parity, and adversarial failure scenarios.
  The valid compound-history budget bypass and exact Reopen publication gaps were fixed and
  reverified; no remaining EFI-U1 P0 or P1 was accepted.

# Follow-up

Activate EFI-U2 only. Build one immutable `EditorFrameModel`, one ordered UI-neutral action ingress,
and one Host dispatch path; split the existing private Editor Host implementation by responsibility
without adding a public Host trait. Keep revision-bound egui drafts and fixed panel behavior in
inactive EFI-U3.

# Citations

- `crates/nara_scene/src/authoring.rs`
- `crates/nara_scene/src/patch.rs`
- `crates/nara_tooling/src/workspace.rs`
- `crates/nara_tooling/src/inspector.rs`
- `src/project_host/persistence.rs`
- `src/project_host/runtime/editor.rs`
- `tests/scene_authoring_session.rs`
- `tests/editor_persistence.rs`
- `tests/scene_inspector.rs`
- `reference-game/tests/editor_journey.rs`
- `docs/architecture/adr/0026-editor-command-patch-and-undo-model.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- Commit `6f1444b`
