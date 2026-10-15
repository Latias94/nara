---
type: "Work Progress"
title: "Foundation U4 file-effect ownership implemented and U5 execution migration started"
description: "Windows filesystem and Host cleanup regressions pass; CLI and native Linux verification continue while two isolated U5 source slices are implemented."
timestamp: 2026-10-15T10:14:50Z
record_id: "f66566f6e82242569ad63e218f56ad7c"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bc71449"
---

# Summary

U1/U2/U3 are committed as 7e3449a, 722ff27, and bc71449. U4 implementation is present
in the shared worktree; Windows filesystem and Host cleanup regressions pass. U5 is
actively migrating the existing execution owner, not creating a second runtime kernel.

# Details

U4 adds capability-instance preflight before candidate creation, a failure retaining only
cleanup authority, explicit cleanup retry, and retained Host/session ownership. Failed
cleanup blocks another allocation; session close cannot report Complete while a candidate
remains. Image metadata publication transfers a local failure owner into the long-lived
image Host. One-shot repair and schema-bundle CLIs now attempt cleanup at their real terminal
error handlers and report residual names/identities on failure.

Windows Nextest run 32181f04-8fc3-4c26-85c2-b689647b4414 passed 59 nara_fs library/contract
tests (3 platform/privilege skips), with CARGO_BUILD_JOBS=1 and the existing target. Host run
dbe4b6ba-d3e8-456c-8f18-9d034ee8dedd passed six focused publication, Scene-save cleanup,
image-metadata cleanup/close, and Prefab publication lifecycle tests. The earlier broader run
d7735e54-f5a9-41c9-a404-227a95864caf passed 27/30; two fixture defects have since been fixed
(save without a dirty edit; duplicate TOML authoring table), while an injected prepared-start
unwind aborted the process with 0xc0000409 and is assigned to U5. Do not label that broad run green.

The pre-existing SPL test used a removed runtime_availability getter; its assertion now uses
project_readiness().runtime(). No production compatibility getter was resurrected.

Two read-only reviewers found no remaining U4 ownership/preflight defect after fixing the
one-shot CLI omission. CLI failure-exit tests are written but not yet compiled: root Cargo
was paused when it encountered U5's expected in-progress field migration. Native Linux WSL2
is available (Ubuntu-26.04, Rust 1.97.1, nextest 0.9.115), but the first offline workspace
metadata attempt lacked parley in that distribution's Cargo cache. Linux behavior is unverified.

U5 implementation ownership: /root/u5_execution_owner owns crates/nara_app/src/**; App holds
the existing RuntimeOwner and execution state moves directly into RuntimeOwnerState. It is
adding RuntimeInstance::start(App) with failure retirement retained. /root/u5_editor_runtime
owns authoring_runtime.rs, preview.rs, and TaskPoolsCloseParticipant/report tests in nara_tasks.
It removes manual TaskPools creation and the Editor's second shutdown path, and uses private
typed image operations through the existing driver-port seam. Neither worker runs Cargo/index.

All original staged/worktree edits remain. U4 overlaps pre-existing nara_fs/tooling/Editor SPL
edits, so those files remain uncommitted rather than staging unrelated work wholesale. The
original index/patch/hash backup is in the task's local temporary execution directory; U5 files
were clean before its workers began. Cargo remains serial with CARGO_BUILD_JOBS=1.

# Next Action

Compile U5 nara_app, observe the pending TaskShutdownReport regression red before its participant
implementation, then verify both U5 slices and remaining U4 CLI/native Linux trajectories.
Continue the U1-U13 goal; no U4 completion, U5 completion, final whole-plan review, or Linux/macOS
support evidence is claimed by this progress record.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T093553Z-editor-task-pool-must-transfer-into-the-single-runtime-close-owner-641be150d42f4ec2b9f408d5cd063beb.md
