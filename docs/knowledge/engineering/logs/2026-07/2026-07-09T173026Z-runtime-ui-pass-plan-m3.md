---
type: "Memory Event"
title: "Runtime UI and render pass plan M3 implemented"
description: "M3 added the first nara-owned runtime UI foundation and backend-neutral pass plan."
tags: ["runtime-ui", "render", "pass-plan", "input"]
timestamp: 2026-07-09T17:30:26+08:00
event_kind: "Implementation"
git_commit: "af2be6f"
---

# Event

M3 implemented the first runtime UI and pass-plan foundation. `nara_ui` provides ECS UI authoring
components, computed layout, and pointer interaction state. `nara_ui_render` produces clipped
color/image UI batches through the same image prepare and material-key path as sprite rendering.
`nara_render` owns `RenderPassPlan`, and `nara_render_wgpu` consumes sprite and UI batches through
that plan for clear/world/UI/gizmo ordering.

# Impact

Runtime UI is now an engine domain rather than only an architectural intent. The remaining UI work
is higher-level product depth: text/font integration, widgets, richer layout, editor dogfooding,
and future render graph promotion when pass/resource pressure requires it.

# Citations

- [Progress](../../progress/2026-07-09-runtime-ui-pass-plan-m3.md)
- [Verification](../../verification/2026-07-09-runtime-ui-pass-plan-m3.md)
