---
type: "Verification Evidence"
title: "Bounded authoring query complete review and aggregate budget qualification"
description: "Nine-lens review completed; the confirmed multi-row budget proof gap is closed by focused tests and independent follow-up."
timestamp: 2026-10-25T00:29:33Z
record_id: "893b5a3ffa104ec8bc7c7413c12e0c5d"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7f2cafa"
---

# Verification

Qualifies the bounded R2 actual-component/root discovery and complete selected-root reads on
uncommitted source based on `7f2cafa`. Scene/Prefab retain source interpretation; Workspace,
Session, Application and registered-panel action outcomes transport exact bounded observations.
The independent non-active consumer discovers an editable integer, reads it, computes an edit,
then exercises prepared commit, Undo/Redo and Save/Reopen. Prepared panel submission is not
implemented by read-only panel outcomes.

# Result

Full CE review `nara-r2-read-ce-review-p3x21vks` completed against 29 changed paths with nine
Codex lenses: correctness, standards, testing, maintainability, security, performance,
API contract, reliability and adversarial. The actual report receipt has `status: complete`
and verdict `Ready with fixes`. No P0/P1 or unresolved validation gate remains. All nine
reviewers, merge, independent validator and report handles reached terminal outcomes and closed.
No external-model route ran and no cross-model independence is claimed.

The sole actionable finding, P2 #1, identified missing multi-row aggregate metadata regression
coverage, not a demonstrated accounting defect. A fresh validator confirmed that gap. The caller
added a table-driven Scene/Prefab test using literal budgets: two Components rows cost 25 UTF-8
ID bytes with no continuation; two Roots cost 36 including the continued target. Both exact
budgets succeed, 24/35 reject, actual IDs match, and the Roots cursor advances to `defaulted`.
No production behavior changed while addressing this finding. Targeted reviewer
`01a17075-616f-70b0-a91f-3e504b8612ac` independently verified the arithmetic, refusal and cursor
assertions; charging only the last row would fail both the count and under-budget assertions.
The test does not directly measure allocation or copy timing; source ordering establishes
pre-copy admission.

# Evidence

Explicit Rust 1.95.0, serial Cargo, `CARGO_BUILD_JOBS=1`, root `target`:

```text
cargo +1.95.0 nextest run --locked -p nara_tooling --all-features --test authoring_queries
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 fmt --manifest-path tests/fixtures/module-consumer/Cargo.toml --all -- --check
```

Nextest `5027c350-5a73-42d3-ba4c-efc2d4e916d7` passes all five tests; one timing diagnostic is
skipped. Both format checks pass. This is test-only proof strengthening, so no artificial red
production failure or mutation run is claimed.

The prior normalized 16-Rust-file manifest is
`d25ce513868bc698bd0239cebd680383cc9522447a5f071dbc9009a83288012a`. Comparing every retained entry
after the correction finds only `crates/nara_tooling/tests/authoring_queries.rs` changed; its
new LF-normalized SHA-256 is `c0f96f936e1c8bc8ece5cadd7d069a23d2cdc57920ec5b98cdcad563344b20d4`.
All production, owner-test and independent-consumer Rust hashes remain unchanged.

The [preceding checkpoint](../../progress/2026-10/2026-10-24T234726Z-bounded-authoring-query-public-workflow-and-simplification-checkpoint-032e0546f45246c1b04dcc4eb2fc64a5.md)
records the actual pre-review qualification on that unchanged production source: 400 Scene/Tooling
tests, four independent workflows, 582 root/Editor library tests, three selected Windows/DX12
native trajectories, the feature-off guard, portable wasm compile, workspace/all-target check
and correctness Clippy. Those historical results were not rerun or relabeled as final test-file
results. Existing non-correctness/unused warnings remain. The diagnostic timing was already
executed separately, not silently included in the five-test run.

# Follow-up

The scalar-read benchmark does not measure direct-fact index storage/rebuild/clone costs with
growing populated/degraded documents. Classification builds that map and field preparation
clones classification state; the existing timing fixture grows empty entities. Retain this
attribution boundary rather than infer an index-maintenance speedup or regression. Reflect's
successful `get_path` diagnostic-prefix allocations remain an owning follow-up, not a reason
to introduce a Scene resolver.

Next: inspect/stage only owned paths and create the local Conventional Commit, then continue
registered-panel prepared editing and remaining dependency-ordered foundation work. Do not
mark R2, any complete delivery wave or the full Goal complete. No push, PR, additional migration
guide, browser execution, Linux/macOS native coverage or new native query widget is claimed.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Read projection contract](../../../../architecture/authoring-read-projection-design.md)
- [Bounded cost report](../../../../benchmarks/2026-10-25-authoring-root-read.md)
- [Aggregate regression](../../../../../crates/nara_tooling/tests/authoring_queries.rs)
- [Pre-copy accounting](../../../../../crates/nara_tooling/src/workspace/authoring_query.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
