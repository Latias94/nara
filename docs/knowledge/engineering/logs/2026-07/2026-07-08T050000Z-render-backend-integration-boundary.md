---
type: "Architecture Decision"
title: "ADR 0032 render backend integration boundary accepted"
description: "Records the platform/window/render backend foundation boundary before implementation."
tags: ["engineering-memory", "architecture", "render", "platform"]
timestamp: 2026-07-08T05:00:00Z
status: "accepted"
---

# Event

Accepted ADR 0032 and started the platform/window/render backend foundation plan.
nara will use a fallible owned-app runner, backend-only raw handle providers, frame-local main-world extracted render data, and optional `winit`/`wgpu` facade features for the next implementation slice.

# Impact

Implementation should avoid a full RenderGraph and a separate render world for this slice.
`nara_winit` remains the only `winit` crate, `nara_render_wgpu` remains the only `wgpu` crate, and extracted render data must stay renderer-domain rather than gameplay prelude API.

# Citations

- [ADR 0032](../../../architecture/adr/0032-render-backend-integration-boundary.md)
- [Platform/window/render backend foundation plan](../../../plans/2026-07-08-001-platform-window-render-backend-foundation-plan.md)
