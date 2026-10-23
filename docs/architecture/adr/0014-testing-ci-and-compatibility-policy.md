# ADR 0014: Testing, CI, and Compatibility Policy

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-10-23
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

### Scope verification to the claim

Research feasibility, an executable tracer, a first public owner API, a reusable protocol, and
production/platform support are distinct claims with distinct evidence:

- A bounded study or tracer needs a named workflow, explicit assumptions and a falsifiable result.
  It may challenge a current contract before a complete product or second implementation exists;
  it does not silently revise an Accepted contract or qualify a public API.
- A first public owner API needs an independently compiled consumer and relevant ownership,
  validation, failure and lifetime tests. A second implementation is not a prerequisite to define
  the first concrete owner contract.
- Portable/shared abstractions and protocol freezes need evidence from the consumers and
  implementations they claim to cover. One panel or adapter does not prove toolkit interchangeability.
- Native input, rendering, accessibility, process cleanup and platform support require matching
  native evidence. Window-free semantic tests, compile-only targets and hidden-window backend tests
  do not establish user-visible platform support or OS IME candidate placement.
- A semantic authoring tool can be traced through the selected Dear ImGui frontend or a window-free
  session using current admitted actions and exact results. It need not wait for complete Nara UI,
  another toolkit, or an unrelated extension marketplace. Preview, rejection, Undo and persistence
  are qualified to the capabilities actually exercised, not inferred from panel registration.

The Reference Game remains cross-cutting production evidence, not a veto on independent extension
experiments. Update the affected Accepted contract before changing its implementation boundary;
proposed designs and historical plans are not substitute authority. Record implementation evidence
and unsupported capabilities separately from architectural intent. Documentation-only corrections
require link, status and contract-consistency checks, not a new native product acceptance cycle.

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
| Evidence scope | Tracer, public API, protocol and platform claims carry matching evidence | Review of tests and qualification records |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Breaking changes frustrate early users | Medium | Medium | Communicate pre-1.0 status clearly |
| Tests lag behind architecture | High | Medium | Add smoke tests for each accepted interface |
| ADRs become stale | Medium | Medium | Update ADR status or supersede when decisions change |
| Unrelated acceptance gates block useful experiments | High | Medium | Scope evidence to the changed capability and keep research distinct from product support |
