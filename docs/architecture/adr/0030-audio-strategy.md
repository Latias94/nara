# ADR 0030: Audio Strategy

**Status**: Superseded
**Date**: 2026-07-08
**Refined By**: ADR 0042: Runtime Service and Backend Boundary; ADR 0079: Root Product
Capabilities and Placeholder Domain Retirement
**Superseded By**: [ADR 0095](0095-plugin-owned-specialized-domains-and-project-configuration.md)

## Context

nara needs basic audio playback, future spatial audio, streaming music, backend replacement, and editor/runtime control. Audio should follow the same domain/adapter pattern as rendering and physics.

## Decision

nara audio uses stable authoring components/commands with backend adapters.

Rules:

- Phase 1 focuses on simple sound effects and music playback.
- Audio assets are typed assets.
- Runtime audio commands/events trigger playback.
- Long-lived emitters can be represented by ECS components.
- Backend-specific voice/device handles are not serialized.
- Spatial audio is a future extension: 2D first, 3D later.
- Streaming music is supported by the asset/backend design, but not required immediately.

## Alternatives Considered

### Option A: Fire-and-forget audio only

**Pros**: Very simple.

**Cons**: Weak editor, spatial, mixer, and save/load story.

**Decision**: Rejected as the long-term model.

### Option B: Full audio graph/mixer from day one

**Pros**: Mature audio capabilities.

**Cons**: Too much scope for Phase 1.

**Decision**: Rejected for Phase 1.

### Option C: Simple playback plus backend-ready domain model (Chosen)

**Pros**: Useful early while keeping path to mixer/spatial/streaming.

**Cons**: Requires careful command/component split.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Basic playback | SFX/music can be triggered by typed handles | Future example |
| Backend neutrality | Scene/save data stores no audio backend handles | Schema review |
| Spatial readiness | Audio emitter components can add spatial fields later | Design review |
| Streaming readiness | Asset model can represent streamed audio | Future design test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Audio commands and components overlap unclearly | Medium | Medium | Use commands for one-shot actions, components for persistent emitters |
| Streaming adds threading complexity | Medium | Medium | Use IO/async pools and backend adapter ownership |
| Spatial audio conflicts with 2D/3D transforms | Medium | Medium | Define 2D spatial first, 3D later |
