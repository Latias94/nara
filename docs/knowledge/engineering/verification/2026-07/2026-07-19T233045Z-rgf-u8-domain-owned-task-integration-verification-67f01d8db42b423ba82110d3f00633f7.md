---
type: "Verification Evidence"
title: "RGF-U8 domain-owned task integration verification"
description: "Commit 60292e7 moves asset task phases to their domain, linearizes Poll entry cutoffs, and makes watcher loss bounded, sticky, and observable."
timestamp: 2026-07-19T23:30:45Z
record_id: "67f01d8db42b423ba82110d3f00633f7"
tags: ["rgf-u8", "verification", "tasks", "assets", "watcher", "observability"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "60292e7"
verified_by: "focused nextest,workspace nextest,reference-game nextest,workspace check,minimal-feature check,clippy,fmt,formal review"
---

# Verification

RGF-U8 was reviewed at implementation commit `60292e7` on
`refactor/engine-foundation-contracts`. The unit removes asset-domain scheduling vocabulary from
`nara_app` and `nara_tasks`, preserves the reference game's asynchronous last-good behavior, and
replaces best-effort filesystem watcher delivery with bounded, non-blocking, observable admission.

# Result

RGF-U8 passed all implementation and review gates.

- `nara_app` owns only `CoreStage::TaskUpdate`; `nara_tasks::TaskPlugin` configures no asset or
  other business-domain sets. `nara_asset::AssetTaskUpdateSet` owns and orders Poll,
  ResolveSourceChanges, SpawnJobs, and ApplyResults.
- `TaskPools::capture_completion_cutoff` linearizes terminal publication per pool instance.
  `OrderedTaskResults::capture_ready_prefix` consumes only the predecessor-complete prefix
  published by that cutoff, rejects a foreign pool without mutation, and moves captured handles
  into an owned snapshot.
- The image poller captures one cutoff for every asset stream at entry, validates all streams
  before moving any handle, fully decodes captured snapshots before mutating attempts, and retains
  same-frame apply, next-frame completion, stale retirement, and last-good semantics.
- `AssetWatchEventQueue` bounds event count and retained bytes. Callback send is non-blocking and
  all-or-nothing per batch; Poll cannot contend on producer admission and drains only batches
  published at its entry cutoff.
- Overflow, concurrent-producer contention, disconnect, backend/translation failure, and queue
  unavailability remain observable after receiver loss through independent counters. They publish
  runtime diagnostics and pressure, enter sticky `RescanRequired`, stop the live watcher backend,
  and suppress/count subsequent batches.

# Evidence

- U8 package matrix: `cargo nextest run --locked -p nara_app -p nara_tasks -p nara_asset
  -p nara_asset_watch -p nara_image --test-threads=1` -> 234 passed.
- Root ownership/integration gate: `cargo nextest run --locked -p nara --test
  task_update_integration --test-threads=1` -> 2 passed.
- Focused independent-game gate: `cargo nextest run --manifest-path reference-game/Cargo.toml
  --locked --test asset_task_flow --test-threads=1` -> 1 passed.
- Full root workspace: `cargo nextest run --workspace --locked --test-threads=1` -> 917 passed,
  3 declared skips. The first invocation reached the 120-second command wrapper limit without a
  result; the identical command was rerun with a 600-second wrapper and completed in 206 seconds.
- Full independent reference game: `cargo nextest run --manifest-path reference-game/Cargo.toml
  --locked --test-threads=1` -> 50 passed.
- `cargo check --workspace --locked`, minimal `runtime-core`, and independent reference-game
  `--all-targets` checks passed.
- Strict Clippy passed across `nara`, App, task, asset, watcher, and image targets with only the
  repository's documented pre-existing allowances.
- Root/reference-game formatting and `git diff --check` passed. A scoped source search found no
  asset integration-set vocabulary in `nara_app` or `nara_tasks`.
- Formal correctness, specification, testing, maintainability, and simplification review produced
  six actionable findings. All six were fixed and independently revalidated: Poll/callback lock
  contention, post-entry task completion, receiver-loss observability, translation discard
  accounting, snapshot/result-stream ownership, and contention-safe statistics.

# Follow-up

`RescanRequired` intentionally has no in-place reset. A future Host-authorized watcher workflow
must perform a complete source scan, reconcile the asset index, and construct a new watcher runtime;
RGF-U8 does not claim that recovery product loop. Host-authorized watcher composition remains a
separate gap in ADR 0079.

The pending U13 human Windows play check is unaffected and remains the gate for U14, U17, and U7.
RGF-U19 may continue independently because it changes governance validation rather than gameplay
or desktop behavior.

# Migration

Downstream advanced integrations replace `nara_app::TaskUpdateSet` with
`nara_asset::AssetTaskUpdateSet`, replace `OrderedTaskResults::drain_ready_prefix` with a
same-pool cutoff plus owned snapshot, and send watcher events through
`AssetWatchEventQueue::sender`. No compatibility alias or unbounded watcher path remains. The full
before/after contract is recorded as RGF-U8-1 in the July foundation migration guide.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u8-restore-domain-owned-task-integration-sets`
- `docs/architecture/adr/0080-domain-owned-task-update-integration-sets.md`
- `crates/nara_asset/src/reload.rs#AssetTaskUpdateSet`
- `crates/nara_tasks/src/runtime.rs#TaskPools::capture_completion_cutoff`
- `crates/nara_tasks/src/runtime.rs#OrderedTaskResults::capture_ready_prefix`
- `crates/nara_asset_watch/src/queue.rs#AssetWatchEventQueue`
- `crates/nara_asset_watch/src/observability.rs#AssetWatchRuntimeStatus`
- `crates/nara_image/src/reload.rs#poll_image_reload_results`
- `tests/task_update_integration.rs`
- `reference-game/tests/asset_task_flow.rs`
- Commit `60292e7`
