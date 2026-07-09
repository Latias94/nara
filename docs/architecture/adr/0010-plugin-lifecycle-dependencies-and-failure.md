# ADR 0010: Plugin Lifecycle, Dependencies, and Failure

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0040: Render Resource Lifetime and Submitter Ownership; ADR 0046:
Plugin Metadata and Default Plugin Groups

## Context

nara owns its app/plugin lifecycle. Here, "plugin" means a Bevy-style Rust engine module or capability package, not a Zed-style WASM extension. Plugins install core systems, assets, render backends, window adapters, audio, tooling, and editor integration. Some plugin actions are pure registration; others may fail due to GPU/device/window/filesystem conditions.

## Decision

nara plugins use a staged lifecycle:

```text
build      fallible registration of systems/resources/types and cheap dependency installation
configure  optional validation and dependency checks
finish     fallible initialization for backends/adapters
cleanup    optional transfer/removal of temporary setup state
```

Phase 1 implements `build`, `finish`, and `cleanup`. `build` is already fallible so dependency
installation and expected setup failures do not need panic paths, while expensive backend/device
work still belongs in runners, backend systems, or a later initialization phase.

Rules:

- Plugins are Rust-side engine modules compiled into the application or editor.
- WASM scripting/extensions are a separate topic and should not be conflated with `Plugin`.
- Unique plugins are rejected if added twice through `App::add_plugin`; prerequisite groups can use
  `App::add_plugin_if_missing`.
- Plugin dependencies should become declared metadata before the surface stabilizes; until then,
  dependent plugin groups install cheap prerequisites through fallible app APIs.
- `build` should not do expensive backend/device work.
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

### Option C: Staged lifecycle with fallible app/plugin boundaries (Chosen)

**Pros**: Keeps registration simple while supporting mature backend initialization and structured
dependency failures.

**Cons**: More lifecycle states to document and test.

**Decision**: Chosen.

## Implementation Notes

As of 2026-07-09:

- `Plugin::build(&self, app: &mut App) -> Result<(), PluginError>`.
- `App::add_plugin` returns `Result<&mut App, PluginError>` and does not register a plugin whose
  `build` fails.
- `App::add_plugin_if_missing` preserves ergonomic plugin groups without panic-on-duplicate helper
  functions.
- `PluginError::MissingPrerequisite` is the current structured dependency error; future dependency
  metadata can build on it without changing the panic policy.

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
