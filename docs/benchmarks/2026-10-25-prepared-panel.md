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
