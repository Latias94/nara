---
type: "Research Findings"
title: "Root capability, task ownership, and manifest IO audit"
description: "Fresh audit of root compilation closure, plugin composition, TaskUpdate ownership, placeholder audio, and capability-bound project manifest ingest."
timestamp: 2026-07-11T11:41:31Z
record_id: "d3b79814f13b4bc3980973c209bf1e72"
resource: "nara engine foundation"
tags: ["architecture", "cargo", "plugins", "tasks", "filesystem"]
producer_id: "codex-root"
run_id: "goal-019f5096"
source_session: "019f4f36-42c9-7043-92b5-661311b14e21"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "94530a1"
---

# Summary

The root facade's Cargo features, project plugin-plan enum, and installed plugin groups do not yet
describe one product capability closure. Task integration also exposes asset-domain phase names
from `nara_app` and configures them from `nara_tasks`, while `nara_project` still opens the project
manifest through ambient filesystem authority. ADR numbers 0077 and 0078 are occupied by a
concurrent render-document slice, so the corrective decisions must use ADR 0079 and ADR 0080.

# Details

## Root compilation and composition

- Fresh `cargo tree -p nara --no-default-features --depth 1` at `94530a1` reports 24 direct engine
  domain dependencies even though `default = []`; the tree includes image, render, sprite,
  tilemap, runtime UI, tooling, window, and the unused audio placeholder.
- The committed `nara_identity` core exists as a workspace member and workspace dependency, but the
  root package `[dependencies]`, facade, and scene/gameplay/reflect/tooling consumers have not yet
  been wired to that owner; the current root dependency tree therefore still excludes it.
- `Runtime2dPlugins` installs runtime UI, `DesktopWgpuPlugins` fixes 2D/UI/window/wgpu into one
  bundle, and `nara_render_wgpu` unconditionally depends on sprite and UI submitter crates.
- `ProjectPluginPlan` is a mutually exclusive preset enum. `apply_project_settings` inserts
  settings, time, diagnostics, and task resources before a missing compiled backend capability can
  fail, so capability rejection is not a pre-mutation operation.
- The required product model has three normalized layers:
  `required product capabilities of the resolved plugin plan <= normalized project request <=
  compiled Cargo product capabilities`. Plugin-internal service requirements/conflicts close
  separately because they are not all project-requestable product capabilities.
  `runtime-core` includes the `nara_input` compilation capability because local/headless runtime
  and `nara_gameplay` depend on it, while `ServerPlugins` still installs no raw-input resources.

## Placeholder domain

- `crates/nara_audio/src/lib.rs` contains only `AudioClip`, `AudioCommand`, and `AudioSink`.
  There is no plugin, state machine, service adapter, or production consumer.
- Active references are limited to workspace membership, root facade/prelude exports, the lockfile,
  and architecture direction. The implementation unit should delete the crate and live exports
  while retaining ADR 0030 as the admission contract for a future real audio vertical slice.

## Task integration ownership

- `nara_app::TaskUpdateSet` contains asset-specific phases and `nara_tasks::TaskPlugin` configures
  their ordering despite owning only execution mechanics.
- Asset, watcher, and image systems are the real consumers. `nara_app` should retain only
  `CoreStage::TaskUpdate`; `nara_asset::AssetTaskUpdateSet` should own and configure Poll,
  ResolveSourceChanges, SpawnJobs, and ApplyResults; `nara_tasks` should configure no business set.
- Each poller needs one immutable ready membership or queue-prefix snapshot captured at system
  entry. An eligible predecessor-unblocked outcome in that snapshot must apply in the same frame;
  readiness/arrival after the snapshot waits for the next frame. Stale/superseded outcomes retire,
  only eligible missing-predecessor work buffers, and an eligible synchronous rejection/removal
  produced during Spawn must apply later in that same frame.

## Project manifest authority

- `crates/nara_project/src/manifest.rs` owns `File::open` and bounded reading by ambient path.
  This contradicts the host-issued capability boundary already implemented in `nara_fs`.
- Host/composition code must read bounded manifest bytes through an issued capability and pass an
  immutable byte/string candidate to side-effect-free `nara_project` parsing and lowering.
- The same boundary search still finds `std::fs::read` in image reload and canonicalize-then-open
  authority in the asset database; U11/U12 remain responsible for those consumers.

# Next Action

Accept ADR 0079 for root product capabilities and placeholder retirement and ADR 0080 for
domain-owned TaskUpdate integration sets. Revise the active plan with U32/U33, the capability-bound
manifest ingest contract, exact dependency waves, and verification gates; then resume U8 consumer
migration before implementing the new units.

# Citations

- `Cargo.toml`
- `src/lib.rs`
- `crates/nara_audio/src/lib.rs`
- `crates/nara_app/src/lib.rs`
- `crates/nara_tasks/src/runtime.rs`
- `crates/nara_asset/src/reload.rs`
- `crates/nara_asset_watch/src/lib.rs`
- `crates/nara_image/src/lib.rs`
- `crates/nara_project/src/manifest.rs`
- `crates/nara_render_wgpu/Cargo.toml`
- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
