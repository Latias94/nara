# ADR 0007: Asset Identity and Import Pipeline

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0049: Untrusted Project Input and Parse Budget Policy; ADR 0050: Asset Root,
Symlink, Junction, and Package Trust Policy; ADR 0051: Persistent File Envelope, Migration, and
Golden Fixtures

## Context

nara scenes and prefabs need stable references to textures, audio, tilemaps, scripts, prefabs, and future meshes/materials. Asset identity affects hot reload, import caches, AI-generated scene data, editor workflows, and package portability.

The engine should support a fast Phase 1 path without locking itself into path-only identity forever.

## Decision

nara will use **typed handles with UUID-ready asset identity**.

Phase 1 may resolve assets by project-relative path, but the data model and APIs must be designed for stable asset IDs and importer metadata.

```mermaid
flowchart TD
    Source[Source Asset Path] --> Meta[Future .meta AssetId]
    Source --> Importer[Asset Importer]
    Meta --> Importer
    Importer --> Artifact[Imported Artifact Cache]
    Artifact --> AssetServer[AssetServer]
    AssetServer --> Handle[Handle<T>]
    Handle --> Scene[Scene / Prefab References]
```

Core rules:

- User code and scene data refer to assets through typed `Handle<T>` or serialized asset references, not raw filesystem access.
- Phase 1 can use path-first identity for speed.
- The model must be UUID-ready so Phase 2 can add `.meta` files without changing scene/prefab semantics.
- Hot reload changes asset versions behind stable handles; it should not require all scenes to rewrite references.
- Source assets and imported artifacts are distinct concepts.

## Alternatives Considered

### Option A: Path-only identity forever

**Pros**: Simple, human readable, easy for AI to generate.

**Cons**: Rename/move breaks references; import cache identity is fragile; editor workflows become difficult.

**Decision**: Rejected as a long-term model.

### Option B: UUID-first from day one

**Pros**: Mature editor-friendly identity, robust renames, better cache keys.

**Cons**: Requires `.meta` lifecycle, importer cache, and project database before the runtime MVP.

**Decision**: Deferred. APIs should be ready for UUIDs, but Phase 1 does not need the full pipeline.

### Option C: Path-first now, UUID-ready model (Chosen)

**Pros**: Fast Phase 1 implementation while preserving a mature future importer/editor path.

**Cons**: Requires discipline to avoid hardcoding path identity into scene semantics.

**Decision**: Chosen.

## Consequences

- `AssetRef` in scene data should be able to represent both path and future stable ID forms.
- `Handle<T>` should remain stable across reloads; asset data may carry an internal generation/version.
- Import cache design is deferred but not ignored.
- Asset loading can begin synchronous/path-based, but the API must allow asynchronous load states later.

## Implementation Notes

- `nara_asset` now separates persistent identity (`AssetRef`, `AssetPath`, `StableAssetId`,
  `.meta`, and `ProjectAssetDatabase`) from runtime identity (`AssetId`, `Handle<T>`, and
  `AssetVersion`).
- Source changes enter asset loading through `AssetSourceChanges` and `SourceChangeResolver`.
  The resolver coalesces source changes, maps `.meta` updates to source assets, walks source
  dependency edges, and emits `AssetReloadRequest` values.
- Same-frame source changes coalesce by logical path with the last semantic event winning. This keeps
  atomic-save sequences such as remove-then-modify from being permanently interpreted as deletion.
- Dependency-triggered reload walks source dependency edges transitively with de-duplication, so a
  changed source can enqueue directly and indirectly dependent runtime assets.
- Asset reload requests carry `AssetLoadGeneration` values. Apply systems must ignore stale task
  results when a newer generation has been requested for the same asset.
- Domain apply systems also check the request's expected `AssetVersion` before committing successful
  first-load results or failure states. Generations reject superseded requests; expected versions
  reject older task completions after any other state mutation advanced the asset.
- Import work receives owned `ImportJobInput` values and returns typed `ImportedAsset<T>` values
  through `TypedImporter<T>`. Importers may read source bytes and produce backend-neutral runtime
  assets, but they must not allocate GPU resources.
- `AssetPlugin` installs the asset resources and the `TaskUpdateSet::CoalesceAssetChanges` resolver.
  Domain plugins are responsible for registering their own typed importers and spawning/applying
  domain reload jobs.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Typed handles | Asset references are typed in Rust APIs | API review |
| UUID readiness | Scene asset references can evolve from path to ID without semantic rewrite | Schema review |
| Reload stability | Handle identity survives asset content reload | Future test |
| Source/artifact split | Import cache can be added without changing source asset references | Design review |
| AI usability | JSON scene can still express readable asset references | Example scene |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Path-first leaks into stable file format | High | Medium | Define `AssetRef` as an enum-like semantic model, not just `String` |
| UUID/meta adds editor burden too early | Medium | Medium | Keep `.meta` Phase 2; write APIs now, not full implementation |
| Hot reload invalidates handles | High | Medium | Separate handle identity from loaded asset version |
| AI emits broken paths | Medium | High | Validate asset references before instantiation |

## Follow-Up Questions

- Which import profile fields belong in artifact cache keys for desktop-only Phase 1?
- How should rename/move operations preserve stable IDs once the editor owns `.meta` lifecycle?
- What project-level diagnostics should be emitted for repeatedly failing hot reloads?

## Citations

- Scene/prefab decision: [0006-scene-and-prefab-data-model.md](0006-scene-and-prefab-data-model.md)
