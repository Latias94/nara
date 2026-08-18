---
type: "Verification Evidence"
title: "EUSC-U1 ordered Prefab structural patch authority"
description: "Verifies failure-atomic ordered Prefab entity structure, inverse history, canonical order, and continued component-structure rejection."
timestamp: 2026-08-18T02:33:52Z
record_id: "8386ed22067743af8cfb1f88b3ffd314"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fb66b84"
verified_by: "codex-root"
---

# Verification

EUSC-U1 was verified at commit `fb66b84` with Cargo commands run serially from the
repository root and with `CARGO_BUILD_JOBS=1` for the owner suites.

# Result

Passed. `PrefabAuthoringSession` now accepts ordered `AddEntity`, `RemoveEntity`,
`RestoreEntity`, and `MoveEntity` operations directly against an unpublished nominal
`PrefabDocument` candidate. Scene and Prefab share only private ordered-record mechanics; no
public generic document abstraction or Prefab-to-Scene mutation roundtrip was introduced.

Accepted transactions preserve exact placement, canonical record order, inverse/history
admission, monotonic revision, and branch-safe content state. A later cycle rejection leaves the
document, revision, content state, history depth, and retained history cost unchanged. Independent
component add/remove/replace operations remain closed with a typed Prefab diagnostic.

# Evidence

- `cargo nextest run --locked -p nara_scene --test-threads=1` - 110 passed.
- `cargo nextest run --locked -p nara --features runtime-core --test scene_patch_transactions
  --test scene_authoring_session --test-threads=1` - 39 passed.
- The focused Prefab transaction covers add, cross-owner move, subtree removal, `RestoreEntity`
  inverse construction, canonical order, Undo, and Redo in one history record.
- The focused rejection cases cover a cycle after an earlier candidate mutation and all three
  deferred component-structure operation kinds without publication.
- `cargo fmt --all -- --check` and `git diff --check` passed before commit; the latter reported only
  the repository's existing LF-to-CRLF notices.

# Follow-up

EUSC-U2 owns the closed Container/Text/Image/Button recipes, semantic brush migration, exact
current image choices, complete-source placement facts, and typed UI composition preflight. It
must not allocate Editor entity IDs or import tooling, egui, persistence, or GPU ownership.

# Citations

- [Editor UI Prefab Structural Composition v1 plan](../../../../plans/2026-08-18-0848-feat-editor-ui-prefab-structural-composition-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
