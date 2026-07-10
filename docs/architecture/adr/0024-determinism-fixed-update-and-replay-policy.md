# ADR 0024: Determinism, Fixed Update, and Replay Policy

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0039: Main Loop, Time Domains, Pause, and Runtime State; ADR 0056:
Headless Runtime and Dedicated Server Readiness

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
- What data is required for a future replay capture?
