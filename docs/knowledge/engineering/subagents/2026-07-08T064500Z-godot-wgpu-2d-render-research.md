---
type: "Subagent Finding"
title: "Godot and wgpu 2D render research"
description: "Read-only research findings for the 2D sprite/tilemap render foundation plan."
tags: ["engineering-memory", "subagent", "render", "2d", "tilemap", "wgpu"]
timestamp: 2026-07-08T06:45:00Z
status: "complete"
producer_id: "readonly_godot_wgpu_2d_render"
related_plan: "docs/plans/2026-07-08-002-feat-2d-sprite-tilemap-render-foundation-plan.md"
---

# Finding

The next implementation slice should be the 2D sprite/tilemap renderer
foundation, not a full asset import pipeline, material system, or RenderGraph.
Godot's useful prior art is the CanvasItem/rendering-server separation,
z/layer/y-sort behavior, tile dirty update model, and renderer-side batching.
The OOP scene tree model should not be copied into nara's ECS architecture.

# Design Implications

- Move `Sprite` and sprite `Texture2d` out of `nara_render`.
- Add `nara_sprite`, `nara_tilemap`, and `nara_sprite_render`.
- Keep user authoring simple: `Transform2d + Camera2d + Sprite/Tilemap`.
- Keep render internals explicit: `Extract -> Prepare -> Queue -> Sort -> Render -> Cleanup`.
- Give tilemap authoring chunk coordinates and dirty semantics now, even if the
  first renderer rebuilds batches every frame.
- Prefer instanced colored quad batches over per-sprite draws.
- Defer texture upload, atlases, samplers, and full RenderGraph until a later
  concrete slice.

# Citations

- `repo-ref/godot/scene/main/canvas_item.h`
- `repo-ref/godot/scene/2d/sprite_2d.cpp`
- `repo-ref/godot/scene/2d/tile_map_layer.cpp`
- `repo-ref/godot/servers/rendering/renderer_canvas_cull.h`
- `repo-ref/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
- `repo-ref/wgpu/examples/standalone/02_hello_window/src/main.rs`
- `repo-ref/wgpu/examples/features/src/bunnymark/mod.rs`
- `repo-ref/wgpu/examples/features/src/texture_arrays/mod.rs`
