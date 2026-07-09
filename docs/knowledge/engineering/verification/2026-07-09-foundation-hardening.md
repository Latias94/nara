---
type: Verification Evidence
title: foundation hardening verification
tags: nara,foundation,verification
timestamp: 2026-07-09T12:37:27+08:00
status: passed
---

# Focused Verification Already Passed

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run -p nara_reflect`
- `cargo nextest run -p nara_reflect -p nara_scene -p nara_sprite -p nara_tilemap -p nara_transform -p nara_render`
- `cargo nextest run -p nara_app -p nara_render_wgpu -p nara_winit -p nara_sprite_render -p nara`
- `cargo nextest run -p nara_diagnostic -p nara_render -p nara_render_wgpu -p nara`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- backend boundary searches for `winit`, `wgpu`, and `egui`
- serialization leak searches for runtime `Entity`, `AssetId`, `AssetEvent`, and
  `Handle<T>` derives
- `git diff --check`

# Final Verification Passed

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run --workspace` with 206 tests passed
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo run -q --features serde --example scene_prefab_roundtrip`
- `cargo run -q --features serde --example scene_patch_roundtrip`
- `cargo run -q --features serde --example prefab_patch_override`
- `cargo run -q --features serde --example component_schema_export`
- `rg -n "winit::|winit =" crates src Cargo.toml`
- `rg -n "wgpu::|wgpu =" crates src Cargo.toml`
- `rg -n "egui::|egui =" crates src Cargo.toml`
- `rg -n "AssetId.*Serialize|Deserialize.*AssetId|AssetEvent.*Serialize|Deserialize.*AssetEvent|Entity.*Serialize|Deserialize.*Entity|Handle<.*Serialize|Deserialize.*Handle" crates examples tests`
- `git diff --check`
- engineering wiki memory validation for `docs\knowledge\engineering`

# Result

- Full workspace validation passed.
- Backend feature examples compile.
- Serde scene/prefab/patch/schema examples execute.
- Boundary searches keep `winit`, `wgpu`, and `egui` inside their intended adapter crates and optional facade exports.
- Runtime identity leak search only matched stable persistent IDs with hand-written serde, not runtime ECS entities, runtime asset IDs, asset events, or typed handles.

# Citations

- [Progress](../progress/2026-07-09-foundation-hardening.md)
