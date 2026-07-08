# ADR 0022: 3D Coordinate System

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara is 2D-first but 3D-ready. Future 3D rendering, physics, cameras, asset import, editor gizmos, and AI-generated scenes need a stable coordinate convention before those systems exist.

ADR 0018 defines world units and 2D conventions. This ADR extends the spatial model to 3D.

## Decision

nara 3D world space uses a **right-handed, Y-up coordinate system**.

Rules:

- World units are engine units, shared with 2D.
- 3D axes:
  - `+X`: right
  - `+Y`: up
  - `+Z`: backward / out of the screen
  - `-Z`: forward / into the screen
- Default forward direction for cameras and 3D entities is `-Z`.
- Default up direction is `+Y`.
- Rotations use radians.
- `Transform3d` should use translation `Vec3`, rotation quaternion, and scale `Vec3`.
- Camera projection should distinguish perspective and orthographic 3D projections.
- Imported assets must be converted into nara's coordinate convention at import time when needed.

```text
          +Y up
           |
           |
           o------ +X right
          /
         /
       +Z backward

Default camera forward: -Z
```

## Relationship to 2D

2D world space remains X right, Y up. Conceptually, 2D content lives in the XY plane, with Z or layer/sort values used by renderer-specific ordering rules when needed.

This keeps 2D authoring intuitive while allowing 3D systems to share units, math conventions, and editor gizmo behavior.

## Alternatives Considered

### Option A: Right-handed Y-up (Chosen)

**Pros**: Common in Rust/math/graphics ecosystems, aligns with Bevy-style conventions, works well with default camera looking down `-Z`.

**Cons**: Some imported assets or tools use different conventions and require conversion.

**Decision**: Chosen.

### Option B: Left-handed Y-up

**Pros**: Familiar to some engines and graphics APIs.

**Cons**: Less aligned with Bevy-style conventions and many Rust graphics math examples.

**Decision**: Rejected.

### Option C: Z-up

**Pros**: Common in some DCC/CAD workflows.

**Cons**: Less convenient for 2D/3D consistency and many game engine defaults.

**Decision**: Rejected.

## Consequences

- `Transform3d`, `Camera3d`, gizmos, physics adapters, and importers should use this convention.
- Import pipeline must record and apply coordinate conversion for assets from Z-up or left-handed tools.
- Scene/prefab data remains component-based; coordinate semantics live in component definitions and importer metadata.
- 2D and 3D share world unit semantics but keep separate authoring components.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Coordinate clarity | `Transform3d` and `Camera3d` docs define axis and forward directions | Docs/API review |
| Import consistency | Importers convert external coordinate systems explicitly | Future importer tests |
| 2D compatibility | 2D X/Y semantics remain unchanged | Example review |
| Editor consistency | Gizmos use the same axis colors/directions across 2D/3D | Future editor test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Imported assets appear rotated/flipped | High | Medium | Store importer coordinate conversion settings and test common formats |
| Users expect engine-specific conventions from other tools | Medium | Medium | Document convention clearly and provide import presets |
| 2D render ordering conflicts with Z semantics | Medium | Medium | Keep 2D layer/sort explicit; use Z only when the 2D renderer opts in |

## Follow-Up Questions

- Should 2D renderer use transform Z once `Transform3d` exists, or keep explicit layer/sort only?
- What coordinate presets should asset importers support first?
- What axis color convention should editor gizmos use?

## Citations

- Coordinate/time decision: [0018-coordinate-units-and-time.md](0018-coordinate-units-and-time.md)
- Dimension-aware runtime decision: [0005-dimension-aware-runtime-with-2d-first-authoring.md](0005-dimension-aware-runtime-with-2d-first-authoring.md)
