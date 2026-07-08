# ADR 0019: Physics Strategy

**Status**: Accepted
**Date**: 2026-07-08

## Context

Physics is a major extension seam. nara should support replaceable mature physics backends without putting backend-native handles in scene files or core gameplay components.

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
- Backend plugins synchronize ECS data to backend state and emit collision/contact events.
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

**Pros**: Mature engine shape, replaceable backend, stable scene data.

**Cons**: Adapter synchronization complexity.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Backend neutrality | Scene physics data contains no backend handles | Schema review |
| Fixed timestep | Physics steps from fixed update only | Future test |
| Replaceability | At least one fake/test backend can satisfy the interface | Future test |
| Diagnostics | Invalid collider/body combos emit structured diagnostics | Future test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| High-level components mismatch backend capability | High | Medium | Spike one concrete backend before freezing schemas |
| Sync code gets complex | Medium | High | Use generation IDs and clear ownership |
| Event ordering nondeterministic | Medium | Medium | Emit events at fixed schedule boundaries |

