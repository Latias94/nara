# Production Cost Measurement

Status: Complete within the scope below. Source baseline:
`33248b79b069e4d9023601e38e4c76515ea8e937`.

## Purpose

Measure the next ordinary author and runtime workflows before selecting further foundation
refactors. The fixed world canvas and native UI slice is qualified. Earlier Save, Reopen and
startup measurements remain historical evidence; do not reclassify repaired synchronous paths
as current defects. Follow STRATEGY, ADR 0093 and ADR 0099. Tile authoring tools remain deferred.

## Lane Map

- Mode: research followed by bounded implementation and independent review.
- Repository/base: current `main`, existing checkout; preserve concurrent changes and `vendor/`.
- Global constraints: no generic benchmark service, public timing API, speculative executor or
  hard performance assertion. Separate warm-up, compilation, owner blocking, completion and
  actual presentation. Keep raw numeric observations and finite correctness oracles.
- Verification owner: root; all Cargo, formatting and Git operations are serial, with the shared
  target and one build job. Researchers do not edit files or run Cargo.
- Stop conditions: unexpected concurrent edits, invalid semantic oracle, or an unbounded workload
  require reconciliation before the affected operation proceeds.
- A / interaction: inspect scalar edits, Undo, Browser admission and scan finalization through
  existing public Editor consumers. Initially read-only; propose the smallest workload and seam.
- B / runtime: inspect changing HUDs, large Tilemaps and the precise scope of resource accounting.
  Initially read-only; distinguish estimated owned allocation from process or driver residency.
- C / Rust iteration: inspect function-body and structural edits in an independent game. Initially
  read-only; separate source save, build, semantic outcome and first presented frame.
- Root: integrate findings, own this plan, benchmark results and continuity records; assign
  disjoint measurement files after research. AGENTS.md and historical evidence are excluded.

## Work And Acceptance

- [x] Identify current consumers and retained measurement seams in all three lanes; reconcile
  existing research and pinned Bevy/Godot design evidence where it affects interpretation.
- [x] Implement bounded decision-local workloads using existing consumers. Assert successful
  operations and changed outcomes outside timed regions. Do not modify frozen formal protocols.
- [x] Run optimized interaction/runtime observations and finite Rust iteration experiments
  serially. Retain samples, exact source/build context, aggregation and explicit limitations.
- [x] Attribute material costs to their owning boundary. Repair confirmed correctness defects;
  document evidence-triggered performance refactors without expanding into speculative systems.
- [x] Remove the confirmed quadratic Available-schema lookup through a private classification-owned
  index; preserve Scene/Prefab readiness/history/source contracts and repeat the same public
  measurements. Keep format-validation equivalence and Browser/Tile locality as separate work.
- [x] Obtain independent review, run relevant checks, publish a concise English report and update
  the existing external HTML issue overview. Commit and push precise changes to main.

The result is a decision-local baseline and a ranked next action list, not a release SLO,
cross-platform qualification, stable tail estimate, complete GPU residency measurement or a
claim that all future architecture risks have been eliminated.

## Result And Successor

The [report](../benchmarks/2026-10-20-production-costs.md) retains three Editor baseline,
three runtime baseline and three post-correction Editor processes, plus seventeen independent
Rust build/run observations. Baseline collector/data commit: `b8e15cb`. Exact schema-lookup
implementation: `d21d8c0`. The same 4,096-record model drive falls from approximately 60 ms to
2 ms while scalar dispatch remains approximately 117-120 ms. All 274 Scene/tooling tests pass.

The [proposed locality follow-up](2026-10-20-measured-work-locality-plan.md) orders remaining
scalar attribution, Browser preparation, Tile visibility/cache and Rust/Play attribution. It
does not turn those separate refactors into unfinished work in this measurement slice.

The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T081224Z-production-cost-baselines-and-indexed-authoring-schema-qualification-494a94898cf340b695e7332bbca06fc5.md)
binds implementation, checks, corrected fixture failures and explicit interpretation limits.
