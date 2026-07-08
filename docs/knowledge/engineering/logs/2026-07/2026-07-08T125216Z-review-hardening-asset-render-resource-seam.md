---
type: "Memory Event"
title: "Review hardening: asset render resource seam"
description: "Read-only review findings were resolved for asset identity, source-kind preflight, image prepare cleanup, invalid atlas tiles, and wgpu texture module boundaries."
timestamp: 2026-07-08T12:52:16Z
event_kind: "Verification"
---

# Event

The asset/render resource seam was hardened after read-only review.

Resolved findings:

- `AssetServer` now rejects rebinding one runtime `AssetId` to multiple paths or stable IDs.
- `AssetRef` database resolution can validate the expected `AssetSourceKind`.
- Sprite texture refs must resolve to `image` assets, and tilemap tilesets must resolve to `tileset` assets.
- Path refs are validated when a `ProjectAssetDatabase` is present, even when scene/prefab preflight is running without mutating a target `AssetServer`.
- Image prepare removes stale prepared image resources when the source image is removed, missing, or marked removed.
- Tilemap extraction skips out-of-range atlas tiles instead of rendering them as untextured fallback quads.
- wgpu sprite texture upload/cache/sampler logic moved from `sprite.rs` into backend-private `texture.rs`.
- Import dependency digest tests now guard dependency source-hash changes.

# Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo check --examples`
- `cargo run -q --features serde --example scene_prefab_roundtrip`
- `cargo run -q --example asset_import_texture`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo nextest run --workspace` passed with 136 tests.
- `cargo tree -p nara --no-default-features | Select-String -Pattern 'wgpu|winit'` returned no matches.
- `rg -n "winit::|winit =" crates src Cargo.toml` found only manifest/facade feature lines and `nara_winit`.
- `rg -n "wgpu::|wgpu =" crates src Cargo.toml` found only manifest/facade feature lines and `nara_render_wgpu`.
- Engineering memory validation passed.

# Residual Follow-Ups

- Before adding many more importers, revisit whether `ImporterRegistry` should return typed importer payloads instead of artifact records only.
- Split `nara_scene` before adding patch transactions, migration chains, or nested prefab resolution.
- Extract shared asset-ref codec helpers when the next asset-bearing component arrives.
- Add CI scripts for dependency-boundary searches once CI exists.
- Add symlink/canonical containment tests when `.nara/import-cache/` becomes real file IO instead of artifact path records.

# Citations

- [Plan](../../../plans/2026-07-08-004-feat-asset-render-resource-seam-plan.md)
- [ADR 0033](../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Review hardening verification](../../verification/2026-07-08T125216Z-asset-render-resource-seam-review-hardening.md)
