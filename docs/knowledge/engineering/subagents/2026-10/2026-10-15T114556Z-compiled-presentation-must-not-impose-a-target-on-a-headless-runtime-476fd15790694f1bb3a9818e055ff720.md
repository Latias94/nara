---
type: "Subagent Finding"
title: "Compiled presentation must not impose a target on a headless runtime"
description: "Subagent Finding for Compiled presentation must not impose a target on a headless runtime."
timestamp: 2026-10-15T11:45:56Z
record_id: "476fd15790694f1bb3a9818e055ff720"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
---

# Finding

Compiled renderer availability was being confused with a selected runtime's active presentation.
Under all features, a headless Editor Play Step entered Stepping forever while waiting for a Game
View candidate that could never exist. The actual active target must decide this obligation.

# Evidence

The main agent observed editor_host_owns_prepare_start_pause_step_stop_and_fresh_restart fail at
tests/workspace_play_runtime.rs:729: expected Paused, observed Stepping. The selected project is
headless. drive_active in trusted_play.rs set pending_step_presentation under a compile-time cfg,
regardless of its game: Option<ActiveEditorGameView>. The native presentation test separately
requires Stepping until the exact candidate is accepted and must continue to do so.

The same broad run also found two missing-schema-bundle fixture declarations and obsolete tests
that parsed enum declarations from fixed source files or prohibited a renderer-neutral render crate
from headless dependencies. These are test assumptions, not reasons to weaken native agreement or
presentation completion. Run: 0286c2cd-d9e5-461a-a09e-22765c72aae2, 197/202 passed before corrections.

# Recommendation

Wait for presentation only when the active runtime owns a Game View. Keep the native exact-frame
completion protocol unchanged. Require headless dependency consumers to exclude WGPU/Winit/egui
and presentation adapters, while permitting renderer-neutral data. Compile and run the renamed-root
schedule fixture instead of deriving Rust API authority by source spelling and file layout.

# Disposition

The main agent corrected the runtime presence check and fixture declarations. The renamed-root
runtime/schedule consumer recheck passes seven tests, run a9603b41-cd07-482d-b125-1ea956942018.
The complete workspace Play recheck is pending. This record is a directly observed integration
finding, not an additional independent reviewer receipt.

# Citations

- src/project_host/runtime/editor/trusted_play.rs: drive_active
- tests/workspace_play_runtime.rs
- tests/runtime_runner_contract.rs
- tests/schedule_extension_contract.rs
- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5 / U13
