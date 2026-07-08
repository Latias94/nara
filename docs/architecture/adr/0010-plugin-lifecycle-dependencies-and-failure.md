# ADR 0010: Plugin Lifecycle, Dependencies, and Failure

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara owns its app/plugin lifecycle. Here, "plugin" means a Bevy-style Rust engine module or capability package, not a Zed-style WASM extension. Plugins install core systems, assets, render backends, window adapters, audio, tooling, and editor integration. Some plugin actions are pure registration; others may fail due to GPU/device/window/filesystem conditions.

## Decision

nara plugins use a staged lifecycle:

```text
build      infallible registration of systems/resources/types
configure  optional validation and dependency checks
finish     fallible initialization for backends/adapters
cleanup    optional transfer/removal of temporary setup state
```

Phase 1 can implement only `build`, but the interface design should reserve the lifecycle.

Rules:

- Plugins are Rust-side engine modules compiled into the application or editor.
- WASM scripting/extensions are a separate topic and should not be conflated with `Plugin`.
- Unique plugins are rejected if added twice.
- Plugin dependencies are declared as metadata, not encoded through hidden ordering assumptions.
- `build` should not do fallible backend work.
- Fallible initialization belongs in `finish` or runner/backend initialization.
- Plugin errors should be diagnostics-aware.

## Alternatives Considered

### Option A: Single `build(&mut App)` only

**Pros**: Simple and Bevy-like.

**Cons**: Backend initialization and dependency errors become ad hoc.

**Decision**: Good Phase 1 implementation, but not enough as the long-term lifecycle.

### Option B: Fully async plugin lifecycle from day one

**Pros**: Handles GPU/device/import setup uniformly.

**Cons**: Makes every plugin pay async complexity.

**Decision**: Rejected for now.

### Option C: Staged lifecycle with fallible finish (Chosen)

**Pros**: Keeps registration simple while supporting mature backend initialization.

**Cons**: More lifecycle states to document and test.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Duplicate behavior | Unique plugin duplicate returns error | Unit test |
| Dependency behavior | Missing dependency is diagnosed | Unit test |
| Backend failure | Render/window init can fail without panic | Future integration test |
| Simplicity | Common plugins only implement `build` | API review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Lifecycle over-designed before implementation | Medium | Medium | Reserve phases in ADR; implement incrementally |
| Hidden dependency ordering persists | Medium | High | Add dependency metadata before many plugins exist |
| Async backend init leaks into all plugins | Medium | Medium | Keep fallible finish separate from general build |
