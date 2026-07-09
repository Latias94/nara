# ADR 0014: Testing, CI, and Compatibility Policy

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0055: Feature Matrix, Boundary Checks, and Compatibility Fixtures

## Context

nara is early-stage engine infrastructure. It needs fast iteration, but architectural regressions can become expensive quickly. The project also follows local Rust conventions: prefer `cargo nextest` for tests and `cargo fmt` for formatting.

## Decision

nara will optimize for correctness and refactorability over early backwards compatibility.

Rules:

- Use `cargo fmt --all` for formatting.
- Use `cargo nextest run --workspace` as the preferred test command.
- Use `cargo check --workspace` as a fast compile gate.
- Examples should compile and serve as API smoke tests.
- Early versions may make breaking changes; update call sites directly rather than keeping compatibility layers.
- ADRs should be updated when architectural decisions change.

## Alternatives Considered

### Option A: Strong backwards compatibility immediately

**Pros**: Stable early user API.

**Cons**: Slows engine foundation design before product-market fit.

**Decision**: Rejected for pre-1.0.

### Option B: No policy

**Pros**: Maximum flexibility.

**Cons**: Inconsistent verification and stale docs.

**Decision**: Rejected.

### Option C: Fast gates plus no pre-1.0 compatibility promise (Chosen)

**Pros**: Supports fearless refactoring while keeping quality gates.

**Cons**: Users must tolerate breaking changes early.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Formatting | `cargo fmt --all` passes | Local/CI |
| Compile health | `cargo check --workspace` passes | Local/CI |
| Tests | `cargo nextest run --workspace` passes | Local/CI |
| Examples | Examples compile as smoke tests | Local/CI |
| Docs alignment | ADRs match current architecture | Review |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Breaking changes frustrate early users | Medium | Medium | Communicate pre-1.0 status clearly |
| Tests lag behind architecture | High | Medium | Add smoke tests for each accepted interface |
| ADRs become stale | Medium | Medium | Update ADR status or supersede when decisions change |
