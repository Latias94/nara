---
type: "Memory Event"
title: "Verification: U7 wgpu texture cache and shader sampling implemented"
description: "U7 connected SpriteBatch texture keys and UVs to nara_render_wgpu texture upload, bind groups, samplers, and WGSL sampling."
timestamp: 2026-07-08T11:35:34Z
event_kind: "Verification"
---
# Event

U7 wgpu sprite texture path implemented. `nara_render_wgpu` now depends on `nara_asset` and `nara_image`, reads `Assets<ImageAsset>` plus `PreparedRenderResources<PreparedImageResource>`, uploads ready RGBA8 images into a `WgpuSpriteTextureCache`, binds either an image texture or a white fallback texture per sprite batch, and samples through `sprite.wgsl` using per-instance UV data. The cache compares `RenderResourceSnapshot` for hot-reload updates and prunes unused image resources from the current frame's `SpriteBatches`.

# Verification

- `cargo fmt --all`
- `cargo nextest run -p nara_render_wgpu`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo nextest run --workspace`
- `rg -n "wgpu::|wgpu =" crates/nara_sprite crates/nara_tilemap crates/nara_sprite_render crates/nara_image crates/nara_render`
- `rg -n "winit::|winit =" crates/nara_sprite crates/nara_tilemap crates/nara_sprite_render crates/nara_image crates/nara_render crates/nara_render_wgpu`
- `cargo tree -p nara --no-default-features | rg -n "wgpu|winit|nara_render_wgpu|nara_winit"`

# Impact

The backend now consumes `SpriteBatches` without inspecting gameplay components. U8 can focus on scene/prefab stable asset preflight and diagnostics, while U9 can add textured examples/docs on top of the working resource path.

# Citations

- [Plan](../../../plans/2026-07-08-004-feat-asset-render-resource-seam-plan.md)
- [Foundation](../../../architecture/nara-foundation.md)
