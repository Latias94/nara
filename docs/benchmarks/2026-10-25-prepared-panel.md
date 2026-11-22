# Prepared Panel Cost And Responsibility Comparison

## Experiment Entry

Compare the existing direct Application property-edit path and the implemented registered-panel
path on the same Windows host, Rust 1.95.0, debug profile, root target and one Cargo build job.
The operation replaces one declared integer in a named non-active Scene or Prefab. Small and
populated/degraded sources must be distinguished: grow actual known component records and
preserved unknown records, not only empty entities. Record exact fixture counts with the result.

Use eight warmups and 64 measured samples per mode/source/size. Construct a fresh matched project
for each case, run control-first in fixed order, and alternate changed values so the operation
cannot silently become a no-op. Setup/admission, intent and captured-basis construction and
result disposal stay outside each measured interval. This is a diagnostic, not a timing test gate.

Keep three boundaries separate:

1. Direct preparation versus actual panel-context preparation. The latter includes its complete
   retained query-basis check; these are comparable workflows, not identical instruction counts.
2. Panel typed enqueue. This moves the existing candidate into the normal submission and must
   not re-prepare it. It is not source commit.
3. Direct consuming commit plus ordinary Application drive versus panel frontend dispatch plus
   the same drive. This is not owner-only publication, native rendering or end-to-end tool latency.

Before execution, use greater than twice the same-case direct P95 for preparation or
dispatch/drive as an investigation trigger. Repeat and attribute such an observation before
changing ownership or choosing async execution; never fail noisy CI or weaken admission on it.
No numeric native latency, RSS, allocator-isolation or index-maintenance speedup claim follows.

The final report must include true applied results, one Undo entry per accepted edit, unchanged
non-active navigation, the precise cost test, source/run IDs, sorted samples and quantile rule.
Unmeasured paths and differences between control and candidate remain explicit. Full R2 and the
foundation Goal are not closed by one diagnostic comparison.

## Results

Measured on an Intel Core i9-13900KF, Windows, Rust 1.95.0, unoptimized + debuginfo, on
uncommitted source based on `7638409`. Run 1 is nextest
`a69bd72c-77d2-4470-a577-fea2e5f5753d` (four ordinary consumer workflows plus the diagnostic pass).
Run 2 is `5d46804d-572f-400b-9db2-1ad2db3b5895` (the repeated diagnostic passes). Both use one test
thread. Each source has 1 or 257 entities, each carrying an actual `consumer.Link` and an unknown
record, including the existing `anchor`. Every Link retains a real source-local entity reference.

Quantiles use nearest rank: sorted index `ceil(64 * p) - 1`, so P50 is index 31 and P95 index 60.
The table records P50 / P95, not independent extrema or differences between quantiles.

| Run | Source / entities | Direct prepare ms | Panel prepare ms | Direct commit/drive us | Panel dispatch/drive us | Panel enqueue us |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Scene / 1 | 0.2423 / 0.3193 | 0.2807 / 0.3670 | 30.3 / 43.3 | 58.9 / 66.7 | 1.1 / 1.5 |
| 1 | Scene / 257 | 29.6533 / 30.3401 | 29.7246 / 30.8321 | 525.1 / 643.7 | 589.9 / 731.5 | 4.1 / 4.8 |
| 1 | Prefab / 1 | 0.2468 / 0.2714 | 0.2803 / 0.4858 | 30.2 / 36.5 | 58.7 / 75.5 | 1.3 / 1.6 |
| 1 | Prefab / 257 | 30.2598 / 45.5313 | 29.7939 / 32.0765 | 587.4 / 795.7 | 569.8 / 753.6 | 4.3 / 5.1 |
| 2 | Scene / 1 | 0.2551 / 0.3050 | 0.2852 / 0.3225 | 32.6 / 45.2 | 59.8 / 72.7 | 1.1 / 1.6 |
| 2 | Scene / 257 | 30.4066 / 31.0231 | 30.8360 / 37.0351 | 560.8 / 704.1 | 621.2 / 828.4 | 4.4 / 5.2 |
| 2 | Prefab / 1 | 0.2765 / 0.5370 | 0.2815 / 0.4011 | 35.0 / 69.6 | 58.8 / 86.8 | 1.2 / 1.9 |
| 2 | Prefab / 257 | 30.7415 / 33.8952 | 32.1594 / 53.0697 | 575.0 / 724.6 | 693.4 / 1103.5 | 4.5 / 5.9 |

Run 1's small Prefab dispatch P95 crosses the entry trigger: 75.5 us is about 2.07 times 36.5 us.
The repeated ratio is about 1.25 (86.8 / 69.6 us), not another crossing. The panel path includes
frontend admission and callback bookkeeping omitted by the direct control; the higher small-case
median remains visible in both runs. This does not isolate that overhead from cache/scheduling
effects or establish a throughput/native-latency regression. No other preparation or consuming
P95 ratio exceeds two in these observations. A lower panel tail in one row is not a speedup.

The dominant observed scaling is shared preparation: about 30 ms for 257 entities carrying
257 known Link components and 257 preserved unknown components in both interfaces, rather than
the 1-6 us typed enqueue. This is important owner work to
attribute, not evidence that synchronous preparation is generally responsive or that an async
wrapper fixes it. It also does not prove the cost of any individual validation/copy/index phase.
The two sizes and debug profile do not establish complexity or release behavior.

The diagnostic is
[`prepared_panel_property_cost_diagnostic`](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prepared_panel_cost.rs).
It asserts actual success, exactly one history entry per edit, exact value readback, retained
navigation and final Undo/Redo. Candidate/history tests and ordinary Save/Reopen consumer evidence
remain separate. Sorted sample arrays are retained in the per-user temporary logs
`nara-r2-panel-cost-and-public.log` and `nara-r2-panel-cost-repeat.log`.

```text
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/module-consumer/Cargo.toml --target-dir target -p nara-editor-panel-module-consumer --test prepared_authoring --run-ignored only -E 'test(prepared_panel_property_cost_diagnostic)' --test-threads=1 --success-output final
```

## Shared Owner Attribution Follow-Up

Read-only tracing confirms this public property path already enters the field-candidate path;
it is not a missing fast-path call in the panel adapter. Stable-property lowering copies and
validates the touched component before producing root field operations. The field path retains
full document/canonical-envelope work, forward/inverse admission and classification-state copying.

The exact Link fixture contains `EntityReference::SceneLocal`, which the current publication
shape certificate declines. Format validation therefore retains the encoded JSON/RON buffers
and parser prechecks. Certificate success would still perform both full encodings. Local
classification can reuse admitted facts independently of this format decision; unknown siblings
do not by themselves force full classification. At this source-tracing checkpoint actual branch
counters were not measured. The subsequent
[phase attribution](2026-10-25-reference-preparation-phases.md) now records the actual branches
and disposal-inclusive timing; do not reinterpret this earlier trace as that dynamic evidence.
See `crates/nara_scene/src/authoring/property.rs`,
`crates/nara_scene/src/authoring_decode/field_candidate.rs`,
`crates/nara_scene/src/format/publication_shape.rs` and the
[earlier scalar attribution](2026-10-20-scalar-preparation.md).

The following experiment entry required this real 257-Link/257-unknown source, owner format
validation versus classification/manifest work within the actual preparation call, branch evidence
and per-call total timing. Its [execution](2026-10-25-reference-preparation-phases.md) locates the
populated instrumented wall time in format admission, not full classification. Include
canonicalization and internal disposal in their owning phases;
do not allocate time by subtracting unrelated P50s, remove references/unknown data, weaken the
format limits, or choose an async facility before attribution. This remains an explicit R2
shared-owner locality obligation, not a reason to mislabel the bounded panel transport complete
interactive-performance qualification.

### Owner Phase Experiment Entry

Before changing the owner, reuse this diagnostic's exact reference-bearing 1/257-entity
Scene/Prefab sources, eight warmups and 64 changed-value samples. Keep the Rust 1.95.0 debug
configuration and root target. Attribute these intervals within each actual preparation call:

- Whole public preparation, including property lowering and internal result/candidate disposal.
- Whole format admission, including canonical-envelope construction and destruction, exact JSON
  and RON encoding, and any parser preflight. Record certificate success/fallback explicitly.
- Candidate classification plus unavailable-manifest comparison. Record local/full classification
  explicitly rather than inferring it from the fixture.

Buffer observations and emit them outside the timed calls. Keep parent and child intervals
distinct; do not add nested intervals twice or subtract unrelated quantiles. The attribution
target is at least 90% of measured preparation time covered by non-overlapping owner phases.
If format and classification leave more than 10% unexplained, split property lowering, document
clone/field mutation and inverse/history admission before selecting an optimization. An increase
above 10% in matched instrumented versus uninstrumented median preparation requires investigating
measurement disturbance, not reporting it as product regression. These are diagnostic stop rules,
not an interactive-latency SLA or a performance win criterion.

Do not add a permanent public profiling API, generic job framework or persistent instrumentation
feature for this experiment. A selected owner correction must keep the existing admission oracle,
including exact encoded-byte/shape limits, unavailable-record preservation, inverse replay and
history publication. Any proposal to defer those guarantees until Save instead is a separate
contract decision, not an optimization of the current contract. Optimized-build and actual native
latency remain separate measurements before making a responsiveness claim.

### Reference-Engine Contract Comparison

The [pinned Bevy/Godot source study](../research/2026-10-25-editing-versus-format-admission.md)
finds no whole-document serialization in the inspected ordinary scalar-edit paths. Bevy writes
live reflected ECS values without Nara's source revision/history contract; Godot records property
do/undo operations and packs/encodes at Save. Neither path establishes Nara's current exact
dual-encoding admission guarantee, so this is not evidence of an equivalent cheaper algorithm.

Private exact summaries/fragments, a smaller reference-shape certificate extension and an explicit
editing/Save contract change remain distinct alternatives. The study's first experiment under the
current contract is not a verdict that the contract is globally optimal. In the fearless rebaseline,
ask whether coupling every accepted source edit to both wire formats earns its implementation,
retained-memory and latency costs. A different contract needs a complete repair/failure workflow
and owning ADR reconciliation; it must not arrive disguised as an equivalent performance patch.
Choose after phase attribution and a fair same-workflow comparison, not from reference names alone.

## November 20 Release Baseline

The original diagnostic at `0d3e76f` reports the current optimized-build cost on Windows x64,
Rust 1.95.0. It uses the same 1/257-record Scene and Prefab files with real source-local
entity references and unknown components, eight warmups and 64 changed-value samples per case.
The release test binary was built before the preview changes entered its dependencies. Direct
execution selected the ignored test by its full Rust test name after the initial exact nextest
filter selected no tests. The diagnostic passed with the existing edit, dispatch and history
assertions.

Each public preparation result reports nearest-rank P50 / P95 in milliseconds.

| Source / entities | Direct | Registered panel |
| --- | ---: | ---: |
| Scene / 1 | 0.0182 / 0.0196 | 0.0224 / 0.0380 |
| Scene / 257 | 1.8401 / 1.9858 | 1.9136 / 2.2952 |
| Prefab / 1 | 0.0180 / 0.0205 | 0.0219 / 0.0400 |
| Prefab / 257 | 2.0722 / 3.1634 | 1.9411 / 3.0163 |

For 257 entities, direct commit and drive measures 182.4 / 227.2 µs for Scene and
223.5 / 426.7 µs for Prefab. The panel dispatch and drive measures 219.9 / 287.7 µs and
223.0 / 464.8 µs, respectively. Typed panel enqueue ranges from 0.8 to 0.9 µs at P50.

The larger case still requires whole-document work. The preparation result supplies a scale
reference for the prepared-preview workflow and retains both format encoders and the original
admission policy. The measurement excludes native presentation and covers a narrow source-size
range. The earlier debug diagnostics use a different profile and source revision.

The same diagnostic with the prepared-preview successor passes as part of release run
`5b17f2c8-5adc-4ebf-a467-882ce32bea75`. The four 257-entity preparation P50 measurements range
from 1.6479 to 1.8460 ms, with P95 from 1.8996 to 2.0270 ms. The timers cover the existing
prepare-and-commit workload. Candidate reads fall outside those timers. Both runs use the same
fixed case order under ordinary machine load, so their variation provides a scale reference.

## November 22 Keyed Collection Workload

The independent `prepared_authoring` target edits a scalar beside a keyed collection in an
open, non-active Scene or Prefab bound to JSON. Each edit replaces an equal-length note in
the same component as the collection. The 257- and 32,768-row cases use the same shape and
retain an unknown component. The Direct diagnostics use `EditorApplication`. The Panel
diagnostics use a registered `EditorPanelHost` and its actual typed submission. Both paths
measure a last-row point read on the live source and on the exact candidate, then consume
the candidate through the normal history owner. The Panel records enqueue separately.

The diagnostic excludes fixture construction, query-basis and request construction, assertion
work, Save, Reopen, and native presentation from the timed calls. It takes eight warmups and
64 samples per case. Percentiles use nearest rank. The raw samples retain their acquisition
order to help distinguish drift within a case from differences between cases. The source
file is 113,898–113,899 bytes at 257 rows and 14,495,295–14,495,296 bytes at 32,768 rows.
Each timed last-row read visits five scan entries and charges 73 logical bytes on both sources
and both candidate sizes. Other selectors and enumeration have separate costs.
[Diagnostic source](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages/baseline.rs)

From the repository root, use the shared `target` and one build job.

```powershell
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
$env:CARGO_BUILD_JOBS = '1'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-editor-panel-module-consumer --test prepared_authoring -E 'test(property_pages::baseline::)' --run-ignored only --test-threads=1 --no-tests fail --success-output immediate
```

Windows x64 release run `f87e0c1f-f693-456b-98b2-514483a8cf2d` passes all 38 tests in the
target, including the eight keyed diagnostics. The engine code is `0a38771`. P50 / P95 are
milliseconds for the real prepare and commit/drive calls.

| Source / rows | Direct prepare | Panel prepare | Direct commit/drive | Panel submit/drive |
| --- | ---: | ---: | ---: | ---: |
| Scene / 257 | 0.412 / 0.490 | 0.415 / 0.431 | 0.020 / 0.050 | 0.020 / 0.024 |
| Prefab / 257 | 0.404 / 0.453 | 0.443 / 0.529 | 0.019 / 0.035 | 0.024 / 0.077 |
| Scene / 32,768 | 193.305 / 216.456 | 182.684 / 202.950 | 14.484 / 18.103 | 14.218 / 17.774 |
| Prefab / 32,768 | 180.858 / 206.857 | 188.067 / 209.387 | 14.043 / 16.721 | 14.453 / 17.179 |

At 32,768 rows, live last-row reads take 4.7–5.0 µs at P50 and 5.5–6.4 µs at P95. Candidate
reads take 9.2–9.4 µs at P50 and 11.0–11.8 µs at P95. Panel enqueue takes 1.1–1.6 µs at P50
and 1.5–2.2 µs at P95. A separate eight-diagnostic pass in run
`90f5ac2f-2b9c-4927-8fb4-a91ba57cef22` places the four large preparation medians at
186.067–191.888 ms. In both subsequent chronological captures, the large cases shift from
roughly 80–95 ms to sustained higher occupancy around the thirtieth preparation, including
warmups. The shift's cause remains unisolated. The results give a scale for repeated editing
of this workload with its growing history, including source copying, file admission and
classification. They establish neither individual phase costs nor a native gesture frame budget.

### Borrowed Candidate Comparison

The field-candidate owner already orders Scene entities before publication admission. Its
Prefab projection moves those same entities and placements back under their original inputs.
The format owner now borrows that ordered payload for ordinary field edits and their history
replay, using the same path as budgeted candidate admission. This removes a complete document
copy and a second entity sort. It still checks the domain budget, canonical encoding and exact
byte limit, and uses the original parser preflight when the shape certificate declines the input.

The matched release comparison uses the same Windows x64 Core i9-13900KF, Rust 1.95.0 and
32,768-row registered-panel workload from the baseline. Two fresh test processes for each
source and variant run serially, reversing variant order on the repeat. Each run has eight
warmups and 64 samples. The table gives the range across the two processes in milliseconds.

| Source | Baseline P50 | Borrowed P50 | Baseline P95 | Borrowed P95 |
| --- | ---: | ---: | ---: | ---: |
| Scene | 185.0–185.5 | 150.4–152.6 | 201.7–205.4 | 172.9–185.6 |
| Prefab | 183.6–185.6 | 147.4–156.3 | 201.1–209.3 | 169.0–171.6 |

The early sample window, 1–16 after warmup, has medians of 83.9–90.0 ms on the baseline and
69.3–72.5 ms on the borrowed path. The late window, 33–64, has medians of 191.1–194.1 ms and
155.7–169.4 ms, respectively. The repeated-workload shift remains visible. Each diagnostic
still spends time creating the owned candidate, writing the admitted format and classifying
the touched component. This comparison measures repeated source preparation; the absolute
time remains material for a continuous interaction.

Release nextest `91f4db7b-c007-4b51-b9eb-8eaa6ec165be` also passes 38/38 on the changed
independent target. The four large Direct and Panel cases take 145.0–151.3 ms at P50.
The 257-row cases take 0.366–0.422 ms at P50. The format and field-candidate cohort passes
32/32 in `8f34a00d-632c-4c32-b031-02266bf39af3`. Its cases compare the borrowed check to
the complete format oracle, including precise byte limits and parser fallback, and exercise
atomic rejection and exact history through the actual field-candidate owner.

Nextest `770a9b79-96e5-4eae-9d4f-ab72dc746a46` passes the explicit JSON/RON selection control
and the native Windows/DX12 Drawn workspace trajectory. The latter includes actual Inspector
draft cancellation, submission and history, along with the workspace's source persistence checks.
Scene correctness Clippy and workspace formatting checks pass.

## Caller Responsibilities

The direct Application and registered-panel interfaces share the source-owned candidate,
inverse/history admission and consuming commit. The panel no longer needs a private preparation
hook, its own dry-run validator/inverse, or a Host candidate registry. Its local
`Option<PreparedEditorDocumentEdit>` is an unpublished operation, not another document.

The panel instead owns its preview inputs, retained candidate and exact submission ID. Capacity
refusal returns that candidate. A successful queue entry is still distinct from owner acceptance;
historical observations are not current existence authority. Construction has the same rule for
its fixed local-to-stored mapping. Several queued operations remain ordered, not atomic together.

For metadata-driven discovery, the current consumer uses component/root/read action-result
rounds before preparation; it does not demonstrate fewer calls or paints than direct Application
access. Context preparation is synchronous and read-only, while commit is deferred. The R1
placement and inline-tool comparison must still evaluate whether that read choreography is the
right native-tool surface; neither a universal async SDK nor native query latency is established.

## References

- [Prepared-panel design](../architecture/editor-prepared-panel-design.md)
- [Implementation checkpoint](../knowledge/engineering/progress/2026-10/2026-10-25T011645Z-prepared-panel-implementation-and-scoped-public-qualification-checkpoint-2b1d6218ef0042ae8c83303e6aabfbf1.md)
- [Earlier retained-construction owner comparison](2026-10-25-retained-construction.md)
