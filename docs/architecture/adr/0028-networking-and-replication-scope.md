# ADR 0028: Networking and Replication Scope

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara does not need multiplayer in Phase 1, but networking can influence entity identity, component schemas, event models, determinism, and scene loading. The architecture should not accidentally block future replication.

## Decision

Networking is a non-goal for Phase 1, but nara remains replication-ready.

Rules:

- No Phase 1 multiplayer promise.
- Component schema IDs, stable entity IDs, events/messages, and deterministic-friendly fixed update should remain compatible with future replication.
- Full deterministic lockstep is not promised.
- Future networking should be an optional domain module, not built into core ECS.
- Replication should use stable component/schema metadata, not raw reflection internals.

## Alternatives Considered

### Option A: Build networking from day one

**Pros**: Strong multiplayer foundation.

**Cons**: Major scope expansion before runtime/rendering/product basics.

**Decision**: Rejected.

### Option B: Ignore networking entirely

**Pros**: Simpler near-term work.

**Cons**: May create avoidable identity/schema/determinism blockers.

**Decision**: Rejected.

### Option C: Non-goal with readiness constraints (Chosen)

**Pros**: Controls scope while preserving future path.

**Cons**: Some constraints must be maintained without immediate use.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Scope control | No Phase 1 networking implementation required | Roadmap review |
| Schema readiness | Components have stable IDs/versioning | ADR/code review |
| Event readiness | Typed events/messages can later be captured/replicated | Design review |
| Optionality | Future networking lives in optional module/plugin | Dependency review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Future networking needs deeper determinism | Medium | Medium | Document current deterministic-friendly but not lockstep guarantee |
| Replication needs stable IDs not present everywhere | Medium | High | Add persistent IDs only for replicated/saved entities |
| Scope creep | High | Medium | Keep networking out of Phase 1 milestones |

