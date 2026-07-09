---
type: "Verification Evidence"
title: "Server-ready runtime authority verification"
description: "Verification Evidence for Server-ready runtime authority verification."
timestamp: 2026-07-09T15:19:10Z
tags: ["nara", "server", "verification", "ce-work"]
related_plan: "docs\\plans\\2026-07-09-007-feat-server-ready-runtime-authority-plan.md"
---

# Verification

Focused and final gates observed during implementation:

- `cargo nextest run -p nara_project -p nara_input -p nara_gameplay -p nara_diagnostic -p nara_tasks`
- `cargo nextest run -p nara_input -p nara_gameplay --features nara_input/serde,nara_gameplay/serde`
- `cargo nextest run -p nara_diagnostic -p nara_input -p nara_gameplay --features nara_diagnostic/serde,nara_input/serde,nara_gameplay/serde`
- `cargo check -p nara_project`
- `cargo nextest run -p nara_project`
- `cargo fmt --all`
- `cargo check --workspace`
- `cargo nextest run --workspace`
- `cargo check --workspace --features serde`
- `cargo check -p nara --features asset-watch`
- `cargo check -p nara --features egui`
- `cargo check -p nara --example headless_server`
- `cargo check -p nara --features winit,wgpu --example windowed_clear`
- `cargo check -p nara --features winit,wgpu --example windowed_sprites`
- `cargo check -p nara --features winit,wgpu --example runtime_ui_panel`
- `git diff --check`
- `wiki_memory.py validate --root docs\knowledge\engineering`
- Boundary checks for `winit`, `wgpu`, `egui`, `notify`, and runtime `Entity`.

# Result

Focused unit and crate gates passed. The durable identity boundary check returned no matches for
`crates/nara_gameplay` or `crates/nara_project`, confirming public manifest/command data did not
introduce runtime entity handles.

Final workspace-wide gates passed after review hardening and `nara_project` module splitting.

# Evidence

- Runtime diagnostics now use a bounded shared bus with an O(1) dedupe index; focused
  `nara_diagnostic` serde check and 9 tests passed.
- Project manifest/profile tests: 12 passed after splitting `nara_project` into focused modules.
- Input/action and gameplay command focused serde run `61dd58bc-a0e2-4cc9-859c-a4b62ff95d60`
  covered 29 tests across diagnostics, input, and gameplay.
- Gameplay command serde rejects invalid command IDs, persistent IDs, named targets, source names,
  and payload keys; `ActionCommandBinding.context` defaults to `gameplay` for serialized maps.
- `ActionMap` and `ActionCommandMap` now rebuild internal lookup indexes after serde load and avoid
  full scans on frame hot paths.
- Root facade tests cover `HeadlessRuntimePlugins`, `ServerPlugins`, `add_project_plugin_plan`, and
  server execution without raw input resources.
- Final `cargo fmt --all`, `cargo check --workspace`, and `cargo nextest run --workspace` passed.
  Nextest run `0f2647fc-0023-4cfe-b347-f3d9a51e4739` ran 335 tests and all passed.
- Final `cargo check --workspace --features serde`, `cargo check -p nara --features asset-watch`,
  `cargo check -p nara --features egui`, `cargo check -p nara --example headless_server`, and all
  required winit/wgpu examples passed.
- `git diff --check` passed with only Windows line-ending warnings.
- Engineering memory validation passed with one pre-existing style warning: `current-state.md` is a
  large rollup, so future facts should continue going into sharded concepts.
- Final dependency boundary searches kept backend/tooling/watch imports inside root feature
  declarations and adapter crates; the durable command/manifest crates still had no `Entity`
  matches.

# Follow-up

- Remaining architectural follow-ups are outside this plan: richer input routing/UI consumed flags,
  full server metrics snapshots, plugin profile suitability metadata, and the broader persistent
  runtime identity model for save/replay/future replication.
- Future scale work: keep the new action/command indexes covered when adding mutable/removable
  bindings or richer input contexts.

# Citations

- Plan: [server-ready runtime authority plan](../../../plans/2026-07-09-007-feat-server-ready-runtime-authority-plan.md)
- Progress shard: [server-ready runtime authority implementation](../progress/2026-07-09T151910Z-server-ready-runtime-authority-implementation.md)
