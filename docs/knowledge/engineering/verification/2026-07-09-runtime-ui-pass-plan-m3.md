---
type: "Verification Evidence"
title: "Runtime UI and Render Pass Plan M3 Verification"
description: "Verification record for runtime UI, UI render batching, pass planning, and wgpu pass consumption."
tags: ["runtime-ui", "render", "verification", "ce-work"]
timestamp: 2026-07-09T17:30:26+08:00
status: "passed"
related_plan: "docs/plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md"
git_commit: "af2be6f"
---

# Verified Scope

Verified the M3 implementation before the U9/U10 documentation pass:

- `nara_ui` layout, runtime-only schema, codec, interaction, overlap, and image-panel behavior.
- `nara_ui_render` colored panel, ready image panel, missing image fallback, clipping, and clip
  projection behavior.
- `nara_render` pass-plan ordering and dependency validation.
- `nara_render_wgpu` UI-after-world pass consumption and optional desktop examples.
- Root facade exports for `ui` and `ui_render`.

# Commands

- `cargo fmt --all`
- `cargo nextest run -p nara_input -p nara_ui -p nara_winit`
- `cargo check -p nara_ui --features serde`
- `cargo nextest run -p nara_ui_render`
- `cargo nextest run -p nara_ui_render -p nara_render_wgpu`
- `cargo nextest run -p nara_render -p nara_sprite_render -p nara_ui_render -p nara_render_wgpu`
- `cargo check -p nara --features winit,wgpu --example runtime_ui_panel`
- `cargo nextest run -p nara_ui -p nara_ui_render -p nara_render -p nara_render_wgpu`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo check -p nara --features winit,wgpu --example windowed_clear --example windowed_sprites --example runtime_ui_panel`
- Boundary search over `crates/nara_ui` and `crates/nara_ui_render` for `wgpu`, `winit`, egui, and
  dear-imgui imports.
- Legacy input alias search over Rust source and examples.
- `git diff --check`

# Result

Passed. Final U10 still needs the full plan Verification Contract after U9 documentation changes.

# Citations

- [Plan](../../../plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md)
- [Progress](../progress/2026-07-09-runtime-ui-pass-plan-m3.md)
