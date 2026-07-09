# ADR 0037: Asset Load Request, Cache, and Lifetime Policy

**Status**: Accepted
**Date**: 2026-07-09
**Refined By**: ADR 0050: Asset Root, Symlink, Junction, and Package Trust Policy; ADR 0051:
Persistent File Envelope, Migration, and Golden Fixtures

## Context

ADR 0033 defined the asset import to render-resource preparation seam. The implementation now has
stable asset IDs, `.meta` records, importer registry metadata, import artifacts, typed `Assets<T>`,
`LoadState`, reload generations, dependency-aware reload requests, async image imports, and backend
GPU caches.

The missing contract is the public lifetime model: how load/reload requests are represented, how
failures are observed, when cached runtime values remain alive, how stale task results are rejected,
and how prepared/render backend resources are evicted.

## Decision

`nara_asset` owns source asset identity, request scheduling, runtime load state, reload
generations, source-change diagnostics, and typed runtime asset tables. Domain asset crates own
decode/import logic and typed value application. Render crates own prepared render resources and
backend GPU caches.

The first lifetime policy is:

- `Handle<T>` is runtime identity. It is stable across reloads but not persistent scene data.
- `AssetRef` is persistent semantic identity. It resolves to a runtime handle through
  `ProjectAssetDatabase` and `AssetServer`.
- `AssetSourceChange` values are coalesced into generation-stamped `AssetReloadRequest` values.
- Each reload request carries expected asset version and load generation. Domain apply systems must
  reject stale results.
- A failed first load stores no asset value and sets `LoadState::Failed`.
- A failed reload preserves the last good typed asset value when one exists, records
  `LoadState::Failed`, and emits a failure event.
- Removed source assets clear typed runtime values and prepared render resources.
- Source-change translation or scheduling failures produce structured diagnostics; they must not be
  silently dropped.
- Prepared render resources are backend-neutral cache entries. Backend-native GPU objects remain in
  backend caches and are invalidated from prepared resource identity/version changes.

```mermaid
sequenceDiagram
    participant Watch as Watcher / Manual Change
    participant Asset as nara_asset
    participant Domain as Domain Import Plugin
    participant Prepared as nara_render Prepared Resources
    participant Backend as Render Backend Cache

    Watch->>Asset: AssetSourceChange
    Asset->>Asset: coalesce + resolve ProjectAssetDatabase
    Asset->>Asset: create AssetReloadRequest(version, generation)
    Domain->>Asset: drain requests for source kind
    Domain->>Domain: import/decode job
    Domain->>Asset: apply result if generation/current version match
    Domain->>Prepared: invalidate/update prepared resource
    Prepared->>Backend: backend cache rebuilds on prepared snapshot change
```

## Cache Modes

The first implementation may keep a simple default cache behavior, but public design should reserve
these modes:

| Mode | Meaning | First implementation |
|---|---|---|
| Automatic | Asset may unload when no strong runtime owner requires it | Default policy, exact eviction may be conservative |
| Pinned | Asset stays resident while a project/runtime setting or tool pins it | Reserved |
| Transient | Asset may be dropped aggressively after use | Reserved |
| Generated | Runtime/editor-generated asset with explicit owner | Reserved |

## Rules

- Asset loading failures must be visible through `AssetEvents`, `LoadState`, diagnostics, or all
  three depending on failure class.
- Watcher adapters translate filesystem events into semantic source changes; `nara_asset` remains
  watcher-agnostic.
- Import artifacts are backend-neutral. GPU objects, bind groups, and render targets are not
  imported artifacts.
- Domain plugins should drain only the source kinds they own.
- Stale generation or expected-version mismatches must not overwrite newer runtime asset state.
- Cache eviction must not invalidate persistent scene data.

## Alternatives Considered

### Option A: Direct synchronous load on every `AssetRef`

**Pros**: Simple mental model.

**Cons**: Blocks hot reload, async import, dependency propagation, progress reporting, and editor
diagnostics.

**Decision**: Rejected.

### Option B: Renderer-owned asset cache

**Pros**: Easy texture upload path.

**Cons**: Leaks backend lifetime into asset identity and breaks non-render assets, scenes, prefabs,
audio, and future editor tooling.

**Decision**: Rejected.

### Option C: Asset-owned requests/state plus domain apply and backend caches

**Pros**: Matches ADR 0033, keeps backend isolation, supports reload failures and stale-task guards,
and leaves room for cache modes.

**Cons**: More state to test and document.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Stale task safety | Old generation results cannot overwrite newer asset state | Unit tests |
| Failure visibility | Source-change scheduling failures emit diagnostics | Unit tests |
| Last-good reload | Failed reload preserves previous typed value | Unit tests |
| Removal cleanup | Removed source clears typed value and prepared resource | Unit tests |
| Backend isolation | `nara_asset` never stores GPU objects | Dependency search |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Diagnostics become another event bus | Medium | Medium | Keep diagnostics observational and separate from load intent. |
| Cache modes are overdesigned too early | Medium | Medium | Reserve names now; implement only default behavior until pressure exists. |
| Failed reload behavior surprises users | Medium | Medium | Document last-good preservation and expose failure state clearly. |
| Backend cache eviction drifts from asset state | High | Medium | Route backend invalidation through prepared resource snapshots and versions. |

## Consequences

- ADR 0033 remains the import/render seam. This ADR defines load request and lifetime policy inside
  that seam.
- `AssetSourceChange` resolution failures must become structured diagnostics.
- Future public load APIs should lower into `AssetReloadRequest` or a compatible request model
  rather than creating separate queues.

## Open Questions

- What public API shape should synchronous-looking `load` calls expose while still using the request
  model internally?
- Which cache mode should be implemented first after automatic retention?
- Should asset progress be a diagnostic/status resource or a dedicated observable asset-load table?
