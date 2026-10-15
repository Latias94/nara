---
type: "Subagent Finding"
title: "U11 frame admission hides an existing document transaction and preservation-safe history"
description: "Subagent Finding for U11 frame admission hides an existing document transaction and preservation-safe history."
timestamp: 2026-10-15T16:31:50Z
record_id: "a04b19e987b94838aa9c1ff6e0648f08"
status: "open"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
subagent_id: "/root/u11_document_transactions"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
---

# Finding

U11 should remove frame-batch admission from ordinary document transactions instead of adding a
transaction manager. Generic Authoring also needs its preservation-safe multi-target field path
and history decoding migrated together; accepting a forward edit alone would break Undo/Redo.

# Evidence

- `src/project_host/runtime/editor/actions.rs` rejects an already-dispatched frame, reserves one
  mutation slot per document before attempting the operation, and compares a request's base
  revision with both the published frame and the live document.
- `crates/nara_scene/src/authoring.rs` already applies a multi-operation `ScenePatch` once through
  `apply_forward`, producing one revision and one inverse history entry.
- `crates/nara_tooling/src/workspace.rs` deliberately rejects raw patches in Generic Authoring.
  Its accepted `AuthoringFieldEdit` preparation in `authoring_decode.rs` currently selects one
  entity/component, and the history decoder enforces that same restriction.
- Geometry retains independent exact-display admission in `editor/src/platform/viewport.rs`.
  Browser, image import, runtime-generation, and Closing locks are separate boundaries.

# Recommendation

Use the existing document/base-revision transaction and conflict result. First prove that a
multi-field patch followed by Undo can execute without publishing a replacement frame. Extend
only preservation-safe field preparation/history to finite multi-target edits; keep unknown
data and unsupported structural operations protected. Do not add implicit batch merging,
rebasing, OT/CRDT, or another action protocol.

# Disposition

Source-confirmed read-only preparation from `/root/u11_document_transactions`. No files were
edited and no Cargo command was run by the worker. Production implementation remains queued
behind U6. This record does not claim a regression test or a completed U11 gate.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md (U11, KTD6/KTD7)
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- tests/workspace_play_runtime.rs
- tests/scene_patch_transactions.rs
