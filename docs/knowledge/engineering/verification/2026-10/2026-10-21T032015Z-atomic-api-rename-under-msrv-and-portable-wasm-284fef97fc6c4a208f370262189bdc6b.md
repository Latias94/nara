---
type: "Verification Evidence"
title: "Atomic API rename under MSRV and portable wasm"
description: "Verification Evidence for Atomic API rename under MSRV and portable wasm."
timestamp: 2026-10-21T03:20:15Z
record_id: "284fef97fc6c4a208f370262189bdc6b"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-21-task-execution-budget-plan.md"
git_branch: "main"
git_commit: "bf8711f"
verified_by: "MSRV workspace check, nextest, wasm check and mechanical diff audit"
---

# Verification

The maintenance renames **22 first-party calls in 15 Rust files** from `fetch_update` to
`try_update`. Memory ordering, closures, return/error handling and ID/counter bounds are unchanged.
Local installed core source marks try_update stable since 1.95.0 and shows fetch_update forwarding
to it. Nara's MSRV remains 1.95. This is equivalent API maintenance, not a performance optimization.

- `cargo +1.95.0 check --workspace --locked --all-features --all-targets`: pass, **2m44s**.
  Local log: `target/atomic-rename-msrv-check.log`.
- Existing App/Tasks/Identity/AssetWatch all-feature library nextest: **173/173 pass**,
  `5926160f-206f-4de5-8227-70d6c499b6f3`, compile **52.91 s**, tests **10.947 s**.
  Local log: `target/atomic-rename-contract-tests.log`. This includes last-ID/exhaustion,
  uniqueness, cancellation/terminal races, physical close and watcher queue contracts.
- Portable wasm: `cargo +1.95.0 check -p nara --locked --no-default-features --features
  runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib --target wasm32-unknown-unknown`
  passes in **24.50 s**. Log: `target/atomic-rename-wasm-check.log`. This is compilation,
  not browser execution, window/GPU support or an exported game qualification on wasm.
- `cargo +1.95.0 fmt --all -- --check` passes. Existing feature-subset unused/dead-code warnings
  remain; no claim that every workspace warning is resolved.

# Result

No compatibility wrapper, new test duplicating the standard implementation, MSRV increase or
vendored dependency edit is needed. Mechanical comparison against the preceding commit is the
appropriate change-shape proof; existing behavioral tests cover the owning counters and custody.

# Evidence

One subsequent Cargo command was accidentally started before the previous check's final result
was collected. It initially waited for the artifact lock; root stopped only that owned command
tree, collected the completed check and then reran the test cohort serially. No other process or
source tree was terminated or reverted. The final tests above come from that clean serial retry.

# Follow-up

The separate [execution baseline](../../../../research/2026-10-21-task-execution-baseline.md)
records all four inspected feature unions and one native thread observation. The
[budget plan](../../../../plans/2026-10-21-task-execution-budget-plan.md) orders remaining
owner/workload measurements before selecting a worker policy or Bevy feature change.

# Citations

- [Initial rename/MSRV study](../../../../research/2026-10-21-task-dependency-and-deprecation-preliminary.md)
- [Tasks source](../../../../../crates/nara_tasks/src/runtime.rs)
