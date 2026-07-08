---
type: Work Progress
title: Scene patch prefab schema foundation implemented
timestamp: 2026-07-08T14:52:08Z
tags: ["nara", "scene", "prefab", "patch", "schema", "migration"]
git_branch: "feat/scene-patch-prefab-schema"
related_plan: "../../../plans/2026-07-08-005-feat-scene-patch-prefab-schema-foundation-plan.md"
---

# Summary

Implemented the scene patch / prefab schema foundation from the related plan.

Completed units:

- Split `nara_scene` into document, validation, spawn, export, prefab, patch, format, and tests modules.
- Added structured `ComponentFieldPath` and component value field editing helpers.
- Added `ComponentSchemaCatalog`, owner-registered field schemas, and built-in schema registration.
- Added component value migration chains before scene/prefab preflight.
- Added atomic `ScenePatchDocument` transactions with operation-indexed diagnostics and inverse patch generation.
- Replaced whole-component prefab overrides with patch-based field overrides.
- Added `PrefabSourceResolver`, `InMemoryPrefabSourceResolver`, nested prefab expansion, cycle/missing/depth diagnostics, and deterministic `anchor/source_entity` namespacing.
- Added backend-free examples for schema export, scene patch JSON/RON roundtrip, and prefab patch overrides.
- Updated architecture docs and repo-local agent guidance to reflect the implemented contracts.

# Commits

- `39b1ec9 refactor(scene): split scene modules by responsibility`
- `57a4fcc feat(reflect): add component field path editing`
- `63dcfa5 feat(reflect): export component schema catalog`
- `08e978b feat(reflect): add component value migrations`
- `8cfdfc5 feat(scene): add atomic scene patch transactions`
- `fd0edad feat(scene): use patch transactions for prefab overrides`
- `63b791f feat(scene): add prefab source resolver expansion`
- `92d43a0 feat(scene): add patch schema prefab examples`

# Current Contracts

- Patch operations serialize as `op + args`.
- `ComponentTypeId`, `AssetRef`, and `ComponentFieldPathSegment` have stable serde shapes for JSON/RON.
- `ScenePatchDocument::apply_to_scene` uses authoring validation so unresolved prefab instances can be patched before resolver expansion.
- `SceneDocument::validate` and scene spawn remain strict: prefab instances must be expanded before runtime spawn.
- Prefab instance overrides are source-relative patches applied before namespacing.

# Next Action

Run final workspace verification, optional backend checks, dependency boundary searches, serialization leak searches, and memory validation before merging.

# Citations

- [Plan](../../../plans/2026-07-08-005-feat-scene-patch-prefab-schema-foundation-plan.md)
- [ADR 0026](../../../architecture/adr/0026-editor-command-patch-and-undo-model.md)
- [ADR 0011](../../../architecture/adr/0011-component-schema-ids-and-migrations.md)
- [ADR 0006](../../../architecture/adr/0006-scene-and-prefab-data-model.md)
