---
type: Verification Evidence
title: P0 Cross-Cutting Engine Contracts Verification
timestamp: 2026-07-09T18:41:08+08:00
tags:
  - verification
  - architecture
  - asset
---

# Result

Passed local verification for the P0 cross-cutting contract pass.

# Commands

- `cargo fmt --all`
- `cargo nextest run -p nara_asset`
- `cargo check -p nara_asset --features serde`
- `cargo check --workspace`
- `cargo nextest run --workspace`
- `cargo check --workspace --features serde`
- `cargo check -p nara --features asset-watch`
- `cargo run -q --example asset_import_texture`
- `rg -n "let _ = resolver\.resolve_change|discard.*SourceChangeResolver|SourceChangeResolver.*errors" crates docs AGENTS.md`
- `rg -n "wgpu::|wgpu =" crates src Cargo.toml`
- `rg -n "winit::|winit =" crates src Cargo.toml`
- `rg -n "egui::|egui =" crates src Cargo.toml`
- `rg -n "notify::|notify =" crates src Cargo.toml`
- `git diff --check`

# Notes

- `cargo nextest run --workspace` ran 268 tests and all passed.
- Boundary searches showed `wgpu`, `winit`, `egui`, and `notify` remain in their expected optional
  facade metadata or adapter crates.
- The stale swallowed-error search now only finds the new AGENTS rule that forbids discarding
  `SourceChangeResolver` errors.
