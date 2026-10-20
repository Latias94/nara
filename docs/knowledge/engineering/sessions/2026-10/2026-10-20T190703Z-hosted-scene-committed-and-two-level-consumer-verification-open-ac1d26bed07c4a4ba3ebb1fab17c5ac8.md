---
type: "Session Handoff"
title: "Hosted Scene committed and two-level consumer verification open"
description: "Session Handoff for Hosted Scene committed and two-level consumer verification open."
timestamp: 2026-10-20T19:07:03Z
record_id: "ac1d26bed07c4a4ba3ebb1fab17c5ac8"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-persistent-two-level-game-plan.md"
git_branch: "main"
git_commit: "104f14d"
---

# Summary

Local main is `104f14d`, ahead of origin/main `3a9284a` by two commits. `cf1f354` owns the committed
Scene source/receipt pair; `104f14d` adds original-Host background preparation and preflighted CPU
Image/TileSet/catalogue residency. Preserve unrelated untracked `vendor/`. The current plan remains
open; do not create another Goal from ordinary continuation.

# Verified State

Committed P2 evidence `d8d63279f87c4b34bc83eeeccdc7550e` records 328 domain, 304 root, 50 public,
one actual compiled Editor travel and 47 selected Reference Game tests passing, plus the final
workspace all-feature/all-target check and formatting. Actual Editor A -> B -> unchanged A keeps
the preview A, stays within two generations, retains one after Stop and releases all after close
and session drop. This is owner-drive evidence, not native presentation qualification.

The independent untracked `samples/two-level-game/` is authorized coordinated work, not unrelated
user edits. Root formatted it and updated its Cargo.lock offline. Full sample all-feature/all-target
check passes after fixing component/field aliases and an invalid Transform2d provider assumption.
Root made ActiveLevel private to remove its new private-interface warning. Desktop-feature binaries
build (3m35s, `target/two-level-consumer-build.log`). Actual product tool protocol 2 generates its
schema bundle, schema-check passes, and strict Check reports eight content members.

The initial actual `--verify-travel` fails at 200,000 ticks after only printing startup:
`target/two-level-first-travel.log`. No successful sample travel, fresh-process restore, export or
native acceptance is claimed. The earlier pure DTO cohort passed 9/9, separately from this full game.

# Open Threads

The consumer lane identified the headless stall in source: HeadlessRun drives StepFixedTick;
run_exact_fixed_tick runs FixedUpdate and FrameCleanup, without PreUpdate or TaskUpdate. The
sample's local-command verification producer and the new hosted request poll therefore do not run
on that path. Resolve the owning headless maintenance/command contract; do not call a private poll
from the sample or silently change exact-step semantics without tests. This is the next concrete
integration correction, not a reason to add a new executor or make all traits async.

Normal Retry was corrected to use retained CurrentScene source and the full expected receipt;
E/Q still prepare new content. Full-World marker observation now checks actual entities against
exact tokens, IDs, live identities and positions. Independent Save I/O review found no data-loss
blocker; Linux exact temporary-file cleanup remains explicitly unsupported by Accepted ADR 0070,
and cannot be replaced by unchecked pathname deletion or called complete cross-platform recovery.

At capture, the sample lane is closing two caller issues: ignore new movement presses while
Preparing but always consume releases; cancel/ack a still-Ready request after early commit rejection
before discarding the game's pending state. It is adding finite script-stall diagnostics and child
deadlines. Read its final STOP message and inspect changes before formatting or building. These
latest changes remain unverified. A bounded postcommit Degraded-recovery injection is not qualified.

The user's latest request allows a full deprecation/dependency study after P1-P3. Preliminary
primary-source research confirms try_update is stable since the existing MSRV 1.95, while
fetch_update is deprecated since 1.99 and forwards to it. Current selected Cargo features include
one bevy_tasks 0.20 through ECS, without multi_threaded. Dependencies alone do not establish actual
thread counts, CPU, linked size or duplicate execution. The research note keeps reuse/feature
alternatives and their measurement gates separate from a decision to rewrite.

# Next Action

Root alone runs serial Cargo/fmt/Git with Rust +1.95.0, CARGO_BUILD_JOBS=1 and the existing root target,
including explicit CARGO_TARGET_DIR for independent workspaces. No root Cargo command is running.
After sample STOP, review the caller fixes, reconcile headless maintenance and semantic command
collection, then run focused externally bounded travel before the complete sample tests and
fresh-process/exported proof. Rebuild schema only if its provider changed. Keep native first
presentation separate from a complete interactive desktop control trajectory.

Remote CI run 39463001464 concerns 3a9284a: six jobs pass, including Linux Root and compile-only wasm;
Windows Root is still running at the last observation. A normal push cancels it under current
workflow concurrency. Check again at the next implementation boundary before pushing completed
commits. P3 remains unstaged/uncommitted; do not erase its files or claim it is complete.

# Citations

- [Plan](../../../../plans/2026-10-20-persistent-two-level-game-plan.md)
- [P2 qualification](../../verification/2026-10/2026-10-20T183137Z-hosted-scene-residency-and-owned-preparation-qualification-d8d63279f87c4b34bc83eeeccdc7550e.md)
- [Deprecation and dependency basis](../../../../research/2026-10-21-task-dependency-and-deprecation-preliminary.md)
