# ADR 0027: Save Game and Runtime Persistence

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0045: Component Schema Capability Metadata; ADR 0056: Headless Runtime and
Dedicated Server Readiness

## Context

Scene and prefab files are authoring data. Save games are runtime persistence. They overlap in component serialization but have different semantics: save files capture game state, not editor-authored layout.

## Decision

nara will treat save games as a separate persistence layer built on selected registered component data.

Rules:

- Scene/prefab documents are not save games.
- Save data stores explicit save-eligible components/resources.
- Runtime `Entity` values are not stable save identifiers.
- Save systems use stable IDs where available: scene entity IDs, persistent entity IDs, asset refs, and component type IDs.
- Components opt into save behavior separately from editor inspection.
- Backend-native transient state, task state, and GPU/audio handles are not saved directly.

## Alternatives Considered

### Option A: Serialize the whole `World`

**Pros**: Simple conceptually.

**Cons**: Captures transient/backend state, unstable entity IDs, and non-save data.

**Decision**: Rejected.

### Option B: Use scene files as save files

**Pros**: Reuses scene serialization.

**Cons**: Conflates authoring data and runtime state.

**Decision**: Rejected.

### Option C: Separate save layer over registered save components (Chosen)

**Pros**: Mature game model, explicit control, works with scene/prefab IDs.

**Cons**: Requires save eligibility metadata and migration.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Authoring/runtime separation | Scene files are not treated as save files | Design review |
| Stable identity | Save data avoids runtime `Entity` IDs | Schema review |
| Component opt-in | Only save-eligible data is persisted | Future test |
| Asset stability | Asset refs survive load/save | Future test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Save schema drifts from component schema | High | Medium | Use `ComponentRegistry` and versioned migrations |
| Runtime-created entities lack stable IDs | Medium | High | Add persistent entity IDs for save-eligible runtime entities |
| Backend state needed for restore | Medium | Medium | Reconstruct backend state from stable components after load |
