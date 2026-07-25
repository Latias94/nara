---
type: "Engineering Research"
title: "Durable asset, product, document, and prefab identity source audit"
description: "Non-normative primary-source audit of Nara ADR 0083 against pinned Godot and Bevy source plus Unity official identity, prefab, import-product, and merge contracts."
timestamp: 2026-07-21T13:12:55Z
record_id: "790f04dcc82645e890d83b58293f0504"
resource: "docs/architecture/adr/0083-durable-project-asset-and-document-entity-identity.md"
tags: ["architecture", "identity", "asset", "scene", "prefab", "package", "import", "bevy", "godot", "unity"]
status: "research"
producer_id: "codex-durable-identity-primary-sources"
run_id: "20260721-adr0083-primary-source-audit"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f7e5ee283e06ff156224b0f11fcc1df0c31284a3"
---

# Summary

ADR 0083 has the right high-level direction: movable asset identity, source-local imported-product
identity, document-local authored-entity identity, prefab projection provenance, runtime scene
instance identity, and `bevy_ecs::Entity` must remain separate axes. Pinned Godot and Bevy source
and Unity 6.0 official documentation all support that separation.

The current proposal is not ready for acceptance. The main gaps are not the UUID width or text
encoding. They are identity scope and reference lowering:

1. An entity-bearing document is not necessarily identified by `StableAssetId` alone once one
   source may publish multiple products. The durable document key must be an `AssetProductRef`, or
   version 1 must explicitly prohibit auxiliary entity-bearing products.
2. A source-document entity reference cannot select a live entity when the same document has zero,
   one, or many runtime instances. Authoring references, prefab-projection references, explicit
   outer-instance bindings, and runtime references need distinct representations and lowering
   rules.
3. `PrefabProjectionLocator.root_scene_instance` cannot be a persistent runtime
   `SceneInstanceId`. Persistent provenance needs an authored root anchor plus an exact nested source
   chain. Runtime lookup composes that authoring key with a runtime scene instance separately.
4. The project, its resolved package content, and immutable mounted content need one declared
   `StableAssetId` collision domain. A collision must reject composition or enter an explicit
   import/fork remap transaction; a read-only package must never be silently rewritten.
5. Whole-asset duplication, product retirement, deleted entity IDs, source-control merges, and the
   pre-1.0 migration each need deterministic rules. Stable IDs improve these operations but do not
   make them correct by themselves.

The recommended whole-asset duplicate rule is deterministic: allocate a new `StableAssetId`,
preserve source-local `ImportedProductId` and document-local `SceneEntityId` values because their
enclosing scope changed, and atomically rewrite references inside the copied closure that targeted
the original asset to target the copy. Same-document entity or subtree duplication still allocates
new entity IDs and rewrites only references inside the copied closure.

This record is non-normative research evidence. It does not accept ADR 0083, select public Rust type
names, authorize implementation, or change the active plan.

# Question

Does the proposed `StableAssetId + ImportedProductId + SceneEntityId + structured prefab
provenance + runtime identity` model remain coherent under real engine behavior for asset moves,
packages, compound importers, scene and prefab duplication, cross-document references, deletion,
merge, migration, and crash recovery?

# Repository Baseline

The audit used these fixed baselines:

- Nara commit `f7e5ee283e06ff156224b0f11fcc1df0c31284a3` for tracked implementation
  evidence. The dirty working-tree ADR 0083 and related Proposed ADRs were read as proposal evidence,
  not implementation authority.
- Bevy commit `f6c6e6eebb94e81c090614f19039319e9acb3c85`, dated 2026-07-07, under
  `repo-ref/bevy`.
- Godot commit `c939bf3791ce40ff70e0ee29f06486da1ebb6a84`, dated 2026-07-07, under
  `repo-ref/godot`.
- Unity 6.0 official documentation at versioned `6000.0` URLs, retrieved on 2026-07-21. Unity's
  engine internals are not available in the local reference tree, so only first-party documentation,
  the official Unity C# reference source, and one first-party Issue Tracker record were used.

The relevant Nara implementation is transitional:

- `StableAssetId` wraps `Uuid`, while current parsing accepts any syntactically valid UUID,
  including nil; ADR 0083 proposes non-nil IDs
  (`crates/nara_asset/src/identity.rs:30-49`).
- `AssetMeta` currently stores both `stable_id` and `path`, and the database rejects one active ID
  at two paths only after comparing current records
  (`crates/nara_asset/src/database.rs:38-52`, `:211-251`).
- `SceneEntityId` is still a validated path-like string
  (`crates/nara_identity/src/types.rs:15-67`).
- Prefab expansion currently namespaces only entity IDs and parent IDs as
  `anchor/local`; it does not rewrite declared entity references inside component values
  (`crates/nara_scene/src/prefab.rs:888-960`, `:1043-1053`).
- Persistent `EntityReference::SceneLocal` stores only `SceneEntityId`, and
  `RuntimeEntityReference::Scene` stores only `SceneInstanceId + SceneEntityId`
  (`crates/nara_identity/src/types.rs:343-379`).
- The reference-game enemy prefab stores `SceneLocal("player")`, then resolves it against the
  containing runtime scene instance. That deliberately crosses from the prefab source into the
  outer scene without an explicit prefab binding
  (`reference-game/src/components.rs:53-66`, `reference-game/src/systems.rs:721-736`).
- Reference-game systems also select the player and a specific enemy by comparing authored ID text
  such as `"player"` and `"enemy-anchor/enemy"`. Opaque identity cannot safely retain that gameplay
  role (`reference-game/src/snapshot.rs:110-120`,
  `reference-game/src/systems.rs:463-472`, `:1211-1234`).
- ADR 0091 already proposes the committed project snapshot, multi-document publication fence, and
  bounded recovery journal that an identity migration would need. ADR 0083 should not create a
  competing transaction or journal (`docs/architecture/adr/0091-editor-persistence-recovery-and-concurrent-writer-policy.md:69-113`,
  `:138-159`).

# Refined Identity Model

The following names are illustrative, not proposed public API commitments.

| Axis | Recommended meaning | Persistent equality scope | Must not be confused with |
|---|---|---|---|
| `StableAssetId` | Logical source asset across move, rename, reimport, and package update | One composed project content universe | Path, package release, artifact digest, runtime `AssetId` |
| `ImportedProductId` | One logical product across reimport | All product kinds within one `StableAssetId` | Label, name, array index, Rust type, content hash |
| `AssetProductRef` | Complete durable product or document key | `StableAssetId + ImportedProductId` | Runtime handle or residency lease |
| `SceneEntityId` | One authored entity record | One entity-bearing `AssetProductRef` document | Name, hierarchy path, prefab projection, runtime entity |
| Source entity reference | One authored source record | `AssetProductRef + SceneEntityId` | A live instance selection |
| Prefab projection path | One projected authored location | Root authored anchor plus bounded nested source-document/anchor chain and terminal entity | Runtime `SceneInstanceId` |
| Runtime scene entity key | Local or structured projected key inside one runtime scene instance | One `SceneInstanceId` | Source-document identity alone |
| Runtime entity reference | Live lookup key | `SceneInstanceId + runtime scene entity key`, or an ADR 0058 persistent-runtime reference | `bevy_ecs::Entity` |
| `bevy_ecs::Entity` | Allocator slot and generation | One `World` allocation history | Any persistent or cross-World identity |

This implies the following lowering route:

```text
SourceEntityRef { document: AssetProductRef, entity: SceneEntityId }
    -> authoring/source resolution only

PrefabProjectionPath {
    root_anchor: SceneEntityId,
    steps: [{ source_document: AssetProductRef,
              source_entity_or_nested_anchor: SceneEntityId }, ...]
}
    -> one key in the expanded authoring projection

RuntimeEntityReference::Scene {
    instance: SceneInstanceId,
    key: Local(SceneEntityId) | Projected(PrefabProjectionPath)
}
    -> host-owned runtime map
    -> bevy_ecs::Entity
```

Every non-terminal prefab step must identify the authored prefab-instance anchor that selects the
next source document. The terminal step identifies the source entity. This distinguishes two nested
instances of the same source prefab without manufacturing another `SceneEntityId`.

A direct source reference deliberately does not load a document or choose a runtime instance. If a
runtime component needs an entity from another scene instance, a loader, gameplay domain, or
explicit scene-link binding must select that instance and produce a runtime reference. Zero or
multiple eligible instances is a typed failure unless a separate domain contract defines selection.

# Primary-Source Findings

## Godot

Godot now uses a hybrid model. It is no longer accurate to describe current Godot scenes as purely
path identified.

### Resource identity

- `ResourceUID` is a positive 63-bit identity with a `uid://` text form. Random creation checks the
  current UID set; the UID is not a path
  (`repo-ref/godot/core/io/resource_uid.h:41-84`,
  `repo-ref/godot/core/io/resource_uid.cpp:55-124`).
- UID-to-path state lives in a project cache at `uid_cache.bin`, and `add_id`, `set_id`, and
  `remove_id` maintain the current projection
  (`repo-ref/godot/core/io/resource_uid.cpp:47-49`, `:155-230`, `:252-333`).
- Text resources serialize both UID and fallback path. Loading prefers a known UID but falls back
  to the path with a warning when the UID cannot be resolved
  (`repo-ref/godot/scene/resources/resource_format_text.cpp:487-506`, `:1880-1888`).
- Editor moves preserve the UID by moving the source and its `.uid` or `.import` companion. Resource
  duplication normally creates a new UID, and directory copy rewrites dependencies within the copied
  closure
  (`repo-ref/godot/editor/docks/filesystem_dock.cpp:1560-1598`,
  `repo-ref/godot/editor/file_system/editor_file_system.cpp:3127-3190`, `:3640-3655`).
- Duplicate active UIDs are detected, but Godot may automatically reassign one scanned file. Nara's
  proposed fail-closed candidate publication is intentionally stricter
  (`repo-ref/godot/editor/file_system/editor_file_system.cpp:920-943`, `:1398-1411`).
- Deletion removes the active UID mapping. The audited source has no durable asset-deletion
  tombstone or lineage proof (`repo-ref/godot/editor/file_system/editor_file_system.cpp:2415-2423`).

This supports path-independent asset identity, but it does not prove Nara's UUID representation,
cross-package collision policy, or crash-safe source-plus-sidecar transaction.

### Scene-local identity and path repair

- `SceneState` stores readable node paths, parallel ID paths, and scene-local node IDs
  (`repo-ref/godot/scene/resources/packed_scene.h:41-48`).
- On save, every saved node receives a positive 31-bit `unique_scene_id`. Missing or colliding IDs
  are regenerated (`repo-ref/godot/scene/resources/packed_scene.cpp:1172-1196`).
- ID paths follow the independent scene `owner` chain, not simply the structural parent chain
  (`repo-ref/godot/scene/resources/packed_scene.cpp:1464-1489`). `owner` is separate from parent and
  selects what belongs to a saved scene (`repo-ref/godot/scene/main/node.cpp:2300-2316`,
  `repo-ref/godot/scene/resources/packed_scene.cpp:865-886`, `:1106-1119`).
- Text scenes retain human-readable `parent` and `owner` paths while writing node `unique_id` and
  parent/owner ID paths (`repo-ref/godot/scene/resources/resource_format_text.cpp:2014-2048`).
- Instantiation first uses NodePath and falls back to ID-path recovery when inherited nodes moved or
  were renamed (`repo-ref/godot/scene/resources/packed_scene.cpp:232-243`, `:355-379`,
  `:2022-2080`).

This strongly supports an opaque document-local ID plus readable name/path projection. It also shows
that Godot's ID is a recovery aid rather than a universal entity-reference contract.

### Duplication and prefab provenance limits

- PackedScene instances maintain instance/inherited state and compare source defaults to save
  overrides (`repo-ref/godot/scene/main/node.cpp:2780-2797`,
  `repo-ref/godot/scene/resources/packed_scene.cpp:302-311`, `:912-1077`,
  `repo-ref/godot/scene/property_utils.cpp:80-138`, `:239-285`). This proves that source and
  instance state are distinct, but it does not expose Nara's proposed first-class projection path.
- Duplicating an instantiated Node re-instantiates its source scene. Ordinary subtree duplication
  recursively creates nodes and remaps some closure-local Node properties by old path
  (`repo-ref/godot/scene/main/node.cpp:2819-2832`, `:2919-2955`, `:3101-3147`).
- Scene-local IDs are not pre-remapped through one complete injective transaction. Save-time collision
  repair may assign new IDs later.
- Arbitrary Node references are still serialized as NodePath and resolved after instantiation
  (`repo-ref/godot/scene/resources/packed_scene.cpp:670-713`, `:960-1049`).
- Rename and reparent repair recursively scans selected property shapes, while external resources,
  materials, animation tracks, dictionary keys, and deletion paths have special or skipped behavior
  (`repo-ref/godot/editor/docks/scene_tree_dock.cpp:2078-2242`, `:2271-2401`,
  `:2888-2966`).

Godot therefore demonstrates both the benefit of stable local IDs and the incompleteness of
heuristic path repair. It cannot prove Nara's complete typed-reference rewrite, cross-document
entity references, product reconciliation, semantic merge, deletion lineage, or multi-file crash
recovery.

## Bevy

Bevy is strongest as evidence for the runtime lowering boundary and weakest as evidence for durable
authoring identity.

### Runtime entities and world serialization

- `Entity` is an ID in a `World`; it can become stale or alias after generation wrap. Bevy gives
  direct `Entity` serde zero long-term wire-compatibility guarantee
  (`repo-ref/bevy/crates/bevy_ecs/src/entity/mod.rs:337-364`). Its bit form is useful only in the
  same application instance (`:559-588`).
- `MapEntities` explicitly states that source-World entities are invalid in another World
  (`repo-ref/bevy/crates/bevy_ecs/src/entity/map_entities.rs:22-33`).
- `DynamicWorld` stores source raw `Entity` values, then creates a complete target map before
  applying and remapping components
  (`repo-ref/bevy/crates/bevy_world_serialization/src/dynamic_world.rs:17-40`, `:87-161`). This
  proves the shape of a source-to-runtime remap, not a durable document ID.
- Its serializer writes raw entities as map keys, and its fixtures expose runtime bits. Combined
  with Bevy's own stability warning, this is a negative example for Nara's persistent format
  (`repo-ref/bevy/crates/bevy_world_serialization/src/serde.rs:103-126`, `:355-369`,
  `:690-725`).
- The new Template system uses `SceneEntityReference` values derived from source location, a
  macro-local name index, and a runtime scope counter. `EntityTemplate` is valid only during scene
  spawn (`repo-ref/bevy/crates/bevy_ecs/src/template.rs:132-167`, `:418-476`). It is not a durable
  document ID.
- Bevy's current `.bsn` asset file remains future work, so current scene APIs cannot be cited as a
  mature persistent-editor format (`repo-ref/bevy/crates/bevy_scene/src/lib.rs:75-90`).

Bevy also has fallbacks Nara should reject: an unknown Template entity reference can spawn an empty
entity, and world deserialization can warn and substitute a default handle for an ephemeral asset
reference (`repo-ref/bevy/crates/bevy_ecs/src/template.rs:92-128`,
`repo-ref/bevy/crates/bevy_world_serialization/src/serde.rs:190-200`,
`repo-ref/bevy/crates/bevy_asset/src/reflect.rs:283-337`). Nara persistent loading should fail before
World mutation instead.

### Asset identity and imported products

- `AssetIndex` is generational and runtime-only
  (`repo-ref/bevy/crates/bevy_asset/src/assets.rs:18-20`, `:45-90`).
- `AssetId<A>` defaults to an unstable opaque `Index`; an explicit `Uuid` variant is stable across
  runs, but it is still scoped by Rust asset type and is not a project asset database identity
  (`repo-ref/bevy/crates/bevy_asset/src/id.rs:15-43`, `:173-199`;
  `repo-ref/bevy/crates/bevy_asset/src/assets.rs:276-290`).
- AssetServer does not own or path-resolve UUID assets. Its path-to-index mappings live only while
  the relevant runtime handle information remains active
  (`repo-ref/bevy/crates/bevy_asset/src/server/mod.rs:1226-1254`, `:1417-1435`, `:1492-1500`;
  `repo-ref/bevy/crates/bevy_asset/src/server/info.rs:80-83`, `:202-260`, `:710-756`).
- Bevy's asset metadata has importer/processor configuration and hashes, but no Unity-like movable
  source GUID (`repo-ref/bevy/crates/bevy_asset/src/meta.rs:34-109`).
- `AssetPath` identifies named sub-assets by a label such as `scene.gltf#PlayerMesh`
  (`repo-ref/bevy/crates/bevy_asset/src/path.rs:17-60`). Duplicate labels can replace a previous
  labeled asset in the current loader path rather than rejecting publication
  (`repo-ref/bevy/crates/bevy_asset/src/loader.rs:497-525`).
- The real Bevy glTF importer uses index-derived labels such as `Scene0`, `Mesh3/Primitive1`, and
  `Animation2` (`repo-ref/bevy/crates/bevy_gltf/src/label.rs:33-95`,
  `repo-ref/bevy/crates/bevy_gltf/src/loader/mod.rs:591-599`, `:737-741`, `:905-919`,
  `:1144-1148`). Reordering source products can therefore change those references.

This is strong negative evidence against using label, source name, array index, Rust type, or
runtime `AssetId` as Nara's `ImportedProductId`. It does not prove that an opaque UUID and a semantic
reconciliation algorithm will work; a Nara multi-product tracer still must prove those claims.

### Runtime instance separation

`WorldInstanceSpawner` allocates a fresh UUIDv4 instance ID for each spawn and retains a
source-Entity-to-runtime-Entity map. Exact instance despawn follows that membership map, including
entities no longer reachable through hierarchy
(`repo-ref/bevy/crates/bevy_world_serialization/src/world_asset_spawner.rs:40-57`, `:82-105`,
`:213-269`). This supports distinct source, runtime-instance, and runtime-entity axes. It does not
prove Nara's persistent runtime IDs, tombstones, prefab projection paths, or external-document
binding.

## Unity

Unity provides the closest production precedent for a composite persistent identity, but several of
its limits are important negative evidence.

- Unity 6.0 stores a unique asset ID and importer settings in the adjacent `.meta` file. Editor move
  and rename move the companion metadata; losing it creates a new identity and breaks references.
  This supports path-independent identity but not Nara's proposed fail-closed metadata-loss behavior.
  See [Asset metadata](https://docs.unity3d.com/6000.0/Documentation/Manual/AssetMetadata.html).
- A serialized Unity object pointer contains a 128-bit asset GUID plus a 64-bit file ID unique only
  inside that asset. Loading resolves the pair to a session/runtime `InstanceID`. This directly
  supports source plus scoped-local identity and runtime separation. See
  [Direct reference asset management](https://docs.unity3d.com/6000.0/Documentation/Manual/assets-direct-reference.html).
- `GlobalObjectId` is authoring-only and project-scoped. It carries a version, identifier kind,
  asset GUID, source-object local ID, and prefab-instance local ID. For a prefab instance, source
  object ID alone is insufficient; moving a scene object to another scene changes the ID, and the
  target scene must be loaded for resolution. See
  [GlobalObjectId](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/GlobalObjectId.html)
  and the official
  [Unity C# reference binding](https://github.com/Unity-Technologies/UnityCsReference/blob/master/Editor/Mono/GlobalObjectId.bindings.cs).
- `AssetImportContext.AddObjectToAsset` requires an importer-supplied identifier that is unique only
  inside the source asset and deterministic across reimport. This supports source-local product
  identity, but Unity does not prove Nara's opaque allocation, ambiguity policy, or retirement
  ledger. See
  [AddObjectToAsset](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AssetImporters.AssetImportContext.AddObjectToAsset.html).
- Prefab instances retain a connection to their source and store object/property overrides;
  nested Prefabs and variants can have multiple valid source/apply targets. This supports explicit
  provenance, but not the exact `source_chain` proposed by Nara. See
  [Prefab instance overrides](https://docs.unity3d.com/6000.0/Documentation/Manual/PrefabInstanceOverrides.html),
  [GetCorrespondingObjectFromSource](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.GetCorrespondingObjectFromSource.html),
  and [SetPropertyModifications](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/PrefabUtility.SetPropertyModifications.html).
- Unity prevents cross-scene references by default because they cannot be saved in scene files.
  Unity is therefore not evidence that Nara's persistent external entity reference is already
  feasible. See
  [EditorSceneManager.preventCrossSceneReferences](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SceneManagement.EditorSceneManager-preventCrossSceneReferences.html).
- UnityYAMLMerge uses stable `fileID`, GUID, and property path fields as semantic merge keys, while
  retaining an explicit conflict path. This proves stable IDs help semantic merging, not that UUIDs
  eliminate merge conflicts. See
  [Smart merge](https://docs.unity3d.com/6000.0/Documentation/Manual/SmartMerge.html).
- `AssetDatabase.CopyAsset` promises a new asset but does not promise that internal local IDs remain
  stable. A first-party historical issue demonstrates that copying assets together with `.meta`
  files can create colliding GUIDs and broken references. See
  [CopyAsset](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AssetDatabase.CopyAsset.html)
  and [Unity Issue 809590](https://issuetracker.unity3d.com/issues/materials-not-found-when-exporting-and-importing-package).

Unity strongly supports the identity axes, but it cannot choose Nara's whole-asset duplication
rule, cross-document resolution contract, package collision policy, product retirement behavior,
or migration protocol.

# P0 Findings Before ADR Acceptance

## P0-1: Document identity is incomplete for multi-product sources

ADR 0083 defines an external entity reference as `StableAssetId + SceneEntityId`, while the same
proposal allows one source to publish multiple products, including compound documents. Two
entity-bearing products under the same source can contain the same document-local entity UUID, so
the reference is not unique.

The ADR must choose one of two explicit contracts:

1. Recommended: identify every source scene/prefab document by `AssetProductRef` and define an
   external source entity reference as `AssetProductRef + SceneEntityId`.
2. Narrower alternative: require every entity-bearing scene/prefab document to be the reserved
   `Primary` product and reject auxiliary entity-bearing products in version 1.

The first option composes with real compound importers and avoids a later wire-format break.
`ImportedProductId` must be unique across all product kinds in one source, not merely within an
expected Rust type, and its descriptor must bind an immutable product kind/schema lineage.

## P0-2: Source identity cannot select a runtime instance

The same source document may be unopened, loaded once, instantiated additively several times, or
instantiated in two independent runtimes. `AssetProductRef + SceneEntityId` names a source record,
not a live entity.

The ADR must separately define:

- source-document references used by authoring, patching, provenance, and apply-back;
- source-local references that are remapped when a prefab is projected;
- explicit prefab bindings for a source prefab to consume an outer-instance entity;
- runtime scene references that include one `SceneInstanceId` and a local or structured projected
  key;
- resolution results for absent document/product, unavailable package, wrong kind/schema, missing or
  deleted entity, unloaded runtime instance, and ambiguous multiple instances.

Identity resolution must not imply loading, and authoring source resolution must not silently choose
a runtime instance.

## P0-3: Prefab projection identity mixes authoring and runtime axes

`PrefabProjectionLocator.root_scene_instance` is ambiguous and conflicts with ADR 0058 if it means
runtime `SceneInstanceId`, which must not enter project documents.

Persistent provenance should start with the authored instance anchor in the containing document,
then carry a bounded chain of source `AssetProductRef` values and nested authored anchors, ending in
the terminal source entity. A runtime lookup separately adds `SceneInstanceId`.

The ADR also must require complete reference lowering during expansion:

- a prefab source-local reference to another source entity becomes a projected key under the same
  instance chain;
- nested prefab references extend the chain rather than concatenate strings;
- an intentional reference from a prefab to an entity in the containing scene uses an explicit
  binding/override contract, not `SceneLocal` lookup against the eventual outer instance;
- duplicate, convert-to-local, apply-back, and export precompute a complete injective key/reference
  remap before mutation.

The current reference game's enemy-to-player link is a concrete failing tracer for this distinction.

## P0-4: Persistent identities need one fail-closed runtime lowering boundary

`StableAssetId`, `ImportedProductId`, and `SceneEntityId` must never be aliases of Bevy
`AssetId::Uuid`, `AssetIndex`, `Handle`, `SceneEntityReference`, raw `Entity`, or serialized
DynamicWorld entity bits.

Before first target-World mutation, the Host must:

1. resolve the complete document/product generation and schema catalog;
2. reject nil, duplicate, wrong-kind, reused-product-kind, and missing references;
3. build the complete injective authored/projection-key to runtime-Entity allocation plan;
4. allocate and apply only after the candidate is proven internally closed;
5. fail persistent serialization when a process-local identity is encountered.

Missing references must return typed failures. Nara should not adopt Bevy's empty/dead Entity or
default-handle fallback behavior for persistent project data.

## P0-5: Project and package collision scope is undefined

`StableAssetId` is called project-wide while references are also described as valid across packages.
The resolver needs one explicit collision rule.

Recommended version-1 rule:

- all project assets and resolved dependency/mounted package assets visible in one composed content
  generation share one `StableAssetId` collision domain;
- package identity, source, release, and mount generation are provenance/resolution evidence, not an
  extra equality field for ordinary stable asset references;
- any active collision rejects composition before catalog publication, including a collision between
  project content and a read-only package;
- updating a package preserves IDs for the same logical assets and reports removed targets as
  unavailable/missing;
- vendoring or forking package content while the original remains visible is an explicit closure
  import that allocates new IDs and rewrites references atomically.

Silently rewriting an immutable package, selecting the first path, or letting mount precedence choose
identity would create two authorities.

## P0-6: Whole-asset duplication must have one deterministic rule

The current `may preserve local IDs` wording leaves source-control and tooling behavior dependent on
implementation details. Mature engine evidence does not supply a portable guarantee, so Nara must
choose.

Recommended rule:

- duplicate one source asset: allocate a new `StableAssetId`;
- preserve all source-local `ImportedProductId` values and all document-local `SceneEntityId` values,
  because the enclosing asset/product scopes differ;
- keep ordinary local references byte-identical;
- rewrite references inside the copied closure that target the original asset/product to the copied
  asset/product, including self external references, prefab sources, projection steps, and overrides;
- leave references to assets outside the copied closure targeting the originals;
- preflight the complete mapping and publish no partial copy.

Same-document entity/subtree duplication still allocates all-new entity IDs. Copying a directory,
package content set, or project is a closure operation with a complete asset/product/document remap,
not a loop of unrelated single-file copies.

## P0-7: Product continuity and retirement need non-aliasing semantics

An `ImportedProductId` cannot be reused for a different logical product merely because the old
product disappeared. Otherwise an old durable reference silently reconnects to unrelated content.

ADR 0083 and ADR 0087 together must prove:

- one source-wide product ID namespace across product kinds;
- product ID plus immutable kind/schema lineage in the candidate descriptor;
- exact duplicate-ID, kind drift, and ambiguous continuity rejection;
- retained last-known missing/retired evidence or another bounded mechanism that prevents a retired
  ID from being rebound while references may still surface;
- an explicit remap transaction when semantic continuity cannot be proven;
- duplicated-source behavior that preserves the internal product-ID pattern only under the new
  enclosing `StableAssetId`.

Unity's deterministic importer identifier and Bevy's labels are useful precedents, but neither proves
this stronger non-aliasing contract.

## P0-8: Entity deletion and non-reuse claims must be honest

The ADR currently says IDs are never intentionally reused in a document lineage while also rejecting
an unbounded permanent tombstone catalog. Without durable lineage evidence, a loader cannot tell
whether a manually reintroduced UUID restores the old entity or names a different logical entity.

The enforceable version-1 contract should be:

- official creation and duplication tools never allocate a live or previously retired ID for a
  different entity;
- active duplicates reject before publication;
- undo, redo, branch revert, or explicit restore may restore the same entity record with its old ID;
- a deleted target leaves the typed reference bytes intact and resolves as missing/deleted, rather
  than clearing or retargeting it;
- bounded undo, diagnostics, recovery, or source-control evidence may explain deletion, but the scene
  format does not claim to prove unlimited lineage;
- if the product later requires strict historical non-reuse across arbitrary manual edits, a separate
  bounded lineage/tombstone policy must be admitted.

Random UUID collision resistance is not historical lineage proof.

## P0-9: Reference inventory and migration must be complete by construction

Godot demonstrates why heuristic scanning of paths and selected property containers is insufficient.
Every Nara persistent entity reference that may be remapped must be structurally represented and
enumerable through the canonical wire format plus schema capability.

Acceptance must establish that:

- entity references cannot hide in untyped strings, opaque importer blobs, map keys, callbacks, or
  unavailable schema payloads and still claim rewrite support;
- unknown/unavailable components either have a proven lossless structural reference traversal or
  fail the migration before any write;
- document-level migration runs before component-value migration and uses the complete frozen schema
  and source-document graph;
- old `anchor/source` strings are reconstructed from validated prefab expansion/provenance, not split
  on `/`, because old path-like IDs themselves may contain multiple segments;
- the UUIDv5 mapping is specified byte-for-byte, for example using the document's non-nil
  `StableAssetId` UUID directly as the namespace and the exact decoded old `SceneEntityId` UTF-8
  bytes as the name;
- migration uses ADR 0091's persistence coordinator, committed project snapshot, publication fence,
  receipts, and recovery journal. ADR 0083 must not introduce an independent transaction log.

If ADR 0091 remains Proposed, ADR 0083 may remain Proposed or accept only a narrower identity
semantic decision whose migration activation is explicitly blocked on a compatible Accepted
persistence contract.

## P0-10: Opaque identity must not carry gameplay semantics

Changing `SceneEntityId` from readable strings to random UUIDs will expose systems that use IDs as
tags, names, singleton roles, or sort policy. The reference game already compares identity text to
`"player"` and `"enemy-anchor/enemy"`.

Gameplay selection must use typed components, roles/tags, explicit references, or `Name` for display.
Opaque IDs may be used for equality, maps, deterministic tie-breaks, diagnostics, and durable
references, but their bytes have no domain meaning. ADR admission should randomize all authored IDs
in a reference-game fixture and prove unchanged gameplay outcomes.

# P1 Refinements

1. Version and discriminate every durable reference wire form. `Local`, `ExternalSource`, explicit
   prefab binding, projected provenance, and runtime identity must not be inferred from field
   omission or string syntax.
2. Keep expected document/product kind and schema lineage in validation descriptors, while keeping
   equality based on the declared identity fields. Wrong kind must differ from missing identity.
3. Treat paths and display names as diagnostic/recovery hints only. Unlike Godot fallback paths, a
   stale hint must never override or silently substitute for a present stable ID.
4. Define asset move as one recoverable source-plus-sidecar/database transaction. External filesystem
   edits may be detected and reconciled, but metadata loss must not silently allocate a replacement
   ID.
5. Keep local/product ID generation opaque at public boundaries. UUID version and allocator details
   may be implementation facts except where migration determinism requires a fixed algorithm.
6. Resolve IDs through indexes and batch APIs. Unity's authoring `GlobalObjectId.*Slow` APIs are a
   warning against repeated full-scene scans.
7. Define canonical text stability: a no-op load/save changes no IDs, record order, reference bytes,
   or unrelated formatting. Keep names and structural paths available for readable diffs.
8. Provide a semantic scene/prefab merge layer keyed by stable IDs. Stable keys permit merge; they do
   not resolve delete-versus-edit, two-parent, conflicting override, or same-ID/different-record
   conflicts automatically.
9. Distinguish duplicate, import, fork, move, restore, and package update as separate user commands.
   Their ID behavior must be visible in preview and diagnostics.
10. Keep deletion evidence, unavailable package state, and reference resolution diagnostics bounded
    and privacy-classified under existing project input and diagnostic policies.

# ADR Revision Checklist

Before ADR 0083 can be considered for acceptance, revise it to address each item below without
prematurely fixing crate ownership or public Rust names:

- [ ] Replace document identity based on `StableAssetId` alone with `AssetProductRef`, or explicitly
      restrict entity-bearing documents to `Primary`.
- [ ] Define `ImportedProductId` uniqueness across all source product kinds and bind kind/schema
      lineage plus retirement/non-reuse behavior.
- [ ] Separate source entity references, source-local references, explicit prefab outer bindings,
      structured projection keys, and runtime entity references.
- [ ] Remove or rename `root_scene_instance` so persistent provenance cannot contain runtime
      `SceneInstanceId`.
- [ ] Specify every nested projection step and prove that repeated nested instances remain distinct.
- [ ] Require prefab-internal reference remap and an explicit binding for intentional outer-scene
      references; cite the reference-game enemy target as a tracer.
- [ ] State that a source reference neither loads a document nor chooses among runtime instances.
- [ ] Define one project-plus-package `StableAssetId` collision domain and fail-closed behavior for
      immutable package collisions.
- [ ] Make whole-asset, same-document subtree, closure/package fork, move, restore, and update
      identity rules deterministic.
- [ ] Specify whole-asset self-reference and copied-closure rewrite behavior.
- [ ] Make official entity-ID non-reuse claims match what can be proven without an unlimited
      tombstone history; explicitly allow restoration of the same entity lineage.
- [ ] Require all remappable references to be structurally enumerable and define unavailable-schema
      behavior.
- [ ] Define missing/deleted target preservation and typed diagnostics for authoring and runtime
      resolution.
- [ ] State the Host lowering invariant and reject Bevy/process-local identities in persistent
      serialization.
- [ ] Pin Godot and Bevy citations to the audited commits and replace the claim that current Godot is
      purely path-based with its current local-ID-plus-path-recovery model.
- [ ] Define the exact UUIDv5 migration mapping and delegate publication/recovery to ADR 0091's
      committed-project transaction contract.
- [ ] Add an explicit invariant that opaque IDs carry no gameplay meaning.
- [ ] Keep the ADR `Proposed` until the full acceptance matrix below exists.

# Required Acceptance Evidence

Acceptance needs production-shaped fixtures, not only UUID parsing or equality unit tests.

## Asset and package identity

1. Move and rename a source plus sidecar through the public editor/Host flow; zero durable references
   change.
2. Inject failure between every source, sidecar, database, and committed-snapshot step; reopen to a
   diagnosed complete old or complete new mapping.
3. Lose or orphan metadata; retain the last-known identity conflict and never silently publish a new
   identity over old references.
4. Copy a source with and without metadata, present two active identical IDs, and prove candidate
   composition rejects without choosing a path.
5. Compose a project with two read-only packages, and a project override plus package, that collide
   on `StableAssetId`; prove fail-closed behavior and actionable provenance diagnostics.
6. Update, remove, reinstall, vendor, and fork a package while preserving or explicitly remapping the
   intended asset closure.

## Imported products and entity-bearing documents

7. A real compound importer produces several kinds and at least two entity-bearing documents from
   one source. Every document/entity reference resolves uniquely through `AssetProductRef`.
8. Rename, reorder, and edit products without changing IDs when continuity is proven.
9. Remove and later add a different product at the same name/index; prove the retired ID is not
   rebound.
10. Present duplicate product IDs, kind/schema drift, and ambiguous reconciliation; publish no
    candidate group.
11. Drop all runtime handles, reload to different runtime AssetIndex values, and prove durable
    product references remain unchanged.

## Scene, prefab, and reference semantics

12. Rename and reparent authored entities; IDs and reference bytes remain unchanged.
13. Duplicate a same-document subtree; allocate an injective new ID set, rewrite closure-internal
    references, and preserve external targets.
14. Duplicate a whole scene/prefab asset; allocate a new asset ID, preserve local product/entity IDs,
    rewrite self/closure references to the copy, and preserve references to outside assets.
15. Move or merge records between documents with colliding and non-colliding IDs; preflight every
    local/external reference rewrite atomically.
16. Delete an entity referenced locally and externally, undo the delete, then create a different
    entity; prove missing diagnostics, same-lineage restore, and non-aliasing behavior.
17. Instantiate the same source document zero, once, and several times. Source references remain
    authoring identities; runtime references resolve only with an explicit instance binding.
18. Instantiate the same prefab more than once and nest the same source through different anchors.
    All projection keys are distinct while source IDs remain shared.
19. Remap references between entities inside a prefab projection, and resolve the reference-game
    enemy-to-player case only through an explicit outer binding or override.
20. Move a prefab source, duplicate it, convert one projection to local, apply overrides back to
    source/anchor, and prove provenance routes to exactly one intended target.

## Runtime and persistence isolation

21. Spawn the same document into two Worlds and multiple scene instances; every runtime map differs
    while persistent source bytes remain identical.
22. Present missing entity references before spawn and prove typed failure before the first World
    allocation or persistent component insertion.
23. Attempt to serialize `Entity`, `Handle`, `AssetIndex`, runtime `AssetId`, or
    `SceneInstanceId` through every persistent format; fail rather than warn, default, or leak bits.
24. Randomize every reference-game authored entity UUID and prove gameplay, replay, editor selection,
    and snapshots use typed roles/references rather than identity text.

## Migration, merge, and text stability

25. Commit predecessor/current fixtures for the exact UUIDv5 mapping, including nested old IDs that
    contain `/`, repeated prefab sources, external references, patches, overrides, and workspace
    selection.
26. Run migration with complete schemas, an unavailable schema, a missing package, duplicate old IDs,
    duplicate asset IDs, and an unresolvable old projection. Only the complete case may publish.
27. Inject a crash or ambiguous filesystem result at every ADR 0091 project mutation boundary; the
    active `CommittedProjectSnapshot` is always old or new, never mixed.
28. Three-way merge fixtures cover independent additions, rename versus component edit, reorder
    versus edit, delete versus edit, delete versus external reference, duplicate active IDs, and
    conflicting prefab overrides. Clean cases merge semantically; ambiguous cases remain explicit
    conflicts.
29. No-op load/save and unrelated edits preserve every unaffected ID, reference, record order, and
    formatting region required by the canonical text contract.
30. Human-readable diagnostics and diffs show names and paths as projections while always carrying
    the stable typed locator needed for unambiguous repair.

# Decision Assessment

The evidence supports retaining ADR 0083 as the owner of durable identity semantics, after a
substantial revision. The preferred architecture is not a single global object UUID. It is a set of
scoped identities connected by explicit lowering and provenance transactions:

```text
StableAssetId
  + ImportedProductId       = source document/product
  + SceneEntityId           = authored source entity
  + PrefabProjectionPath    = projected authoring location
  + SceneInstanceId         = runtime instance selection
  -> host runtime map
  -> bevy_ecs::Entity
```

The correction cost is high after content and packages proliferate, so the semantic direction is
worth settling before broad authoring. Acceptance should still wait for the compound-document,
explicit-prefab-binding, cross-package, semantic-merge, and ADR 0091 crash-recovery tracers. Until
then, this research is revision guidance only.

# Next Action

Rewrite ADR 0083 against the checklist above while keeping it `Proposed`. The first design tracer
should combine two entity-bearing products from one source, two instances of one prefab, one
prefab-internal reference, one explicit outer binding, one cross-document source reference, and one
read-only package collision. That single tracer closes the highest-risk identity-scope questions
before implementation.

# Citations

- Nara proposal: `docs/architecture/adr/0083-durable-project-asset-and-document-entity-identity.md`
- Nara related import proposal:
  `docs/architecture/adr/0087-asset-dependency-import-product-and-artifact-publication-graph.md`
- Nara persistence proposal:
  `docs/architecture/adr/0091-editor-persistence-recovery-and-concurrent-writer-policy.md`
- Nara current identity types: `crates/nara_identity/src/types.rs`,
  `crates/nara_asset/src/identity.rs`
- Nara current prefab lowering: `crates/nara_scene/src/prefab.rs`
- Nara current product tracer: `reference-game/src/components.rs`,
  `reference-game/src/systems.rs`, `reference-game/src/snapshot.rs`
- Bevy pinned source: commit `f6c6e6eebb94e81c090614f19039319e9acb3c85`; cited files under
  `repo-ref/bevy/crates/bevy_ecs`, `bevy_asset`, `bevy_scene`,
  `bevy_world_serialization`, and `bevy_gltf`.
- Godot pinned source: commit `c939bf3791ce40ff70e0ee29f06486da1ebb6a84`; cited files under
  `repo-ref/godot/core/io`, `scene/resources`, `scene/main`, `scene/property_utils.cpp`, and
  `editor`.
- Unity 6.0, [Asset metadata](https://docs.unity3d.com/6000.0/Documentation/Manual/AssetMetadata.html).
- Unity 6.0, [Direct reference asset management](https://docs.unity3d.com/6000.0/Documentation/Manual/assets-direct-reference.html).
- Unity 6.0, [GlobalObjectId](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/GlobalObjectId.html).
- Unity 6.0,
  [TryGetGUIDAndLocalFileIdentifier](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AssetDatabase.TryGetGUIDAndLocalFileIdentifier.html).
- Unity 6.0,
  [AssetImportContext.AddObjectToAsset](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/AssetImporters.AssetImportContext.AddObjectToAsset.html).
- Unity 6.0,
  [Prefab instance overrides](https://docs.unity3d.com/6000.0/Documentation/Manual/PrefabInstanceOverrides.html).
- Unity 6.0,
  [EditorSceneManager.preventCrossSceneReferences](https://docs.unity3d.com/6000.0/Documentation/ScriptReference/SceneManagement.EditorSceneManager-preventCrossSceneReferences.html).
- Unity 6.0, [Smart merge](https://docs.unity3d.com/6000.0/Documentation/Manual/SmartMerge.html).
- Unity Issue Tracker,
  [Issue 809590: Asset Database only handles one of multiple colliding GUIDs](https://issuetracker.unity3d.com/issues/materials-not-found-when-exporting-and-importing-package).
