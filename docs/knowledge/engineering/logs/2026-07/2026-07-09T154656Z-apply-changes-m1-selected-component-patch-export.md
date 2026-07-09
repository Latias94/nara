---
type: Memory Event
title: apply changes m1 selected component patch export
tags: nara,tooling,scene,apply-changes,verification
timestamp: 2026-07-09T15:46:56+08:00
status: passed
---

# Event

Apply Changes M1 is implemented for the selected-entity / explicit-component subset. The tooling
API now exports candidate `ScenePatchDocument` values from an isolated Play world and applies them
through `SceneAuthoringSession` for normal validation, revision updates, inverse patches, and undo.

# Impact

- Apply Changes now has a patch-producing selected-component subset.
- Runtime write-back remains explicit, selected, registry/codec driven, and patch-based.
- Runtime-only components, prefab-expanded entities, stale Play sessions, and failed patch
  validation reject with diagnostics instead of best-effort serialization.
- Stop Play still discards runtime state by default.

# Citations

- [Verification](../../verification/2026-07-09-apply-changes-m1.md)
- [Plan](../../../../plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md)
- [ADR 0034](../../../../architecture/adr/0034-editor-play-mode-world-boundary.md)
