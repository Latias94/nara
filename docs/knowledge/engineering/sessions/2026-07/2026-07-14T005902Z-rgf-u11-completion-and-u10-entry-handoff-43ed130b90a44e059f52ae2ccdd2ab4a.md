---
type: "Session Handoff"
title: "RGF-U11 completion and U10 entry handoff"
description: "Safe native surface retirement is committed and verified; RGF-U10 bounded PNG ingest is next."
timestamp: 2026-07-14T00:59:02Z
record_id: "43ed130b90a44e059f52ae2ccdd2ab4a"
tags: ["rgf-u11", "rgf-u10", "render", "winit", "wgpu", "handoff"]
producer_id: "codex-root"
run_id: "019f5096-ee46-7571-a208-be491cc72786"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4821ce8"
verified_by: "codex-root"
---

# Summary

RGF-U11 is complete at `4821ce8` (`fix(render): enforce safe surface retirement`). The supported
native path now uses wgpu safe owning surface creation, generation-bound surface bindings, explicit
surface/provider/native retirement, and a sticky external-destruction failure path. The active plan
continues with RGF-U10 before U12/U4; no RenderGraph, browser WebGPU, device-epoch recovery, or
multi-target frame contract was admitted.

# Verified State

- `BackendWindowHandles` is the shared per-runtime target authority. Each acquisition produces one
  non-cloneable handle owner and one generation-bound lease. Duplicate acquisition and stale lease
  commands fail without changing the current binding.
- `WgpuSurfaceState` owns the safe wgpu surface and lease as one live state. Explicit retirement and
  `Drop` destroy the owner before lease confirmation. Surface loss retains the provider; device loss
  clears the backend generation and makes that backend instance terminally `Unavailable`.
- Winit retires only target IDs it registered. Normal exit, runner failure, external `Destroyed`,
  native teardown failure, and plugin cleanup preserve their distinct outcomes.
- Unknown/unregistered render targets skip with `NoRenderableTarget`; lifecycle invariant failures
  remain fatal. Surface-create failure releases its unpublished binding and permits retry.
- Final gates: focused lifecycle crates 87/87; root public retirement 11/11; workspace 643/643 with
  three existing skips; architecture docs 5/5; smoke example tests 2/2; workspace check, affected-
  crate strict Clippy, three minimum-feature desktop examples, formatting, diff checks, and both
  real Windows smoke modes passed.
- Verification details live in `docs/knowledge/engineering/verification/2026-07/2026-07-13T215323Z-rgf-u11-safe-surface-retirement-verification-7873dc371c574fa3a0330be2b39ca589.md`.

# Open Threads

- Linux/macOS native-destruction timing and a real unsolicited OS destroy callback are unclaimed.
  Browser placement, device epochs, bounded recovery, and multi-target coordination retain their ADR
  admission triggers.
- An exploratory all-workspace/all-features strict Clippy run is blocked by pre-existing
  `nara_asset` lints and an unchanged all-features UI test construction error; the required focused
  Clippy and full workspace build/test gates pass.
- The working tree still contains product/strategy, plan, engineering-memory, and lockfile changes
  outside the U11 commit. Preserve them. Do not restore, reset, stash, clean, or bulk-stage them.

# Next Action

Enter RGF-U10. Read only the U10 unit and its cited R2-R3/R25/AE12/KTD boundaries, then trace the
current initial-import and reload paths from host-issued input through `nara_image`. Replace ambient
unbounded reads and generic PNG expansion with one bounded, reservation-backed path that proves
encoded, dimension, pixel, decoded, working, aggregate, cancellation, reload, and last-good
publication behavior. U10 must not broaden into arbitrary codec support or U4 plugin configuration.

# Citations

- `4821ce8`.
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`, RGF-U10/U11 and
  Sequencing and Evidence Gates.
- `crates/nara_window/src/backend.rs`.
- `crates/nara_winit/src/lib.rs`.
- `crates/nara_render_wgpu/src/{backend,error,lib,surface}.rs`.
- ADRs 0010, 0032, 0040, 0042, and 0078.
