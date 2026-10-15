---
type: "Verification Evidence"
title: "Foundation U5 independent consumers and native examples verified"
description: "Execution consumers and static checks pass in the integrated worktree; overlapping prior work stays uncommitted."
timestamp: 2026-10-15T12:03:02Z
record_id: "dd39a41fb70c462bacd772d8fd26b752"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5535a0d"
supersedes: "7e531ba1a4644ad9a36ccf7876fd2e61"
---

# Verification

U5 consumer closure after the earlier core verification. Cargo ran serially with one build job,
the existing target directory, and locked/offline dependencies. This verifies the integrated
worktree; pre-existing staged SPL work and overlapping implementation deltas remain uncommitted.

# Result

- The initial root/app integration selection ran 202 tests: 197 passed and five exposed obsolete
  source guards or consumer defects (0286c2cd-d9e5-461a-a09e-22765c72aae2). The corrected independent
  runtime/schedule consumers passed all seven tests (a9603b41-cd07-482d-b125-1ea956942018).
- All 17 workspace Play integration tests passed after correcting the desktop schema fixture and
  making native presentation conditional on an actual view (d80e5096-54d6-424c-8b44-0573749024d1).
- The independent Reference Game asset task integration passed
  (93164de4-8f26-4101-bccf-c43ff750f70f). Its initial import establishes a real recipe before reload.
- The hello_world and headless_server examples exited successfully. After explicit hierarchy
  placement was added, asset_import_texture, sprite_tilemap_scene, ui_presentation_gallery,
  interaction_lab, and the native window_surface_retirement_smoke exited successfully. The
  windowed_clear example test target also passed; its output wrapper subsequently encountered a
  Unicode encoding error, which was not a failing Cargo command.
- Affected-package Clippy passed for nara_app, nara_tasks, and nara, with all features and targets,
  denying clippy::correctness and clippy::suspicious. Existing non-denied warnings remain. Its first
  failure identified an explicit drop of an ECS borrow; a lexical scope now ends that borrow.
- cargo fmt --all --check passed.

# Evidence

The registered-driver, startup-unwind, terminal-shutdown, compile/privacy, and incremental-close
regressions and earlier module selections are recorded in the predecessor. Two source-scanning
tests no longer approximate Rust syntax: actual compiler consumers prove privacy and schedule
extension behavior. Raw execution methods and the separate App shutdown path are removed.

The examples revealed that raw spawning of renderable entities omitted required structural-order
placement. They now use the existing hierarchy writer; no implicit lifecycle hook or second
structural owner was added. Code-first creation ergonomics remains a concrete U13 observation.

The complete plan is not achieved: U6-U13 and its full review remain outstanding. No new native
macOS evidence or broad native Linux runtime claim follows from these Windows consumers.

# Follow-up

Commit only owned paths, preserve pre-existing index/worktree changes, and proceed to U6. Its
compiled-project Check/Export must reuse one game definition and retain publication authority
inside the project process. U12 carries the recorded close-retry and background preparation gaps.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5/U6/U12/U13
- docs/knowledge/engineering/verification/2026-10/2026-10-15T114139Z-foundation-u5-core-ownership-privacy-and-incremental-close-verification-7e531ba1a4644ad9a36ccf7876fd2e61.md
- crates/nara_app/src/runtime.rs
- tests/runtime_driver_boundary.rs
- tests/schedule_extension_contract.rs
- reference-game/tests/asset_task_flow.rs
