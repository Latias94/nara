# ADR 0012: Render Crate Boundaries

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-07-15
**Refined By**: ADR 0032: Render Backend Integration Boundary; ADR 0044: Root Facade and Prelude
Layering Policy; ADR 0077: Render Pipeline Recipes, Graph Compilation, and Backend Encoding; ADR
0078: Render Host Affinity, WebGPU Initialization, and Device Recovery

## Context

nara needs a mature renderer architecture without pulling wgpu details into user-facing gameplay crates. We have accepted a dimension-aware runtime and phase-based rendering.

## Decision

Split render crates by authoring domain, render domain, and backend implementation.

Target crate taxonomy:

```text
nara_render
  backend-neutral views, frame lifecycle, render phases, render items, plans, and frame stats

nara_render_wgpu
  wgpu instance/device/surface/pipeline/buffer/texture implementation

nara_sprite
  Sprite, SpriteMaterial, TextureAtlas, sprite authoring data

nara_tilemap
  Tilemap, TileLayer, Tileset, tile authoring data

nara_sprite_render
  sprite/tilemap extraction, queueing, sorting, batching for nara_render
```

Phase 1 may collapse some crates temporarily, but public module responsibilities should follow this split.

## Alternatives Considered

### Option A: One `nara_render` crate with everything

**Pros**: Simple initial Cargo setup.

**Cons**: wgpu, sprite, tilemap, and render-domain abstractions become entangled.

**Decision**: Rejected as the long-term design.

### Option B: Bevy render stack wholesale

**Pros**: Mature renderer.

**Cons**: Too complex and too Bevy-shaped for nara's product boundary.

**Decision**: Rejected.

### Option C: Domain/backend split (Chosen)

**Pros**: Keeps authoring data clean, isolates backend-native state, and makes 3D expansion
additive without inventing a mirrored RHI or speculative universal backend trait.

**Cons**: More crates and integration points.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Backend isolation | Gameplay/domain crates do not import `wgpu` | `rg "wgpu::" crates` |
| Authoring clarity | Sprite/tilemap users do not touch mesh/pipeline internals | Example review |
| Phase model | Extraction/queue/sort/render are explicit | Code review |
| 3D readiness | Future mesh/PBR crates can add render phases without replacing sprite pipeline | Design review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Too many crates too early | Medium | Medium | Collapse temporarily but keep module responsibilities clear |
| Render traits become too generic | High | Medium | Design from concrete sprite/tilemap and future mesh needs |
| Backend leaks upward | High | Medium | Enforce dependency direction and tests |

## Consequences

- `nara_render` owns stable semantic render concepts and logical frame planning, not a generic
  backend API that mirrors wgpu.
- Wgpu remains the only RHI. Backend-native capabilities and handles stay out of gameplay,
  project-facing, backend-neutral, and ordinary render-provider Interfaces. Explicitly selected
  wgpu/native interop contributions and replacement Render Host Adapters may bind the exact raw API
  under ADRs 0032, 0077, and 0078 without creating another RHI.
- A future backend requires a concrete product need and a new decision; this ADR does not promise
  source-compatible or behaviorally equivalent backend replacement.
