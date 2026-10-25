# Reference-Bearing Preparation: Owner Phase Attribution

## Question And Scope

Where does the roughly 30 ms debug preparation cost of a single scalar edit go when the source
contains real entity references and preserved unknown records? The
[prepared-panel comparison](2026-10-25-prepared-panel.md#owner-phase-experiment-entry) fixed the
workload, 90% attribution target and 10% disturbance investigation trigger before this execution.
This is diagnosis, not an optimized implementation, native-latency qualification or full R2 closure.

Source is `5ebe751cffeb34a93dbe0e023e0db863b5e638b7`. The existing ignored
`prepared_panel_property_cost_diagnostic` runs Scene/Prefab, 1/257 entities and Direct/Panel in
that fixed order, with fresh matched projects, eight warmups and 64 actual changed-value samples
per case. Each entity carries one known Link with `EntityReference::SceneLocal` and one preserved
unknown component. Existing success, exact value, history, retained navigation and final Undo/Redo
assertions remain unchanged. Setup, painting outside the measured panel calls and caller-owned
result disposal remain outside the public timers.

Windows, Intel Core i9-13900KF, Rust 1.95.0, unoptimized + debuginfo, shared root target,
`CARGO_BUILD_JOBS=1`, one test thread. The machine is not isolated. No allocation, CPU-time,
release-profile or native presentation measurement was made.

## Apparatus And Review

Temporary private instrumentation records the actual Scene/Prefab property-preparation call.
The five non-overlapping top-level phases are property lowering; document clone/instantiate plus
field-candidate preparation; whole format admission; classification plus unavailable-manifest
comparison; and forward/inverse history admission. Format contains separately recorded canonical
envelope construction, certificate, JSON and RON intervals; encoding/preflight intervals nest
within their respective format. Never add children to their parent when attributing the total.

The whole-format guard precedes the canonical envelope, so the envelope's final destruction is
included. Whole JSON/RON guards include their internal encoded buffers and parser disposal.
The canonical-construction child does not itself include the envelope's later destruction. A
parent-minus-children residual is not an isolated destructor measurement. Public-minus-Scene
includes ordinary adapter work and probe bookkeeping, not pure adapter overhead.

Thread-local rows buffer all observations; output occurs at test-thread exit. A temporary
post-case print retains the public samples before their ordinary quantile sorting. Each of eight
public cases has 64 ordered samples and exactly 72 owner rows, including warmups. Analysis checks
the complete fixed `(kind, entities, mode)` sequence, sequence numbers, owner kind/count and
parent/child timing bounds before mapping rows. It does not silently discard unexpected rows.

An independent read-only reviewer checked the apparatus and found one P2 analysis guard gap:
same-size Direct/Panel labels could be swapped without rejection. The analyzer now asserts all
eight tuples; a deliberately swapped-label input fails before writing a result. Both unchanged
instrumented logs were reanalyzed after this correction. No successful-path acceptance,
production result or production-object destruction-order change was identified by that review.
A second independent reviewer recomputed the retained metrics, both report tables, raw branch
counts, source/patch hashes and the swapped-label rejection; no new actionable issue remained.

The [evidence context](data/2026-10-25-reference-preparation-phases/context.json) contains source
hashes, all run IDs, summaries and artifact hashes. Its deterministic gzip artifacts retain the
five complete logs, exact temporary patch and corrected decision-local analyzer. They are
historical measurement evidence, not a supported profiling facility or product script.
The full reviewed numeric context is retained losslessly in `observations.json.gz`; the readable
context keeps compact case summaries and its hash instead of duplicating all control samples.

## Observed Attribution

Across both instrumented runs, all **1,152 owner calls**, including warmups, decline the shape
certificate and take local classification. Thus the field fast path actually runs; unknown
siblings do not force these samples through the full classifier.

Percentages below are medians of **within-call phase/public-total ratios**, not ratios or
differences between independent quantiles. P50/P95 use nearest rank, indices 31/60 of 64 samples.

| Run | Source / entities | Mode | Public prepare P50 / P95 ms | Format share | Classification/manifest share |
| --- | --- | --- | ---: | ---: | ---: |
| 1 | Scene / 257 | Direct | 30.9885 / 32.7216 | 97.25% | 1.77% |
| 1 | Scene / 257 | Panel | 38.3246 / 48.5388 | 97.15% | 1.56% |
| 1 | Prefab / 257 | Direct | 34.6047 / 45.0368 | 97.53% | 1.50% |
| 1 | Prefab / 257 | Panel | 38.3371 / 50.8525 | 97.42% | 1.47% |
| 2 | Scene / 257 | Direct | 30.7112 / 31.9730 | 97.29% | 1.72% |
| 2 | Scene / 257 | Panel | 43.2661 / 57.3841 | 97.39% | 1.41% |
| 2 | Prefab / 257 | Direct | 46.5017 / 56.9596 | 97.52% | 1.37% |
| 2 | Prefab / 257 | Panel | 36.7995 / 52.7121 | 97.46% | 1.48% |

For these populated cases, median within-call shares span approximately 12.38-13.32% for JSON
encoding, 10.25-10.79% for JSON preflight, 20.10-20.97% for RON encoding and 48.55-51.71% for
RON preflight. Canonical-envelope construction is 1.46-1.77%. These are separate nested phase
medians and must not be summed as if they were one observed call. They identify work to test,
not a predicted or measured speedup from deleting that work.

The five top-level phases cover at least 94.77% of each case's aggregate public time, and over
99.87% in each populated case. At the individual-call level, 1,023/1,024 measured calls exceed
90% coverage; one single-entity Direct Scene call covers only 79.39%. Nothing is trimmed. The
small-call residual remains unclassified, so this is not complete attribution of every adapter
call. The entry rule's lowering/candidate/history splits are included; no substantial populated
residual remains that would justify a different owner investigation before format work.

## Disturbance Investigation And Controls

Five serial successful executions are retained:

| Execution | Nextest run ID | Source / instrumentation |
| --- | --- | --- |
| Baseline | `0691f176-bcdd-4d5b-8255-d2c6ef70001a` | Original committed source |
| Instrumented 1 | `0dc6e1b9-6ca9-4415-9b84-376f3266bb43` | Temporary timers and branch observations enabled |
| Instrumented 2 | `40a91949-7aa8-4431-9328-8aba1ae592e9` | Same instrumented source and enabled recording |
| Disabled control | `0ee35f6b-7783-4e49-8d2b-a3e668ffc61d` | Same binary; recording disabled before process start |
| Restored control | `a474bfff-7faf-4e67-a58c-ebcebe77466e` | All original source restored byte-for-byte and recompiled |

Public preparation P50, in milliseconds:

| Source / entities / mode | Baseline | Instrumented 1 | Instrumented 2 | Disabled | Restored |
| --- | ---: | ---: | ---: | ---: | ---: |
| Scene / 1 / Direct | 0.2505 | 0.2503 | 0.2628 | 0.2554 | 0.2486 |
| Scene / 1 / Panel | 0.2876 | 0.2899 | 0.2920 | 0.2929 | 0.2854 |
| Scene / 257 / Direct | 31.5391 | 30.9885 | 30.7112 | 30.8707 | 30.8686 |
| Scene / 257 / Panel | 49.4311 | 38.3246 | 43.2661 | 32.4877 | 32.7195 |
| Prefab / 1 / Direct | 0.4969 | 0.2784 | 0.2583 | 0.4715 | 0.2673 |
| Prefab / 1 / Panel | 0.5106 | 0.5151 | 0.3362 | 0.2972 | 0.2999 |
| Prefab / 257 / Direct | 39.0294 | 34.6047 | 46.5017 | 38.4355 | 31.1054 |
| Prefab / 257 / Panel | 35.6264 | 38.3371 | 36.7995 | 45.0584 | 35.5921 |

Run 2's populated Direct Prefab median is about 19.15% above the original baseline, crossing
the entry investigation trigger. The disabled and restored controls show large changes without
an optimized implementation, including in small Prefab cases. Instrumentation versus background
load, cache, compiler-layout and scheduling contributions are **not isolated**; the trigger is
investigated, not declared a zero-disturbance proof. These controls do not establish an improvement,
regression or precise uninstrumented phase percentage. The repeated observations locate
wall-clock occupancy in the instrumented format boundary; they are not a CPU profile or latency SLA.
The same-binary control is supported by unchanged-source/no-recompilation logs; an executable
digest was not captured, so the evidence is not independent binary-identity authentication.

## Causal Boundary And Next Comparison

The actual path lowers a declared property, prepares a field candidate and invokes
`format::validate_publication_shape`. That function constructs the canonical envelope, tries the
accept-only certificate, then performs complete JSON and RON encoding and fallback parser
preflight. `publication_shape.rs:155` declines `ComponentValue::EntityReference`; these real Link
records therefore take that fallback. The measured format scopes dominate, while the actual
local classification branch remains small. The measurements do not justify replacing ECS,
classification storage, history or the task kernel to fix this particular cost.

The next bounded comparison should start at the format owner, not with a per-field cache or an
async wrapper. Test finite reference-shape certification against the unchanged full oracle;
it could avoid repeated parser preflight without changing current admission. It still performs
both full encodings, so passing that comparison would not establish complete edit locality.
If encoding remains material, compare a conservative format-owned accept-only byte bound with
full fallback before adopting exact retained fragments or a persistent summary/index. This is a
next experiment, not an implemented certificate or a selected production authority.

Keep the [editing/format contract alternatives](../research/2026-10-25-editing-versus-format-admission.md)
open. Exact private proof and a deliberate editing/Save/repair redesign solve different contracts.
The existing edit-time dual-encoding contract is not optimal merely because it is Accepted, but
deferring its checks to Save cannot be described as an equivalent optimization. Reconcile the
owning ADRs and prove the complete repair/history/persistence workflow before changing guarantees.

For the finite certificate comparison, record entry criteria before implementation: unchanged
full-oracle acceptance and canonical bytes, all relevant shape/byte boundary cases and preserved
unknown data; zero parser-preflight calls for the selected admitted reference-bearing samples;
and a target populated-case debug preparation median at most 60% of its matched current control.
The latency target is an experiment criterion, not CI or native qualification. Failure must remain
visible; do not weaken limits or replace the workload to meet it. A broader cache/representation
or Save-policy change needs its own comparison rather than silently widening this unit.

## Final Source State

All seven touched original files match their saved pre-experiment bytes. The sole temporary
module is removed. The archived patch forward-checks against restored `5ebe751` source, and the
restored diagnostic recompiles Scene and its public consumer and passes. No production code,
permanent feature, public profiling API, acceptance policy or candidate/history mechanism is
changed by this attribution. Full R2 and the foundation Goal remain open.
