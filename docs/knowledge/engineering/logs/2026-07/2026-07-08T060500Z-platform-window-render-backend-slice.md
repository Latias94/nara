---
type: "Implementation Slice"
title: "Platform window render backend foundation implemented"
description: "Records the U2-U7 implementation state for runner, window, winit, render-domain, and wgpu backend boundaries."
tags: ["engineering-memory", "implementation", "platform", "render"]
timestamp: 2026-07-08T06:05:00Z
status: "implemented"
---

# Event

Implemented the platform/window/render backend foundation slice from the July 8 plan.
The slice adds a fallible app runner contract, deterministic fixed-update stage, backend-independent window data, `nara_winit`, graph-ready render-domain extraction data, and `nara_render_wgpu` as the only `wgpu` backend crate.

# Impact

Default `nara` remains backend-free.
Desktop windowing and GPU rendering are opt-in through root features `winit` and `wgpu`.
Gameplay-facing crates should continue using `Window`, `Camera2d`, `RenderTarget`, and `WgpuRenderPlugin` through nara-owned APIs rather than importing raw backend crates.

# Verification Snapshot

- `cargo test -p nara_winit`
- `cargo test -p nara_render`
- `cargo test -p nara_render --features serde`
- `cargo test -p nara_render_wgpu`
- `cargo check --examples`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo nextest run --workspace`
- `cargo run -q`
- `cargo run -q --example hello_world`
- `cargo check --workspace`
- `cargo check -p nara --features wgpu`
- `cargo tree -p nara --no-default-features | Select-String -Pattern 'winit|wgpu'`
- `rg -n "winit::|winit =" crates src Cargo.toml`
- `rg -n "wgpu::|wgpu =" crates src Cargo.toml`

# Citations

- [Platform/window/render backend foundation plan](../../../plans/2026-07-08-001-platform-window-render-backend-foundation-plan.md)
- [ADR 0032](../../../architecture/adr/0032-render-backend-integration-boundary.md)
