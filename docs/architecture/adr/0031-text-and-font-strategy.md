# ADR 0031: Text and Font Strategy

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-11-04
**Refined By**: ADR 0042: Runtime Service and Backend Boundary; ADR 0095: Plugin-Owned Specialized
Domains and Project Configuration;
[ADR 0102](0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md): Renderer-Neutral
UI, Text, Widget, and Presented-Frame Topology

## ADR 0095 Refinement

Unicode-capable text, stable font assets, and non-persistent glyph caches remain product goals. A
dedicated `nara_text` crate, shared UI/world shaping layer, or replaceable text backend is not
admitted until a real text workflow proves the consumer boundary. The first text integration may be
owned end to end by the runtime UI or a concrete text plugin.

## ADR 0102 Refinement

The reference-game UI workflow now proves a bounded CPU text consumer and the need to keep text
independent from ECS and presentation ownership. ADR 0102 therefore admits `nara_text` as a
workspace-internal deep module for deterministic font admission, shaping, bidi/line layout,
measurement, glyph keys/raster requests, and diagnostics. Project font assets, localization,
world text, replaceable providers, atlas allocation and GPU residency retain their separate owners
and qualification requirements.

The [owned Inspector unit](../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md)
extends that seam with an explicit finite ordered set of caller-selected immutable font faces.
Each shaped glyph identifies its actual admitted face, and rasterization uses that exact face.
Compiler and presentation owners receive the same configuration; system discovery stays with the
Host rather than the text module. Bounded collection containers and selected face indices are
valid immutable font inputs. Bundled single-face defaults remain available. This admits neither
project font assets nor a mutable global font service.
Selected faces use their default variation instance with automatic style synthesis disabled.
Requested variable axes and synthetic weight/slant require a separate complete raster identity.

The text owner uses pinned Parley editing primitives for a bounded single-line buffer, visual
movement, selection, hit testing and logical caret/candidate geometry. ICU segmentation constrains
navigation and deletion to Unicode grapheme boundaries; Parley's shaped ligature components do
not independently define editable characters. Preedit is visible but does
not replace the committed buffer or enter its local history. Cancellation restores the original
selection and text; confirmation replaces that selection once. A rejected operation cannot adopt
partial buffer, layout, selection or history changes. Local buffer Undo/Redo remains independent
of document history, and the receiving engine must retain the original font/limit configuration.
Field focus lifetimes, platform IME/clipboard, semantic Apply and document history remain outside
this text owner. Native candidate placement and a complete Editor workflow need their own evidence.
Complete proposed text is checked before shaping, including a 32-scalar-per-grapheme ceiling for
the pinned shaper. Actual selected faces and missing shaped glyphs drive bounded diagnostics;
the union of separate face coverage cannot certify a composed cluster.

## Context

nara will build its own runtime UI. Text and fonts are one of the hardest parts of UI: shaping, layout, fallback, glyph atlases, internationalization, and rendering all interact with assets and render phases.

## Decision

nara treats Unicode-capable text and font handling as an explicit product responsibility, not an
ASCII-only rendering shortcut. The first concrete owner may be runtime UI or a text plugin; a shared
domain/crate is extracted only after real UI/world/tooling consumers prove it.

Rules:

- Fonts are typed assets.
- One concrete owner is responsible for shaping, bidi, fallback, layout, rasterization, and cache
  semantics for its first complete workflow.
- UI and world text share lower-level font/glyph infrastructure only after both are real consumers
  with compatible requirements.
- Glyph atlas/cache management is renderer-facing backend data, not scene data.
- International text support should not be blocked by early ASCII-only assumptions.
- Phase 1 may start with simple text, but the architecture must allow shaping and fallback fonts.

## Alternatives Considered

### Option A: Simple bitmap/ASCII text only

**Pros**: Very fast.

**Cons**: Bad long-term UI/editor/internationalization foundation.

**Decision**: Rejected as the architecture.

### Option B: First Text Integration Owned by Runtime UI

**Pros**: Fewer crates.

**Cons**: World/editor consumers may later require extraction of shared infrastructure.

**Decision**: Allowed for the first complete workflow if ownership remains explicit.

### Option C: Dedicated Shared Text/Font Domain Immediately

**Pros**: Mature UI foundation and reusable rendering infrastructure.

**Cons**: More design and dependencies later.

**Decision**: Deferred until multiple real consumers prove the boundary.

### Option D: Concrete Owner First with Stable Product Constraints (Chosen)

**Pros**: Delivers Unicode-capable text without freezing crate topology or a portable backend API.

**Cons**: A later second consumer may require a deliberate extraction.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Font identity | Fonts are typed assets | API review |
| UI readiness | One concrete UI/text owner completes shaping, layout, fallback, and diagnostics | Product tracer |
| Render readiness | Glyph atlas is backend/runtime data, not scene data | Code review |
| I18n readiness | Architecture does not assume ASCII-only text | Review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Text shaping scope explodes | High | High | Start with one complete concrete owner and resist shared topology until a second consumer |
| Font fallback is complex | Medium | High | Add fallback after basic shaping/layout |
| Glyph atlas backend leaks into UI | Medium | Medium | Keep atlas in render/text backend resources |
