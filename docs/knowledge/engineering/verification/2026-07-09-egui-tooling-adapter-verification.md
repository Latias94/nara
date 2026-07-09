---
type: Verification Evidence
title: egui tooling adapter verification
tags: nara,tooling,egui,verification
timestamp: 2026-07-09
status: passed
---

# Verified Commands

- `cargo check -p nara_tooling_egui`
- `cargo fmt --all`
- `cargo nextest run -p nara_tooling_egui`
- `cargo check -p nara --features egui`
- `cargo check --workspace`
- `cargo nextest run --workspace`
- `rg -n "egui::|egui =" crates src Cargo.toml`
- `rg -n "winit::|wgpu::|winit =|wgpu =" crates/nara_tooling_egui crates/nara_tooling`

# Result

- New egui adapter tests passed as part of the full workspace nextest run.
- Full workspace nextest passed with 189 tests.
- The egui dependency is limited to the optional facade feature and
  `nara_tooling_egui`.
- No `winit` or `wgpu` imports were found in `nara_tooling` or
  `nara_tooling_egui`.

# Citations

- [Progress](../progress/2026-07-09-egui-tooling-adapter.md)
