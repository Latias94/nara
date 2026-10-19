# Tilemap File-Backed Foundation Slice

## Objective

Make one real Tilemap workflow authoritative before adding terrain, animation, collision, or
streaming semantics:

```text
TileSet source -> Image dependency -> saved map -> cold project load -> Runtime/Editor publication -> render
```

The slice keeps TileSet decoding and publication in `nara_tilemap`/the project-content owner. It
does not introduce a generic importer executor, public RenderGraph, runtime residency manager, or
Tilemap render cache.

## Current Slice Status (2026-10-18)

- N01 is implemented: duplicate serialized coordinates and non-positive `tile_size` values are
  rejected before Tilemap publication.
- N02 cold-start admission is implemented: a versioned TileSet source is admitted with metadata,
  its Image dependency is added to the same startup closure, and Runtime/Editor publish typed
  `Assets<ImageAsset>` followed by `Assets<TileSet>`.
- A saved two-TileSet map cold-loads, resolves path/stable Image references, produces textured
  render batches with the expected immutable pixels, and opens/retires its trusted Editor session.
  Snapshot refresh publishes changed sources and retains earlier consumers after dependency failure.
- Native GPU readback, generic Editor source hydration, and typed TileSet hot reload remain
  separate evidence gates. No generic importer executor or residency manager is implied.

Verification: `nara_tilemap --features serde --lib` passes 19 tests;
`project_content_boot` passes 4; `project_runtime_boot` passes 11. Its TileSet consumer also passes
with `presentation-2d,serde,tooling`, proving static render batches and trusted Editor startup.
The no-GPU result does not establish WGPU readback or native Editor qualification.

## Order

1. Reject duplicate Tilemap coordinates and non-positive dimensions in the domain codec.
2. Add a versioned TileSet source envelope with an image AssetRef and atlas/material data.
3. Extend project closure admission to classify TileSet references and discover image dependencies.
4. Publish typed `Assets<TileSet>` after the already admitted Image snapshot in Runtime and Editor.
5. Add a saved-map cold-start/render-batch consumer and source identity/last-good tests.
6. Only after this slice, add a typed reload consumer for TileSet plus dependency generation checks.
7. Measure a real map before choosing culling, chunk cache, or dirty-subtree propagation.

## Ownership

- `nara_tilemap`: TileSet source payload, envelope decode, semantic validation, and conversion from
  source data to a runtime TileSet using a caller-supplied Image handle.
- `ProjectContentLoader`: source-kind classification, metadata identity, dependency closure,
  budgets, and immutable snapshot ownership.
- Project Runtime/Editor Host: reserve handles and publish immutable snapshot values into typed
  ECS asset stores.
- `nara_asset`: remains the identity/state substrate. Its Image-only reload consumer is not
  generalized until TileSet has a complete cold-start and real reload consumer.

## Contract

The [file-backed TileSet guide](../guides/file-backed-tilesets.md) describes the supported workflow.

TileSet files use the existing persistent envelope policy with canonical `format_version = 1` and
kind `tileset`. The payload contains one image `AssetRef`, a validated atlas layout, and the
bounded material sampling/alpha/tint values needed by the current renderer. The source payload
does not contain runtime handles, GPU state, or authored Tilemap cells.

Project closure records the TileSet metadata/source and the dependent Image source in one immutable
receipt. A stable reference is resolved against the admitted metadata catalogue and must retain its
source kind. A TileSet cannot publish when its image dependency is absent or stale.

All TileSet dependencies resolve before any TileSet identity is inserted into the loader database.
This permits late stable-reference catalogue admission without treating previously loaded path
references as duplicate identities. Metadata receipts are rechecked after catalogue admission.
The source requires finite tint channels and a valid atlas; metadata settings are the unit type
because this direct source format currently has no import options.

## Verification

- Duplicate serialized Tilemap coordinates reject before Scene/Prefab publication.
- Zero, negative, and narrowed-to-zero Tilemap dimensions reject with field diagnostics.
- TileSet envelope wrong kind/version/shape and invalid atlas values reject before source admission.
- A saved TileSet and Scene reopen, cold-load, publish `Assets<ImageAsset>` and `Assets<TileSet>`,
  and render through the existing Sprite/Tilemap packet path.
- Image or TileSet identity mismatches, missing metadata, missing image dependency, and failed
  source reads retain no partial Runtime/Editor publication.
- Existing Image reload, Scene/Prefab, Editor and reference-game suites remain passing.

## Deferred

Typed TileSet hot reload, recursive dependency readiness during concurrent reload, per-region
acquisition/residency, terrain/autotile/custom tile data, collision/navigation, chunk visibility,
GPU cache invalidation, and arbitrary third-party importer registration require the completed
consumer evidence above.
