# ADR 0029: Animation Strategy

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0045: Component Schema Capability Metadata

## Context

Animation affects 2D-first experience, component schemas, reflection, assets, and editor timelines. nara should support sprite animation early while leaving room for field animation, skeletal animation, and future 3D animation.

## Decision

nara animation is asset-driven and component-targeted.

Rules:

- Phase 1 should support simple sprite/frame animation as a first 2D feature.
- Animation clips are assets.
- Persistent animation targets use a stable entity selector plus `ComponentTypeId` and
  `ComponentFieldId`. A schema-aware `ComponentFieldPath` may be resolved while authoring or
  binding, but path text is not the durable field identity.
- Runtime animation state is component data.
- Future animation domains can include timeline curves, skeletal 2D, skeletal 3D, and animation graphs.
- Animation should run in deterministic-friendly schedules when it affects gameplay state; presentation-only animation may run in frame update.

## Alternatives Considered

### Option A: Hardcode sprite animation only

**Pros**: Fast 2D results.

**Cons**: Poor path to transform/material/UI/3D animation.

**Decision**: Rejected as the long-term model.

### Option B: Full animation graph from day one

**Pros**: Mature high-end feature set.

**Cons**: Too large before basic runtime/rendering.

**Decision**: Rejected for Phase 1.

### Option C: Asset clips targeting registered component fields (Chosen)

**Pros**: Works for sprite animation now and grows toward timelines/3D later.

**Cons**: Requires stable schema IDs, a binding phase, and explicit interpolation rules.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| 2D usefulness | Sprite animation can be expressed as an asset clip | Future example |
| Schema integration | Persistent targets use stable component/field IDs and survive field rename | Design review |
| Future growth | 3D/skeletal animation can be added as new domains | Architecture review |
| Schedule clarity | Gameplay-affecting animation can run in fixed update | Future tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Target binding drifts across schema changes | High | Medium | Persist stable IDs, resolve paths only during binding, and validate migrations/tombstones |
| Animation scope explodes | High | Medium | Start with sprite/frame clips |
| Interpolation rules are unclear | Medium | Medium | Define per-field animation value traits later |

## Consequences

- Display names and authoring paths can change without rewriting animation target identity.
- Clip loading must bind stable IDs against a frozen schema catalog before animation writes become
  active; missing or tombstoned targets produce typed diagnostics.
- Write arbitration, blend order, root motion, event timing, and gameplay-versus-presentation
  scheduling remain separate decisions for the first non-trivial animation slice.
