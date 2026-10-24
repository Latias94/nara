# Direct `nara_scene` consumption

`nara_scene` can be used directly for the supported scene parse, validation, and spawn slice. The
consumer owns its registry and ECS world; it does not need the integrated root `nara` facade.

The checked-in `tests/fixtures/module-consumer/Cargo.toml` uses these paths relative to its
fixture directory:

```toml
[dependencies]
nara_reflect = { path = "../../../crates/nara_reflect" }
nara_scene = { path = "../../../crates/nara_scene", default-features = false, features = ["serde"] }

[dev-dependencies]
bevy_ecs = "0.20"
```

- `nara_scene` owns the canonical scene file candidate, scene component registration, stable scene
  identity, and spawn operation. Enable `serde` to decode canonical scene files.
- `nara_reflect` supplies the caller-owned `ComponentRegistry`. Register the required component
  providers and freeze the registry before publishing, validating, or spawning a scene.
- `bevy_ecs` supplies the caller-owned `World`. Its version must match the version supported by the
  selected `nara_scene` release.

An external project should select the corresponding published, Git, or local package locations;
the package names, `nara_scene/serde` feature, and direct prerequisite roles remain the same. The
checked-in [`module-consumer`](../../tests/fixtures/module-consumer/) workspace is the executable contract. It uses
its own manifest and lockfile, parses a committed RON scene, validates it against a frozen registry,
spawns it into a fresh world, and ends the exact instance through the Scene owner.

## Source And Placement

Documents remain local graphs. Bind a plain document to an explicit `SourceDocumentKey` with
`ExpandedSceneCandidate::plain(source, document)`. Prefab expansion returns a move-only
`ExpandedSceneDocument`; its source owner consumes `bind_source(source)` to obtain the same
required materialization input without copying or assembling provenance.

Materialize with `spawn_scene(world, registry, &candidate, &occurrence, database)`. The occurrence
is an explicit `SceneOccurrenceKey::new(namespace, slot)` selected by the instantiating consumer.
Source identity says which document; occurrence says where that document is logically placed.
Two placements of one source require different slots, and two different sources cannot occupy
the same slot concurrently. A product can declare its fixed startup or preview slot as consumer
policy; Scene does not infer one from a source, instance counter or callsite.

Runtime `SceneEntitySource` exposes `containing_source()` and `occurrence()` alongside its
source-local entity and structured Prefab provenance. Scene shares the containing context across
an activation rather than copying a long root path into every entity. This does not change the
independent exact runtime incarnation carried by Identity.

## Instance Lifetime

Retain the successful `SpawnedSceneInstance` receipt. Use
`retire_and_despawn_scene_instance(&mut world, &instance)` to unload that activation and remove
its remaining runtime entities. It also ends an empty instance, including one whose entities were
individually retired. Deleting the final entity or dropping the receipt does not end the instance.
Both retain the logical slot. Exact end releases that activation's slot; a stale retry cannot
release a successor. Exact replacement obtains the predecessor's already-declared slot from the
Scene association and changes source/activation in the same exclusive publication transaction.
The active association is World-bound and separately budgeted by `SceneOccurrenceLimits`; it
does not retain permanent slot history or resolve entities independently of Identity.

The operation rejects foreign or stale receipts and unsupported native lifecycle work before
Scene retirement. Owning native domains must complete their cleanup first; the function does not
run arbitrary destruction callbacks. Pending World commands are flushed before admission, so
rejection does not promise to undo those commands. Ordinary identity history remains retained;
ending one instance does not reset the World domain or affect independently resident instances.

`SceneAuthoringSession::clear_live_world` uses this same retirement path while updating its session
receipt. A `SceneProjection` owner instead uses `SceneProjection::clear`, which additionally retires
its complete disposable domain epoch. Construct it with an explicit occurrence, then publish
source-bound candidates. Do not end their retained receipts behind those owners.

This evidence covers the documented direct scene-module workflow only. It does not promise
arbitrary cross-engine compatibility, arbitrary combinations of Nara modules, or a stable dynamic
ABI. Other module workflows must document and prove their own prerequisites.

## Prefab Source Binding

`PrefabSourceResolver` has one required operation: return a `ResolvedPrefabSource` containing
the immutable Prefab and its canonical `SourceDocumentKey` together. Lookup requests are not
source identity. A code author declares the source explicitly, independently of the path used to
request it:

```rust
use nara_asset::AssetRef;
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, SourceDocumentKey, SourceDocumentKind,
};

let key = SourceDocumentKey::named(
    "46806b15-ef25-4da2-9983-32078fd5b5a7".parse().unwrap(),
    "game.characters.hero",
    SourceDocumentKind::Prefab,
).unwrap();
let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
    AssetRef::path("prefabs/hero.ron").unwrap(),
    key,
    PrefabDocument::default(),
);
```

The in-memory resolver keeps one document per canonical key. `insert(request, key, document)`
rebinds that request and replaces the document for **every** alias of the key. Its return value is
the previous document for that key, not the request's previous target. Adding an alias through
this operation is a content publication too: inserting an old copy replaces newer content rather
than inferring which copy is newer. An unaddressed old key is released after request rebinding.
File-backed conflict admission still belongs to Project Content; this author-owned container does
not replace its duplicate stable-ID refusal.

Expansion detects cycles by canonical source, not lookup spelling, and retains the required key
on every projection step. Repeated siblings remain independent placements. The separately compiled
[`Prefab consumer`](../../tests/fixtures/module-consumer/tests/prefab_sources.rs) exercises alias
replacement, cycle refusal and successful re-expansion through public interfaces.

Containing Scene binding and runtime occurrence are supplied separately when materializing the
expanded document. Complete Runtime/Canvas key lowering remains a separate integration obligation.
