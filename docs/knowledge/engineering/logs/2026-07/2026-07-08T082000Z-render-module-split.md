---
type: "Implementation Log"
title: "Render module split tail"
description: "Records the post-implementation modularization of sprite render and wgpu backend code."
tags: ["engineering-memory", "render", "sprite", "wgpu", "refactor"]
timestamp: 2026-07-08T08:20:00Z
status: "implemented"
---

# Event

Split the newly implemented 2D render foundation into narrower modules after
the first complete U1-U6 implementation landed.

# Durable Shape

- `nara_sprite_render/src/lib.rs` now owns plugin wiring and public re-exports only.
- `nara_sprite_render/src/types.rs` owns extracted/queued/batched data resources.
- `nara_sprite_render/src/extract.rs` owns ECS extraction and tilemap lowering.
- `nara_sprite_render/src/queue.rs` owns view projection, deterministic sorting, and batching.
- `nara_sprite_render/src/tests.rs` owns the focused bridge tests.
- `nara_render_wgpu/src/surface.rs` owns surface resize/acquire policy, present mode selection,
  clear color conversion, surface creation, and configuration helpers.
- `nara_render_wgpu/src/sprite.rs` remains the private colored quad pipeline/buffer/shader module.

# Behavior Notes

- Equal view/phase/layer/sort sprite keys now preserve extraction source order before falling back
  to entity bits. This better matches the planned stable-sort contract.
- Pure clear frames no longer force creation of the sprite render pipeline; the backend only creates
  or looks up the sprite pipeline when a non-empty `SpriteBatch` exists.

# Citations

- [Plan 002](../../../plans/2026-07-08-002-feat-2d-sprite-tilemap-render-foundation-plan.md)
- [Implementation log](2026-07-08T071500Z-2d-sprite-tilemap-render-foundation-implemented.md)
