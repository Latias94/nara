# Scalar Preparation Locality

Status: Complete within the attribution and owner-local correction scope. Selected unit: L1 from the
[measured locality plan](2026-10-20-measured-work-locality-plan.md).
Starting source: `b9d25dc9688ac6d66edc47f31e3a5882c5e9a07a`.
Implementation: `601b4671545f0d6835620e88c7e2e51c7b36895d`.

## Intent

Attribute and remove the dominant repeated work in ordinary field edits and Undo. The preceding
private schema index already reduced model drives to approximately 2 ms at 4,096 Transform
records; edit dispatch remains approximately 117-120 ms. Preserve synchronous successful commit,
exact history, schema/readiness/reference admission and both JSON/RON format acceptance.

## Lane Map

- Mode: research, bounded implementation, independent review and serial verification.
- Repository/base: current main in the existing checkout; preserve concurrent changes/vendor.
- Verification owner: root; only root runs Cargo/fmt/Git, one build job and shared target.
- Global constraints: no generic benchmark framework, public timing API, unchecked schema route,
  deferred Save-time rejection or new async edit-result contract without contract reconciliation.
- A / format researcher: read-only exact format contract, shape/budget semantics, equivalent
  lower-cost validation alternatives and decisive counterexamples.
- B / authoring researcher: read-only field preparation/classification/history costs and state
  invariants; propose responsibility-local simplification and regression oracles.
- C / measurement reviewer: read-only current public consumer, attribution endpoints and bounded
  allocation/retention evidence; independently challenge equivalence and final measurements.
- Root / integrator: own temporary private probes, this plan, results and continuity. Assign
  disjoint implementation files only after source research and measured attribution.
- Stop conditions: invalid outcome oracle or concurrent ownership conflict requires reconciliation
  before affected edits continue. AGENTS.md, historical datasets and repo-ref are excluded.

## Work And Acceptance

- [x] Capture optimized phase attribution in a separate exploratory process; keep probe effects
  and complete public timings distinct. Remove temporary probes before final production checks.
- [x] Select and implement the smallest format/authoring-owner correction supported by those
  results. Extract private responsibilities when useful; keep public interface and owner authority.
- [x] Prove accepted/rejected Scene and Prefab behavior against the existing complete validation
  oracle, including format/byte/shape limits, field/reference errors, unavailable preservation,
  history refusal and failure nonmutation.
- [x] Run relevant regression/build checks and three unchanged optimized public measurement
  processes. Record timing, memory coverage, remaining costs and exact source identities.
- [x] Independently review and prepare the external HTML and precise integration changes.
  Final commit/push identity is recorded in the external HTML and delivery response.

## Outcome And Limits

The [report](../benchmarks/2026-10-20-scalar-preparation.md) records temporary phase attribution,
the private conservative publication-shape certificate with complete fallback, and touched-record
classification through the existing validator. Public encodings, synchronous success and history
authority remain unchanged. Scene/Prefab refusal and unavailable-preservation oracles pass.

Three uninstrumented public runs reduce 4,096-record edit-dispatch P50 from approximately
117-120 ms to 38-42 ms. A separate identical-probe comparison reduces requested allocation per
edit/Undo from 65.8 to 22.2 MB for Scene and 83.9 to 22.7 MB for Prefab in its simpler fixture.
283 owner tests, 17 public persistence tests and the locked all-feature/all-target workspace
check pass. The [qualification](../knowledge/engineering/verification/2026-10/2026-10-20T090217Z-scalar-preparation-locality-qualification-7da17674f8c7464ea1612dc119686c45.md)
binds checks, corrected diagnostic attempts, source identities and measurement limits.

The 8 ms investigation trigger is still exceeded. Full canonical encoding, candidate cloning
and O(N) state copying remain; unsupported grammar uses the original full validation. Further
scalar work needs attribution of the current path before selecting storage or format reuse.
This completed bounded correction does not close the larger responsiveness requirement.

L2 Browser preparation, L3 Tile locality and L4 Rust/Play attribution remain separately proposed
units. A timing improvement here does not establish native input latency, a release SLO or a
whole-engine performance guarantee.
