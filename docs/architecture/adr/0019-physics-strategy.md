# ADR 0019: Physics Strategy

**Status**: Superseded
**Date**: 2026-07-08
**Last Revised**: 2026-07-15
**Refined By**: ADR 0042: Runtime Service and Backend Boundary
**Superseded By**: [ADR 0095](0095-plugin-owned-specialized-domains-and-project-configuration.md)

## Context

Physics is a major extension seam. nara should isolate mature physics backends without putting
backend-native handles in scene files or core gameplay components. Isolation permits another
adapter when a real product requires it; it does not make different solvers numerically or
behaviorally interchangeable.

## Decision

nara will define high-level physics domain crates and backend adapters.

Initial direction:

```text
nara_physics2d
  RigidBody2d
  Collider2d
  Sensor2d
  PhysicsMaterial2d
  CollisionLayers
  PhysicsWorld2d settings/events

nara_physics2d_box2d / nara_physics2d_rapier / nara_physics2d_avian
  concrete backend adapters
```

Rules:

- Phase 1 does not self-build a complete physics engine.
- Physics runs on fixed timestep.
- Scene/prefab files store high-level nara physics components, not backend handles.
- Backend plugins negotiate declared capabilities, synchronize ECS data to backend state, and emit
  collision/contact results at named schedule boundaries.
- The first concrete backend uses plugin-installed resources and systems plus a private,
  runtime-generation-scoped session. That session owns the solver world, native body/fixture
  handles, callback state, runtime-entity mappings, queues, and domain close mechanics. The
  enclosing runtime-generation owner retains its typed close obligation, polls the close
  participant, and blocks conflicting replacement until retirement reaches a terminal result.
- Do not publish a generic `PhysicsBackend2d` trait before a fake/test integration or a second real
  solver proves which behavior actually varies. A first-party Box2D-style Adapter and a third-party
  Adapter follow the same domain rules; first-party support does not move solver handles into core.
- Scene schemas express nara-owned intent. Unsupported required capabilities reject composition;
  optional capabilities require explicit fallback policy.
- Changing solver or adapter may change contacts, ordering, stability, determinism, tuning, and
  serialized backend-specific extension data. Nara does not promise transparent save/replay or
  behavioral equivalence across backends.
- 3D physics is a future parallel domain: `nara_physics3d`.

## Alternatives Considered

### Option A: Self-build physics

**Pros**: Full control.

**Cons**: Large scope and high correctness/performance risk.

**Decision**: Rejected.

### Option B: Hardcode one backend into nara core

**Pros**: Fast integration.

**Cons**: Backend choice leaks into scene schema and public API.

**Decision**: Rejected.

### Option C: High-level physics components plus backend adapters (Chosen)

**Pros**: Mature engine shape, backend isolation, explicit capability negotiation, and stable core
scene data.

**Cons**: Adapter synchronization complexity.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Backend neutrality | Scene physics data contains no backend handles | Schema review |
| Fixed timestep | Physics steps from fixed update only | Future test |
| Adapter isolation | A fake/test integration can satisfy the domain boundary without backend handles in scene data or a premature public backend trait | Future test |
| Capability rejection | Missing required solver capabilities reject composition before runtime mutation | Future test |
| Diagnostics | Invalid collider/body combos emit structured diagnostics | Future test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| High-level components mismatch backend capability | High | Medium | Spike one concrete backend before freezing schemas |
| Sync code gets complex | Medium | High | Use generation IDs and clear ownership |
| Event ordering nondeterministic | Medium | Medium | Emit events at fixed schedule boundaries |
| First backend freezes the wrong public trait | High | Medium | Keep the first session private; extract a physics-specific Interface only after a fake or second solver supplies counterevidence |

## Consequences

- Nara can admit a second adapter without redesigning scene identity or leaking native handles.
- Nara may ship a first-party physics package and concrete solver Adapter through the same plugin,
  service-session, schema, and tooling seams available to external packages.
- Backend selection is an explicit product/profile choice with capability diagnostics, not a claim
  that every physics scene behaves identically on every solver.
- Transform authority, contact/query freshness, and write-back timing still require a concrete
  physics integration decision before the first production adapter freezes those semantics.
