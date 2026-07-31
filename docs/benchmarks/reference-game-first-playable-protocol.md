# Reference-Game First-Playable Evidence Protocol

This document explains the version-1 decision contract used by RGF-U22. The machine authority is
[`data/protocol/v1/reference-game-first-playable.json`](data/protocol/v1/reference-game-first-playable.json).
Its canonical UTF-8 bytes include the final LF and are bound by the adjacent BLAKE3 sidecar. The
current protocol is 50,376 bytes with digest
`82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.

The protocol is a pre-target product budget, not a benchmark result or a performance claim. It was
frozen before RGF-U4, U5, U12, U24, or U25 implementation existed. Later units may bind a semantic
subject to a concrete public entry point, but that binding is outside the digest and cannot change
the subject, metric, source, range, sample floor, environment class, invalidation rule, decision
rule, or evidence-envelope policy.

## Decision Subjects and Owners

| Subject | Owner | Purpose |
|---|---|---|
| `u14.headless_iteration` | U14 | Headless wave, Rust/data iteration, module use, build, and coverage |
| `u14.desktop_product` | U14 | Manually playable desktop path, slot setup, frame time, and memory |
| `u20.release_candidate` | U20 | Exact approved candidate size and startup |
| `u22.calibration_review` | U22 | Pre-target source and budget review only |
| `u26.manual_counterfactual` | U26 | Frozen raw-App ownership baseline |
| `u25.ownership_comparison` | U25 | Current-source Host versus manual ownership decision |

U26 owns the immutable counterfactual record. U25 owns the paired decision metrics. U14 and U20
must not rewrite U22 ranges after seeing results.

## Frozen Sources

Version 1 has three source identities in the revision- and digest-bound
[`first-playable-product-budgets.json`](data/sources/v1/first-playable-product-budgets.json).
That exact artifact is separately approved by the two-scope
[`first-playable-product-budget-review.json`](data/sources/v1/first-playable-product-budget-review.json),
which binds the separate
[`performance-measurement-review.json`](data/reviews/v1/performance-measurement-review.json) and
[`protocol-provenance-review.json`](data/reviews/v1/protocol-provenance-review.json) evidence files.
The protocol binds the source and aggregate review digests plus their common source revision. The
adopter and reviewers are distinct identities, both reviewers attest that they did not see
U4-or-later target results, and the validator enforces adoption-before-attestation-before-review
ordering.

These values are intentionally normative product constraints rather than observed Nara baselines:

- `strategy_productivity_v1` bounds how long a first-playable authoring task may take before the
  foundation route is redirected. It does not claim parity with another engine.
- `strategy_runtime_v1` bounds the narrow 2D PC reference-game runtime and release candidate. It is
  not a general engine SLA.
- `zero_tolerance_contract_v1` requires complete public-path success and rejects ownership or
  lifecycle correctness gaps.

The product-budget artifact owns every target. The protocol owns the matching metric population,
sample floor, environment class, collector, and decision suite. Tests require a one-to-one metric
and target match across both artifacts, so neither can silently drift. This document intentionally
does not repeat the normative values. A failure is evidence to simplify or redirect, not permission
to select a friendlier range after the fact.

The baseline-after-threshold rule still governs empirical, comparative, and future
baseline-derived thresholds: a baseline describes the implementation that produced it. U22 v1 is
the narrower pre-target exception for product constraints and correctness hard stops. A
result-informed change creates a new version and cannot judge the result that caused the change.

## Measurement Boundaries

Every budget entry carries a rationale, semantic subject, environment class, statistic,
population, workload, start and end boundary, measurement method, and required context. The most
failure-prone scopes are fixed as follows:

| Scope | Frozen definition |
|---|---|
| Candidate size | Measure the exact compressed archive and validated unpacked regular-file tree as separate required metrics, each bound to candidate identity and digest. |
| Candidate startup | Pair headless process creation to first authoritative tick with desktop process creation to first playable present; each sample records the slower duration. |
| Frame P99 | Use the pressure-wave scene at 1920x1080 after 600 warm-up frames, with adapter/driver and present mode recorded and VSync disabled. |
| Process memory | Measure Windows peak working set or Linux RSS high-water mark from runtime publication through the terminal frame; device-local GPU resources are excluded. |
| GPU resources | Measure the peak bytes of backend-owned tracked resource categories for one device epoch; unobservable driver-wide residency is explicitly not claimed. |
| Edit latency | Start when the declared data/body/structural edit is committed and end only when its authoritative result is observed, including the selected reload, patch, rebuild, or restart route. |

The frozen ownership task resolves one plan, consumes one content snapshot, starts and observes one
fixed tick, requests bounded stop, and proves registered-obligation retirement. Its eight injected
fault cases cover plugin preparation, registry freeze, scene spawn, startup hook, fixed-tick queue,
admission cancellation, shutdown deadline, and provider retirement. Caller glue is measured as a
positive Host-over-U26 delta; Host-only concepts require a named closed ownership/fault gap; every
lifecycle state needs exactly one owner. Ambiguity is `Redirect`. Public production coverage uses
the frozen inventory of all engine-facing calls in the supported slice as its denominator.

The lifecycle graph starts at `candidate`, admits only the frozen transitions, makes `stopped` a
terminal state, and requires every declared state to be reachable from `candidate` and able to
reach `stopped`. U26 must publish exactly one baseline record for every ownership metric. U25
candidate evidence must bind that baseline, one trusted U24 candidate digest, the correctness and
fault contracts, the lifecycle graph, and one independent reviewer attestation. The generic
aggregation and decision entry points reject `ownership_gate`; only the ownership-specific entry
can carry this admission.

## Outcome Rules

The evaluator is deterministic:

1. A failed hard-stop metric produces `Stop`, even if other metrics would produce `Redirect`.
2. Any other missing or failed required metric produces `Redirect`.
3. Only complete passing required evidence for the evaluated suite produces `Continue`.

`ownership_gate`, `first_playable`, and `candidate_gate` are evaluated independently by U25, U14,
and U20. A collector never needs to fabricate evidence owned by a later unit.

Every metric must name a collector, source, target, minimum sample count, population, and
environment class. A missing source or raw-record floor cannot produce `Continue`. Percentile-only
summaries without their bounded raw records are invalid evidence.

## Raw-Sample Aggregation

The protocol freezes `nearest_rank_v1` for P50/P95/P99. Samples are sorted ascending, the one-based
rank is `ceil(percentile * sample_count / 100)`, and that observed value is selected without
interpolation. `all_equal_v1` governs Exact metrics: every admitted raw value must agree. Sample
indices are unique and contiguous from one; counts are derived from validated records rather than
collector-supplied summaries. Duplicate, gapped, mixed-environment, cross-suite, or inconsistent
Exact samples reject before suite evaluation.

## Environment Equivalence

The protocol records seven normalized fields: OS, runner image, Rust toolchain, CPU, GPU or software
adapter, build profile, and collector version. Each metric names the subset that must match. Empty,
unknown, or out-of-class required fields force recollection. Cold and warm populations are distinct
aggregation keys and are never combined.

`windows-latest`, `stable`, and `debug` are not sufficient identities. A collector must bind an
immutable runner image where applicable, the actual `rustc -vV`/Cargo toolchain, CPU/GPU class,
complete build settings, and collector implementation version. The committed calibration fixture
records Rust 1.97 on Windows; it is not equivalent to the workspace MSRV merely because both builds
compile.

## Source Invalidation

All matching path rules are expanded and unioned. Rule order never chooses a winner. Changes below
`src/` or `crates/`, Cargo/build/workflow configuration changes, a protocol-digest mismatch, an
invalid path, or an unknown path invalidate the complete suite. Rename checks include both old and
new paths. Reference-game source changes invalidate both first-playable and ownership evidence;
content, settings, and module-consumer changes invalidate their declared suites.

Version 1 intentionally has no `NoImpact` rule. Over-invalidation costs a rerun; under-invalidation
can approve the wrong engine state.

Cross-revision reuse requires an opaque admission created from an explicitly supplied clean Git
repository root. The admission verifies exact HEAD, ancestor and merge-base relationships, and the
complete `git diff --name-status -z` manifest and digest. There is no constructor for a collector to
self-report a current revision. Git paths retain legal UTF-8 spaces, Unicode, and lengths outside
the evidence-identifier grammar; an unsafe or unrepresentable path forces full invalidation rather
than selective reuse. Repository override environment variables are removed before Git proof.

## Evidence Envelope

Collector output is untrusted data, including output from a credential-free first-party job. The
trusted caller supplies the exact transfer path/table/size/digest plus expected generator, run,
source, protocol, subject, complete normalized environment fields, and raw-log references
independently of the envelope. Ingestion performs these stages in order:

1. Reject missing, aliased, linked, special, oversized, or unexpected transfer-table entries.
2. Check the encoded-byte ceiling and outer transfer digest.
3. Preflight serde depth, nodes, container items, single/total string bytes, and duplicate keys.
4. Preflight record, per-record/aggregate field, and raw-log counts before typed decode.
5. Decode the strict versioned type shape and compare every trusted identity/environment field.
6. Validate the subject-owned record, metric, population, peer, field, and relative-path catalogues.
7. Verify compact canonical payload bytes and their digest.
8. Return an unpublished validated candidate; the trusted entry publishes it only after all checks.

Fields are typed as identifier, project-relative path, integer, boolean, digest, or value-free
sensitive/secret marker. Identifier grammar is not a privacy boundary: every dynamic identifier is
also matched against a protocol-owned semantic catalogue or an independently trusted expectation.
There is no arbitrary string or JSON value. Sensitive and secret markers cannot carry raw data, a
hash, or an original length. Unknown fields, commands, URLs, credentials, host paths, links, special
files, traversal, aliases, and unexpected transfer entries reject with static errors. Raw logs
remain access-controlled, retention-bounded artifacts referenced only by the exact trusted opaque
identity and digest list.

The canonical calibration envelope is
[`data/envelope/v1/calibration-review.json`](data/envelope/v1/calibration-review.json). It records
the three source approvals, source/review identities and digests, source revision, and
`target_results_seen = false`; it contains no U4-or-later sample.

The collector/evaluator boundary is intentional. RGD-U9's committed
`reference-game/tools/measure_first_playable.py collect` command creates an isolated worktree,
executes the bounded automatic measurements, and writes raw sample and diagnostic-log transports.
Its `verify` command checks only local source identity, byte budgets, digests, and the minimal
transport shape. The executing helper must come from the measured subject and byte-match the helper
blob at HEAD; `.gitattributes` pins the helper to LF so this check is stable across checkouts. The
collector digest covers those executed bytes.
Diagnostic logs are explicitly non-canonical and are not part of the integrity graph.

Rust policy and oracle code owns environment compatibility, metric-catalog interpretation,
aggregation, invalidation, and the final `Continue` / `Redirect` / `Stop` verdict. U25 and U26 reuse
that Rust route. The Python helper does not accept evidence or implement a second decision protocol;
it remains a one-shot collection tool rather than a production benchmark framework or CLI.

## Non-Claims

- U22 does not implement a benchmark runner, telemetry exporter, histogram service, or production
  evidence API.
- These budgets do not establish Bevy, Godot, or Fyrox parity.
- The offline envelope is not `RuntimeDiagnostics` or `RuntimePressureSnapshots` and cannot feed
  gameplay or overload policy.
- Protocol ancestry is later verified against landed commits; the in-memory policy fixture only
  freezes the required U22 -> target and U26 -> U24 -> U25 relationships before those commits exist.
