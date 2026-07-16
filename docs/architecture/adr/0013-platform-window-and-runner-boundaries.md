# ADR 0013: Platform, Window, and Runner Boundaries

**Status**: Accepted
**Date**: 2026-07-08
**Last Revised**: 2026-07-16
**Refined By**: ADR 0039: Main Loop, Time Domains, Pause, and Runtime State; ADR 0041:
Input Routing, Actions, Text Input, UI Focus, and Accessibility
**Proposed Refinement Under Evaluation**: ADR 0082 and ADR 0084 may place managed product/runtime
ownership around this boundary; OQ-038 owns the reusable Platform Adapter and Runtime Driver shape.

## Context

nara needs desktop windows, input events, headless tests, future web/mobile targets, and editor/runtime runners. The app lifecycle should not be tied directly to winit.

## Decision

Keep platform adapters outside `nara_app`.

Target crate taxonomy:

```text
nara_app
  App, Plugin, schedules, top-level runner callback and managed runtime drive boundary

nara_window
  Window, WindowId, WindowEvent, cursor, monitor, display mode data

nara_winit
  winit adapter and desktop runner

nara_input
  normalized input state and action maps
```

Rules:

- `nara_app` does not depend on `winit`.
- Headless runner is first-class.
- Window events are normalized into nara event/data types before game systems consume them.
- Fixed timestep belongs to app/runtime scheduling, not winit-specific code.
- The accepted contract is a top-level driver authority and lifecycle boundary, not a public
  `Runner` trait. Its reusable Rust shape waits for OQ-038 evidence from a second production Adapter.

## Alternatives Considered

### Option A: Put winit directly in `nara_app`

**Pros**: Fast desktop MVP.

**Cons**: Pollutes app lifecycle, hurts headless tests and future platform adapters.

**Decision**: Rejected.

### Option B: No top-level runtime-driver boundary

**Pros**: Simple library design.

**Cons**: Weak engine experience and poor platform/tooling story.

**Decision**: Rejected.

### Option C: Concrete top-level driver plus platform adapters (Chosen)

**Pros**: Mature engine shape, headless-friendly, portable.

**Cons**: More boundary code.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Headless support | App can run schedules without windowing | Test |
| winit isolation | Only adapter crate depends on winit | Dependency review |
| Input normalization | Game systems consume nara input, not raw winit events | API review |
| Fixed timestep | Fixed update works in headless and windowed runners | Future test |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Driver abstraction becomes speculative | Medium | Medium | Keep callback/concrete owners until two production Adapters justify a shared shape through OQ-038 |
| Window model misses web/mobile needs | Medium | Medium | Keep window data minimal and adapter-owned |
| Input latency from over-normalization | Medium | Low | Preserve raw event access behind advanced diagnostics if needed |

## Follow-Up Questions

- [OQ-038](../open-questions.md#oq-038-platform-adapter-and-runtime-driver-interface) owns the
  eventual split and Rust shape for reusable Platform Adapter, event-loop, and Runtime Driver
  integration. This ADR does not imply a universal Host or object-safe driver trait.
