# ADR 0018: Coordinate, Units, and Time

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0039: Main Loop, Time Domains, Pause, and Runtime State; ADR 0097:
Future-Capable 2D and 3D Spatial Transform Model

## Context

Transform, camera, tilemap, physics, animation, and rendering all depend on consistent coordinate and time semantics. Delaying these choices creates subtle incompatibilities.

## Decision

nara uses explicit world units, dimension-specific transforms, and fixed-step simulation.

Rules:

- World units are engine units, not pixels.
- 2D uses X right, Y up in world space.
- 2D rotation is stored in radians.
- `Transform2d` uses translation `Vec2`, rotation radians, and scale `Vec2`.
- `Camera2d` maps world units to screen pixels through explicit zoom/viewport/pixels-per-unit configuration.
- Pixel-perfect behavior is a camera/rendering option, not the default coordinate system.
- Fixed timestep is the authoritative simulation cadence for physics and deterministic gameplay systems.
- Render interpolation is allowed but does not change simulation state.

Default recommendations:

```text
fixed timestep: 1 / 60 seconds
2D world: X right, Y up
tile coordinates: integer grid, converted to world units through tile size
rotation: radians
```

## Alternatives Considered

### Option A: Pixel coordinates as world coordinates

**Pros**: Simple for 2D sprites and UI-like games.

**Cons**: Poor fit for physics, zooming, multi-resolution games, and future 3D.

**Decision**: Rejected.

### Option B: World units with Y down

**Pros**: Matches screen coordinates and many 2D art tools.

**Cons**: Conflicts with physics/math conventions and future 3D.

**Decision**: Rejected for world space. Screen/UI spaces may use Y down where appropriate.

### Option C: World units with Y up and explicit camera mapping (Chosen)

**Pros**: Mature engine convention, physics-friendly, 3D-ready, still supports pixel-perfect cameras.

**Cons**: Requires clear docs for sprite/tilemap users.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Transform clarity | `Transform2d` semantics are unambiguous | Docs/API review |
| Camera clarity | Pixel mapping is controlled by camera settings | Example |
| Physics consistency | Physics runs in fixed timestep world units | Future tests |
| 3D readiness | 2D units do not conflict with future 3D space | Design review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| 2D users expect pixels | Medium | High | Provide pixel-perfect camera and tile helpers |
| UI coordinate space conflicts | Medium | Medium | Treat UI/screen coordinates as separate spaces |
| Fixed timestep causes visual stutter | Medium | Medium | Support render interpolation |
