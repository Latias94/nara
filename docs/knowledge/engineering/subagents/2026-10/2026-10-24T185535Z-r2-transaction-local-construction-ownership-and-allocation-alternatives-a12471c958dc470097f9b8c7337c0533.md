---
type: "Subagent Finding"
title: "R2 transaction local construction ownership and allocation alternatives"
description: "Read-only next-unit findings identify caller-owned construction choreography and compare owner drafts, declarative intents and scoped builders without adopting an allocator."
timestamp: 2026-10-24T18:55:35Z
record_id: "a12471c958dc470097f9b8c7337c0533"
status: "research-not-adopted"
producer_id: "codex"
run_id: "session-01a1629b-r2-construction-survey"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
subagent_id: "01a16f39-d6b6-72f0-9762-35a8c02df1e9"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "f7ab300c30aeeeb763292c05394f97082c097ce8"
---

# Finding

The next R2 gap is construction ergonomics and durable source-ID allocation, not another atomic
commit mechanism. Existing prepared edits already own the candidate, inverse and publication cut.
Independent consumers still hardcode new IDs, copy admitted template owner/version and assemble
AddEntity/AddComponent/reference/property operations. Workspace's ordinary single-command paths
separately allocate `editor_entity_N` / `editor_prefab_entity_N` after successful application.

# Evidence

The read-only agent inspected current source; it made no edits and ran no tests:

- `crates/nara_scene/src/authoring/prepared.rs`: owned candidate/preview/inverse and exact commit.
- `tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs`, `construction`:
  explicit IDs, copied templates and mutual EntityRefs; existing prepare/drop/stale consumers.
- `crates/nara_tooling/src/workspace.rs`, `apply_scene_composition`, Prefab composition and private
  allocation helpers: separate success-only ordinals rather than a prepared multi-entity owner API.
- `crates/nara_scene/src/authoring_decode/structure.rs`: generic AddEntity requires an empty entity;
  AddComponent must match the admitted template. Do not relax these guards to inject arbitrary records.
- `crates/nara_scene/src/authoring/property.rs`: property lowering begins from existing Available
  records. Sequential public prepare calls cannot resolve new-component fields inside one candidate.

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` allocates runtime entities in
`bevy_ecs/src/system/commands/mod.rs` and uses scene-local reference scopes in
`bevy_ecs/src/template.rs`. Its `bevy_scene/src/resolved_scene.rs` sequential application is not
Nara's document-level failure-atomic publication guarantee. Reuse the local-handle/mapping idea,
not runtime entity bits, callsite scope or running counters as authored identity.

Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` builds nodes and do/undo/reference records
in `editor/docks/scene_tree_dock.cpp`, defers reference restoration and resolves saved local IDs in
`scene/resources/packed_scene.cpp`. `core/object/undo_redo.cpp` executes recorded calls sequentially;
this does not establish final-operation rollback for Nara. These are mechanism references, not
comparative performance measurements or adopted designs.

# Recommendation

Compare these alternatives on the same two-entity/mutual-reference task before freezing the API:

1. An owner-bound move-only draft with opaque local handles, consumed into the existing PreparedEdit.
   Best initial candidate for retained tools; foreign-draft handles need exact private affinity.
2. A bounded declarative intent with local numbers. Convenient transport, but must not grow another
   recursive ComponentValue language or name resolver just to represent reference leaves.
3. A scoped closure builder with generative handles. Strong static isolation; compare ergonomics
   for tools that construct over several calls before committing.

The selected implementation must keep Scene as the allocation/template/candidate/inverse owner;
Tooling/Application forward one operation. Resolve local handles once inside the same candidate,
including forward, mutual and self references; admit final reference closure and replay/history
limits before publication. Bind nested reference leaves with R5a selectors, not string sentinels.
Expose an explicit local-to-authored-ID result instead of making callers parse patches.

Draft-local handles, authored `SceneEntityId` and process-local source/runtime incarnation are
different facts. Never persist `NEXT_DOCUMENT_SOURCE`, runtime counters or draft-local ordinals.
A private UUID-style source-ID generator is one candidate, not a decision: bounded collisions and
entropy failure need explicit outcomes. Random allocation is not proof of absolute historical
non-reuse. If that stronger property is required, compare persistent namespace/lineage authority
instead of silently deriving it from a process counter. Prepare/drop/stale/failure must leave
published document, selection, history, dirty state and allocator state unchanged; entropy already
consumed need not be rolled back. Commit must not reallocate an already prepared mapping.

Use the existing Scene/Prefab and independent window-free/Application consumers. Compare concept
count and measured prepare/commit/retention cost with their current choreography. Test stale basis,
foreign handles, final-operation failure, template/schema refusal, dangling reference and
history/inverse budget failure, plus Save/Reopen/Undo/Redo identity continuity. A root EntityRef
and one keyed nested reference leaf are sufficient to test the interface without a collection DSL.

# Disposition

Research only. The main owner has not adopted the draft API, ID strategy or cost targets, amended
their owning ADRs, or implemented these changes. ADRs 0026/0047/0118 own the eventual selected
contract; ADRs 0038/0058 retain their identity distinctions. Reuse the existing operation kernel
and migrate ordinary creation to its selected allocation owner instead of leaving two authorities.
Full field queries, panel-preparation hooks, cross-document transactions and remaining foundation
waves remain separate obligations, not silently removed from the goal.

# Citations

- [Foundation R2](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r2-owner-owned-prepared-editing-and-selection-independent-queries)
- [Durable property identity](../../../../architecture/durable-property-addressing-design.md)
- [Prepared source owner](../../../../../crates/nara_scene/src/authoring/prepared.rs)
- [Independent Application consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
- [Workspace owner](../../../../../crates/nara_tooling/src/workspace.rs)
