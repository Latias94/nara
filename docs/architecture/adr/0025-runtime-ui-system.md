# ADR 0025: Runtime UI System

**Status**: Accepted
**Date**: 2026-07-08

## Context

nara needs runtime/game UI for games and eventually editor/tooling UI. The user wants nara to self-build the runtime UI system. Previous ADRs allow egui/dear-imgui-rs as pragmatic editor/debug tooling, but runtime UI should be a nara engine capability.

## Decision

nara will build its own runtime UI system.

Rules:

- Runtime/game UI is a nara-owned ECS-driven UI domain.
- UI authoring should be declarative and data-driven.
- UI components live in ECS and are inspectable/serializable where practical.
- UI layout, input routing, focus, text, style, and rendering are nara engine modules.
- egui/dear-imgui-rs may still be used for early debug/editor tooling, but they are not the foundation of runtime UI.
- Editor UI may dogfood nara UI gradually once the runtime UI is mature.

Target domain split:

```text
nara_ui
  UI tree/domain components, layout model, style, focus, input routing

nara_text
  text shaping/cache/render integration

nara_ui_render
  UI extraction, batching, clipping, render phases

nara_editor
  may initially use egui/dear-imgui, later dogfoods nara_ui where practical
```

## Alternatives Considered

### Option A: Use egui as runtime UI

**Pros**: Very fast to ship tools and debug UI.

**Cons**: Immediate-mode model may not match scene/prefab/data-driven runtime UI goals.

**Decision**: Rejected for runtime UI. Allowed for debug/editor phases.

### Option B: Use dear-imgui-rs as runtime UI

**Pros**: Mature debug/tooling UI.

**Cons**: Not appropriate as the core game UI model.

**Decision**: Rejected for runtime UI. Allowed for tools/editor experiments.

### Option C: Build nara ECS UI (Chosen)

**Pros**: Aligns with data-driven engine philosophy, scene/prefab serialization, and AI generation.

**Cons**: Significant implementation scope.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| Data-driven UI | UI can be declared as components/data | Future example |
| Input routing | Focus/hover/click routing works through engine input | Future tests |
| Render integration | UI has its own render phase and clipping | Future render tests |
| Tooling compatibility | Inspector can inspect UI components | Future editor test |
| Editor dogfooding | Editor can gradually adopt nara UI modules | Future milestone |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| UI scope delays runtime MVP | High | Medium | Keep Phase 1 UI minimal; use egui/imgui for debug tools if needed |
| Text shaping is hard | High | High | Treat text as its own module and use mature shaping libraries later |
| Editor dogfooding too early slows editor | Medium | Medium | Allow phased adoption |
| UI conflicts with scene hierarchy | Medium | Medium | Define UI tree/domain components explicitly |

## Follow-Up Questions

- Flexbox-like layout, grid, or custom retained layout?
- How does UI relate to scene hierarchy and cameras?
- What text shaping/rendering libraries are acceptable?
- What minimum runtime UI is required before editor dogfooding?

## Citations

- Editor/tooling boundary: [0015-editor-tooling-and-dogfooding-boundary.md](0015-editor-tooling-and-dogfooding-boundary.md)
- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
