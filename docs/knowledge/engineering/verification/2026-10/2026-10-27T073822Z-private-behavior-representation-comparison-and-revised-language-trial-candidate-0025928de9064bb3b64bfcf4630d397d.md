---
type: "Verification Evidence"
title: "Private Behavior representation comparison and revised language-trial candidate"
description: "Records data-only envelope versus Rows evidence, review corrections and remaining public Runtime and VM lifecycle scope."
timestamp: 2026-10-27T07:38:22Z
record_id: "0025928de9064bb3b64bfcf4630d397d"
producer_id: "codex-root"
run_id: "behavior-representation-2026-10-27"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "experiment/behavior-representation"
---

# Verification

This records the private comparison based on `66989bec2cffa2b5f9eaf5426a61f9c66cf5a611`.
It changes no production code and does not close the foundation Goal or adopt ADR 0120.
The comparison plan and report distinguish hidden integration from public Runtime support.
Untracked `vendor/` was not read, modified or staged. No remote publication occurred.

# Result

Both stores execute the same admitted Mover/Sensor data alongside native Name, source history,
JSON/RON disk persistence, declarative Prefab overrides, repeated occurrence references and
runtime export. Missing bindings/invalid last records refuse a fresh unpublished World without
affecting a separate active predecessor. Version-two evidence is an explicitly transformed data
candidate, not a catalog migration service or migration Undo.

Rows inspect one record versus 33 carriers for the selective Mover query; this is not a timing
or memory comparison. Envelope data retires automatically with the entity. Resource rows survive
ordinary destruction unless the owner cleans them. An on-remove marker demonstrates cleanup on
direct despawn and rejection by strict Scene retirement. The next language trial provisionally
uses a data-only envelope, revising the earlier preference for Rows. Actual VM custody and
dispatch evidence may reverse this choice.

The parent corrected retirement order before qualification: a fallible Scene refusal now precedes
row deletion. Beauvoir's independent review identified an export oracle that derived expected
targets from the exported payload, and a Schema-only setter that accepted missing local targets.
The corrected export oracle identifies each pair through independently changed native Name;
the setter uses the existing declared-reference plan and checks actual live World membership
before replacement. Missing and already-destroyed targets preserve both original members.
Beauvoir re-reviewed the final code/report and found no remaining P1/P2 blocker in that bounded
scope. The reviewer ran no Cargo. The known initial-roster limitation for export after partial
entity retirement remains explicit; it must not enter a production owner unchanged. The
reviewer is closed.

# Evidence

Windows 11 / PowerShell, Rust 1.95.0, serial Cargo, `CARGO_BUILD_JOBS=1`, root `target`.
The independently compiled fixture workspace explicitly reused that target.

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test behavior_representation --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Final behavioral run `38668b96-286f-4613-b83b-c32518380dcf`: **9 passed, 0 skipped**,
build 5.08 seconds, execution 0.262 seconds. Most tests execute both alternatives internally.
These are test execution observations, not engine workload benchmarks.

```text
cargo +1.95.0 clippy --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test behavior_representation --locked -- -D clippy::correctness
```

Final focused correctness Clippy passed in 1.23 seconds; existing dependency/feature warnings
remain. No broad workspace or native Editor qualification was run for this private fixture.
Focused `rustfmt --check` passed. All 766 local Markdown link targets in the seven touched/new
documents resolve, and `git diff --check` passed. Memory validation has no errors; historical
legacy metadata/large-ledger advisories remain outside this unit.

# Follow-up

ADR 0120 remains Proposed and its production implementation status remains not-started.
Compare the same gameplay task through bounded C# and Lua adapters. Select and integrate one
complete public representation owner only after actual toolchain, VM instance/code lifetime and
author workflow evidence. Owner-catalog migration, actual assets, cross-owner publication
closure, same-type multiplicity, body replacement and unload remain open.

# Citations

- [Comparison plan](../../../../plans/2026-10-27-behavior-representation-comparison-plan.md).
- [Comparison report](../../../../research/2026-10-27-behavior-representation-comparison.md).
- [Private fixture](../../../../../tests/fixtures/module-consumer/tests/behavior_representation.rs).
- [ADR 0120](../../../../architecture/adr/0120-schema-publications-and-execution-representations.md).
- Independent reviewer: Beauvoir, `01a17c27-16bb-71e3-a275-b6bb4e34774d`; no edits or Cargo.
