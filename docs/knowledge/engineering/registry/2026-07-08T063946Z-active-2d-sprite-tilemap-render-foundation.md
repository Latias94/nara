---
type: "Active Work Registration"
title: "2D sprite tilemap render foundation"
description: "Tracks the active implementation context for the 002 2D sprite/tilemap render foundation plan."
tags: ["engineering-memory", "registry", "render", "sprite", "tilemap"]
timestamp: 2026-07-08T06:39:46Z
status: "implemented"
---

# Active Work

Historical registration for the 2D sprite/tilemap render foundation plan. The implementation has landed on local `main`.

# Implementation Authority

- [Plan 002](../../../plans/2026-07-08-002-feat-2d-sprite-tilemap-render-foundation-plan.md)
- [ADR 0005](../../../architecture/adr/0005-dimension-aware-runtime-with-2d-first-authoring.md)
- [ADR 0012](../../../architecture/adr/0012-render-crate-boundaries.md)
- [ADR 0017](../../../architecture/adr/0017-render-graph-policy.md)
- [ADR 0032](../../../architecture/adr/0032-render-backend-integration-boundary.md)

# Boundaries

- `nara_render` remains backend-neutral and stops owning sprite/tilemap authoring data.
- `nara_sprite`, `nara_tilemap`, and `nara_sprite_render` become the primary 2D authoring and render-domain crates.
- `nara_render_wgpu` remains the only crate allowed to import `wgpu`.
- `nara_winit` remains the only crate allowed to import `winit`.
- Texture upload, atlases, material specialization, full RenderGraph, editor UI, and 3D rendering remain deferred.

# Current Notes

The Godot/wgpu read-only research reinforced adding tilemap chunk identity and dirty revisions early, while keeping chunked GPU mesh caching deferred. The slice is implemented; no active claim remains.
