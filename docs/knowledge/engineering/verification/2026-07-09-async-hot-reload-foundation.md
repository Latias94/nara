---
type: Verification Evidence
title: async hot reload foundation verification
tags: nara,async,tasks,asset,hot-reload,verification
timestamp: 2026-07-09T14:31:38+08:00
status: passed
---

# Focused Verification Passed

- `cargo check -p nara_app -p nara_tasks -p nara`
- `cargo nextest run -p nara_app -p nara_tasks`
- `cargo check -p nara_asset -p nara_image -p nara`
- `cargo nextest run -p nara_asset -p nara_image`
- `cargo run -q --example asset_import_texture`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo nextest run -p nara_asset_watch`
- `cargo check -p nara --features asset-watch`
- `cargo nextest run -p nara_asset_watch -p nara_asset -p nara_image -p nara_sprite_render`

# Final Verification Passed

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run --workspace` with 235 tests passed
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo check -p nara --features asset-watch`
- `cargo run -q --example asset_import_texture`
- `rg -n "tokio::|tokio\s*=|async_std::|async-std\s*=" crates src Cargo.toml`
- `rg -n "notify::|notify-debouncer|notify\s*=" crates src Cargo.toml`
- `rg -n "wgpu::|wgpu\s*=" crates src Cargo.toml`
- `rg -n "winit::|winit\s*=" crates src Cargo.toml`
- `rg -n "egui::|egui\s*=" crates src Cargo.toml`
- `rg -n "AssetId.*Serialize|Deserialize.*AssetId|AssetEvent.*Serialize|Deserialize.*AssetEvent|Entity.*Serialize|Deserialize.*Entity|Handle<.*Serialize|Deserialize.*Handle|TaskHandle.*Serialize|Deserialize.*TaskHandle" crates examples tests`
- `cargo tree -p nara --no-default-features | rg -n "notify"`
- `python <engineering-wiki-memory>/scripts/wiki_memory.py validate --root docs\knowledge\engineering`
- `git diff --check`

# Result

- Full workspace validation passed.
- The task/update stage and all async hot reload tests pass.
- The code-first asset import/prepare example executes and asserts image prepare happens once.
- Optional watcher facade wiring compiles while the default root facade dependency tree has no
  `notify` entries.
- Boundary searches keep Tokio/async-std absent, `notify` isolated to `nara_asset_watch`, and
  `wgpu`/`winit`/`egui` inside their adapter crates plus optional facade metadata.
- Runtime identity serialization search only matched hand-written serde for stable persistent IDs:
  `StableAssetId` and `SceneEntityId`.
- Engineering memory validation passed with an existing large-rollup warning for
  `current-state.md`.

# Review Hardening Covered

- Expected-version guards prevent stale first-load success/failure results from mutating newer
  asset state.
- Same-frame source changes use last-event-wins coalescing for atomic-save sequences.
- Source dependency reload propagation walks transitive dependencies once.
- `ImagePlugin` composes `ImagePreparePlugin`, avoiding duplicate prepare system registration in
  `MinimalPlugins` + `SpriteRenderPlugin`.
- Watcher translation preserves in-root rename sides, maps `.meta` removal to source removal, rejects
  root mismatch with `AssetSourceRoot`, and proves queued watcher events resolve in the same
  `TaskUpdate` frame.

# Citations

- [Progress](../progress/2026-07-09-async-hot-reload-foundation.md)
- [Plan](../../../plans/2026-07-09-003-feat-async-hot-reload-foundation-plan.md)
