---
type: "Verification Evidence"
title: "U2 task return and physical running accounting"
description: "Executor return observer replaces local preparation flags; gated disposal and panic regressions qualify its scope."
timestamp: 2026-10-22T18:17:37Z
record_id: "ce30f14820ee4350931de3355bb0c955"
producer_id: "codex-root"
run_id: "low-level-contract-refactor-20261023"
related_plan: "docs/plans/2026-10-23-0135-refactor-low-level-contracts-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "d6dc05c"
---

# Verification

Windows 11, default Rust 1.99, based on `d6dc05c` plus U2. Cargo ran serially with
`CARGO_BUILD_JOBS=1` and the root target. No native Editor or other-platform qualification is
claimed by this unit.

# Result

Task terminal publication no longer substitutes for executor-owned return. An independent atomic
observer covers closure/capture and rejected late-result disposal without retaining output storage.
Caught execution unwinds conservatively report cleanup failure: the boundary cannot distinguish a
body panic from capture destruction panic. Already transferred results and escaped owners remain
outside the observation. Spawn rejection supplies its disposal outcome even without a task ID.

Physical running metadata remains present while the worker drops its final owned references, outside
the queue mutex. Scene, source scan and document Open preparation use this owner-provided observation
instead of three field-order `PhysicalReturn` flags. Native persistence custody remains domain-owned.

# Evidence

- A04 red run `ce45c0d6-fc29-4d64-b942-018e0727c74f`: a gated completed-output destructor
  observed running count zero instead of one on the original accounting path.
- Independent review found a running-closure capture destructor panic was still classified as
  `Returned`. Added behavioral regression failed in `ef7fea8d-cd7c-46b0-9c93-6cfca3d30a57`,
  then passed after the conservative execution-unwind classification.
- `cargo nextest run --locked -p nara_tasks --test-threads=1`: 51/51 pass, final complete run
  `8e974ef6-d581-4035-bfce-f46721cf1bf4`. Covers cancellation with held late-result disposal,
  panic cleanup, observer output independence, rejection including ID exhaustion, coalescing,
  physical running count/age and existing close/retry paths.
- Added a reaper observation assertion to the existing held pending-capture shutdown test after
  that full run: Pending persists through held destruction, then Returned. Focused run
  `43528eec-2ea7-4982-8c67-4f0ae9485fde` passes.
- `cargo nextest run --locked -p nara --no-default-features --features tooling,runtime-2d,serde
  --lib -E 'test(scene_requests::) | test(browser_preparation::) | test(document_open::)'
  --test-threads=1 --no-fail-fast`: 24/24 pass, 185 filtered out, run
  `31d29152-b058-43a1-8060-ab6b9ffad40d`. Includes public Scene slot reuse, Browser exact
  acknowledgement and held scan/session close. This feature combination retains 29 unrelated
  unused-import/dead-code warnings; no claim of a warning-free root build.
- Tasks all-target Clippy passes with `-D warnings` and the same three command-line exemptions
  documented for the preexisting App dependency baseline: `result_large_err`, `collapsible_if`,
  `type_complexity`. No repository allowance was added.
- Independent read-only review confirmed the execution-unwind fix and the three preparation failure
  paths have no blocking finding. Document Open retains real persistence hosts in `HostCustody`;
  returning an unavailable preparation outcome does not certify native cleanup.

# Follow-up

U3-U11 remain in the active Goal. Caller-thread disposal latency remains the separately triggered
O2/G9 opportunity; this observation does not introduce a disposal executor or general cleanup proof.

# Citations

- [Plan](../../../../plans/2026-10-23-0135-refactor-low-level-contracts-plan.md)
- [Runtime](../../../../../crates/nara_tasks/src/runtime.rs)
- [Regressions](../../../../../crates/nara_tasks/src/tests/execution.rs)
- [Shutdown regression](../../../../../crates/nara_tasks/src/tests/close.rs)
- [ADR 0052](../../../../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
