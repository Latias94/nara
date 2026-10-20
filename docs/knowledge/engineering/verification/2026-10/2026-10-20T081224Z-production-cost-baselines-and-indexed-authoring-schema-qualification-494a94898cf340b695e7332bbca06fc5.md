---
type: "Verification Evidence"
title: "Production cost baselines and indexed authoring schema qualification"
description: "Optimized public interaction, offscreen runtime and Rust iteration observations with exact schema lookup correction."
timestamp: 2026-10-20T08:12:24Z
record_id: "494a94898cf340b695e7332bbca06fc5"
status: "verified"
producer_id: "codex-root"
run_id: "production-costs-20261020"
related_plan: "docs/plans/2026-10-20-production-cost-measurement-plan.md"
git_branch: "main"
git_commit: "d21d8c0bc2b63df6aa8eb2bd70a7b8ba8298596e"
---

# Verification

The finite production-cost measurement slice and its first narrow schema-lookup correction
are complete. Product baseline is `33248b79b069e4d9023601e38e4c76515ea8e937`; collector/raw
baseline commit is `b8e15cb91b8f48e235dafa9694ecb360a6b07e76`; correction implementation is
`d21d8c0bc2b63df6aa8eb2bd70a7b8ba8298596e`. This closure adds documentation and observations.

# Result

- Three optimized Editor baseline processes and three unchanged post-correction processes each
  retain 768 numeric rows with exact action/value/history/source/replacement/close oracles.
- Three optimized real-GPU offscreen runtime processes each retain 654 numeric rows, including
  labelled warm-ups and close observations. Actual adapter is RTX 4090/NVIDIA 617.14/Vulkan.
- Seventeen independent-game dev/incremental build/run observations retain actual semantic
  victory and changed player coordinates. Body/layout medians are 6.633/7.633 seconds.
- A classification-owned exact Available direct-record index removes two linear searches per
  schema query. The 4,096-record frame drive falls from approximately 60 ms to approximately
  2 ms. Edit dispatch remains approximately 117-120 ms; Browser filesystem and large Tilemap
  work remain measured pressure, not completed refactors.
- All 274 Scene/tooling tests pass. Public API, format validation and history authority remain
  unchanged; private index heap cost is not measured.

# Evidence

Root runs all Cargo serially with Rust 1.95.0, one build job and the existing shared target.
The report owns workload, hardware, profile, cache, endpoint and resource-accounting details.
Independent researchers implement disjoint helpers; separate reviewers inspect code and
independently recompute every baseline/post-correction group and displayed table.

Passing checks:

- New exact-schema tests: nextest `38e5292f-68c1-4d39-9b7b-b11d90a64605`, three passed. Includes
  Scene and Prefab readiness/version/owner/patch-only/Native barriers, history and Reopen.
- Focused authoring/Inspector pass: `f9da7d57-b662-4d61-b7a9-f384676ec3ae`, 31 passed. The
  subsequent full `nara_scene`/`nara_tooling` all-feature run
  `bfb45609-9711-4453-9246-ab6efd9c5a39` passes all 274; counts overlap.
- Three baseline Editor and three Vulkan runtime run IDs, raw filenames and hashes are in
  `docs/benchmarks/data/2026-10-20-production-costs/baseline-summary.json` (252 groups).
- Three post-correction Editor run IDs, filenames and hashes are in
  `docs/benchmarks/data/2026-10-20-production-costs/indexed-summary.json` (144 groups).
- `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline` passes in
  1m 13s; existing dead-code/unused warnings remain.
- `cargo +1.95.0 fmt --all --check`, `git diff --check`, Python collector compilation and
  changed-document local-link checks pass. Engineering memory and external HTML receive
  separate final document validation; no product rerun is inferred from documentation closure.

The first Editor measurement attempt omitted the public terminal CloseDocument acknowledgement,
then rejected the next replacement. Its partial rows are excluded; the fixture now verifies
and acknowledges the exact result outside timing. A first WGPU compile used nine plugins in
one tuple while the current convenience supports eight; the fixture uses a second ordinary
add call. Neither correction weakens product behavior. Native window tests from the preceding
canvas qualification remain historical; no new native UI/first-presentation result is claimed.

With sixteen samples per timing group, nearest-rank P95 is the observed maximum. Independent
processes are not pooled. Runtime phase gaps/overlap preclude a complete frame-budget sum.
GPU counters are completion-correlated logical payloads, not complete allocations/residency;
close-to-Retired excludes later World destruction. No leak follows from bounded glyph history.

# Follow-up

The proposed locality successor orders scalar phase attribution, finite Browser preparation,
Tile visibility/cache design and Rust/Play build attribution. It explicitly addresses pending
Open outcomes, irreversible close custody, Tile public-field/component/incarnation invalidation
and Cargo timing limits. These are separate future units; Tile authoring tools remain deferred.
No universal async trait, dynamic ABI, second scheduler or new release qualification is added.

# Citations

- docs/benchmarks/2026-10-20-production-costs.md
- docs/plans/2026-10-20-production-cost-measurement-plan.md
- docs/plans/2026-10-20-measured-work-locality-plan.md
- docs/architecture/adr/0099-decision-local-product-evidence-and-publication-admission.md
- crates/nara_scene/tests/authoring_schema_lookup.rs
