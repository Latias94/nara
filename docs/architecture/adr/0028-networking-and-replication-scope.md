# ADR 0028: Networking and Replication Scope

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0042: Runtime Service and Backend Boundary; ADR 0045: Component Schema
Capability Metadata; ADR 0056: Headless Runtime and Dedicated Server Readiness

## Context

nara does not need multiplayer in Phase 1, but networking can influence entity identity, component schemas, event models, determinism, and scene loading. The architecture should not accidentally block future replication.

## Decision

Networking is a non-goal for Phase 1. Nara will avoid known blockers to future replication, but it
is not replication-ready until a concrete networking decision defines authority, identity,
protocol, interest, prediction, and compatibility contracts.

Rules:

- No Phase 1 multiplayer promise.
- Component schema IDs, stable runtime identity, typed commands/messages, and deterministic-friendly
  fixed update are useful prerequisites. They do not by themselves define network object identity,
  server authority, ownership, spawn/despawn, relevancy, dormancy, wire schemas, bandwidth budgets,
  prediction, rollback, or reconciliation.
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

### Option C: Non-goal with anti-blocking constraints (Chosen)

**Pros**: Controls scope while preserving future path.

**Cons**: Some constraints must be maintained without immediate use.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Scope control | No Phase 1 networking implementation required | Roadmap review |
| Schema foundation | Components have stable IDs/versioning without claiming a wire schema | ADR/code review |
| Event foundation | Typed events/messages can later inform a networking design | Design review |
| Optionality | Future networking lives in optional module/plugin | Dependency review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Future networking needs deeper determinism | Medium | Medium | Document current deterministic-friendly but not lockstep guarantee |
| Replication needs stable IDs not present everywhere | Medium | High | Add persistent IDs only for replicated/saved entities |
| Scope creep | High | Medium | Keep networking out of Phase 1 milestones |

## Consequences

- Headless/server readiness, fixed ticks, stable IDs, and schema metadata remain valuable
  foundations but are not presented as replication completeness.
- The first networking product slice must add a dedicated ADR before public replication APIs or
  persistent network formats are frozen.
- Nara continues to avoid promising deterministic lockstep or cross-platform solver equivalence.
