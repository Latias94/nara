---
type: "Session Handoff"
title: "Foundation U5 raw execution removed with unwind regression pending"
description: "Continue the active U1-U13 goal from the shared dirty worktree; U4 is verified and U5 consumer validation plus one review fix remain in flight."
timestamp: 2026-10-15T10:52:14Z
record_id: "a723ad2ee28842d1bbfdaba6bb015333"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5535a0d"
---

# Summary

Continue the explicitly authorized active U1-U13 goal. Do not stop at U5 or mark the goal complete.
The target is F:/SourceCodes/Rust/nara, not the tool default pokel directory. Conversation is Chinese;
source/technical docs are English. Plan: docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md;
Accepted authority: ADR 0117. Current branch refactor/engine-foundation-contracts, HEAD 5535a0d.

Commits: 7e3449a U1 authority; 722ff27 U2 hierarchy; bc71449 U3 resources; 5535a0d U4 evidence.
U4/U5 source remains uncommitted. Never discard/stash/reset pre-existing SPL/fs/Host/tooling changes.
The original staged patch was verified byte-identical immediately before the latest doc commit.
Original index/worktree patches and hashes are in local temporary directory
C:/Users/Frankorz/AppData/Local/Temp/nara-foundation-execution-1kxxdfu8.

# Verified State

Active registration successor: f8cc88aac8214e40a7f24ae09f14f000. U1-U4 implemented. U4 evidence
4f79a46cf5c4455a8700d9090c13e386: 59 Windows and 57 native WSL2 Linux filesystem tests, focused Host
cleanup tests, 12 repair CLI/workflow tests, two Reference Game bundle CLI tests. No macOS claim.
WSL commands require the wsl-operations wrapper, distro Ubuntu-26.04; Cargo is
/home/frankorz/.cargo/bin/cargo and needs explicit PATH. Authorized HTTP proxy populated its cache.

U5 production is in the worktree: App owns the existing RuntimeOwner from construction; all World,
schedules, installed plugin objects, fault/startup/closure state lives directly in RuntimeOwnerState.
No RuntimeOwnedValue<App>, duplicate reporter, App Drop or public raw run/update/run_once/run_schedule/
set_runner/shutdown_plugins remains. RunnerFn/default_runner, raw-only errors, unused PluginCapability/
PluginServiceId declarations and corresponding root diagnostic arms/reexports were removed.
Managed epoch, generation and domain checks remain. RuntimeInstance::start/start_with_options and
consuming finish share the owner; failures retain physical cleanup. Normal owner Drop completes one
bounded policy attempt; panic/already-quarantined Drop retains without blocking.

EditorAuthoringRuntime holds RuntimeInstance. Manual TaskPools creation and independent shutdown
are gone; TaskPlugin owns cleanup and each poll publishes TaskShutdownReport. Private concrete ports
coordinate image import/commit; preview drives the runtime. Session close also waits for U4 files.
Simple/headless/desktop examples, Gallery and Interaction Lab use start/drive/finish; the shared
example retirement helper only combines prior runner error with retained finish failure.

Raw module/root tests were migrated to managed execution or pure domain functions, with narrow
specific ports and ordinary systems; no general World callback framework. nara_ui's 81 library tests
passed post-removal (run bb6f70fa-dd1f-4909-9a42-e6b572fc686b). Before removal, nara_app's 55 library
tests passed (94f67196...), tasks' 43 passed (47cdcfa9...). Their new TaskShutdownReport assertions
were observed red first (53aad110...) then green. Full post-removal suite is not yet verified.

Broader initial U5 root Host run was 23/30: normal-drop task owners entered quarantine before joining.
Canonical normal Drop was subsequently fixed, not yet reverified broadly. Prepared publication
unwind test now explicitly drains actual quarantine after catch and passes. Global all-target check
found old fixtures (missing cargo_facts Value/PathBuf imports, temporary frame borrow, Scene replacement
enum diagnostics); these are corrected without production compatibility methods. A second full check
is still required.

# Open Threads

U5 blocker: start_inner sealed before transferring external Host obligations; staged admission bound/
materialized observers before transfer. Panic drops Host ledger. Public root regression
ordinary_start_validator_unwind_retains_host_close_obligations failed as expected with begin=0,
run fe627d28-96fb-4b95-b7de-ba4b8db175a4. It drains quarantine before assertions. Finding:
51473f488c09410c8db5bb9f5579070a. /root/u5_execution_owner prepared the second regression
runtime::tests::admission_observer_initialization_unwind_retains_host_close_obligations, production
unchanged pending red. It uses the real Add<Observer> initialization event path, not hook replacement.
Run that red next, then instruct the worker to move Host obligations into the same owner before any
callback, preserving Host-first order and both physical entries on duplicate-ID failure.

Live serial Cargo at capture: Windows exec session 75861, cargo check --workspace --all-features
--lib. Finish it before any other Cargo. The second red test is ready; no worker may run Cargo.

U5 staging types/APIs are still public for root's cross-crate private Host implementation. Ordinary
consumers no longer use them. Settle an explicit internal bridge/visibility boundary before claiming
candidate choreography is private; do not just break root by privatizing the existing types.

U6-U13 are not implemented. U6 implementation map: finding 040afbb34ab24947b5f1678d25d3e129.
Reuse ProductRecipe/SchemaContribution/RuntimePlan. Parent CLI bootstraps headless without empty-
recipe strict admission; project process owns strict Check/Export and publication. Keep build/source/
selected-product agreement. Five-file build input inventory and Reference Game export observation
support are concrete limits to resolve honestly, without Rust source parsing/provenance proofs.

# Next Action

Finish U5 red/fix/green, then appropriate full shared-runtime/root/consumer verification and precise
commit(s), then continue U6-U13/AE1-AE5. Use serial Cargo, CARGO_BUILD_JOBS=1, existing target,
nextest preferred. Run cargo fmt --all after active writers are ready and inspect resulting diffs.
Preserve pre-existing staged/worktree changes. Full ce-code-review receipt remains required at plan
closure; current focused reviews are not that receipt. Do not push/PR outside the local plan scope.

Worker ownership: u5_execution_owner owns nara_app src/tests and the pending fix. u5_module_tests
finished assigned engine test modules plus Scene format_contract. u5_ui_tests finished adapter and
root bridge tests. u5_editor_runtime finished Editor/tasks and all example cleanup. Others are idle
after final replies: use followup_task to wake them; send_message alone does not start idle work.
All workers are forbidden Cargo/global fmt/index/commits and must preserve other edits.

# Citations

- docs/architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md
- docs/knowledge/engineering/registry/2026-10/2026-10-15T103700Z-nara-2d-foundation-contracts-codex-root-f8cc88aac8214e40a7f24ae09f14f000.md
- docs/knowledge/engineering/verification/2026-10/2026-10-15T102927Z-foundation-u4-windows-and-native-linux-publication-cleanup-4f79a46cf5c4455a8700d9090c13e386.md
