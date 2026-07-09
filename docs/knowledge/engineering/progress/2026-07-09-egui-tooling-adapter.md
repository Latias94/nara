---
type: Work Progress
title: egui tooling adapter boundary
tags: nara,tooling,egui,editor
timestamp: 2026-07-09
status: verified
---

# Summary

The first concrete debug/editor UI adapter is being added as `nara_tooling_egui`.
It consumes `nara_tooling` editor/inspector models and returns explicit editor
actions plus `SceneInspectorCommand` values.

# Boundary

- `nara_tooling` remains UI-toolkit agnostic.
- `nara_tooling_egui` is the only crate that should import `egui` for the early
  debug/editor UI slice.
- The adapter does not own scene mutation, ECS storage, windowing, or GPU
  resources.

# Verification

- `cargo fmt --all`
- `cargo nextest run -p nara_tooling_egui`
- `cargo check -p nara --features egui`
- `cargo check --workspace`
- dependency boundary searches for `egui`, `winit`, and `wgpu`

See [verification evidence](../verification/2026-07-09-egui-tooling-adapter-verification.md).

# Citations

- [Foundation architecture](../../../architecture/nara-foundation.md)
- [Editor open questions](../../../architecture/open-questions.md)
