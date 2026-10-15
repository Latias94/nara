---
type: "Verification Evidence"
title: "Foundation U5 core ownership privacy and incremental close verification"
description: "Core and library migrations pass; independent compiled consumers and examples still gate U5 completion."
timestamp: 2026-10-15T11:41:39Z
record_id: "7e531ba1a4644ad9a36ccf7876fd2e61"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5535a0d"
---

# Verification

U5 consolidates configuration and managed execution into the existing owner, migrates real domain
consumers, and removes raw App execution. Verification runs serially with CARGO_BUILD_JOBS=1,
locked/offline dependencies, one Nextest test thread, and the existing target directory.

# Result

- nara_app/nara_tasks library and integration selection: 138 passed, run
  ce9d32df-7d3e-4e33-89ee-32be92fa82f5, before the final terminal-shutdown correction.
- Workspace all-features/all-targets cargo check passed after the registered-driver migration.
- Seventeen affected module/root libraries: 917 run, 913 passed and four input test migrations
  failed, run fe0fefcc-2a3d-45ea-9631-cf9faac3c78b. This run includes the corrected terminal finish
  failure and historical timeout retry regressions. The four failures mixed configuration-time
  ButtonInput writes with runtime InputTimeline events; all now start with real driver Press events.
- Complete root/gameplay/input library recheck: 261 passed, run
  74b2b851-0f5a-4a7b-a8b0-c53fad0fd611, including all four corrections and incremental Editor close.
- Actual compiler/privacy consumers: three tests passed, run
  70b8c56d-b9fc-45c2-87e6-9b56da253ec9. Rust rejects raw App execution, mutable Runtime World access,
  and escaping a short-lived driver scope. Winit's managed runner signature is compiled directly.
- Root/runtime/plugin/workspace/independent integration selection is still being completed. Its
  first run 0286c2cd-d9e5-461a-a09e-22765c72aae2 exposed obsolete source-parser and dependency guards;
  no passing whole-integration claim is made here.

# Evidence

Before production changes, validator and observer unwind regressions demonstrated lost Host close
responsibility (runs fe627d28-96fb-4b95-b7de-ba4b8db175a4 and
a56284a0-8687-41d1-a1d0-bd94a9dc5bc9). Host obligations now enter the canonical owner before callbacks.

A driver trait override ran an unconfigured World operation instead of rejecting a missing Resource
(e9ca2c82-fcaf-4d7a-8520-24850ebd4c56). The override is removed. Coordinated operations must be
configured before execution in a private typed slot, while ordinary ports stay resource-local.

Ordinary finish silently returned Ok with PluginShutdown failure evidence
(15411b74-e011-4d44-8347-efa36d264835). It now returns the same retirement owner in truthful Retired
state for this terminal failure. A completed retry still succeeds despite historical timeout evidence.

The Editor's first close advance completed an internal wait loop instead of yielding progress
(174732dd-a793-4ec1-aa99-0bd7fbecef70). It now requests/advances once and returns Incomplete until
the runtime is Stopped and its task report is complete. Controlled held-task tests retain timeout,
later release, physical join, and budget-accounting assertions without blocking the UI per call.

The original staged binary diff remains byte-identical to the prework inventory (SHA256
371623fe86914052384e5271ad25295e831bb66b44cdc0d211de238c7afaf88e). Overlapping SPL changes remain
uncommitted; this record verifies the integrated worktree, not a clean independently buildable HEAD.

# Follow-up

Complete independent runtime/schedule consumers, examples, and affected-package lint before U5's
precise owned-file commit and U6/U12 implementation. Replace source inference with compiled behavior
when it obstructs legitimate module moves; do not extend it into a Rust resolver. Full U1-U13 review
remains outstanding. No native macOS or new Linux runtime verification is claimed.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5 / U13
- crates/nara_app/src/runtime.rs
- src/project_host/runtime/editor/authoring_runtime.rs
- tests/runtime_instance.rs
- tests/runtime_driver_boundary.rs
- tests/schedule_extension_contract.rs
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T111906Z-runtime-driver-transactions-must-be-configured-before-execution-c8b70859ecb1443bb070b7c1bb4cb01b.md
