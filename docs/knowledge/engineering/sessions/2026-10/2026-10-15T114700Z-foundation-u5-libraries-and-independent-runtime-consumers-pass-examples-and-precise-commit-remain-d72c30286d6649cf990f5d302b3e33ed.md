---
type: "Session Handoff"
title: "Foundation U5 libraries and independent runtime consumers pass; examples and precise commit remain"
description: "Session Handoff for Foundation U5 libraries and independent runtime consumers pass; examples and precise commit remain."
timestamp: 2026-10-15T11:47:00Z
record_id: "d72c30286d6649cf990f5d302b3e33ed"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5535a0d"
---

# Summary

The active goal still covers U1-U13. U1-U3 are committed, U4 is verified in the integrated worktree,
and U5 is close to consumer closure. Do not stop at U5 or mark the goal complete. HEAD is 5535a0d
on refactor/engine-foundation-contracts. The target repository is nara, not the tool's default pokel.

# Verified State

All U2/U3/U4 facts in the preceding records remain valid. U4 has Windows and native WSL2 Linux
filesystem evidence and real CLI failure-exit cleanup. No U4/U5 implementation commit exists yet.

U5 App now configures one existing RuntimeOwner, whose State directly owns World/schedules/plugins,
fault and close responsibilities. No nested App, raw execution API, second App Drop close path,
unused PluginCapability/PluginServiceId, raw runner errors, or competing shutdown authority remains.
Ordinary RuntimeInstance::start/drive/finish has one finite cleanup attempt and retained failure owner.
Host close obligations move before all validator/observer callbacks. Red unwind regressions preceded
the fixes and pass. Staging bridges needed across workspace crates remain public but doc(hidden),
absent from the normal prelude, and honestly documented as first-party plumbing.

The newly added trait World override was rejected after its real red regression. It is removed.
App::__register_driver_port<R>(typed_fn) installs a private frozen RegisteredDriverPort<R> slot.
Runtime ports cannot select callbacks; unregistered ports only mutate their existing resource.
State/Resource gates and move-only input refusal remain. Consumers migrated from trait override to
private inherent operation plus App-time registration. Four regressions pass. Do not restore the
unsafe trait override or invent a TypeId permission framework / generic operation queue.

finish now returns Err(same retirement) for a terminal plugin shutdown failure even when physically
Retired; completed retry with historical DeadlineExceeded remains Ok. Both regressions pass in the
large module run. EditorAuthoringRuntime owns RuntimeInstance and TaskPlugin owns task closure.
shutdown_tasks now performs one request/drive and returns progress instead of polling/sleeping on
the UI. Complete requires Stopped plus a complete TaskShutdownReport. Controlled close regressions
preserve deadline, late release, joined workers, and image-budget assertions.

Verification:
- ce9d32df-7d3e-4e33-89ee-32be92fa82f5: 138 core library/integration tests passed before terminal finish fix.
- fe0fefcc-2a3d-45ea-9631-cf9faac3c78b: 917 tests in 17 libraries, 913 passed and four input fixture
  migrations failed. All failures now use the InputTimeline Press source consistently.
- 74b2b851-0f5a-4a7b-a8b0-c53fad0fd611: complete root/input/gameplay libraries, 261 passed after those
  four corrections and the nonblocking Editor close fix. Other 14 libraries passed in the prior run.
- 70b8c56d-b9fc-45c2-87e6-9b56da253ec9: actual compile/privacy tests passed (3); inspected stderr
  proves raw App execution, mutable Runtime World access, and scope escape are rejected.
- Full workspace all-features/all-targets cargo check passed after registered-port migration.
- 0286c2cd-d9e5-461a-a09e-22765c72aae2: 202 integrations, 197 passed. The five failures are corrected
  and rechecked below; no need to rerun every unrelated integration.
- a9603b41-cd07-482d-b125-1ea956942018: runtime/schedule independent consumers, 7 passed. Removed
  obsolete source-parsing authority checks. The headless tree permits nara_render data but still
  excludes WGPU, Winit, egui and presentation adapters. Schedule fixture runs real managed behavior
  under renamed root and reuses target.
- d80e5096-54d6-424c-8b44-0573749024d1: all 17 workspace Play integrations passed. Fixed an actual
  compiled-feature/runtime-presence bug: headless Step must not await native presentation merely
  because the engine binary includes presentation support; native Game View Step still awaits its
  exact presentation. Added the missing schema-bundle declaration in the desktop fixture.

runtime_driver_boundary shrank from a source parser to Cargo dependency direction, actual runner
signature compilation, a real short-lived scope operation, and three trybuild negative consumers.
schedule_extension_contract now compiles/runs the independent fixture instead of parsing Rust enum
spelling/file positions. rust-product-recipes guide documents ordinary start/drive/finish.

# Open Threads

The only active Cargo command at capture is exec session 40701. It serially runs six root examples
with all features, locked/offline, CARGO_BUILD_JOBS=1: hello_world (already exit 0), headless_server,
asset_import_texture, sprite_tilemap_scene, ui_presentation_gallery, window_surface_retirement_smoke.
Read it using write_stdin; do not launch another Cargo command until it finishes. It captures output
and prints an example's tail only on failure. The finite native surface smoke owns its child/deadline.

Then run the UI interaction_lab example, Reference Game asset_task_flow, focused retirement-helper
example tests, and appropriate affected-package Clippy. Reference asset_task_flow has been migrated
to registered operation and now seeds an accepted image recipe through real file admission/import/
commit before Modified; it was an outdated first-load assumption like the corrected root fixture.
It has scoped rustfmt but no actual test run yet. No new native Mac/Linux runtime claim is made.

Original user staged/worktree/untracked SPL work remains. Last exact staged binary diff comparison
was True, SHA256 371623fe86914052384e5271ad25295e831bb66b44cdc0d211de238c7afaf88e. Prework backup
is the TEMP directory nara-foundation-execution-1kxxdfu8 (index.patch, worktree.patch, status, hashes).
Never git add -A, bare commit, restore/reset/stash/drop. Preserve all concurrent edits. Many U4/U5
changes overlap pre-dirty Host/fs/tooling files; keep them uncommitted until exact own hunks can be
isolated. Any evidence currently describes the integrated worktree, not a clean HEAD build.

The U6 worker /root/u6_project_tooling is completed after read-only design and has not written code.
Use followup_task, not send_message, to wake it after U5's consumer/commit gate. Its ownership includes
product/check/native project_tooling, CLI check/export/args/thin main run_check/run_export, starter
headless/library, starter product_contract, exact Cargo feature and root lib module export hunks.
Do not reuse it for a different implementation unit. Prior worker handles are completed; native
spawn hit thread-limit for a fresh U12 worker and no close API is exposed. Run U12 inline if capacity
cannot be recovered. Its three concrete gaps are captured in df85ee87b18f42d88e257c480b5f83fb.

The loaded ce-simplify-code persona assets were read; thread capacity requires inline review.
Four redundant consumer Resource-presence checks were removed after the owner gained that gate;
no additional state/fence removal is justified. Final formal ce-code-review is still required at
whole-plan completion. Earlier focused independent reviews are not its final receipt. Its skill
and initial references have been read but the full formal workflow has not been executed.

# Next Action

Collect example session 40701, finish U5 checks, capture a final verification successor/ledger,
and precisely commit owned code. Keep the original staged diff intact. Complete U6 then U7/U8/U9,
U10/U11 and U12 per dependency table; U12 is allowed after U4/U5, and U13 closes real AE1-AE5 consumers.
Do not restart U5 from stale pending-unwind snapshots a723ad2e or older progress as if fixes were absent.

U6 must reuse SchemaContribution/ProductRecipe/RuntimePlan, not add GameDefinition. Parent CLI only
discovers/bootstrap-builds headless and passes bounded tool request/summary. Project headless tool
mode branches before gameplay, uses its own recipe, retains strict closure/build agreement and
publication capabilities through Export in that process. No empty-recipe strict precheck or digest
receipt reconstruction. Move existing private export Cargo/acceptance/native fs adapters, don't
duplicate them. Fresh no-lock project may generate its initial lock in bootstrap; existing lock is
locked, strict Export is locked/revalidated. Explicit desktop-profile Check needs matching compiled
features; default headless tooling excludes WGPU. Extra Reference Game bins are excluded by explicit
--bin headless --bin desktop selection. Keep module-consumer Cargo/process free.

Use English code/docs and Chinese conversation. Cargo serial, jobs=1, nextest, cargo fmt --all.
Keep progress updates meaningful at most 60 seconds apart. Continue the active goal without asking
the user to reauthorize already requested refactoring. No push/PR is part of this plan.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- docs/knowledge/engineering/registry/2026-10/2026-10-15T112650Z-nara-2d-foundation-contracts-codex-root-e88d1a2cbabc408bbff02a2d7c0d1442.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T114139Z-foundation-u5-core-ownership-privacy-and-incremental-close-verification-7e531ba1a4644ad9a36ccf7876fd2e61.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T104337Z-reuse-existing-product-contribution-and-project-owned-tooling-for-u6-040afbb34ab24947b5f1678d25d3e129.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T110728Z-supervised-play-close-retry-and-background-preparation-ownership-gaps-df85ee87b18f42d88e257c480b5f83fb.md
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T114556Z-compiled-presentation-must-not-impose-a-target-on-a-headless-runtime-476fd15790694f1bb3a9818e055ff720.md
