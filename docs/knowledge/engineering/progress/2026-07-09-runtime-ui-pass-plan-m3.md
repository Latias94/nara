---
type: "Work Progress"
title: "Runtime UI and Render Pass Plan M3"
description: "Progress record for the runtime UI, UI render batching, pointer input, and pass-plan slice."
tags: ["runtime-ui", "render", "input", "pass-plan", "ce-work"]
timestamp: 2026-07-09T17:30:26+08:00
status: "implemented"
related_plan: "docs/plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md"
git_commit: "af2be6f"
---

# Scope

Implemented M3 from the render/UI/apply foundation plan:

- `nara_input` now owns normalized `PointerState` alongside keyboard and mouse `ButtonInput`.
- `nara_winit` updates pointer position and pointer-leave state from desktop events.
- `nara_ui` owns the first runtime ECS UI authoring and runtime projection layer:
  `UiRoot`, `UiNode`, `UiPanel`, `UiPanelMaterial`, `UiStyle`, `UiVal`, `ComputedUiLayouts`, and
  `UiInteractionState`.
- `nara_ui_render` owns backend-neutral UI extraction, queueing, clipping, sorting, and batching.
- `nara_render` owns `RenderPassPlan` for static clear/world/UI/gizmo ordering.
- `nara_render_wgpu` consumes sprite and UI batches through the pass plan and reuses the image
  prepare plus material-key cache path for UI image panels.
- The root `nara` facade exports `ui` and `ui_render` without making `winit` or `wgpu` default
  dependencies.

# Residuals

- Text and font work remains in the future `nara_text` domain.
- Runtime UI has a panel/layout/input foundation, not a full widget toolkit.
- Full `RenderGraph` remains deferred until resource lifetime, post-processing, render-to-texture,
  editor viewport composition, or 3D depth/prepass needs justify it.

# Citations

- [Plan](../../../plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md)
- [Runtime UI ADR](../../../architecture/adr/0025-runtime-ui-system.md)
- [Render graph policy ADR](../../../architecture/adr/0017-render-graph-policy.md)
