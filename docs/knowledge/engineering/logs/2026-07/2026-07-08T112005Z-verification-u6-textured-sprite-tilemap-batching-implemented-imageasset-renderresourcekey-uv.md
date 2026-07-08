---
type: "Memory Event"
title: "Verification: U6 textured sprite/tilemap batching implemented"
description: "U6 replaced sprite-local Texture2d with ImageAsset handles and added resource-keyed UV sprite batches."
timestamp: 2026-07-08T11:20:05Z
event_kind: "Verification"
---
# Event

U6 sprite/tilemap render resource seam implemented. `nara_sprite::Sprite` now binds `Handle<ImageAsset>` and `TextureRegion` normalized UV regions. `nara_tilemap::TileSet` now stores an optional `Handle<ImageAsset>` plus `TileAtlasLayout`. `nara_sprite_render` extracts texture handles and UVs, queues textured items only when `Assets<ImageAsset>` and `PreparedRenderResources<PreparedImageResource>` are ready, records missing/unprepared/invalid texture stats, and splits batches by `RenderResourceKey` without reordering transparent source order.

# Verification

- `cargo fmt --all`
- `cargo nextest run -p nara_sprite -p nara_tilemap -p nara_sprite_render -p nara_image -p nara_render_wgpu`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo nextest run --workspace`
- `rg -n "Texture2d|unsupported_textured_sprites" crates src examples docs/architecture`
- `rg -n "wgpu::|wgpu =" crates/nara_sprite crates/nara_tilemap crates/nara_sprite_render crates/nara_image crates/nara_render`
- `rg -n "winit::|winit =" crates/nara_sprite crates/nara_tilemap crates/nara_sprite_render crates/nara_image crates/nara_render`

# Impact

U7 can now focus on the wgpu texture cache, bind groups, sampler state, and shader sampling using `SpriteBatch::texture` and `SpriteInstance::uv`; it no longer needs to understand gameplay `Sprite` or `Tilemap` components.

# Citations

- [Plan](../../../plans/2026-07-08-004-feat-asset-render-resource-seam-plan.md)
- [Foundation](../../../architecture/nara-foundation.md)
