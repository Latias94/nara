---
type: "Verification Evidence"
title: "Render UI Apply Foundation Final Verification"
description: "Final verification record for the Apply Changes, material-aware 2D, runtime UI, and pass-plan foundation slice."
tags: ["apply-changes", "material", "runtime-ui", "render", "verification", "ce-work"]
timestamp: 2026-07-09T17:48:07+08:00
status: "passed"
related_plan: "docs/plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md"
---

# Verified Scope

Final U10 verification passed after U9 documentation alignment and after updating root integration
tests to the material-first sprite schema and patch-producing Apply Changes contract.

# Commands

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run --workspace`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo check -p nara --features winit,wgpu --example runtime_ui_panel`
- `cargo run -q --example asset_import_texture`
- `cargo check -p nara --features asset-watch`
- `rg -n "wgpu::|wgpu =" crates src Cargo.toml`
- `rg -n "winit::|winit =" crates src Cargo.toml`
- `rg -n "egui::|egui =" crates src Cargo.toml`
- `rg -n "notify::|notify =" crates src Cargo.toml`
- Runtime identity leak search for serialized `AssetId`, `Entity`, task handles, timers, `wgpu`,
  and `winit` names in scene/prefab/patch-facing source, tests, and examples.
- Stale-contract search for old image sampler, resource-only batch, old input alias, and old
  diagnostics-only Apply Changes language.
- `python <engineering-wiki-memory>/scripts/wiki_memory.py validate --root docs/knowledge/engineering`
- `git diff --check`

# Results

- Full workspace tests: 267 passed.
- Boundary searches only found expected adapter/facade optional-feature locations.
- Runtime identity leak search only found negative serialization assertions.
- Stale-contract search returned no matches.
- Engineering memory validation passed with a non-blocking warning that `current-state.md` remains a
  large historical rollup.

# Citations

- [Plan](../../../plans/2026-07-09-004-feat-render-ui-apply-foundation-plan.md)
- [Runtime UI and pass plan M3 verification](2026-07-09-runtime-ui-pass-plan-m3.md)
