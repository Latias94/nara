# ADR 0025: Runtime UI System

**Status**: Accepted
**Date**: 2026-07-08
**Refined By**: ADR 0041: Input Routing, Actions, Text Input, UI Focus, and Accessibility
**Related Design Draft**: [UI Product Boundaries, Editor Dogfooding, and Porting
Strategy](../ui-product-boundaries-editor-dogfood-and-porting-strategy.md)

## Context

nara needs a first-party runtime/game UI for shipped games. Editor/tool UI is a separate product
layer that may dogfood this capability later, but it must not determine the runtime UI's persistent
model or delivery schedule. Previous ADRs allow egui/dear-imgui-rs as pragmatic editor/debug
tooling, while runtime UI remains a nara engine capability.

## Decision

nara will build its own runtime UI system.

Rules:

- Runtime/game UI is a nara-owned retained UI domain with ECS-backed authoring.
- UI authoring should be declarative and data-driven. Scene/ECS component data is authoring truth,
  not a promise that the hot execution path permanently uses one runtime `Entity` per materialized
  widget.
- Persistent UI components live in ECS and are inspectable/serializable where practical. Internal
  runtime projections may use stable logical widget identities, incremental invalidation, spatial
  indexes, measurement caches, and virtualized materialization without becoming project data.
- UI layout, input routing, focus, text, style, and rendering are nara engine modules.
- egui/dear-imgui-rs may still be used for early debug/editor tooling, but they are not the foundation of runtime UI.
- Editor UI may dogfood nara UI gradually once the runtime UI is mature. A first complete panel
  proves an adapter path; only heterogeneous editor workloads can justify selecting nara UI as the
  primary editor toolkit.

Target domain split:

```text
nara_ui
  UI tree/domain components, layout model, style, focus, input routing

nara_text
  text shaping/cache/render integration

nara_ui_render
  UI extraction, batching, clipping, render phases

nara_editor
  may initially use egui/dear-imgui and later dogfood nara_ui where evidence supports it;
  Editor Shell and Host authority remain editor/platform concerns
```

Implementation note, 2026-07-09:

- `nara_ui` exists as the first runtime ECS UI slice. It provides `UiRoot`, `UiNode`, `UiPanel`,
  `UiPanelMaterial`, simple style values, runtime-only `ComputedUiLayouts`, and runtime-only
  `UiInteractionState`.
- UI uses ordinary ECS hierarchy through `Parent`/`Children`. Persistent UI authoring components
  are registered in the component registry; computed layout and interaction resources are not scene
  data. This Phase 1 representation does not freeze the final optimized execution projection.
- `nara_input::PointerState` feeds hover, press, and focus foundation. Keyboard/gamepad focus,
  navigation, widgets, and actions remain future work.
- `nara_ui_render` extracts panels, resolves color/image materials through `nara_image` and
  `nara_material`, clips panels, and emits `UiBatches` for the UI render phase.
- Text/font work remains delegated to `nara_text`; no placeholder text system is hidden in UI
  rendering.

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
| Data-driven UI | UI can be declared as ECS components/data | `nara_ui` component codec and layout tests |
| Execution scalability | Large dynamic collections can bound materialized work while preserving stable selection, focus, and reveal behavior | Future virtual-collection workload tests and frame traces |
| Input routing | Hover/press/focus foundation works through engine pointer input | `nara_ui` interaction tests |
| Render integration | UI has its own render phase, clipping, color/image panel batching, and desktop example | `nara_ui_render` tests and `runtime_ui_panel` example check |
| Tooling compatibility | Inspector can inspect UI components | Future editor test |
| Editor dogfooding | One complete panel can adopt nara UI without changing tooling commands or granting Host authority; later heterogeneous workloads decide broader convergence | OQ-010 cross-adapter and workload evidence |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| UI scope delays runtime MVP | High | Medium | Keep Phase 1 UI minimal; use egui/imgui for debug tools if needed |
| Text shaping is hard | High | High | Treat text as its own module and use mature shaping libraries later |
| Editor dogfooding too early slows editor | Medium | Medium | Allow phased adoption |
| ECS authoring tree becomes an inefficient permanent execution tree | High | Medium | Keep authoring identity stable while permitting incremental, virtualized internal projections |

## Follow-Up Questions

- Which advanced layout model should follow the current simple style resolver: flexbox-like layout,
  grid, or a smaller retained model?
- How should UI/screen-space cameras, multiple viewports, and editor overlays compose?
- What text shaping/rendering libraries are acceptable?
- Which complete editor panel should prove the first adapter migration, and which later virtualized
  hierarchy/table, viewport, timeline, or graph workload should decide whether broader convergence
  is justified?

## Citations

- Editor/tooling boundary: [0015-editor-tooling-and-dogfooding-boundary.md](0015-editor-tooling-and-dogfooding-boundary.md)
- Render crate boundaries: [0012-render-crate-boundaries.md](0012-render-crate-boundaries.md)
- UI product boundaries and evidence gates:
  [ui-product-boundaries-editor-dogfood-and-porting-strategy.md](../ui-product-boundaries-editor-dogfood-and-porting-strategy.md)
