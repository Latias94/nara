# ADR 0024: Determinism, Fixed Update, and Replay Policy

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0039: Main Loop, Time Domains, Pause, and Runtime State; ADR 0056:
Headless Runtime and Dedicated Server Readiness; ADR 0057: Authoritative Fixed-Tick and Command
Ingress; ADR 0076: Play Runtime Debug Control and Observation

## Context

Physics, AI-generated tests, replays, editor previews, and future networking all benefit from deterministic simulation. Full cross-platform determinism is expensive, but nara should avoid making deterministic workflows impossible.

## Decision

nara will support **deterministic-friendly fixed-step simulation**, but will not promise full cross-platform deterministic replay in Phase 1.

Rules:

- Fixed update is the authoritative simulation path for physics and deterministic gameplay.
- The fixed clock advances one tick at a time immediately before each fixed schedule run. Systems
  observe a monotonic tick, one-timestep delta, and elapsed fixed time for that tick.
- Catch-up separates whole-tick debt from the sub-tick interpolation remainder. Desktop policy may
  discard excess whole ticks; server policy preserves debt up to a hard cap and fails structurally
  rather than silently dropping authoritative time.
- Variable frame update is for input collection, UI, animation interpolation, rendering, and non-authoritative presentation.
- Randomness used by deterministic systems should come from explicit seeded RNG resources.
- Background async tasks do not mutate simulation state directly; they apply results at scheduled boundaries.
- Replay support is a future feature, but input/events/state capture should not be made impossible by early design.
- Gameplay replay capture and acknowledgement require successful consumption of the exact admitted
  runtime generation and tick (ADR 0057). An earlier successful tick does not authorize a successor.
  An execution fault can leave partial effects, including deferred commands; the generation must be
  closed and explicitly reconstructed rather than replaying its unacknowledged batch in place.
- Fixed scheduling, ECS access analysis, serial execution and Schema admission do not establish
  that arbitrary Rust or native operations can be restored or replayed. Future prediction admits
  only an explicitly bounded state and effect model qualified by its concrete consumer.

```text
Frame:
  collect input
  apply task results at TaskUpdate
  run zero or more fixed simulation ticks
  run presentation update
  extract/render with interpolation
```

## Implementation Notes

- `nara_app::CoreStage::TaskUpdate` is the current async result boundary. It runs after `First` and
  before `PreUpdate`, with named sets for polling tasks, coalescing asset source changes, spawning
  asset jobs, and applying asset results.
- Production and server task pools are threaded. Tests may explicitly drive the same bounded queue
  inline. Worker completion never mutates `World` directly.
- Typed domain apply systems use stable task order keys, generations, and expected versions.
  Ordered-prefix streams defer later terminals until prior work is terminal when cross-frame
  completion-order independence is required; ready-snapshot sorting alone promises only local
  ordering at one integration boundary.

### Future Prediction Qualification

The [R4c/R10 consumer](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r10-motion-time-repeatability-and-concrete-domain-seams)
must define its recoverable state before selecting a prediction Interface. Include every state
that affects its result: selected components/resources, entity lifecycle, RNG, used system-local
state, pending commands, admitted external outcomes and any participating solver state. A tested
reconstruction strategy may restore that state; an unmodeled native or external effect refuses
admission to this prediction scope. This is a qualification boundary, not implemented rollback
support or a requirement that every ordinary system acquire a purity declaration.

Command consumption, successful simulation and authoritative confirmation are distinct facts.
A successful speculative tick alone cannot authorize an irreversible effect. The concrete effect
owner selects deferred publication, suppression or a verified recovery policy. An execution-attempt
identity rejects stale work from an abandoned run of the same tick; a semantic effect identity
supports that owner's duplicate-suppression policy across resimulation. Task cancellation does
not undo completed I/O. Neither identity promises exactly-once external delivery.

Authoring source/history transactions and debugger fresh-process restoration do not establish
these simulation properties. Real runtime faults retain ADR 0084's sticky-fault and explicit
close/reconstruction contract. The bounded prediction comparison does not select general STM,
a distributed runtime, a universal VM snapshot or a mandatory author language.

## Alternatives Considered

### Option A: Best-effort frame-rate-dependent simulation

**Pros**: Simple.

**Cons**: Poor physics/replay/testing/networking foundation.

**Decision**: Rejected.

### Option B: Full deterministic lockstep from day one

**Pros**: Strong replay/networking foundation.

**Cons**: Too restrictive and expensive before networking/replay are real requirements.

**Decision**: Rejected for Phase 1.

### Option C: Deterministic-friendly fixed-step model (Chosen)

**Pros**: Mature baseline without overcommitting to full determinism.

**Cons**: Some systems must be careful about time, RNG, and async integration.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Fixed simulation | Systems observe one monotonic clock advance per fixed schedule | `nara_app` tests |
| Seeded RNG | Deterministic systems can use explicit seeded RNG | API review |
| Async safety | Typed async results apply at declared boundaries under an explicit ordering policy | Task/domain tests |
| Replay readiness | Input/event capture can be added without replacing schedules | Design review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Users put simulation in variable update | Medium | High | Provide docs, examples, and schedule naming |
| Floating point differences break replay | Medium | Medium | Avoid promising cross-platform determinism early |
| Async jobs introduce nondeterminism | High | Medium | Apply async results only at deterministic boundaries where needed |

## Follow-Up Questions

- What is the canonical seeded RNG resource?
