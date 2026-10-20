---
type: "Verification Evidence"
title: "Scalar preparation locality qualification"
description: "Field-only classification and conservative publication-shape reuse with exact validation, public timing and separate requested-allocation evidence."
timestamp: 2026-10-20T09:02:17Z
record_id: "7da17674f8c7464ea1612dc119686c45"
status: "verified"
producer_id: "codex-root"
run_id: "scalar-preparation-20261020"
related_plan: "docs/plans/2026-10-20-scalar-preparation-locality-plan.md"
git_branch: "main"
git_commit: "601b4671545f0d6835620e88c7e2e51c7b36895d"
---

# Verification

The selected L1 attribution and owner-local correction are complete within this finite scope.
Starting product source is `b9d25dc9688ac6d66edc47f31e3a5882c5e9a07a`; implementation is
`601b4671545f0d6835620e88c7e2e51c7b36895d`. Subsequent delivery changes add the ignored allocation
probe, immutable data and documentation. The ordinary public timing collector is unchanged.

# Result

- A private conservative certificate proves supported Scene/Prefab publication shapes fit both
  parsed grammars. Both actual serializers and exact byte checks still run. Unsupported values,
  embedded Prefabs, patches, excessive proof depth or unproven budgets use complete validation.
  The proof cutoff of 32 is an optimization boundary, not a new accepted-input limit.
- Existing field mutation constructors retain the prior document/state authority. Touched complete
  Available records use the existing schema/default/reference classifier, preserving its order;
  unprovable candidates fall back to full classification. Structural/Reopen paths stay complete.
  Public interfaces, unavailable manifests, synchronous success and history ownership do not change.
- Three optimized public processes reduce 4,096-record edit-dispatch P50 from 117-120 ms to
  38-42 ms; Undo P50 is 37-42 ms. Separate Scene/Prefab requested-allocation windows reduce
  65,772,568 / 83,917,851 bytes to 22,161,541 / 22,685,832 bytes. The allocation fixture is simpler
  than the public Transform timing fixture. These are requested System bytes, not RSS or GPU heap.
- 283 Scene/tooling owner tests and 17 ordinary public persistence tests pass. The locked workspace
  all-feature/all-target check passes. This does not qualify new native input-to-display behavior.

# Evidence

Root runs Cargo serially with Rust 1.95.0, `CARGO_BUILD_JOBS=1` and the existing shared target.
Format and authoring researchers implement disjoint private modules and their oracle tests;
an independent reviewer checks the source boundaries and recomputes raw timing/allocation tables.
The report owns complete endpoints, sample selection, digests and uncertainty.

- Exploratory optimized Scene phase probes: nextest `2e2a25ce-4c9d-41f6-87fd-1a164c3bb7e9` passes.
  Temporary probes and the scalar-only entry are removed. Inclusive nested observations and
  independent medians cannot be summed/subtracted or compared as final public timings.
- Differential format and field tests: `53c80a74-063d-4719-9995-f9cad77385fc`, eight passed. The
  final full `nara_scene`/`nara_tooling` all-feature nextest run
  `bf328ab0-2930-4ac6-85fe-ef0ebd4fd5eb` passes 283, with one ignored allocation probe.
  Coverage includes dual-format byte boundaries and exact diagnostics, every shape-budget axis,
  valid unsupported/deep fallback, unavailable preservation, numeric/default/reference admission,
  history refusal and ordinary-edit wire-limit rejection without state/history mutation.
- Three unchanged public release measurements pass: `067c5e2c-5b4c-452d-a928-282facc575da`,
  `f5bb1878-9b4b-43bb-9945-0d98b9aaf776`, `f17bcb6c-5b68-4b1e-9fea-bba14ab770ce`.
  Each retains 768 numeric rows. Their 144 summary groups use per-process nearest-rank
  percentiles; sixteen samples make P95 the observed maximum, not a stable tail estimate.
- Identical allocation probe against exact archived baseline and current implementation passes:
  before `51bc327c-a3b3-4b59-804d-97449d5fccc5`, after
  `b5e30a9e-2e2a-4477-82ee-047de26a1951`. Each has 32 retained rows and a method row. Archive,
  probe and raw-row digests plus the sixteen complete groups are in `allocation-context.json`.
  A document clone retains 8,286,208 requested bytes before/after; its Drop releases that amount.
  Edit/Undo live-request deltas settle to zero after reports drop in this single-owner fixture.
- The final ordinary root `editor_persistence` release run
  `803a158f-ded8-4271-85df-758314272300` passes 17; two measurement tests remain ignored.
- `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline` passes in
  1m 19s; existing unused/dead-code warnings remain. Formatting, diff, document-link, memory and
  external-HTML checks are final integration checks rather than another product acceptance cycle.

Diagnostic attempts are excluded from acceptance: the first deep fallback fixture hit the
original RON serializer recursion limit, so the fixture was reduced to a valid fallback example
and the conservative proof cutoff tightened. The initial allocation fixture used the wrong
Result accessor and was corrected before retained data. Shared-target reuse initially selected
the archived baseline executable for supposed current runs; those rows are excluded. Touching
owned source mtimes without changing bytes forces compilation; retained logs confirm archive
and current compilation separately. No shared target clean or concurrent user-code rollback occurs.

# Follow-up

4,096-record dispatch still exceeds the local 8 ms investigation trigger. Full canonical encoding,
candidate clone, O(N) classification-state copying/scanning and namespace construction remain;
unsupported grammar retains complete validation, so the improvement is not uniform. Attribute the
current path before choosing format summary reuse, touched-record storage or an explicit pending
edit/commit contract. The last alternative needs deliberate action-contract reconciliation;
asynchronous traits alone do not remove work. Browser preparation, large Tile locality and Rust/
Play attribution remain separate proposed units; Tile tools stay deferred. No new native,
cross-platform release, complete-memory or general leak-absence claim follows from these tests.

# Citations

- docs/benchmarks/2026-10-20-scalar-preparation.md
- docs/benchmarks/data/2026-10-20-scalar-preparation/editor-summary.json
- docs/benchmarks/data/2026-10-20-scalar-preparation/allocation-context.json
- docs/plans/2026-10-20-scalar-preparation-locality-plan.md
- docs/plans/2026-10-20-measured-work-locality-plan.md
- crates/nara_scene/src/format/publication_shape.rs
- crates/nara_scene/src/authoring_decode/field_candidate.rs
- crates/nara_scene/tests/scalar_edit_allocations.rs
