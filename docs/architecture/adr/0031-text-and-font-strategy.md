# ADR 0031: Text and Font Strategy

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0042: Runtime Service and Backend Boundary

## Context

nara will build its own runtime UI. Text and fonts are one of the hardest parts of UI: shaping, layout, fallback, glyph atlases, internationalization, and rendering all interact with assets and render phases.

## Decision

nara treats text as a dedicated engine domain, not a side effect of UI rendering.

Rules:

- Fonts are typed assets.
- Text layout/shaping is handled by a dedicated `nara_text` domain.
- UI and world text share lower-level font/glyph infrastructure where practical.
- Glyph atlas/cache management is renderer-facing backend data, not scene data.
- International text support should not be blocked by early ASCII-only assumptions.
- Phase 1 may start with simple text, but the architecture must allow shaping and fallback fonts.

## Alternatives Considered

### Option A: Simple bitmap/ASCII text only

**Pros**: Very fast.

**Cons**: Bad long-term UI/editor/internationalization foundation.

**Decision**: Rejected as the architecture.

### Option B: Text buried inside UI crate

**Pros**: Fewer crates.

**Cons**: World text, editor text, and glyph cache concerns become tangled.

**Decision**: Rejected long-term.

### Option C: Dedicated text/font domain (Chosen)

**Pros**: Mature UI foundation and reusable rendering infrastructure.

**Cons**: More design and dependencies later.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Font identity | Fonts are typed assets | API review |
| UI readiness | UI can depend on `nara_text` rather than own shaping | Design review |
| Render readiness | Glyph atlas is backend/runtime data, not scene data | Code review |
| I18n readiness | Architecture does not assume ASCII-only text | Review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Text shaping scope explodes | High | High | Start simple but isolate text domain |
| Font fallback is complex | Medium | High | Add fallback after basic shaping/layout |
| Glyph atlas backend leaks into UI | Medium | Medium | Keep atlas in render/text backend resources |
