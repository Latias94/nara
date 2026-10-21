# ADR 0027: Save Game and Runtime Persistence

**Status**: Accepted
**Date**: 2026-07-08
**Amended**: 2026-10-21 by the game-owned two-level checkpoint consumer
**Refined By**: ADR 0045: Component Schema Capability Metadata; ADR 0056: Headless Runtime and
Dedicated Server Readiness

## Context

Scene and prefab files are authoring data. Save games are runtime persistence. They overlap in component serialization but have different semantics: save files capture game state, not editor-authored layout.

## Decision

Nara treats save games as a separate persistence layer over explicitly selected game state.
The game owns the record shape, eligibility, versioning and reconstruction policy. A registered
component codec may help encode selected data, but registration or Editor inspection never grants
save eligibility. The first consumer uses ordinary versioned Rust DTOs before extracting a shared
engine persistence interface.

Rules:

- Scene/prefab documents are not save games.
- Save data stores explicit selected logical records, components or resources.
- Runtime `Entity` values are not stable save identifiers.
- Save systems use stable IDs where available: scene entity IDs, persistent entity IDs, asset refs, and component type IDs.
- Games select save behavior separately from editor inspection; no universal metadata flag is required.
- Backend-native transient state, task state, and GPU/audio handles are not saved directly.
- The two-level checkpoint stores logical level-instance keys, authored pickup IDs, progress and
  game-owned dynamic marker serials. Restore validates the versioned data and referenced content
  before committing, then reconstructs transient simulation under its existing owners.
- Checkpoint restore is distinct from arbitrary-tick quicksave, deterministic replay and native
  backend rollback. Generalize those contracts only when a real consumer requires them.

## Alternatives Considered

### Option A: Serialize the whole `World`

**Pros**: Simple conceptually.

**Cons**: Captures transient/backend state, unstable entity IDs, and non-save data.

**Decision**: Rejected.

### Option B: Use scene files as save files

**Pros**: Reuses scene serialization.

**Cons**: Conflates authoring data and runtime state.

**Decision**: Rejected.

### Option C: Separate game-owned save format (Chosen)

**Pros**: Mature game model, explicit control, works with scene/prefab IDs.

**Cons**: The game must define its selection, migration and reconstruction policy explicitly.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Authoring/runtime separation | Scene files are not treated as save files | Design review |
| Stable identity | Save data avoids runtime `Entity` IDs | Schema review |
| Explicit selection | Only game-selected data is persisted | Two-level DTO and fresh-process tests |
| Asset stability | Asset refs survive load/save | Future test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Save schema drifts from game/content rules | High | Medium | Version the save format and validate logical references before commit |
| Runtime-created entities lack stable IDs | Medium | High | Add persistent entity IDs for save-eligible runtime entities |
| Backend state needed for restore | Medium | Medium | Reconstruct backend state from stable components after load |
