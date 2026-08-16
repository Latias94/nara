---
type: "Verification Evidence"
title: "ESV-U2 revision-bound Scene Composition"
description: "Verifies bounded schema-backed Scene Composition commands, exact revision rejection, canonical hierarchy projection, single-owner selection, and one-record atomic history."
timestamp: 2026-08-16T09:17:30Z
record_id: "2ea0e3e881ff4c6a9d98e52e0937445a"
resource: "editor-scene-composition-view-v1"
tags: ["editor", "scene-composition", "tooling", "esv-u2"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "74dc037"
verified_by: "focused-nextest-check"
---

# Verification

Commit `74dc037` adds one bounded Scene Composition command algebra in `nara_tooling`. The
workspace lowers those commands into existing `ScenePatchOperation` values only after exact
document/revision admission. Existing `SceneAuthoringSession` patch, history, and validation
owners remain unchanged.

# Result

- Empty Entity and Colored Sprite creation allocate stable Editor-owned identities. Colored
  Sprite creation adds its complete `Transform2d` and untextured `Sprite` records in one atomic
  patch and one undo record.
- Admitted component add/remove and cycle-safe move/reparent reuse the existing patch authority.
- Creation selects the new entity; moving preserves selection; removing a selected subtree moves
  selection to the surviving parent or clears it.
- Failed schema admission and stale revision rejection leave document revision, selection,
  history, and the identity allocator unchanged.
- Canonical Scene Tree rows continue to come from ordered roots/children with parent and depth
  facts. No adapter-side hierarchy or selection owner was introduced.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling --no-fail-fast`
  - passed: 26 tests.
- `CARGO_BUILD_JOBS=1 cargo check -p nara --features tooling`
  - passed; only pre-existing dead-code warnings were reported.
- `cargo fmt --all`
  - completed.
- `git diff --check`
  - passed before the implementation commit.

# Evidence Boundary

This record proves the UI-neutral Scene/Tooling owner contract. It does not claim egui controls,
preview-world materialization, a real Scene target, presented-generation picking, or the final
save/reopen/downstream trajectory. Those remain ESV-U3 through ESV-U5.

# Follow-up

Activate ESV-U3 only. Materialize accepted authoring snapshots into a private edit-preview World
and promote preview facts only through the exact U1 composite receipt. Keep egui controls and
pointer picking inactive until U4.

# Citations

- `crates/nara_tooling/src/composition.rs`
- `crates/nara_tooling/src/workspace.rs`
- `crates/nara_tooling/src/frame.rs`
- `src/project_host/runtime/editor/actions.rs`
- `docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md`
- Commit `74dc037`
