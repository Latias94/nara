# ADR 0021: Scripting and WASM Boundary

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0042: Runtime Service and Backend Boundary; ADR 0045: Component Schema
Capability Metadata

## Context

nara's roadmap includes WASM scripting and hot replacement. This must not be confused with Bevy-style Rust engine plugins. Rust systems remain the primary code-first path.

## Decision

Rust ECS systems and Rust engine plugins are the primary execution model. WASM scripting is a future sandboxed extension model.

Rules:

- `Plugin` means Rust-side engine module/capability package.
- WASM scripts/extensions are not `Plugin`; they are loaded through a scripting/runtime adapter.
- Scripts do not receive unrestricted mutable `World` access.
- Scripts operate through capability-limited query/command APIs.
- Hot replacement depends on stable component schema IDs and diagnostics.
- Scripting is Phase 3 and should not block Phase 1 runtime architecture.

## Alternatives Considered

### Option A: Make WASM scripts first-class plugins

**Pros**: One extension vocabulary.

**Cons**: Confuses engine modules with sandboxed scripts and complicates plugin lifecycle.

**Decision**: Rejected.

### Option B: No scripting boundary now

**Pros**: Simpler early design.

**Cons**: Future hot reload and AI-generated logic may conflict with ECS/world access assumptions.

**Decision**: Rejected as a design omission.

### Option C: Separate Rust plugins and WASM scripting boundary (Chosen)

**Pros**: Clear semantics and safer future sandboxing.

**Cons**: More concepts to document.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Terminology clarity | `Plugin` is Rust engine module, not WASM extension | Docs/API review |
| World safety | Scripts cannot mutate `World` directly | Future API review |
| Hot reload readiness | Script data uses stable component schemas | Design review |
| Phase isolation | Phase 1 implementation is not blocked by scripting | Roadmap review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Scripting design constrains ECS too early | Medium | Medium | Only define boundary now; implement later |
| Users confuse plugins and scripts | Medium | High | Keep terminology explicit in docs |
| Capability API too weak | Medium | Medium | Design from real script use cases before implementation |
