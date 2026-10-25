---
type: "Work Progress"
title: "Bounded authoring query public workflow and simplification checkpoint"
description: "Paged actual inventory and bounded root reads reach existing public actions and computed edits; final qualification and complete review remain separate."
timestamp: 2026-10-24T23:47:26Z
record_id: "032e0546f45246c1b04dcc4eb2fc64a5"
producer_id: "codex-root"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "7f2cafa"
---

# Summary

Continues the full foundation Goal on uncommitted source based on `7f2cafa`, not a smaller
replacement goal. Workspace now exposes paged actual component/root metadata and separately
bounded selected-root reads; Session/Application and registered panels carry them through the
existing action/outcome route. The independent consumer discovers an integer without knowing
its component/field keys, reads the current value, computes a replacement and uses the existing
prepared-property/history/Save/Reopen workflow on non-active Scene and Prefab sources.

The current 16-Rust-file LF manifest is
`d25ce513868bc698bd0239cebd680383cc9522447a5f071dbc9009a83288012a`.
Sorted tracked Rust diff paths plus untracked Rust files under `crates/nara_scene` and
`crates/nara_tooling` use the same normalized per-file/composite SHA-256 recipe as the earlier
qualification. This is not a commit, complete R2/wave acceptance or a full CE review receipt.

# Owning Changes

- `EditorDocumentQueryBasis` and `EditorDocumentQueryRejection` replace the entity-specific
  names without aliases. Entity and authoring queries share exact document/kind, incarnation,
  revision and immutable Schema admission checks. All current code/guide consumers are updated;
  historical immutable records keep their original names.
- Discovery separates Components from Roots of a concrete actual component. Continuations retain
  workspace affinity and the complete entity/component/projection/position. Metadata counts all
  returned/cursor UTF-8 IDs, including duplicated targets and a component anchor, before copies.
  Fixed-size facts are bounded by row/type shape; this is not wire size or allocator isolation.
- Selected-root reads retain Present/Defaulted/Missing, including two different Null origins.
  Exact target metadata and complete value node/logical-byte costs precede the only value clone.
  Unknown/unavailable records never gain semantic payload access. No Inspector, general query
  planner, second document/history or full effective-component copy is introduced.
- `QueryDocumentAuthoring` / `ReadDocumentRoot` actions and owned results reuse current Session
  dispatch and Application/panel callbacks. Query-only batches do not require a geometry frame;
  mixed navigation batches retain their existing frame admission. Prepared panel submission is
  still separate work, not implemented by these read-only actions.

# Evidence And Corrections

Initial Tooling tests failed with expected missing APIs/types, then the first three focused tests
passed in run `087ada3b-4ccb-4bb5-8985-74b6ff2d99ba`. A targeted read-only transport review
`01a1703b-502d-75c0-b93a-37c4e2723c0e` found no concrete source defect, but correctly challenged
metadata tests deriving their expected boundary from returned counters. Independent oracles now
fix the first Components page at 26 UTF-8 bytes, first Roots page at 32 and `amount` read at 19;
exact/under-budget refusal, independent node limits, empty observations and defaulted Null are
covered. The complete target cannot be retargeted by mutating a copied page.

Two broad test failures were fixture/expectation errors, not reasons to weaken source admission:

1. Run `777dfd03-9498-4534-ad58-db8c87669cef` passed 399/400 before a test assumed raw declaration
   order rather than the admitted catalog's canonical field-ID order. The literal expectation
   now names that canonical order.
2. Run `be11a894-9c94-4a6e-b11f-2434a9cd980c` passed 399/400 before the zero-root fixture used only
   the data Adapter, whose existing `supports_data_authoring` excludes zero-field Schemas. The
   fixture now obtains a matching trusted-provider authoring receipt for its separate Tag owner.
   No production readiness rule was relaxed or empty-root assertion removed. This source-only
   trusted authoring coverage is not native materialization/executable-replacement evidence.
   Its direct dev dependency adds only `nara_app` to the Tooling entry in `Cargo.lock`; no package
   version changed. One offline focused run updated that lock entry, then locked verification resumed.

After correction and simplification, focused run `7d23ee6e-4244-420c-b9b0-a7224b0300ea` passes four
tests (one diagnostic skipped). Explicit Rust 1.95.0 final owner run
`a7491159-fcf9-4c2b-bb8c-efaa556057dc` passes 400 Scene/Tooling tests, four skipped. The final
independent consumer run `9874a498-9f96-4582-a1de-431a1377453c` passes all four workflows, including
the new computed-edit path and registered-panel actual-inventory/Unavailable-root outcomes.
Earlier direct-consumer run `d40a8138-ea89-493f-98f7-6ec8f5c60a78` predates the panel extension
and final source simplification and is not substituted for that final run.

Final root/Editor all-feature library run `80dcfd3b-f94f-4383-9806-9902c9688e5c` passes 582 tests,
28 skipped. Selected real Windows/DX12 run `e7d63ebd-b9d3-4045-9eb7-b24668fa21c7` passes three
tests, 205 excluded: ordinary structural widgets/history/Save/Reopen, Canvas picking/move/resize/
persistence, and retained Scene construction/Undo/Prefab navigation/bound close. Those qualify
affected existing native paths, not a new query widget, complete platform coverage, browser,
IME/accessibility, native timing or executable replacement. The new complete query workflow is
proved by the independent window-free Application consumer.

Diagnostic run `39ebfcfa-f0b2-49cf-90be-cf917fa31c81` passes one ignored owner measurement. Its six
Scene/Prefab cases at 259/4,099/16,387 entities report scalar read P50 900-1,000 ns / P95
1,000-1,100 ns with a 10,000-byte unrequested sibling. The [baseline report](../../../../benchmarks/2026-10-25-authoring-root-read.md)
records the request-clone timing boundary, result-disposal exclusions, quantization, fixed order,
host and absence of allocator/native/frame-latency claims. No old complete read workflow or
speedup is invented.

# Simplification And Open Owner Finding

Actual pipeline simplification ran five read-only Codex lanes:

- Reuse `01a17047-1c02-7c20-bdde-cbeeaa65645a` and quality
  `01a17047-1df6-78e1-8232-c771931859b5` independently identified the same redundant root-field map.
  Accepted Schema admission already sorts/deduplicates fields; binary search in that immutable
  sequence removes duplicated IDs/tree nodes without weakening lookup or error order.
- Efficiency `01a17047-2029-7b93-9816-8019fa665c44` identified payload-record lookups during header
  discovery. Views now retain entity/fact borrows and resolve the actual record only for a value
  read. Headers/root metadata do not perform one unnecessary record lookup per row.
- Test-value `01a17047-229a-73a2-b72a-400bf27f72ea` confirmed the existing executed zero-root fixture
  defect above; no proof test was deleted, renamed, moved, merged or weakened by simplification.
  The fixture repair is a separate justified test correction, not test pruning.
- Comment `01a17047-24fa-7421-aecb-1616861a2ffe` identified two redundant internal comments and three
  private module introductions. Those five sites were removed; public API contract rustdoc remains.

Applied counts: reuse 1, quality 0 additional (duplicate), efficiency 1, tests 0, comments 5;
one efficiency suggestion remains an owning follow-up. All six transport/simplification agents
are terminal and closed. This is actual simplification, not a substitute for full CE code review.

**Open Reflect owner finding:** successful `ComponentValue::get_path` constructs/clones diagnostic
prefixes on every segment, causing O(depth-squared) copied path segments even on success. Do not
copy a second resolver into Scene. A later Reflect-owned change can record a segment index and
construct identical error paths only on failure, with exact diagnostic/default/mutation consumers
as its proof. It was outside this simplification's write scope and is not a source-read correctness
defect or allocation-free claim. Keep it in the full foundation work rather than losing it.

# Next Action

Unified exec `10434` is terminal with exit zero. Owner/cost/public/root/native results above,
`cargo +1.95.0 check --locked --workspace --all-features --all-targets`, correctness Clippy over
Scene/Tooling/root/Editor all features/all targets, and root/independent-workspace format checks
all pass. Existing non-correctness Clippy/unused-code warnings remain; this is not warning-clean.
No Rust source changed after the recorded manifest.

Unified exec `10497` is also terminal with exit zero. Explicit Rust 1.95.0 feature-off UI guard
run `40a78ec6-95e4-4a37-be66-7c965e98c279` passes its one test with
`--no-default-features --features runtime-2d,tooling,serde`. The root portable compile passes with
`--no-default-features --features runtime-2d,presentation-2d,presentation-ui,tooling,serde --lib
--target wasm32-unknown-unknown`. Browser execution and Linux/macOS/native platform claims remain
outside that compile check. No Cargo command or agent from this unit remains live. Starter
generation, build and export policy did not change, so the long generated-starter export was not rerun.

Next, run actual full CE code review on the bounded read/query diff based on
`7f2cafa` (including new files, excluding `vendor/`). This full review has not started and is not
unavailable/skipped. Use Codex subagents rather than an external model. Address justified findings,
reverify affected scope, render/validate memory, inspect/stage only owned paths and create a local
Conventional Commit. No push, PR or migration guide. Keep the full foundation Goal active; then
continue registered-panel prepared hooks and the plan's dependency-ordered remaining waves.

# Citations

- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Read-projection design](../../../../architecture/authoring-read-projection-design.md)
- [Prior owner checkpoint](2026-10-24T231507Z-retained-construction-committed-and-source-read-projection-checkpoint-843e9e4a886c4d7fa8e9f89ea8433591.md)
- [Source borrowed view](../../../../../crates/nara_scene/src/authoring_decode/read.rs)
- [Query owner](../../../../../crates/nara_tooling/src/workspace/authoring_query.rs)
- [Transport types](../../../../../crates/nara_tooling/src/authoring_query.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/prepared_authoring.rs)
- [Budget/continuation consumers](../../../../../crates/nara_tooling/tests/authoring_queries.rs)
