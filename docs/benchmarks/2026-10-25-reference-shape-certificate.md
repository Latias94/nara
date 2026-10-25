# Finite Reference Publication-Shape Certificate

## Scope And Entry Criteria

This is the bounded successor to the
[reference-bearing phase attribution](2026-10-25-reference-preparation-phases.md), on
`0a05c6c` plus the two `publication_shape` files. It extends the existing private accept-only
certificate, not the public editing contract, file format, Schema, history or task model.

The prior report fixed the comparison before implementation: preserve complete-oracle acceptance,
shape and exact byte limits, canonical encoding and unknown data; observe zero parser-preflight
calls for the selected reference-bearing samples; and obtain populated debug preparation P50 at
most 60% of the matched control. The timing target is an experiment criterion, not a CI/native SLA.

The unchanged independent consumer uses 1/257-entity Scene/Prefab sources, a known reference-bearing
Link and an unknown component per entity, non-active documents, Direct/registered Panel entry,
eight warmups and 64 actual changed-value samples. Preparation includes public admission and
revalidation. Project setup, basis creation, caller assertions/readback/disposal and other paint
work remain outside its timers. Production-internal destruction remains inside the call.
See the [original workload and timing exclusions](2026-10-25-prepared-panel.md).

## Owning Replacement

The certificate now walks both closed `EntityReference` variants. A SceneLocal reference contains
its string ID; a Persistent reference contains its namespace and the actual display-serialized
UUID. The outer `ComponentValue` tag is `entity_ref`; the inner `kind` is a string in JSON **and
RON**, unlike the outer RON unit tag. UUID wire cost is its Display string, not 16 in-memory bytes.
Existing checked node/depth/container/string/total-string accounting and the wire-depth cutoff
of 32 remain unchanged.

Any unproved shape still selects the original complete JSON-then-RON fallback. Embedded Prefabs,
patches and over-cutoff values retain that path. Both actual encoders and their exact bounded-byte
checks execute even when certified. Certification skips parser preflight and also lets the
existing bounded writers avoid retaining encoded buffers; this comparison does not isolate those
two contributions or establish allocation-free preparation. Canonical envelope construction,
complete document traversal and both full encodings remain.

The reference-positive regression failed on the original implementation, then passed with the
candidate. Four reference shapes, declared/absent owner and Scene/Prefab exercise the force-false
full oracle. Independent dimension sweeps cover depth, nodes, container items, individual and total
string bytes; actual JSON/RON lengths are tested at length minus one, exact length and plus one.
The persistent fixture includes a maximum-length namespace and noncanonical accepted UUID spelling.
Cutoff and embedded-Prefab/patch fallback remain covered. Canonical serializers were not changed;
these tests are differential admission evidence, not independently authored canonical-byte snapshots.

## Clean-Source Public Comparison

Windows, explicit Rust 1.95.0, locked dependency graph, debug/test profile, shared root `target`,
`CARGO_BUILD_JOBS=1` and one test thread. No Cargo commands run concurrently in this checkout.
Other projects can build on the same machine; CPU scheduling, clocks and background load are not
controlled. Cases have a fixed order. No allocation or CPU profile was captured.

| Source / entities / mode | Original control P50 ms | Candidate 1 P50 ms | Restored candidate 2 P50 ms |
| --- | ---: | ---: | ---: |
| Scene / 1 / Direct | 0.2582 | 0.1178 | 0.1273 |
| Scene / 1 / Panel | 0.2826 | 0.1515 | 0.1505 |
| Scene / 257 / Direct | 30.2076 | 7.4451 | 7.5403 |
| Scene / 257 / Panel | 72.0624 | 7.5867 | 7.6455 |
| Prefab / 1 / Direct | 0.4857 | 0.1146 | 0.1118 |
| Prefab / 1 / Panel | 0.5532 | 0.1510 | 0.1483 |
| Prefab / 257 / Direct | 72.9438 | 7.5009 | 7.2857 |
| Prefab / 257 / Panel | 72.4554 | 7.4409 | 7.7900 |

All four populated cases in both uninstrumented candidate runs meet the predeclared 60% entry
criterion. The original control itself varies substantially: three cases here are about 72 ms,
while the previous restored control was about 31-36 ms. This is not an isolated or randomized
causal speedup estimate. Do not advertise a fixed 9-10x improvement, release/native latency or
general complexity from these observations. The narrower evidence is repeated candidate P50
around 7-8 ms on this selected workload, alongside direct branch/call evidence below.

| Execution | Nextest run ID | Scope |
| --- | --- | --- |
| Original control | `6b94b95e-fa2a-4e25-960e-737f66ae5890` | Original certificate; one diagnostic |
| Candidate 1 | `00f956cc-2996-4613-add5-d62fcd62c321` | Four ordinary public workflows plus diagnostic |
| Initial instrumented candidate | `d3ca9c26-eda9-46ab-b318-72beb4aa6379` | Timers and branch flags, no direct call counters |
| Counted instrumented candidate | `6337e42d-6bfa-4e3f-a9c8-16df0d1a6321` | Timers, flags and entered-scope call counters |
| Restored candidate 2 | `33ed1ffd-861b-4343-82ca-8a76e6d8a30a` | All temporary source removed; four public workflows plus diagnostic |

## Direct Fast-Path Observation

The counted run records all 576 owner calls, including warmups, in the exact eight-case order.
Every row has `certified=true` and `local_classification=true`. JSON and RON encoder scopes each
enter once per call; their parser-preflight scopes each enter **zero** times. This is stronger
than interpreting a zero-duration timer as a zero-call count. The earlier timer-only run does
not independently prove invocation counts.

Populated counted-run format scope median share remains about 90% of public preparation wall
time. Encoding remains material, but the timers do not establish CPU attribution or undisturbed
uninstrumented phase percentages. Those shares are not a reason to add a field cache, generic
incremental graph or asynchronous wrapper.

Seven instrumented originals were checked against recorded hashes before restoration, then
restored byte-for-byte. The sole owned temporary module was hash-checked and removed. Both frozen
candidate files remained unchanged; the restored public run recompiled Scene and the consumer.
No profiling module, environment-controlled production branch or permanent observation API ships.

Raw logs, public samples/quantiles, phase observations, instrumentation patch, temporary probe and
source/context manifests are retained under
[`data/2026-10-25-reference-shape-certificate/`](data/2026-10-25-reference-shape-certificate/).
The separate [scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-25T041346Z-finite-reference-shape-certificate-and-clean-source-public-native-qualification-d2673296c2bb413c9fd2b05bdf78a0c4.md)
records final owner/public/root/native/static results and the exact independent-review limitations.

## Boundary After This Unit

This meets the finite reference-certificate comparison, not full R2, delivery wave 1 or the
foundation Goal. Complete envelope construction and dual encoding remain whole-document work;
populated/degraded query-index maintenance and broader editing ergonomics are separate obligations.
The [pinned editing/format alternatives](../research/2026-10-25-editing-versus-format-admission.md)
remain distinct: an equivalent private proof must preserve current admission, whereas deferring
admission to Save needs an explicit editing/Save/repair contract redesign and owning ADR changes.

Do not expand this small optimization into a serializer/compiler framework. Further locality
work needs its own measured entry/stop conditions and must be weighed against completing the
remaining public authoring workflow and dependency-ordered foundation replacements.
