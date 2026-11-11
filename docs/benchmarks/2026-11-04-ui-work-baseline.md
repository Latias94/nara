# UI Work Observation Baseline

Status: partial R11a baseline, beginning at `3bcb83e` with source revisions and later CPU/native
observations identified below. This is not a complete R11a baseline or a performance improvement.
The [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
still requires full native allocation/retention, upload/GPU attribution, OS input/display latency,
remaining matched workloads and numeric budgets before measured reuse.

## Observation Contract

The later [maintained-framework compositor study](../research/2026-11-11-open-gpui-engine-integration-layers.md#compositor-and-cache-research-direction)
defines a new comparison, not a continuation of these measurements. Its GPUI consumer must
separately observe description/layout/prepaint/paint, actual recorder staging/upload, GPU raster,
final composition and retained payload. Existing Host native-work counters exclude third-party
renderer callbacks; zero there cannot establish zero GPUI upload. Queue-completion spans remain
distinct from GPU timestamps. The correctness-only same-Scene diagnostic replay is outside the
normal measured workload. The later [hosted GPUI probe](#hosted-gpui-source-and-frame-cost-probe)
supplies scoped current-source CPU/allocation observations, not a complete baseline or cache win.

CPU reuse and GPU raster reuse have separate acceptance. Direct or retained-geometry rendering
can legitimately redraw unchanged primitives at each presentation. A raster-cache experiment
must establish reduced original raster work together with composite/rebuild/memory cost, using
cache-off/on controls with identical group-opacity, clipping and interaction semantics. Apply
only supported feature controls to the selected experiment; the supplied broad matrix does not
make all platforms, HDR or future thread transport prerequisites of an SDR cache comparison.

The [state-locality comparison](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#state-locality-and-compositor-comparison)
also needs separate dependency/projection work, notifications, rebuilt Views and reached layout
nodes. A leaf-only reactive effect count does not establish leaf-only total UI work. Report
subscription and retained-state occupancy through close/recreate and mounted-key churn. Separate
fully materialized rows from virtualization and equal writes from real edits; keep correctness
and authoring effort alongside time/memory. Article charts and upstream experiments do not
supply any of these current Nara measurements.

The first [shared-model consumer](../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-shared-model-editor-projections)
now qualifies public CPU render/paint callback differences and an isolated native source edit
that reuses unchanged Hierarchy rendering. It retains separate real Inspector text/Apply/Undo
and source-removal/Redo workflows. These are executable work-count/correctness observations;
the native trajectory includes forced recovery, focus, refusal and scale changes, so its aggregate
callback/submission counts are not an incremental-layout ratio or a timing baseline. No old
Dear/kernel timing or current GPUI total cost is inferred from these passes.

The subsequent [collection consumer](../research/2026-11-11-open-gpui-engine-integration-layers.md#implemented-variable-height-source-collections)
uses a real 10,001-record native source and public 10k/100k logical-row controls. Its observations
separate query pages/scanned records, filter/reconciliation visits and list render callbacks.
The native checkpoint's callback count is not the entire trajectory, and equality/string
comparison, immutable snapshot construction, allocation, framework traversal and GPU work are
outside these counters. Dense source/measurement metadata remains O(logical rows). A bounded
visible callback count therefore establishes one work property, not complete incremental cost.
The original Dear/kernel measurements retain their own source and consumer scope.

Each existing owner exposes a detached cumulative snapshot. Counters saturate, do not reset on
resource retirement and do not authorize more work, publication, input or presentation. They do
not change admission limits, revision allocation, last-good fallback or exact backend outcomes.

| Owner | Observed work | Important exclusions |
| --- | --- | --- |
| `TextEngine` / `TextWorkStats` | Direct layout attempts and submitted UTF-8 bytes, complete shaped glyphs before result admission, raster attempts and produced image bytes, paint-window attempts and visited glyphs | Direct Parley work inside `TextEditor` construction/edits/projection, font setup, validation-only calls, retained memory and GPU work |
| `UiCompiler` / `UiCompileWorkStats` | Compile/root attempts, committed frames, created Taffy nodes, layout passes/callbacks, last-good fallback selections and its own text-engine work | Description construction, allocation, active incremental layout reuse, other engines and rendering |
| `UiPresentationPlanner` / `UiPresentationWorkStats` | Plan attempts, successfully retained packets/output primitives and its own text-engine work | Partial lowering of refused plans, compiler shaping, backend expansion, uploads and GPU work |
| Private Inspector/Hierarchy `OwnedPanelWork` | Controller calls, successfully built descriptions, stored source-node counts and same-frame returns | Description attempts refused before snapshot construction, byte allocations, Session work and native presentation |
| Private Hierarchy `HierarchyProjectionWork` | Model projection refreshes and examined rows, committed-filter refreshes and tested rows, returns before description construction, current requested filter-index capacity | Session/model production, action-time basis revalidation, temporary lowercase strings, allocator overhead, compiled frame payloads and native work |

Rejected operations count only the work reached at the phase documented by the corresponding
field. Submitted source bytes are not scanned or retained bytes. Produced raster bytes can be
discarded later. Successfully lowered primitives are not an accounting of all lowering work.
Do not add overlapping nested text snapshots as independent work.

Hierarchy projection/filter counters accumulate reached work. `retained_index_bytes` is a live
capacity gauge replaced on refresh, not cumulative allocation or peak process retention. An empty
committed filter uses direct source indices; a nonempty filter retains dense `usize` indices.
Replacing a filter can transiently hold both vectors, outside this live gauge. The optional
allocation observer separately counts current-thread requests during its exact observation window.

`UiAuthoringSnapshot::source_node_count()` sums the stored envelope counts, including original
counts for rejected root bodies, without walking the trees again. A retained root is an inert
failure fallback; it does not demonstrate active incremental reuse. The public tests keep those
distinctions observable through successful/rejected candidates and exact presentation outcomes.

Counting every shaped glyph before projection includes layouts rejected by line/glyph admission.
Parley 0.11.0 has no public constant-time full glyph count: `Layout::len()` counts lines and
`Run::len()` counts clusters. The observation adds one line/run/cluster/glyph traversal, O(L + R +
C + G) time and O(1) auxiliary space. Its overhead is included below. Folding it into the
short-circuiting projection loop would change the documented count for rejected layouts.

## Later Custom-Layout Observation Boundary

The November 5 [child-layout prerequisite](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#custom-child-measurement-and-layout-prerequisite)
replaces the compiler's private Taffy-tree dispatch and adds `layout_work_units`,
`custom_layout_callbacks`, `custom_child_measurements` and `custom_child_placements`.
Dispatches include cache hits; child requests include explicit hiding and failed requests.
The candidate-wide layout-work limit has separate admission from the text/glyph and paint limits.
Wrapping text now distinguishes actual min-content shaping from natural intrinsic measurement.

The distributions and original work counts below retain their stated source revisions. They are
not automatically requalified for the new dispatch/intrinsic implementation. Correctness/native
passes and a finite operation ceiling do not establish latency, allocation or memory improvement.
Before selecting reuse optimizations, repeat matched probes on the current source and attribute
changed node dispatch, text requests and callback work separately. Native test duration under
ordinary background load is not a before/after UI benchmark.

## CPU Probe Protocol

Windows 11, build `10.0.26300.0`; Intel Core i9-13900KF, 24 cores / 32 logical processors;
68,400,455,680 bytes installed physical memory. Rust 1.95.0, normal unoptimized test profile,
`CARGO_BUILD_JOBS=1`, root `target`, bundled font, serial test execution and ordinary background
work. These are single-process exploratory CPU observations, not isolated release-build or native
latency measurements. Percentiles use nearest rank over individually timed calls.

Inspector uses its real opened Application/Session and four-field custom component. The viewport
is 600 × 1,600 logical pixels at scale 1. Static probes warm up 16 calls and retain 512 samples.
The edit probe replaces the local string with 128 distinct values; its timed call includes text
editing and description/compile work. No Apply occurs, and the workspace remains unchanged.

Hierarchy uses 700 × 800 logical pixels at scale 1.5, no filter, selection at the first row and
the same 16 warmups / 512 samples. It expands a real one-row model into an explicitly synthetic
read-only 10,000 / 100,000-row projection. Setup is outside the timed calls. No synthetic row is
dispatched to Session, submitted to a native Host or granted presentation authority.

## Initial Results

| Workload | Samples | P50 µs | P95 µs | P99 µs |
| --- | ---: | ---: | ---: | ---: |
| Unchanged Inspector | 512 | 194.4 | 242.8 | 375.3 |
| Inspector local text replacement + compile | 128 | 9,020.7 | 9,713.6 | 9,828.0 |
| Unchanged 10,000-row read-only Hierarchy projection | 512 | 221.2 | 274.3 | 309.1 |
| Unchanged 100,000-row read-only Hierarchy projection | 512 | 1,378.3 | 1,991.0 | 2,275.9 |

Every unchanged call builds a fresh description before returning the original compiled frame.
During each 512-call sample group, the description and same-frame counts both increase by 512;
the compiler and its text counters do not move. Thus a reused `Arc<UiFrame>` alone does not prove
bounded or absent description work.

Inspector retains 46 layout nodes after initial compilation; 512 unchanged calls construct
23,552 source nodes in aggregate. Initial compilation performs one layout pass, 70 measurement
callbacks and 19 direct text layout calls. The 128 local replacements add 128 complete compiler
passes and 6,144 layout nodes. Compiler totals after that group are 129 passes, 9,414 measurement
callbacks, 2,579 direct text layouts, 3,225 raster attempts and 3,229 paint-window glyph visits.
The controller's separate text engine reports zero for direct-layout counters because editing
uses the explicitly excluded Parley paths; zero must not be read as no text work.

Both Hierarchy populations produce 40 live layout nodes and one initial layout pass / 36
measurement callbacks. Each later 512-call group constructs 20,480 source nodes without another
layout pass. Visible-row construction is bounded in this viewport, while counting/filtering
still scans the logical population on unchanged calls. The larger population's timing is evidence
of that remaining work, not a universal complexity or hardware budget.

The first Inspector result was retained in nextest run `236a3666-59ad-4e15-9887-b5a07881990e`.
The corrected projection-only Hierarchy run is `040d411e-4f20-4e5a-b3b4-f07fad727ee3`.

## Optimized CPU Repetition

Three separate serial nextest processes repeat those exact probes at `b61d410` with the ordinary
Cargo `release` profile, the same hardware/fonts/geometry, 16 warmups and 512 static / 128 edit
samples per process. The first cold release compilation takes 22 min 08 s; compilation and setup
are outside all per-call timing. This is benchmark construction cost, not an Editor code-edit or
hot-reload measurement. All three processes pass both probes and retain identical work counts.

Cells below are **P50 / P95 / P99 µs**; each process keeps its own distribution.

| Workload | Process 1 | Process 2 | Process 3 |
| --- | --- | --- | --- |
| Unchanged Inspector | 38.7 / 42.2 / 62.9 | 38.9 / 43.3 / 64.9 | 39.3 / 45.7 / 64.7 |
| Inspector local replacement + compile | 733.6 / 869.8 / 939.9 | 748.9 / 872.1 / 967.1 | 770.8 / 894.0 / 945.6 |
| Unchanged 10,000-row read-only projection | 35.5 / 41.4 / 51.5 | 36.3 / 46.7 / 69.8 | 40.4 / 51.6 / 89.7 |
| Unchanged 100,000-row read-only projection | 282.1 / 324.1 / 378.5 | 341.2 / 1,286.1 / 1,549.6 | 339.7 / 1,290.4 / 1,544.1 |

Runs: `87763875-b2e1-4e7f-bd3a-cc86c7f3e449`, `462e8616-8f1a-45f3-adf1-af1ea0f29d5f`,
`e2622218-3359-4f51-9e42-9171028f21ad`. The large projection's tails vary materially despite
identical owner work. Background work is not isolated; these observations neither identify the
cause nor establish an SLA. No performance optimization separates the debug and release source;
their timing difference is not a claimed implementation improvement. Selecting only the fastest
process would discard observed cost. Native active-View and allocation/GPU attribution remain
separate below.

## November 5 Matched Source Hierarchy Reuse

The [selected owner unit](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#source-hierarchy-matched-reuse-unit)
first repeats the existing read-only projection probe at `0ed3507`, after custom layout, meshes
and pointer-input changes. It keeps the earlier hardware/toolchain, bundled font, 700 by 800
logical viewport at scale 1.5, 16 warmups and 512 timed calls. Three ordinary serial release
processes pass. Initial recompilation takes 9 min 01 s outside workload timing; this is neither
UI latency nor an extension edit-to-result measurement.

Cells are **P50 / P95 / P99 microseconds**. Retain each process rather than selecting its minimum.

| Before reuse, `0ed3507` | Process 1 | Process 2 | Process 3 |
| --- | --- | --- | --- |
| Unchanged 10,000-row read-only projection | 35.4 / 41.1 / 54.0 | 35.4 / 40.3 / 64.1 | 36.0 / 40.4 / 65.9 |
| Unchanged 100,000-row read-only projection | 285.7 / 320.5 / 455.3 | 277.6 / 330.1 / 402.0 | 283.6 / 314.8 / 412.5 |

Runs: `e163bb47-1575-402e-be90-a1d5acb7cdc2`, `be23937f-5615-48c0-bb91-14308a9c1bd9`,
`97e04268-072b-45f7-8269-39da34dad0f1`. All retain 40 live layout facts. Each measured group
still builds 512 snapshots / 20,480 source nodes while returning the original frame. Compiler
work stays at one initial pass, 36 measurement callbacks and 79 layout-work units. This repeats
the description/scanning pressure on current code rather than borrowing the older timings.

The predeclared change uses one exact immutable `EditorFrameRevision` for private source/index
reuse and explicit local paint invalidation. The frame clock is nominal and not cloneable;
workspace/project contents have no mutable model access. All current `with_*` enrichment methods
change auxiliary observations unused by this Hierarchy. External `read_only`, local input,
menus, pending/results/errors and environment/theme remain separate dependencies. A cache hit
adds no receipt, action authorization or new journal/Session owner.

The selected warm budget is zero description construction, source/filter row visits, compiler
work and current-thread requested Rust allocation, with per-process P99 at most 25 microseconds
over the same 512-call release protocol. Real admitted 128/1,024-entity Application consumers
must preserve their source/history beside the synthetic scale controls. Pure scroll reuses the
same filter index and builds only the active viewport; committed filter changes may scan all
rows once. Empty filters use a direct range. Nonempty filtering retains a dense index, whose
requested capacity is charged separately from live controls and prior compiled frames.

The first after-change repetition reports a flat 100 ns P50/P95/P99 in all four workloads,
close to this clock's observed granularity. The final probe therefore makes the owner, model,
environment, theme, read-only argument and returned frame opaque through `std::hint::black_box`.
This reduces compiler knowledge of repeated inputs; it does not change the runtime controller
or the 25-microsecond target. Final latency qualification uses that protected probe. The original
before table uses its original harness, so it is not a denominator for an exact speedup ratio.
Work-count and allocation changes retain their separate direct evidence.

| Final protected release probe | Process 1 P50/P95/P99 µs | Process 2 P50/P95/P99 µs | Process 3 P50/P95/P99 µs |
| --- | --- | --- | --- |
| Real admitted 128-entity Application | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 |
| Real admitted 1,024-entity Application | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 |
| Read-only 10,000-row projection | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 |
| Read-only 100,000-row projection | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 | 0.1 / 0.1 / 0.1 |

Runs: `069c8020-85f4-4256-bedc-6b379a95bb24`, `89916e98-3630-4282-8fd1-d0ae4744bd88`,
`38f23aab-29c4-4665-b628-1596f5627856`; both complete probes pass in every process. These
quantized observations meet the selected 25-microsecond P99 target but do not resolve an exact
sub-microsecond cost. All four workloads keep 40 live layout facts, one initial description,
one initial source-projection pass and unchanged compiler work. The 512 sampled calls add no
description, authored node, source/filter row visit or filter refresh. Empty filters retain no
index allocation. Construction/setup and source projection remain outside the timed warm path.

The separate unoptimized `ui-work-allocation-probes` process passes both final protected probes,
run `72f934ca-bbf8-46ec-b0f5-1e79eef39b91`. All four empty-filter workloads report zero alloc,
alloc-zeroed, realloc and dealloc requests over 512 individually observed warm calls. Each then
executes an additional 512-call observation window and asserts the complete `AllocationWork`
snapshot is zero. Description, source/filter scan and compiler counters remain unchanged in
both loops. The real 128/1,024-entity consumers retain exact source bytes, workspace and history;
the synthetic projections remain read-only. These are requested current-thread System allocation
observations, excluding native/background allocation and model/controller construction. Their
instrumented timing is not used to judge the ordinary release latency budget.

Any new model conservatively refreshes, including same-document-revision selection, Schema or
display-row changes. The actual Running Game path continually publishes fresh models, so this
unit cannot establish general static-panel/live-View dependency reuse. Session model production,
new-model preparation, full process/GPU memory, OS input/display latency and the independent
Editor remain in R11a/e. No cross-frame or general virtual-list framework claim follows.

The complete Editor/public integration cohort passes 410/410
(`3849ecf9-10d1-4014-b2f7-c85a06457a92`), including the four new reuse regressions. Selected
actual Windows/DX12 ordinary Scene/Prefab, source Hierarchy/1,000-row filter-wheel and specialized
Canvas workflows pass 5/5 (`5c9d5a88-d858-4c46-8571-f5ad184b1ff7`). They preserve the original
action, history, source, Save/Reopen and projection-retirement assertions rather than timing a
partial drawing loop. A later equivalent `is_multiple_of(2)` spelling removes the new Clippy
warning; the affected 37-test source/adapter cohort then passes again
(`98f61f41-941d-4315-97f9-d86e6184537a`). Final binary build, correctness Clippy with all
targets/features and formatting pass. Other existing warning categories remain outside that
gate. Native and public cohorts retain that explicit source/test scope rather than a new entire
R11 qualification.

## CPU Change Pressure

Native-work prerequisite source: `8a574b8`. The accompanying Editor probe source and this record
share the measured working state; no cache optimization separates the repetitions.

The follow-up probe keeps the same real Application/Session four-field Inspector, 600 × 1,600
logical viewport, default limits and fixed bundled single face. Setup makes one local string
draft (`retained R11a draft`) but no source/history edit. Each scenario preconstructs 16 warmup
and 128 measured inputs. Calls include applying the prepared local state, `begin_paint`,
description/comparison, complete compilation, frame replacement and fresh-frame/publication
assertions. Full geometry/semantics, draft/token, source bytes, history and local Undo/Redo
oracles run outside timing/allocation windows. The samples add no Session submission.

| Scenario | Input change and scope |
| --- | --- |
| Local color | Alternate two real hover reducer states prepared outside observation; one Toggle brush switches between secondary-text and panel color, without activation |
| Palette | Change panel/primary-text colors with a matching distinct theme/environment revision on every call |
| Font environment revision | Change only the font environment revision; actual single face, size and fallback stay fixed. This is invalidation pressure, not typography/fallback substitution |
| DPI | Alternate environment scale 1.5/1.0 with advancing presentation revision; validate physical rectangles against the environment mapping |
| Width | Alternate logical width 720/600 with advancing presentation revision; validate the field-width delta and preserved semantic identity |

Each adjacent input differs and every call must publish a fresh diagnostic-free frame. The
current controller/kernel performs full layout/text work for these changes; the assertions make
that baseline explicit rather than claiming paint-only reuse. Compatible field incarnation,
draft and local editing history must survive. A changed palette at the same numeric revision
cannot use the unchanged-frame path; a theme/environment revision mismatch rejects compilation
before layout and preserves the previous frame. The two cases have different outcomes.

Compiler text observations remain nested within compiler totals. Direct Parley editing work
keeps its documented exclusion. These are CPU description/compile measurements, not native
input, final pixels or platform DPI-transition evidence. Real typography/fallback replacement,
font-size changes and timing/byte budgets remain separate open cells.
The current native font policy is selected at construction. The private frontend's
`set_owned_images` drops its controllers when ordered face IDs change, and Inspector text size
is a fixed constant. Those paths do not provide a live typography/fallback update preserving the
existing edit-buffer incarnation. R11b's scoped text/style consumer must settle that transition;
changing only an environment revision here cannot qualify it.

Initial debug qualification passes **51/51** Inspector/Hierarchy owner tests, including the three
new pressure/counter-oracle cases. The ignored CPU pressure and two complete native probes then
pass **3/3** (`c9a64669-e881-4c78-a897-e8e94fe9fb7b`). Each pressure case builds 6,144 description
and layout nodes over 128 calls, performs 128 layout passes / 9,344 measurement callbacks and
2,560 text layout requests, submitting 29,696 UTF-8 source bytes and visiting 29,568 shaped glyphs.
Local color, palette, font-revision and width cases have identical text work: 2,816 raster calls,
234,368 output bytes and 2,816 paint-window glyph visits. The DPI case has 2,688 raster calls
and 332,736 output bytes with the same glyph visits. Compiler text is included in these totals;
the separate editor engine has no newly counted direct-layout work. Retained compatible edit
buffers do not imply reuse of the compiler's full description/layout/text work.

Three separate serial ordinary-feature release invocations pass the pressure probe and both
complete native lanes each time (**3/3** per invocation). Hardware/toolchain match the earlier
protocol; the CPU pressure uses the fixed single bundled font, while native probes keep their
separately declared fallback/geometry. Every pressure scenario retains identical compiler/text
work across processes. Cells are **P50 / P95 / P99 µs** over each 128-call group:

| Pressure | Process 1 | Process 2 | Process 3 |
| --- | --- | --- | --- |
| Local hover color | 655.0 / 746.7 / 771.9 | 647.3 / 747.9 / 906.8 | 642.2 / 792.6 / 855.1 |
| Theme palette | 652.9 / 755.4 / 884.1 | 644.0 / 757.0 / 844.7 | 648.4 / 810.4 / 895.7 |
| Font environment revision only | 645.4 / 717.1 / 751.4 | 650.4 / 778.7 / 855.9 | 652.0 / 774.4 / 866.3 |
| DPI environment | 637.2 / 712.9 / 777.6 | 636.2 / 709.7 / 839.2 | 640.0 / 754.2 / 899.3 |
| Width constraints | 639.3 / 740.3 / 839.6 | 644.7 / 811.9 / 864.7 | 638.5 / 714.6 / 808.3 |

Runs: `dd47129e-3e5a-4cc2-bc20-5cfb0ba19904`, `a57c8274-9146-482d-9551-f79777ef91fd`,
`05a27bb7-c631-4cd4-911f-beafdca19fea`. These are baseline observations with ordinary background
work, no isolated tail guarantee, no selected numeric budget and no optimization between profiles.
The full-compile assertions describe this revision's baseline; later measured reuse must preserve
the separate brush/geometry/semantic/draft/history oracles rather than retain full compilation as
a product requirement.

```powershell
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_owned_inspector_change_pressure)' --test-threads=1 --nocapture
```

The same probe repeats in three separate allocation-enabled release processes, using the
[requested-allocation scope below](#separate-requested-allocation-probe). All three pass.
Cells are totals over **128 calls**; call order is **alloc / alloc_zeroed / realloc / dealloc**.
Allocated and freed bytes balance within every measured group; all groups have summed net zero
and maximum individual-call growth **193,994 bytes**. That is neither total retention nor RSS.

| Pressure group | Processes | Calls | Allocated and freed bytes, each | Allocated bytes per call |
| --- | --- | --- | ---: | ---: |
| Local color, palette, font revision, width (each separately) | 1 and 3 | 230,528 / 128 / 17,920 / 230,656 | 74,414,464 | 581,363 |
| Same four groups (each separately) | 2 | 230,656 / 128 / 17,920 / 230,784 | 74,418,560 | 581,395 |
| DPI | 1 and 3 | 230,272 / 128 / 17,920 / 230,400 | 74,607,104 | 582,868 |
| DPI | 2 | 230,400 / 128 / 17,920 / 230,528 | 74,611,200 | 582,900 |

Process 2 adds one 32-byte allocation/free per call in each group; the observation does not
identify the cause. Runs: `279fbf9e-490b-4362-ac82-86a9c5c72c8c`,
`328b5a33-aeb1-4ad8-bd37-c303016e7ae7`, `09494254-9030-4243-bec8-e8d41173709b`.
The allocator controls plus all ordinary pressure-oracle tests also pass **7/7**
(`fd0172fa-c4a8-4002-ac3d-03fd471807f8`). Allocation-enabled time samples include observation
overhead and do not replace the ordinary-feature distributions above. Balanced temporary
allocations are still work; no cache optimization, plateau or no-leak claim follows.

## Separate Requested-Allocation Probe

`ui-work-allocation-probes` enables a private `System`-delegating allocator **only in the Editor
library test executable**. It is absent from ordinary timing builds and production binaries.
The same ignored CPU probe calls and correctness oracles are reused; no separate synthetic UI
implementation is measured. Each call observes successful Rust allocator requests on the current
thread, then disables observation before storing/reporting the sample. Setup, initial projection,
warmup, sample buffers and output are outside these windows. Local-edit input formatting remains
inside the call, matching that timing workload's existing boundary.

`alloc` and `alloc_zeroed` have separate call counts. Successful `realloc` counts the full new
request as allocated and the full old request as freed; a failed allocation/reallocation does
not change these facts. Net bytes may be negative when a call frees storage allocated before its
window. The reported maximum is **the maximum individual call's requested live-byte growth
relative to that call's entry**, not total retained bytes. Summing window net changes does not
account for work/destruction outside those windows and is not a process retention or leak proof.

The scope excludes other threads, native-library allocations outside Rust's allocator, allocator
usable-size/metadata/fragmentation, process RSS, GPU allocation and driver residency. Constant TLS
and plain counters avoid allocating during observation; unwind cleanup closes the scope. The
allocator tests verify known zeroed/reallocated sizes, release of pre-existing storage and
other-thread exclusion. Timings printed by this opt-in executable include observation overhead
and are marked `true_not_timing_baseline`; use the ordinary-feature timing processes above for
latency, not an allocation-enabled run as a faster/slower implementation comparison.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --features ui-work-allocation-probes --run-ignored only -E 'test(measure_owned_inspector_unchanged_and_local_edit_work) | test(measure_owned_hierarchy_logical_rows_at_a_fixed_viewport)' --test-threads=1 --nocapture
```

Three separate serial release processes pass both probes. The static workloads have identical
requested-allocation counts in all three processes. Local-edit process 2 has 128 additional
allocations and frees, totaling 4,096 additional requested bytes; the observation does not
identify their cause. The variation is retained below rather than choosing one process.

All bytes are exact requested **bytes**, not KiB or allocator usable sizes. Calls are ordered
**alloc / alloc_zeroed / realloc / dealloc**; totals cover the stated sample group.

| Workload | Calls | Allocated bytes | Freed bytes | Summed net bytes | Max individual-call growth bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| Unchanged Inspector, 512 calls, all processes | 257,536 / 0 / 10,752 / 257,536 | 53,550,080 | 53,550,080 | 0 | 59,103 |
| Local edit, 128 calls, processes 1 and 3 | 242,593 / 128 / 20,992 / 241,120 | 77,610,069 | 77,226,175 | 383,894 | 201,302 |
| Local edit, 128 calls, process 2 | 242,721 / 128 / 20,992 / 241,248 | 77,614,165 | 77,230,271 | 383,894 | 201,302 |
| Unchanged 10,000-row read-only projection, 512 calls, all processes | 147,968 / 0 / 12,800 / 147,968 | 39,993,344 | 39,993,344 | 0 | 42,349 |
| Unchanged 100,000-row read-only projection, 512 calls, all processes | 147,968 / 0 / 12,800 / 147,968 | 39,994,368 | 39,994,368 | 0 | 42,351 |

Thus each unchanged Inspector call performs **503 alloc + 21 realloc** requests, accounting for
**104,590 requested bytes**, before returning the unchanged frame. Each Hierarchy call performs
**289 alloc + 25 realloc** requests and **78,112 / 78,114 bytes** in the two populations. The
static calls balance their scoped allocations/frees, but balanced totals do not eliminate churn
or establish process retention. Bounded visible layout/storage can coexist with the measured
full-population scan time. The 128 local edits leave positive scoped net requests; this short
group neither attributes all retained owners nor proves a leak or a long-run plateau.

Runs: `9bf505b6-6a1d-4993-8c4e-96fdacb2b822`, `368cf563-0cca-49a3-9244-aa7d6ef4771d`,
`ac11012d-6e3a-4680-aa44-ba843abca319`. Allocator plus owner debug tests pass **50/50**
(`da9dd47d-80f0-487b-86bc-c73fd5170167`); the two allocator controls also pass optimized
(`145d1a7b-9b86-4494-bc58-13eef773d61d`). Ordinary-feature owner tests remain **48/48**
(`f8d87568-30d8-4b3a-bc42-96f49f97afd2`). Focused independent review finds no P1/P2.
The private observer is absent from production and adds no dependency or allocation authority.
Correctness Clippy with all targets/features and root formatting also pass. This adds a measured
current-thread request cell, not full process/native retention or quantitative observer-overhead
qualification; those remain separate R11a work.

## Native Window Protocol

The native probe uses the existing hidden-window Windows/DX12 Host and trusted compiled Game
input fixture. It builds no external project and performs no code reload. It selects the actual
`image-source` entity, starts Play, captures the Game image, and holds D for 16 warmup plus 128
sampled 20 ms owner drives. The source Inspector is read-only during Play. Every sample requires
the sprite to move within the visible Game image, the Runtime frame and image version to advance,
and the original Game and unchanged Inspector occurrences to be composited in that same current
primary-window frame. Source values, selection, revisions, history and dirty state cannot change.

Use ordinary features for native timing as well. An allocation-enabled test executable still
delegates each Rust allocation through its observer even outside active counting windows; it is
not the ordinary native timing baseline and does not automatically report native allocations.

The probe records separate owner-drive, cursor-flush and native-redraw wall times. The cursor is
injected through the existing native fixture; this is not OS input latency. Completion spans the
start of that owner drive through its exact original queue's callback, including redraw, polling
and backlog. It is not a GPU timestamp or display latency. Maintenance, validation and the final
completion drain are outside the CPU sample timers; the callback interval can include observer
work performed before the callback runs. No sample is dropped or replaced by idle work.

Actual window extent, native scale, Dear viewport/framebuffer scale, Inspector environment and
image placement/extent, and Game placement/extent are observed outside the timers and must stay
unchanged in every sample. A requested logical window size is not used as evidence of its actual
scale. After all callbacks drain, acceptance re-enters the original Host's device-health boundary;
unchanged epochs and completed callbacks alone do not establish device health. An actual
destroyed-device refusal control follows all measured intervals and must return exactly
`DeviceLost` without changing that namespace. The original graphics and application retirement
must complete before the test reports success, including after this control.

The initial guard incorrectly expected an editable `translation.x` TextInput during Play. The
actual read-only Inspector and viewport expose `anchor.x`; correcting that fixture assumption
preserves all source, receipt, frame-work and 128-frame motion assertions. It adds no support for
editing authoring fields during Play. Review then identified the missing final device-health
check; the corrected debug run `6039f230-ca80-482e-8978-13af82784ade` passes the complete trajectory
and the exact destroyed-device refusal through physical cleanup. Earlier release observations
lacked these explicit health/geometry guards and are superseded for the final native baseline.

## Final Native Release Repetition

Native probe source: `a9345ad411ac0222a1bf87674cc939bf9346a40a`.

Three separate serial release processes run the final probe, including the health check, exact
loss refusal and original cleanup. Adapter: NVIDIA GeForce RTX 4090, DX12, driver
`32.0.16.2101`; CPU/OS/toolchain match the CPU protocol above. Each process warms 16 frames,
keeps all 128 measured frames and observes all 128 matching queue completions. The test window
is hidden; background desktop/driver pacing and other machine work are not isolated.

Actual geometry is identical in every sample and process: window **2,400 × 1,650 physical**,
native scale **1.5**; Dear viewport **2,400 × 1,650** in its coordinates with framebuffer scale
**[1, 1]**. Inspector environment/image is **464 × 1,243**, environment scale **1**, with Dear
rectangle **[1,928, 399]–[2,392, 1,642]**. Game image is **584 × 306**, rectangle
**[488, 496]–[1,072, 802]**. Native scale is not substituted for the panel's actual environment
scale. The bundled UI font and configured Windows `simhei.ttf` fallback remain fixed.

Cells are **P50 / P95 / P99 / max ms**, with separate distributions per process:

| Interval | Process 1 | Process 2 | Process 3 |
| --- | --- | --- | --- |
| Owner drive | 0.2581 / 0.4073 / 0.5173 / 0.6885 | 0.2533 / 0.3863 / 0.4446 / 0.4519 | 0.2554 / 0.4282 / 0.5250 / 0.6655 |
| Cursor flush | 0.0039 / 0.0051 / 0.0071 / 0.0189 | 0.0038 / 0.0049 / 0.0055 / 0.0165 | 0.0039 / 0.0047 / 0.0087 / 0.0174 |
| Native redraw | 32.9822 / 33.5185 / 33.8672 / 33.9157 | 32.9823 / 33.4497 / 33.6643 / 34.0423 | 32.9817 / 33.7066 / 34.0880 / 34.0928 |
| Owner-drive start to exact queue callback | 33.3463 / 33.8318 / 34.2259 / 34.2523 | 33.3129 / 33.7648 / 34.0001 / 34.5388 | 33.3252 / 34.0191 / 34.3946 / 34.4261 |

Runs: `37bc366a-6912-4d49-b9b8-1c5c480de8bf`, `372e3949-7e8e-40fc-9e5f-4665fced2dfd`,
`7861a6bd-df91-4a2b-8eee-955e965982e3`. Observer maintenance totals are 0.744 / 0.596 /
0.626 ms per process, outside sampled CPU intervals; end drains are 2.8 / 3.0 / 3.0 µs. Whole
test durations include setup, negative control and retirement; they are not sample intervals.
Earlier runs had lower redraw wall times. These uncontrolled runs do not identify the cause,
establish a 30 Hz product cap or attribute that change to code outside the sampled timers.

All three processes retain identical work: Inspector description calls, snapshots and same-frame
returns each increase by **128**, building **31,872 authored nodes**. Its compiler/text/planner
counters do not move. Host frames 44–171 match Runtime frames 30–157; sprite X in clip space
advances from 0.058219176 to 0.5035959 while staying fully in the Game image. Across 128 samples,
native transaction totals are 128 admissions, zero rejections, 128 surface acquisitions, 256
queue submissions and 128 presents. These are whole-window facts, not isolated Inspector GPU work.

Sampled logical Host retained-resource maximum is **17,271,176 bytes**; candidate metadata maximum
is zero and submitted instance demand is 160 bytes per sampled transaction. The existing managed
Runtime surface telemetry reports **zero samples and tracked resource bytes in this native
lane**. Its native UI packet counters are also zero. Those facts do not mean zero native UI work,
GPU allocations or residency; the exact original queue callbacks and real composite receipts
provide the separate completion/presentation proof. Full native/process allocations, complete native
backend upload bytes, driver residency, GPU timestamps and OS input/display latency remain open.

The two original Game/Runtime-UI correctness trajectories also pass after sharing their existing
setup/retirement harness (`becf4af8-f519-4145-89d8-68ae842cd914`, 2/2). The focused follow-up
review finds no remaining P1/P2 after the final-health correction. This native baseline records
remaining description work and measurement limits; it does not select a cache implementation or
claim R11a's remaining metrics/budgets, an independent self-drawn Editor or complete R11/R12.

## Primary Window GPU Markers

This follow-up starts at `cecd98b` plus the accompanying native requirement/diagnostic
implementation. The native requirement API is committed at `d368c20`; the private marker
implementation accompanies this record. A native frontend now declares features/limits through the existing
`WgpuRenderHost::request_device_requirements` before acquisition, sharing the original closed
Feature request. Late refusal never replaces a healthy device or broadens recovery. The public
independent output consumer uses distinct timestamp contributions, actual query execution/map,
unchanged output pixels, late refusal and original retirement; this is a real native adapter
consumer rather than a dummy render Feature added to request capabilities.

The new ignored `measure_native_primary_gpu_timestamps_beside_advancing_game` requests
`TIMESTAMP_QUERY | TIMESTAMP_QUERY_INSIDE_ENCODERS` before renderer initialization. It reuses the
complete 16-warmup/128-sample trajectory, geometry, source/history, exact composite/work and queue
completion oracles. It writes two timestamps around the original primary paint in the same
window encoder, followed by one resolve into that sample's aligned slot. It adds no sampled
render pass, submission or serialization; marker/resolve work is diagnostic overhead.

All 256 queries and both 32,768-byte resolve/readback buffers remain with the original Native
owner until its real graphics drain, including on failed encoding, mapping, timeout or device
loss. One setup submission and one final readback-copy submission are outside samples. Readback
requires the actual successful map callback and original copy-prefix completion; every pair
must match its sampled window/Runtime frame and backend/device epoch. Reject non-increasing or
wrapped pairs as a whole run, never drop individual samples. Recheck original-device health
after readback and keep the actual destroyed-device refusal/cleanup control.

The markers describe **primary UI/composition paint only**. They exclude Game/image-producer
GPU work, acquire, present, scanout and other windows. WGPU 30 explicitly permits encoder-command
reordering around timestamp writes depending on backend/driver; these are diagnostic intervals,
not strict isolated execution guarantees. Period converts ticks to nanoseconds but does not
establish nanosecond precision. Existing receipts independently prove drawing/presentation.
Query buffers are separately reported diagnostic payload, outside existing backend payload
gauges; no complete GPU-memory or Dear-private-upload claim follows.

The timestamp-enabled lane marks its CPU/completion times instrumented. Its device request also
differs from the ordinary lane. Their difference does not isolate observer overhead, and these
numbers do not replace the ordinary CPU/native baseline. No cache/performance optimization is
introduced. Full R11a GPU/upload/input/retention attribution and numeric budgets remain open.

The first Windows/DX12 debug qualification is **4/4** (`80b551b8-9d6c-4f8e-9ae9-452f2803ad79`):
new timestamp lane, ordinary cost lane and both original Game/Runtime-UI workflows. All 128
query pairs map successfully; period is 1 ns/tick and primary-marker P50/P95/P99/max is
53.248 / 54.272 / 55.296 / 55.296 µs. This debug result is correctness evidence, not the final
optimized cost distribution. Focused lifetime/reliability review finds no P1/P2.

Three separate serial release invocations subsequently pass **both** the marker and ordinary
lanes, with 128 complete samples per lane each time. Hardware, fonts and actual geometry match
the final native protocol above: RTX 4090/DX12, driver `32.0.16.2101`, 2,400 × 1,650 physical
window, native scale 1.5 and actual panel environment scale 1. Every run retains all 128 query
pairs, actual map success, original copy completion, final health and destroyed-device refusal
through physical cleanup. Work counts and source/receipt/geometry assertions remain unchanged.

Cells are **P50 / P95 / P99 / max**. GPU marker values use µs; owner/redraw values use ms.

| Interval and lane | Process 1 | Process 2 | Process 3 |
| --- | --- | --- | --- |
| Primary GPU markers, µs | 53.248 / 54.272 / 55.296 / 55.296 | 53.248 / 54.272 / 54.272 / 55.296 | 53.248 / 54.272 / 55.296 / 55.296 |
| Owner drive, instrumented, ms | 0.2291 / 0.2795 / 0.3067 / 0.4696 | 0.2331 / 0.2937 / 0.3718 / 0.3787 | 0.2279 / 0.2697 / 0.5079 / 0.5173 |
| Native redraw, instrumented, ms | 33.0321 / 33.2583 / 33.3778 / 33.7819 | 33.0240 / 33.2653 / 33.3704 / 33.4400 | 33.0277 / 33.3309 / 33.5877 / 33.9253 |
| Owner drive, ordinary, ms | 0.2273 / 0.2668 / 0.2876 / 0.3062 | 0.2309 / 0.3690 / 0.5201 / 0.5269 | 0.2291 / 0.2844 / 0.4643 / 0.4651 |
| Native redraw, ordinary, ms | 33.0251 / 33.3451 / 33.4368 / 33.5366 | 33.0154 / 33.4557 / 33.6897 / 33.7954 | 33.0293 / 33.2952 / 33.6041 / 33.8492 |

Release runs: `abf3d99c-c04e-4d95-828e-f9a31de7ebd1`, `99b4a450-c986-4ccc-993a-8bac80064959`,
`4b3555c2-e2b5-4d22-a11c-ce5d0fe67ea9`; each is 2/2. End readback wall times are 1.4420 /
1.6643 / 1.2124 ms, outside sampled CPU/completion timers. Period is 1 ns/tick in each process.
Each marker run uses two 32,768-byte buffers, one setup submission and one final readback-copy
submission outside samples. It adds no sampled render pass/submission. Background pacing is not isolated;
the primary interval cannot explain all redraw wall time or establish a display-rate cap.

```powershell
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_native_primary_gpu_timestamps_beside_advancing_game) | test(measure_native_static_inspector_beside_advancing_game)' --test-threads=1 --nocapture
```

Final source verification includes **390/390** render-WGPU library tests
(`c09b57d4-9de5-48f2-b14a-4b45c9f10cf7`), **3/3** selected actual loss/permit/recovery tests
(`676e433a-ba27-4dc6-87f7-ecfcbf4dab51`) and **4/4** independent public native-output/adapter
workflows (`4a3bece7-e52a-4868-9a17-5ba55a3973d9`). The public error classification now names
both declaration sources: `DeviceRequirements` replaces the pre-1.0 `FeatureRequirements`
variant. The first compile exposed one remaining Runtime caller of the removed private helper;
it now uses the same request entry and the final full library verification above passes.
The affected default Editor Inspector, Hierarchy, specialized Canvas and native
detach/loss/recovery/retirement trajectories pass **4/4**
(`bc6e1a65-dfab-4970-b8d7-d35bd8d7c1da`). Correctness Clippy for render-WGPU and Editor with all
targets/features, plus root and independent-consumer formatting, pass. Upload/source inspection
and independent API/lifetime reviews remain distinct from these executed runtime results.

## Upload And Copy Observation Boundaries

Initial inspection at `20d1af2` uses the actual locked registry package `dear-imgui-wgpu 0.21.0`, checksum
`c1c57f3a9f0d5957700a55018ae93aa049db5c6865d6f7ffc6f82a9df3eb560b` in
[Cargo.lock](../../Cargo.lock), rather than assuming a `repo-ref` HEAD matches the dependency.
The following are source mechanisms and observation gaps, not measured transfer totals:

| Owner/path | Source-confirmed work | Current observation boundary |
| --- | --- | --- |
| Dear `texture/upload.rs` | Create/update builds a new managed texture and writes the full padded RGBA image; updates first merge into retained pixels | No public write-call/byte counter. Dirty-rectangle bytes cannot stand in for actual write payload |
| Dear `frame_resources.rs`, `renderer/draw.rs`, `uniforms.rs` | Render preparation packs/writes vertex and index data and an 80-byte uniform; frame resources are acquired per pass and the arena is cleared on epoch change | No public upload or retained-buffer byte getter. The source does not establish cross-epoch buffer reuse or driver release |
| [Native external image registration](../../crates/nara_render_wgpu/src/host/imgui.rs) | Registers an existing texture view and retains its exact display/output lease | Registration itself does not upload/copy the image. Count its producer work separately |
| [Nara texture cache](../../crates/nara_render_wgpu/src/texture.rs) | Image/glyph success/reuse events and a per-frame successful-cache upload allowance | Test-only counters omit some issued writes before a later failure. Allowance is not a complete actual-write counter |
| [Native preparation](../../crates/nara_render_wgpu/src/preparation.rs), [buffer allocation](../../crates/nara_render_wgpu/src/allocation.rs) | Texture writes and mapped-at-creation instance/material initialization | Initialized payload and Queue writes are different work; neither currently exposes complete issued-byte observations |
| [Program preservation](../../crates/nara_render_wgpu/src/program/native.rs) | Encodes texture/buffer preservation copies; stock UI composition can preserve Scene color into its output | GPU copies are distinct from CPU upload and currently have no complete calls/bytes observation |

Future Nara observations belong at these existing owners' actual write, initialization and copy
sites. Keep issued work after later failure visible, with the current Host transaction reset
boundary. Do not relabel instance demand, retained budget, draw totals or successful-cache events
as measured writes. The locked Dear API cannot provide its private values; keep them unmeasured
until an actual dependency contract supplies them. This work changes no vendor/registry source
and introduces no Queue wrapper or replacement telemetry owner to infer unavailable data.

The subsequent Nara observation implementation adds a detached `WgpuNativeWorkStats` value to
the existing Host transaction snapshot. `NativePreparation` records the actual returned Queue
helper calls and complete data-slice lengths, including work followed by a failed view or other
refusal. Successful nonempty initial mapped buffer copies count separately, before later custody
retention can fail; payload bytes exclude padding while aligned storage bytes include it.
The existing graph initializer counts encoded whole-resource Preserve texture/buffer commands
and native-format bytes, including an encoder discarded after later failure. Attempt-local work
joins the original backend after the existing panic boundary. No new mutable observer owner,
Queue wrapper, callback, resource reservation or publication permission is introduced.

Counters saturate and reset with the existing Host transaction. Retrying or executing another
program in that transaction accumulates work; completion, cleanup and allocation refunds do not
subtract it. A Queue helper invocation that panics before returning is excluded. The snapshot
excludes native callback work, commands encoded inside arbitrary Features, clears/shader writes,
third-party renderer work and driver overhead. Native API requests and encoded copy sizes do not
prove hardware transfer or GPU execution; retained budget and instance demand remain distinct.
The full 128-frame native probes now print this bounded scope beside their original receipts.

The first debug native observation in both ordinary and GPU-marker lanes records zero helper
Queue writes during the 128 warmed moving-Game samples, **256 initial mapped buffer copies**
with **20,480 payload/storage bytes**, and **128 texture Preserve commands totaling 91,496,448
bytes**. This is two mapped buffers / 160 bytes and one 584 × 306 × 4-byte Preserve per sampled
frame. Zero helper texture writes means that warm scope reused its textures; it does not include
Dear uploads, native callbacks or arbitrary Feature commands and is not a zero-total-upload claim.
The GPU marker lane's diagnostic resolve/readback is outside this scope, not silently counted
as a Nara program Preserve.
The three ordinary release repetitions listed in the pressure section each preserve these exact
counts in **both** native lanes. Each marker run resolves all 128 pairs and retains its existing
health/loss/cleanup checks; P95 markers are 55.296 / 55.296 / 54.272 µs across the three processes. These complete
workflows verify the changed observations without attributing GPU execution or lifetime resource
gauges to the issued-byte values.

Qualification includes the initial full render-WGPU library **391/391**
(`f83b2ba4-d8af-40f3-89c1-451e672d1c64`), then **20/20** affected resource/count boundary tests
after review (`cda92467-bcee-4012-bab3-137dbb9b8ebf`), **2/2** actual preflight/refused-upload
native programs (`6d1e938d-a6b3-4e5e-9deb-503a28e980de`) and **8/8** independent graph/Feature
consumer cases (`50a7d947-7aba-4cff-af35-81d0d58c0926`). The boundary additions verify zero
work before custody/budget refusal, empty returned writes, normally returned invalid native
requests, failed allocation before mapping, no-copy empty buffers, aligned mapped payloads,
later-retention refusal, upload-before-failed-view, discarded Preserve work, detached snapshots,
saturation and next-transaction reset. No new failure injection owner was added. Independent
correctness and testing reviews found no P1/P2; their requested boundary assertions are included
in these executed tests.
The complete native Canvas workflow and detached-window/device-recovery/retirement control pass
**2/2** (`d2796042-1926-4e84-a84c-e5b5d319e2ae`). Correctness Clippy for render-WGPU/Editor with
all targets/features and root/independent-consumer formatting also pass. Other existing warning
categories remain outside the correctness gate. Full process memory, Dear write counters,
image-producer GPU intervals, actual font substitution, OS input/display latency and fixed
numeric budgets remain unqualified; this follow-up does not complete R11a or R11b-f.

## Repeated Native Lifetime Process Memory

`measure_native_repeated_lifetime_memory` reuses the complete ordinary static-Inspector /
advancing-Game workload in sequential native lifetimes. One process runs 4 warmup lifetimes and
12 measured lifetimes, alternating healthy graphics retirement and destruction of the original
drained device. Every lifetime still has 16 warmup and 128 sampled moving frames, the original
image/composite/window and queue-completion associations, unchanged source/history and Inspector
identity/work, fixed geometry, and final original-device health. Device-loss lifetimes additionally
require exact terminal-loss refusal with an unchanged namespace. The original harness completes
graphics and Session retirement and drops Native/window/Application before reporting success.
The probe verifies a distinct Host identity and a released weak Rust window in every cycle;
source Scene and Prefab bytes remain identical across cycles.
This is the existing Dear-hosted composition, not an independent self-drawn Editor.

Windows `K32GetProcessMemoryInfo` captures detached process values immediately before native
construction, after the complete workload but before graphics cleanup, and at the first
about-to-wait boundary after the original teardown and exact platform-window destruction.
Per-frame observation vectors are dropped before the live snapshot. A bounded, preallocated
vector retains only scalar cycle facts and geometry; it owns no native resources.
The live snapshot for a loss cycle follows the explicit device-loss control; a healthy cycle
still owns its healthy device at the corresponding boundary. These are labeled separately.

[`PROCESS_MEMORY_COUNTERS_EX`](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
defines `PrivateUsage` as private process **commit**, not physical/private resident bytes.
Current Working Set includes shared and private pages, as described in Microsoft's
[process working-set documentation](https://learn.microsoft.com/en-us/windows/win32/procthread/process-working-set).
Lifetime peaks include fixture construction, native setup and all warmups; they are not reset.
Host-retained payload, candidate metadata and backend-tracked texture/instance bytes remain
separate logical observations, not complete native/driver/GPU residency.

The probe adds no allocator trim, working-set reduction or arbitrary delay after cleanup. Each
lifetime returns to Winit before the next begins. Require the exact retired window's `Destroyed`
event, then a subsequent about-to-wait boundary and a read-only `IsWindow` query confirming that
its HWND is absent. Record that dispatch interval separately, with a finite failure deadline.
Post-teardown values can still include allocator, driver and deferred-release effects; they do
not isolate UI ownership or identify the cause of residual growth. The measurements exclude
neither observer/setup allocations nor shared native mappings. Short repeated lifetimes do not
establish a plateau, leak bound or long-running steady state, and add no memory budget or cache
policy.

The initial debug probe passes 1/1 (`74fd21bc-038e-486e-bf37-7c7ae4633833`), but source review
finds that Winit 0.30.13 posts its Windows destruction request from `Window::drop`. That first
probe runs all lifetimes inside one callback, so weak Rust-window release does not qualify the
HWND/platform boundary. Exclude its process numbers from the final baseline. The corrected
event-driven probe and both original ordinary/GPU-marker controls then pass **3/3**
(`2d8c8a2d-1e5c-4f53-b43e-4438bf982efc`), including 16 exact platform-destruction checks and
the original complete old-lane oracles. Independent correctness and testing reviews find no
P1/P2 after this correction. The missing-event timeout and live-HWND refusal paths have source
review evidence; no fault-injected native execution of those observer failures is claimed.

At source base `714d7b4` plus this probe/retirement refactor, three separate serial ordinary
release processes each pass **1/1**: `4366dadf-8408-4e04-92c4-22af3e14cc06`,
`00b0a363-3fd7-4e59-870b-585cd5296a88`, `a22b9439-3715-4dcb-8d06-4ad59a53bb4d`.
Each reports `debug_assertions=false`, allocation-probe feature disabled and no GPU-marker
instrumentation. The first optimized rebuild takes 16 min 46 s, outside all native observations;
test wall times are 65.26 / 65.09 / 65.44 s. Hardware, fonts and exact geometry match the native
protocol above. All **48 lifetimes** preserve every original workload and cleanup guard; the
**36 measured lifetimes** provide six healthy and six loss observations per process.

Cells below are **minimum–maximum MiB** (`1 MiB = 1,048,576 bytes`), over the six measured
lifetimes in that process/path. Commit and Working Set are different process quantities; do not
sum them. The loss live boundary follows device destruction, as specified above.

| Process / retirement | Live private commit | Live Working Set | After private commit | After Working Set |
| --- | --- | --- | --- | --- |
| 1 / healthy | 640.96–779.83 | 439.92–520.38 | 202.59–504.41 | 212.22–391.82 |
| 1 / loss | 686.95–783.41 | 443.00–528.18 | 221.21–480.69 | 257.22–396.68 |
| 2 / healthy | 600.84–774.75 | 383.48–646.23 | 235.75–477.90 | 266.99–361.59 |
| 2 / loss | 536.60–793.41 | 434.82–520.63 | 232.43–485.80 | 239.21–342.96 |
| 3 / healthy | 618.29–743.29 | 390.36–492.94 | 210.28–513.20 | 247.08–357.77 |
| 3 / loss | 538.39–800.22 | 306.34–533.67 | 225.23–501.91 | 209.45–376.23 |

Retain both endpoint directions rather than selecting one low observation. These cells are
**private commit / Working Set MiB**; both endpoint columns follow loss retirement. Peaks cover
the full process lifetime, including setup and all warmups, not only the 12 measured cycles.

| Process | After last warmup, cycle 3 | After final measured, cycle 15 | Lifetime peaks at final observation |
| --- | --- | --- | --- |
| 1 | 461.87 / 462.75 | 221.21 / 257.22 | 828.31 / 575.17 |
| 2 | 464.74 / 466.41 | 289.96 / 327.41 | 830.27 / 696.23 |
| 3 | 304.11 / 171.53 | 501.91 / 376.23 | 830.65 / 549.94 |

Every lifetime has the same sampled Host-retained payload maximum **17,271,176 bytes** and
live retained payload **17,263,176 bytes**. Candidate metadata and the existing backend-tracked
texture/instance/total payload gauges are zero in this workload; those narrow gauges do not
cover complete native allocation. Measured return-to-Winit through post-destruction dispatch
intervals range **0.6388–0.8900 / 0.6135–1.5544 / 0.7266–1.6593 ms** across processes 1/2/3.
They include scheduling/event dispatch and the later about-to-wait boundary, not isolated
`DestroyWindow` execution.

Whole-process residuals vary substantially despite constant logical Host payload observations.
The third process ends above its post-warmup observation while the first two end below theirs.
These observations establish neither convergence nor a leak cause, and do not qualify a numeric
memory budget. Allocator/native/driver and product retention need separate attribution before
selecting an optimization. Full native allocations, exclusive UI retention, complete GPU
residency and long-running behavior remain open alongside R11a's other missing metrics/budgets.
After measurement, equivalent conditional spellings are normalized for Clippy; no workload,
sampling, ownership or destruction boundary changes follow the recorded release observations.
Final Editor correctness Clippy with all targets/features, root formatting and **1,042 local
Markdown link targets** in the four changed documents pass. Other existing Clippy warning
categories remain outside the correctness gate. This completes the bounded process-observation
prerequisite, not R11a, R11b-f or the complete R11/R12 acceptance.

```powershell
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_native_repeated_lifetime_memory)' --test-threads=1 --nocapture
```

## Full Source Open Refusal

The initial attempt used an actual project with 10,000 empty source entities. Open entered
Recovery with code `project.editor.authoring-source-budget`, phase `Startup source closure`,
and summary `Authoring source preparation exhausted its finite allowance`. No candidate Session
was published, so that experiment produced no Hierarchy timing. The initial run failed; a
diagnostic rerun `b8dc941b-fa3f-4d69-a52a-739651caaf27` confirmed the refusal. The 100,000-entity
source case was not reached.

The source-file entity ceiling alone does not establish the startup dependency closure's decoded,
work or retained-candidate capacity. This work does not widen any product admission limit or
claim 10,000 / 100,000-entity source, editing, history or native acceptance. The read-only
projection probe above answers the separate controller work question. Full source capacity needs
its own attributed owner investigation before a corresponding product-scale claim.

Source inspection localizes the generic Recovery projection to
`runtime/editor/authoring_sources/failure.rs`: it combines Host `Budget` errors and accounting
overflow. Scene compilation-budget diagnostics have a separate path. The startup capture charges
encoded reads, decoded retained structure and final source-content verification; no per-entity
file open or clear duplicate charge was established. Decode-work exhaustion is a hypothesis until
the actual error variant and phase are observed. The consistency reread and simultaneous root
clone must not be removed merely to fit a larger fixture under the existing allowance.

## Dependency Boundary

`tests/product_capabilities.rs` queries reachable normal/build dependencies for all targets with
locked Cargo resolution. The independent `ui-module-consumer` must reach text/kernel/widgets
without Editor/project/ECS, the game `nara_ui` adapter, Dear, Winit or WGPU. The game adapter must
remain free of Editor/Dear dependencies. These are architectural exclusions, not a frozen exact
crate inventory or proof of runtime cost.

The presentation layer remains combined: `nara_ui_render` uses `nara_render::RenderTarget`, typed
resource identities and `nara_image::PreparedImageSnapshot`, and those owners currently include
App/ECS/Asset/Window dependencies. Gating only `integration.rs` would not produce a neutral
presentation closure. R11b must retain those exact resource/custody owners while separating
value/planner consumption from runtime installation through a real independent presentation
consumer. No replacement raw-ID namespace or second renderer is selected here.

## Reproduction And Remaining Work

Qualification for this prerequisite:

| Check | Result | Nextest run |
| --- | --- | --- |
| Text/kernel/widgets/render | 240 passed | `dd261d3e-0f33-415a-8866-60d9625f9549` |
| Inspector/Hierarchy owner tests | 48 passed | `980c6656-dec5-4bc9-aefa-e12de0050bf5` |
| Independent UI consumer | 7 passed | `78fe4b7d-f24c-40bf-9109-cae546a13e05` |
| New dependency assertions | 2 passed | `96eee2b3-598e-43f3-8350-eff3c7ee760a` |
| Windows/DX12 Inspector, ordinary Hierarchy and specialized Canvas workflows | 3 passed | `7fffda8a-bb65-4fcd-b766-92c8de57288b` |
| Inspector resize/scroll retry work attribution | 1 passed | `ffd9f765-7987-45a4-b7a3-554cb76a267f` |

The native controls use the production hidden-window Host, actual WGPU submissions and existing
source/history/Save/Reopen oracles. They establish correctness of the affected paths, not a native
cost measurement. Correctness Clippy for text/kernel/render/Editor with all targets/features also
passes; other pre-existing warning categories remain outside that gate.
Review identified a missing retry-attribution assertion in the existing scrolled-resize test.
It now proves one controller call can build two descriptions and compile twice without counting
frame reuse, while retaining the original geometry, dirty-draft and document-preservation oracles.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --locked -p nara-editor --lib --run-ignored only -E 'test(measure_owned_inspector_unchanged_and_local_edit_work) | test(measure_owned_hierarchy_logical_rows_at_a_fixed_viewport)' --test-threads=1 --nocapture
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_owned_inspector_unchanged_and_local_edit_work) | test(measure_owned_hierarchy_logical_rows_at_a_fixed_viewport)' --test-threads=1 --nocapture
$env:WGPU_BACKEND='dx12'
$env:NARA_EDITOR_CJK_FONT='C:\Windows\Fonts\simhei.ttf'
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_native_static_inspector_beside_advancing_game)' --test-threads=1 --nocapture
```

The current composition still hosts Nara panels in Dear. These CPU/native probes are neither an
intact pure-ImGui reference comparison nor an independent self-drawn Editor. The native lane adds
one active Game View, exact native composition and bounded logical Host observations; the CPU
probes retain their narrower scope. Cold graphics cost, process allocations, complete upload/GPU
timestamps, OS input/display latency, theme/typography changes and multiple native windows remain
unmeasured. No numeric SLA, cache architecture or optimization is selected from these observations.
R11a remains open until its remaining matched workloads and fixed budgets are recorded; R11b-f
and complete R11/R12 remain separate implementation work.

## Hosted GPUI Source And Frame Cost Probe

The November 12 probe extends the independently locked
[native consumer](../../tests/fixtures/gpui-engine-consumer/README.md#native-ui-work-measurements)
from Nara `64946fee8211884e13083a6d0d9a576bc1ef0ee6`; maintained Open-GPUI remains
`f75c10b2a9bf7b4dcb2c8c2aa446203997601369`. No production framework mechanism or Nara default
changes. The opt-in `ui-work-observation` feature uses the existing public frame collector;
`ui-work-allocations` additionally installs the existing framework benchmark allocator. The
[source-based observation map](../research/2026-11-11-open-gpui-engine-integration-layers.md#cost-observation-interfaces-and-limits)
documents their exact scopes and thresholds.
The complete probe implementation is Nara `c11e131b5dc705ca73c68221e4716c7fa966805d`; subsequent
qualification-document changes do not change its runtime. The recorded native runs precede only
the equivalent integer predicate spelling described in the final check record below.

The ordinary original Host still owns device, targets, encoding, submission and retirement.
Samples use the real 10,001-record source, Hierarchy and two Inspector projections. Each of five
sequential cases runs eight warmups and 64 observations: equal publication without rendering,
static panels beside refreshed Scene content, alternating original Sprite X edits, list-height
remeasurement, and the same property edits with a broad window refresh. Warmups, initialization,
reporting and correctness-only readbacks are outside samples. Measurement mode performs no
diagnostic replay/readback. Cases share the original Session and retain their normal history;
they do not recreate an identical history/cache state between cases. The list-height case is a
constraint change; broad refresh is change-dense presentation, not edits to all source records.
The probe deliberately requests a source observation per sample. The interactive Viewer publishes
panels after queued commands; these per-observation costs do not establish an idle full scan at
every display frame.

`mutation` includes original admission/preparation/commit, its observation work and
`session.drive_editor_frame(Duration::ZERO)` after dispatch. These internal stages are not
individually timed, so a large mutation interval cannot be assigned to preparation alone.
The successful drive also reaches requested-revision preview compilation, preview Runtime/
capture production and immutable Editor model publication. `publish_editor_frame` builds the
Workspace/Inspector model; `current_frame()` clones the already published Arc. Consequently the
later `preview` and `query_snapshot` intervals do not cover all earlier preview/model/query work.
`query_snapshot` completely enumerates the public entity query and constructs the detached
snapshot; `publish` includes projection equality, subscription effects and any explicit list
remeasurement. `preview` captures/imports original Scene output and requests a frame. `settle`
drives original hosted demand, recorder and actual submission acknowledgement to an idle demand
state, without waiting for every GPU prefix. `owner_sample` includes these phases and their
instrumentation/validation. Window draw phases and recorder time are nested in this work:
their distributions are **not additive**. Request-layout includes View description/layout
requests; prepaint includes layout computation and cached-View misses. Accepted draw events
are required per changed sample; an absent per-View sub-500-microsecond trace is never zero work.

Allocation samples count successful calling-thread allocation/reallocation requests and requested
bytes in each named owner phase, not frees, peak memory or native/GPU storage. The separately
reported process live-request gauge includes other threads, trace buffers, samples and retained
document history; relaxed concurrent reads are not an atomic heap snapshot. It cannot attribute
bytes to a View or prove a leak/bound. Native-frame occupancy reports only the adapter's pending/
in-flight candidate count at sample ends, followed by actual zero occupancy at close. Its maximum
over those checkpoints is not a transient peak. Source/filter/ListState
metadata remains O(logical rows). Allocation instrumentation is a separate run and its clock
values do not replace the CPU-only observations.

These owner-thread wall-clock observations include any blocking reached in their scopes.
They establish neither pure layout/algorithm CPU time, recorder upload bytes, GPU timestamps,
scanout, physical input latency nor whole-Editor/game cost. The machine is shared and not CPU
isolated; finite sample P95/max values are observations, not tail guarantees or selected budgets.
Refreshing a static Scene output does not advance a running Game simulation. That matched
Game workload, multiple Views, fonts/themes, physical input and the complete R11a/e matrix stay
separate from this finite source/preview probe.
No persistent-layout, Signal, GPU/raster-cache or optimization selection follows merely from
collecting these numbers.

### Initial Development-Profile Run

The first DX12 development-profile invocation, `df09707e-5a37-4c92-9feb-f74156d654d3`, fails
0/1 after 313.973 seconds. It completes the first four cases, but source mutation's combined
interval has P50 3.4315019 seconds and the run reaches its declared 300-second automation
deadline during the final case. Repeated deadline handling then exceeds the 256 KiB stderr
bound, which is the harness's reported failure. This is an intermediate diagnostic, not a
completed baseline or evidence of a GPUI layout bottleneck.

Error cleanup now drops the unfinished observation like the existing smoke controller, so
measurement deadlines do not keep gating retirement. The finished flag moves after fallible
retirement checks. Independent review also catches possible prebuilt feature mismatch: the
test now requires the actual allocator/debug configuration to match its own build, and requires
all allocation phase/gauge records in the allocation mode. Manual tracing restores only the
setting changed by this probe. Source cardinality, eight warmups and 64 samples are preserved;
the documented numeric path uses an optimized build instead of extending the experiment's
deadline. The initial failure is retained separately from subsequent qualification.

The source audit follows `Session::prepare_authoring_property_edit` in
`src/project_host/runtime/editor/prepared_edit.rs`, preparation/shape/classification/history
admission in `crates/nara_scene/src/authoring_decode.rs`, commit in
`crates/nara_scene/src/authoring/prepared.rs`, frame publication in
`src/project_host/runtime/editor/model.rs`, preview compilation/Runtime in the existing Editor
and `preview.rs`, and `build_inspector_model` in `crates/nara_tooling/src/inspector.rs`.
Those paths establish reached work, not measured hotspots. If the optimized combined interval
dominates, first split selection observation, prepare, dispatch/commit and frame drive at the
existing fixture call sites, then refine the expensive original owner. Existing history budgets,
compiler modeled-work reports and query counters do not supply these missing durations.

### Optimized CPU Observations

Reference machine: Windows 11, Intel Core i9-13900KF (24 cores/32 logical processors),
68,400,455,680 bytes installed memory, NVIDIA RTX 4090. Rust 1.95.0, ordinary optimized Cargo
`release`, one build job, no profile overrides or machine isolation. The native root is
1,280 × 640 logical pixels at scale 1.5, with a 1,920 × 960 physical attachment; text uses the
original Cosmic system with Segoe UI. Both backends use `Bgra8UnormSrgb`, device epoch 1 and
explicit linear-sRGB Scene recording. DX12 reports driver `32.0.16.2114`. The original font
database warns about malformed `mstmc.ttf`; this nonfatal warning does not establish coverage of
every fallback face. Initial full optimized compilation takes 38m34s, outside every sample.

DX12 completes both the unchanged visual/recovery trajectory and the new five-case probe:
2/2, `e2317ff3-5818-4b85-8f87-62fd80aadc06`. Each probe case has 64 recorded samples after its
eight warmups. Values below are elapsed milliseconds **P50 / P95 / max**; these are one shared-
machine run, not benchmark ratios or a CPU-time/latency SLA.

| DX12 case | Entire owner sample | Complete query/snapshot | Combined mutation/drive |
| --- | --- | --- | --- |
| Equal observation | 0.349 / 0.392 / 0.442 | 0.233 / 0.272 / 0.323 | No source edit |
| Static panels, refreshed preview | 4.875 / 5.136 / 6.123 | 0.211 / 0.275 / 0.360 | No source edit |
| Local property edit | 335.998 / 408.397 / 417.451 | 0.213 / 0.307 / 0.501 | 330.441 / 403.059 / 410.567 |
| List-height change | 4.764 / 5.531 / 5.887 | 0.190 / 0.242 / 0.442 | No source edit |
| Property edit, broad refresh | 334.818 / 386.659 / 406.387 | 0.208 / 0.315 / 0.330 | 329.492 / 379.993 / 401.002 |

| DX12 changed case | GPUI request-layout | GPUI prepaint | GPUI paint | Recorder CPU |
| --- | --- | --- | --- | --- |
| Static panels, refreshed preview | 0.029 / 0.039 / 0.044 | 0.115 / 0.135 / 0.141 | 0.122 / 0.134 / 0.141 | 0.148 / 0.166 / 0.172 |
| Local property edit | 0.031 / 0.062 / 0.090 | 0.239 / 0.388 / 0.592 | 0.171 / 0.241 / 0.476 | 0.149 / 0.194 / 0.513 |
| List-height change | 0.021 / 0.028 / 0.033 | 0.168 / 0.217 / 0.258 | 0.132 / 0.155 / 0.172 | 0.122 / 0.142 / 0.151 |
| Property edit, broad refresh | 0.034 / 0.065 / 0.091 | 0.294 / 0.561 / 0.840 | 0.193 / 0.340 / 0.591 | 0.150 / 0.378 / 0.523 |

The local edit's preview capture/import P50/P95/max is 3.502/4.858/8.558 ms; its projection
publication is 0.190/0.339/0.637 ms. All accepted draws use one recorder call and original Host
submission. Equal observations use neither drawing nor recording/submission. Their complete
query and equality work remain observable even though their render counters stay zero.

The dominant combined source/edit/drive interval is therefore the first follow-up measurement
target for this consumer. This run does not distinguish its prepare, commit, preview compilation,
Runtime or model-publication contribution, and cannot identify a hotspot within them. It gives
no evidence to prioritize a new Signal/layout/raster-cache mechanism over that attribution.
Local versus broad whole-sample timing is not an optimization result: sequential history/cache
state and background load differ, while most elapsed time lies outside the UI draw phases.

Vulkan independently completes the same two native tests, 2/2,
`cfafed5f-114d-4bd8-9412-47075f5a7e5f`, driver `NVIDIA / 621.14`, on the same target/scale and
sample protocol. These are independent backend observations, not a controlled speed ranking.

| Vulkan case | Entire owner sample, ms P50 / P95 / max | Combined mutation/drive, ms P50 / P95 / max |
| --- | --- | --- |
| Equal observation | 0.314 / 0.355 / 0.454 | No source edit |
| Static panels, refreshed preview | 3.995 / 4.735 / 4.934 | No source edit |
| Local property edit | 331.892 / 352.780 / 414.270 | 326.777 / 347.349 / 409.447 |
| List-height change | 4.540 / 5.085 / 5.881 | No source edit |
| Property edit, broad refresh | 332.198 / 366.547 / 385.697 | 327.110 / 360.934 / 380.794 |

Vulkan's local edit records request-layout 0.032/0.044/0.070 ms, prepaint
0.241/0.329/0.614 ms, paint 0.170/0.191/0.472 ms and recorder CPU
0.068/0.078/0.167 ms. The same combined source path dominates. Equal query/snapshot cost remains
0.209/0.243/0.342 ms despite no UI drawing; its publication/equality also remains real work.

The two backend runs produce identical measured work totals:

| Case, 64 samples | Query pages / scanned records | Projection notifications | Hierarchy / Inspector 0 / Inspector 1 renders | Row callbacks | Accepted draws / recorder calls / submissions |
| --- | --- | --- | --- | --- | --- |
| Equal observation | 640 / 640,064 | 0 | 0 / 0 / 0 | 0 | 0 / 0 / 0 |
| Static panels, refreshed preview | 640 / 640,064 | 0 | 0 / 0 / 0 | 0 | 64 / 64 / 64 |
| Local property edit | 1,280 / 1,280,128 | 192 | 0 / 64 / 64 | 0 | 64 / 64 / 64 |
| List-height change | 640 / 640,064 | 0 | 64 / 0 / 0 | 160 | 64 / 64 / 64 |
| Property edit, broad refresh | 1,280 / 1,280,128 | 192 | 64 / 64 / 64 | 128 | 64 / 64 / 64 |

Mutation samples include an internal complete selection observation plus the explicitly timed
query, explaining their doubled query counts. Local edits notify the three projections but
rebuild only the two changed Inspectors. Broad refresh additionally rebuilds Hierarchy/visible
rows; this is measured work locality, not evidence that all downstream work disappears.
Maximum sample-end native occupancy is zero or one in these runs; final occupancy is actually
zero after retirement. No diagnostic readback/replay occurs in either measurement. The separate
original smoke still qualifies its real visual, source/history, text/IME, fractional-scale,
failure/recovery, geometry and independent-producer retirement controls.

### Separate Allocation Observation And Qualification

The optimized allocation-enabled DX12 build independently passes the probe and original smoke,
2/2, `580b5087-02d8-4239-9122-acc796fc6c05`. Its instrumented elapsed values do not replace the
CPU-only tables. The same accepted-work/reuse and zero-diagnostic/retirement assertions pass.
The following are successful calling-thread requests **summed over 64 samples**, not retained
bytes, process peak memory or quantiles:

| Case / owner phase | Allocation/reallocation calls | Requested bytes, total |
| --- | --- | --- |
| Any case / complete query-snapshot | 641,920 | 85,904,000 |
| Equal observation / publication | 64 | 1,536 |
| Static preview / preview capture-import | 2,247,878 | 288,091,008 |
| Local property edit / combined mutation-drive | 79,130,174 | 12,858,454,216 |
| Local property edit / publication | 773 | 16,640 |
| Local property edit / hosted settlement | 133,644 | 42,849,472 |
| List-height change / publication-remeasure | 60,100 | 61,604,352 |
| Broad-refresh property edit / combined mutation-drive | 79,130,172 | 12,858,456,728 |
| Broad-refresh property edit / hosted settlement | 157,450 | 50,801,984 |

Every explicit complete query requests 10,030 allocations and 1,342,250 bytes in this run.
The local mutation/drive phase averages approximately 1,236,409 requests and 200,913,347 bytes
per edit; those allocations can be short-lived and are not a 201 MB live-memory claim.
Its near-identical broad-refresh source-phase churn reinforces measuring the original source/
preview/model owner first. The allocation data still does not isolate which substage causes it.
No Vulkan allocation observation is claimed.

The five process requested-live-byte gauges are 65,608,543; 65,690,215; 66,926,528; 66,959,792;
67,159,484, in case order. They include retained document history, trace/sample capacity and
background work; they are not cache attribution, RSS, a leak result or whole-process close
accounting. Native candidate occupancy returns to zero on actual retirement in all three
optimized runs. GPU/upload bytes, full heap/peak/retirement and numeric budgets remain open.

Source correctness review closes its prebuilt-mode mismatch finding, and an independent
interpretation review verifies the two CPU logs, phase labels, tables and selected next
measurement without P1/P2. Review does not qualify unexecuted workloads. The unchanged official
framework and unrelated component matrices are not rerun for this fixture-only addition.

All-target/all-feature consumer Clippy passes with `-D clippy::correctness`; three unchanged
style warnings in the existing example remain outside that gate. The locked no-default-feature
all-target check passes, as do root and isolated-consumer formatting, manifest/lock parsing,
added document links/fragments and whitespace checks. The final cleanup uses the equivalent
integer `is_multiple_of(2)` spelling in both alternating-edit checks; it changes no workload.
Only the independent fixture's optional profiler/allocator dependency closure is added; no
existing locked package is removed or upgraded. Remote CI is not run or published.

## Original Source And Editor Drive Attribution

The next unit begins from Nara `d2226c8` and preserves the same source, original prepared edit,
history, preview and frame publication. Four fixture scopes subdivide the existing mutation:
basis/selection observation, original preparation, action dispatch/commit and Editor frame drive.
No validation, materialization or model work moves outside the operation. Shared dispatch helpers
retain the original drive-before-outcome-check order. The first check catches a changed closure
error-inference context; explicit `anyhow::Result` preserves the previous conversion behavior.
The complete attribution implementation is `9715832a0b701beacb90f274a1eb1680fb600d73`;
the indexed hierarchy/Scene implementation and ADR0100 refinement are
`f1330617bc1c1d4ab41383b52bb547bb19cd6ea5`. Later qualification-document changes affect no
runtime. Maintained Open-GPUI remains `f75c10b2a9bf7b4dcb2c8c2aa446203997601369` throughout.

An optimized DX12 probe passes 1/1, `ad184ef5-e81b-4188-abb1-2ad7a7b51bfc`, before adding the
deeper tracing scopes. The same five cases retain eight warmups and 64 samples. Source-step
elapsed milliseconds are P50 / P95 / max, nested inside the existing mutation interval:

| Case | Selection/basis | Prepare | Dispatch/commit | Editor drive |
| --- | --- | --- | --- | --- |
| Local property edit | 0.272 / 0.553 / 0.929 | 17.850 / 23.097 / 26.285 | 0.382 / 0.662 / 0.704 | 318.969 / 353.891 / 409.057 |
| Property edit, broad refresh | 0.271 / 0.387 / 0.714 | 17.785 / 21.353 / 36.917 | 0.389 / 0.642 / 0.855 | 322.166 / 360.942 / 366.392 |

This evidence prioritizes the original frame-drive subdivision. The optional standard
`editor-work-observation` tracing feature therefore emits static spans for preview, source
compilation, preview drive, Runtime, materialization and model publication. It is disabled by
default and installs no engine subscriber. The ordinary consumer installs a current-thread
Registry layer filtered to those spans and retains it for its observation lifetime. Timing
includes subscriber overhead; inclusive parent/child spans and their percentiles are not
additive or exclusive CPU time. Outer source-step allocation counters remain separately scoped.
This observation is not a new shared profiler, mutation owner or scheduling mechanism.

The deeper optimized DX12 run passes 1/1, `355f5eb9-fc1e-4773-a11d-6ad652ea3fe1`, retaining
the same five workloads and all original source/render/retirement counts. Each of the six
owner spans is reached exactly once per measured property edit. Its local-edit source drive
is 309.205/334.945/362.527 ms; the nested reached intervals are:

| Local-edit owner span | Elapsed ms P50 / P95 / max |
| --- | --- |
| Preview total | 307.894 / 332.411 / 361.287 |
| Preview source compilation | 80.251 / 84.762 / 99.882 |
| Preview drive | 226.667 / 247.122 / 273.361 |
| Preview Runtime | 224.785 / 245.171 / 271.399 |
| Preview materialization | 223.628 / 243.931 / 270.197 |
| Editor model publication | 1.189 / 1.755 / 2.520 |

The broad-refresh materialization control is 222.997/230.612/270.046 ms. These observations
identify materialization as the largest reached subinterval, not a measured cost for one
function within it. Source review then finds quadratic sibling lookup/anchor/removal in the
original hierarchy candidate and commit replay, plus root-membership scans during Scene
retirement. A maintained-root index and sequential indexed batch can address that owning path
without changing which entities, checks or side effects the Scene publishes.

Before implementation, the selected consumer's target is materialization P95 below 150 ms
for both property-edit cases, with identical source/render counts and complete source/history,
input, failure/retirement controls. The algorithmic scope is sibling maintenance proportional
to touched siblings and operations, apart from existing ancestor/cycle validation. Retained
membership storage is explicit; the public ordered root/child slices and lifecycle/atomicity
contracts remain intact. This target does not set a general engine frame SLA or make the
remaining ~80 ms source-compilation path disappear.

### Indexed Batch Implementation And Correctness

`nara_hierarchy::CandidateTopology` now lazily indexes each reached sibling set with complete
Entity-keyed previous/next links. Placement ownership and exact anchors follow the sequential
candidate; moving/removing/inserting no longer repeatedly scans or shifts its ordered Vec.
After complete operation and lifecycle admission, `PreparedHierarchyBatch` owns the final
touched-owner root/child sequences and final placements. The original commit applies those
projections once. It still invalidates completion/advances the root epoch for a nonempty batch,
and keeps its no-flush/no-hierarchy-mutation custody requirement. No compatibility replay or
second relation remains. Standard public ordered slices and non-linked entity lifetime remain.

`HierarchyRoots` additionally owns a full-Entity membership set, rebuilt from its final ordered
sequence; Scene replacement builds one retirement-membership set for its anchor search. The
retained root index is O(peak live roots) requested capacity for that World and does not shrink
on every retirement. It holds only current root keys. Transient sibling indexes are O(reached
siblings); operation/lifecycle records remain O(batch operations). Cycle checks can still walk
ancestors, explicit diagnostic/private preparation still validates the hierarchy, and standalone
public current-placement lookup retains its ordered-slice scan. This is not a universal O(1)
hierarchy or allocation-free mutation claim.

The first three-library run, `1517699b-bdc4-4f3c-9d5f-77abb3a39261`, passes hierarchy 43/43,
Scene 772/773 and Transform 36/36. Its one new Scene test incorrectly expects total World
population to equal source roots plus two unmanaged roots, overlooking internal observer
entities. The correction records and compares the exact non-Scene population through every
replacement and final clear, retaining strict root/entity/identity checks. The corrected test
passes 1/1, `34e78d1b-4d4c-4906-9fc1-f97d5240f871`; production code is unchanged by that fix.

New owner controls cover 4,096 parallel roots, four replacements preserving unmanaged sibling
positions, invalid-tail failure atomicity, original frontier promotion, canceled intermediate
lifecycle targets, remove/reinsert and anchors, and independent despawn admission. A sparse
control repeatedly edits three children among 2,048 unrelated sibling sets and checks that
candidate storage stays local. Six 2,048-root recycle/retirement rounds verify full-Entity
membership and stable capacity at fixed population, including actual ECS index reuse. The
public Scene projection control performs four 2,048-root source epochs with alternating order,
old-domain/entity refusal, unmanaged neighbors and final clear. These are correctness/storage
controls, not elapsed-time gates or whole-process leak measurements.

Independent source review finds no P1/P2 in either observation or indexed batch. ADR0100 now
explicitly separates sequential validation/final-value publication from fabricated intermediate
component events; admission still examines every operation target even if final values cancel.
Actual final relation changes and original completion remain observable. Foreign lifecycle
observers/hooks are still refused before any mutation. Review does not replace dynamic tests.

### Matched Indexed-Batch Native Results

The optimized indexed implementation passes original smoke plus the full probe on DX12, 2/2
(`856c1495-2005-4cd1-93f0-2cf300da191f`), and Vulkan, 2/2
(`3d5c03b4-9125-4ae9-965c-ee78cdfa8641`). The fixed source, eight warmups/64 samples per case,
fonts, geometry, device/target and observation feature remain as above. The implementation
rebuild takes 13m48s, outside every sample. No measurement diagnostic readback is enabled.

Elapsed milliseconds **P50 / P95 / max** for the two edited-source controls:

| Backend / case | Preview materialization | Preview source compilation | Original Editor drive | Entire owner sample |
| --- | --- | --- | --- | --- |
| DX12 / local edit | 85.291 / 104.706 / 126.061 | 80.039 / 89.399 / 104.152 | 170.041 / 198.505 / 236.745 | 193.599 / 222.068 / 264.071 |
| DX12 / broad refresh | 86.696 / 94.851 / 117.399 | 80.105 / 85.144 / 93.440 | 172.011 / 186.039 / 202.113 | 195.577 / 213.698 / 225.480 |
| Vulkan / local edit | 85.082 / 90.708 / 119.763 | 80.076 / 98.869 / 109.438 | 170.132 / 193.818 / 203.043 | 193.951 / 224.564 / 228.775 |
| Vulkan / broad refresh | 86.407 / 109.762 / 140.246 | 79.870 / 86.589 / 106.177 | 171.477 / 195.068 / 252.903 | 194.893 / 218.494 / 280.885 |

Both backends and both edit controls meet the predeclared materialization P95 target below
150 ms. Against the same traced DX12 baseline, local-edit materialization changes from
223.628/243.931 ms P50/P95 to 85.291/104.706 ms; broad-refresh materialization changes from
222.997/230.612 to 86.696/94.851 ms. These are selected-workload observations on the shared
reference machine, not hardware-isolated tail guarantees or a general engine speedup. Parent/
child phase distributions remain nonadditive. The Vulkan result is a second backend acceptance,
not a traced pre/post Vulkan ratio.

All five query/page/notification/View/row/draw/recorder/submission work totals exactly match
the preceding table in both runs. Each of the six owner spans is observed once per source edit;
the local edit still preserves Hierarchy/row reuse, broad refresh still rebuilds them, and
actual native candidate occupancy reaches zero on retirement. Source/history, text/IME,
Source/Cooked colors, refusal/panic, geometry/scale and independent-producer close controls
remain in the separately executed smoke. Frame correctness has not transferred to a new owner.

The remaining approximately 80 ms preview-source compilation and 85 ms materialization are
still substantial. The next attribution should refine the remaining original compilation/
materialization stages, preserving complete admission and public workflows. This result does
not make the source operation incremental, eliminate full-source preparation/query work or
close R11a/e latency, allocation, GPU, input, memory or running-Game budgets. Open-GPUI and
the Engine's default frontend remain unchanged.

The separate indexed allocation-enabled DX12 pair also passes 2/2,
`d7a91104-9dd5-4212-887d-f87b98f7ce1f`, including the same complete smoke and six-span call
checks. Its local-edit calling-thread allocations, summed over 64 samples, are:

| Scope, nested source rows belong to mutation | Requests | Requested bytes |
| --- | --- | --- |
| Entire mutation | 79,130,448 | 12,952,233,192 |
| Source selection/basis | 641,920 | 85,904,000 |
| Source preparation | 3,398,400 | 1,542,803,584 |
| Source dispatch/commit | 643 | 2,357,824 |
| Original Editor drive | 75,089,357 | 11,321,164,840 |

Mutation still averages 1,236,413 requests and 202,378,644 requested bytes per edit; source
drive accounts for about 1,173,271 requests and 176,893,201 requested bytes. These are churn,
not live memory or per-span allocations. The phase sum need not exactly equal its parent,
which includes observation/validation work outside the named subcalls. Allocation-mode clocks
remain separate from the CPU-only result. No allocation reduction is claimed.

Compared with the preceding allocation run without source subdivisions/tracing, observed
mutation request bytes are about 1.465 MB higher per edit. That comparison changes observer
coverage as well as the hierarchy representation, so it does not isolate index overhead. The
five process requested-live gauges are 65,763,511; 65,845,183; 70,489,368; 70,522,632;
70,722,324 bytes. They include history, trace/subscriber/sample state, allocator timing and
other threads, not just the root index. The observed difference cannot establish native heap,
peak retention or a per-cache size. The owner tests separately verify live membership keys,
fixed-population capacity and actual Entity index recycling. Whole-process and GPU memory
remain unqualified; measured native candidate occupancy still reaches zero at close.

Affected UI/Render libraries pass 176/176 (`91bf3be6-45ce-45bf-934b-385ab6de715f`): Runtime UI
125 and neutral Render 51. Independently locked public Scene spawn, preparation and durable
identity consumers pass 21/21 (`9ed2edc4-a374-42a4-a84b-1dd652a2e810`), including late lifecycle
observer refusal and exact prepared-basis/identity retirement. Their lockfile remains unchanged.
These complement the hierarchy 43, Scene 772 plus corrected wide-root control, and Transform
36 checks above; they are not a fresh aggregate Editor/framework/platform matrix.
No remote CI or publication is performed.

Final all-target/all-feature correctness Clippy passes for hierarchy/Scene/Transform/UI/Render
and the independent GPUI engine consumer with `-D clippy::correctness`. Existing other warning
categories remain outside that gate. The locked consumer all-target no-default-feature check
passes, verifying the observation feature-off path; root and isolated-consumer formatting,
TOML/lock parsing, added links/fragments and whitespace checks pass. No new compiler floor,
foreign native/browser execution, GPU timing or complete framework matrix is qualified.

### Scene Compilation And Publication Attribution

The next observation keeps the indexed implementation, the same complete 10,001-record source,
the original edit/history/preview workflow and the five cases above. It extends the existing
default-off standard tracing feature to `nara_scene::work-observation`, weakly forwarded by
Nara's `editor-work-observation`; the engine installs no subscriber. No mutation, validation,
publication or scheduling owner changes in this observation unit.

Eleven additional static spans use target `nara::scene::work`. Expansion includes structural
closure and final values/provenance output; Authoring completion and Native restriction/flat
admission are subsequent stages. Materialization includes unbound preparation, binding with
its pre-allocation revalidation, and publication with its final revalidation. Together with
the original six Editor spans, the consumer observes seventeen intervals and requires each to
be reached exactly once in each of the 64 measured samples of both property-edit cases.
The original eight warmups are still excluded from those records.
These expectations describe this flat, no-Prefab fixture. One structural-closure span includes
its internal convergence/layout loops; invocation count is not a count of layout passes.
Reference-demand retries in other inputs may reach that scope again.

All intervals are current-thread elapsed times with subscriber overhead. Parent/child spans
and percentiles are inclusive and nonadditive; they are not exclusive CPU time, allocation
attribution, GPU duration or input latency. Publication includes final validation; binding
includes pre-allocation revalidation. Additional future measurements must not label their
parents as pure writes or sum nested phase percentiles. The same native source/render work
and retirement controls remain required before choosing another optimization.

The observation implementation is `a55043b3b32ff702e579d948807095f217d8e34e`. Optimized DX12
smoke/probe passes 2/2 (`951be6c1-cddd-4aec-9f2b-b49039799060`); Vulkan passes 2/2
(`b94e278c-1d21-4de7-b3a2-9bc608d8e625`). Both property cases reach all seventeen spans
exactly 64 times after warmup. All five source/query/notification/View/row/draw/record/submission
totals match the indexed baseline. Sample-end native frame gauges range from zero to one;
current and final retirement gauges reach zero and diagnostics remain zero. These are actual
sample checkpoints, not a GPU/native peak-memory measurement.

Elapsed milliseconds **P50 / P95 / max**, with the same optimized DX12 observer in both cases:

| Inclusive owner span | Local property edit | Property edit with broad refresh |
| --- | --- | --- |
| Editor source drive | 170.622 / 184.792 / 200.851 | 170.316 / 188.073 / 211.020 |
| Preview source compilation | 80.280 / 89.065 / 96.855 | 79.750 / 85.454 / 89.778 |
| Scene expansion | 54.596 / 59.763 / 71.187 | 54.523 / 57.123 / 61.995 |
| Structural closure, within expansion | 32.698 / 36.707 / 45.696 | 32.673 / 34.999 / 37.876 |
| Final values/provenance output, within expansion | 10.192 / 11.633 / 12.523 | 10.217 / 10.733 / 10.930 |
| Authoring completion | 11.729 / 13.053 / 16.082 | 11.631 / 12.076 / 13.053 |
| Native preview restriction/admission | 13.326 / 15.012 / 20.651 | 13.239 / 17.551 / 19.606 |
| Flat admission, within Native preview | 13.296 / 14.982 / 20.622 | 13.212 / 17.515 / 19.575 |
| Preview materialization | 85.364 / 92.711 / 116.590 | 86.094 / 97.140 / 122.422 |
| Unbound Scene preparation | 25.514 / 29.943 / 39.605 | 25.625 / 30.958 / 32.464 |
| Scene binding | 31.527 / 34.044 / 47.740 | 32.017 / 36.831 / 39.608 |
| Pre-allocation revalidation, within binding | 12.302 / 13.550 / 15.670 | 12.538 / 14.690 / 17.934 |
| Scene publication | 27.056 / 29.180 / 32.656 | 27.303 / 30.717 / 55.791 |
| Final revalidation, within publication | 13.510 / 15.041 / 18.453 | 13.738 / 16.099 / 28.103 |

The Vulkan control observes source compilation 79.933/88.617/111.714 ms locally and
80.030/86.051/100.293 ms with broad refresh; structural closure is
32.695/35.657/45.164 and 32.688/36.893/45.857 ms respectively. Its materialization remains
85.570/97.160/113.973 and 86.213/92.050/108.119 ms. These runs subdivide the existing cost;
they do not establish an optimization gain. The release rebuild takes 12m21s outside samples.
Enabled Scene compilation, feature-off Scene all-target compilation and the independently
locked feature-off consumer all-target check pass. Independent observation review finds no
P1/P2; it does not replace the actual native call-count evidence. No package versions change.

### Selected Structural-Closure Reuse

The measured structural closure is the largest reached child of expansion. Source review
finds that it rebuilds the complete layout after opening the pending slots even when every
slot has no selected instance. For this flat input that repeats the layout without adding a
scope. The selected change retains the current unpublished document, postorder and scope
indexes after the complete pending list succeeds **only when the scope count is unchanged**.
Any real instance entry appends a scope and still requires the original loop; later reference
demands still invalidate and reconstruct their original unpublished value/layout candidate.

Before implementation, the selected consumer target is preview-source-compilation P95 below
80 ms in both edit cases on DX12 and Vulkan, preserving all source/render work and complete
native smoke controls. Structural, reference, owner, placement and final materialized-entity
admission remain mandatory. Removed actual scratch work must not be charged as phantom work:
tight working-byte acceptance/diagnostic boundaries can change because fewer allocations are
attempted. Keep the same limit policy, checked arithmetic and final admission against the
current usage after every pending slot; do not claim identical obsolete cumulative charges.
This is compilation-local reuse, not a persistent cache, incremental compiler or changed
World/lifecycle revalidation. No new allocation, GPU or general Editor frame SLA follows.

### Stable Structural Closure Qualification

Implementation: `969c6f52db2d6d6ba7aa7617bb6ec2db8be3c5b7`, on the seventeen-span observation
at `a55043b3b32ff702e579d948807095f217d8e34e`. Open-GPUI remains unchanged at
`f75c10b2a9bf7b4dcb2c8c2aa446203997601369`.

The implementation records the current scope count, opens every pending slot in the original
order with the same error propagation, and retains the current unpublished document only if
no scope was added. Final materialized-entity and working-usage admission is shared after the
loop and uses the actual post-selection usage. No cross-compilation cache, public proof object
or second compiler is introduced. Original postorder, facet aliases, selected-anchor behavior
and later reference-demand reconstruction stay with the same compiler.

The complete `nara_scene` library and package integration cohort passes **1178/1178**, with
three existing ignored performance observations, across 53 test binaries
(`62435a43-2798-4a3e-b53f-111f834d7aea`). This covers nested/routed root bindings, independent
facets, suppressed-demand rescue, selected retained references, exact final-entity caps and
working-byte boundaries, as well as the original spawn/publication lifecycle contracts.
Independent source review finds no P1/P2 or additional concrete unrepresented semantic case.
No pass-count test or new observation kernel is added.

Both optimized original smoke/probe pairs pass again: DX12 **2/2**
(`56e23e04-7e51-4001-8c38-b9bedf522a08`) and Vulkan **2/2**
(`625f8eaa-5f8d-4cd4-9ef2-ea8fdf15c87c`). Geometry, fonts, source, eight warmups, 64 samples,
seventeen-span observer and all five workloads match the preceding attributed baseline. The
release rebuild takes 12m05s outside all samples. No allocation observer or measurement
readback is enabled.

Elapsed milliseconds **P50 / P95 / max**:

| Backend / source-edit case | Structural closure | Preview source compilation | Preview materialization | Original Editor drive | Entire owner sample |
| --- | --- | --- | --- | --- | --- |
| DX12 / local | 16.934 / 19.408 / 24.262 | 64.300 / 71.712 / 81.448 | 85.305 / 91.054 / 98.885 | 154.662 / 167.640 / 179.327 | 178.145 / 191.191 / 207.956 |
| DX12 / broad refresh | 17.052 / 18.234 / 23.148 | 64.501 / 71.132 / 79.143 | 86.599 / 97.817 / 118.075 | 155.462 / 171.591 / 202.271 | 178.709 / 194.668 / 226.694 |
| Vulkan / local | 17.076 / 19.955 / 27.402 | 64.382 / 78.651 / 98.294 | 85.191 / 99.459 / 108.432 | 154.555 / 177.291 / 195.740 | 177.633 / 203.960 / 225.570 |
| Vulkan / broad refresh | 17.020 / 18.340 / 32.791 | 64.430 / 67.537 / 96.674 | 86.347 / 93.810 / 100.605 | 154.997 / 163.897 / 188.596 | 178.467 / 186.600 / 226.637 |

All four source-compilation P95 observations meet the predeclared target below 80 ms. Against
the same seventeen-span DX12 baseline, local structural P50/P95 changes from 32.698/36.707 to
16.934/19.408 ms and compilation from 80.280/89.065 to 64.300/71.712 ms. The Vulkan baseline
compilation P95 was 88.617/86.051 ms for local/broad cases. These are matched observations on
a shared machine, not isolated tail guarantees or a general Editor SLA; maxima can exceed
80 ms and whole-operation latency remains substantially higher.

All five complete source/query/notification/View/row/draw/record/submission vectors and
accepted-draw counts are identical across both baseline and replacement backends. Each of
the seventeen spans is still reached exactly 64 times per measured property case. Local
editing preserves Hierarchy/row reuse, while broad refresh retains its 128 visible callbacks.
Sample-end native frame gauges reach at most one; current/final retirement is zero with zero
diagnostics. The separate smoke retains complete source/history, input/IME, Source/Cooked color,
scale/geometry, refusal/panic and independent-producer retirement controls. There is no new
allocation, retained-memory, GPU-time, physical-input or foreign-platform qualification.

The independently locked public Scene spawn/preparation/durable-identity cohort passes
21/21 (`5fbe5c83-c015-4385-addc-84dd084ad99a`), including late lifecycle rejection and exact
token/predecessor custody. Its lockfile remains unchanged. An initially hand-added optional
tracing edge was not activated by this consumer and caused `--locked` metadata refusal;
ordinary offline Cargo resolution removes that unnecessary edge, with no package changes.
The final locked run above succeeds; this dependency correction does not change product code.

Final all-target/all-feature correctness Clippy passes for Scene and the independently locked
GPUI engine consumer with `-D clippy::correctness`; other existing warning categories are not
promoted by that gate. The locked all-target feature-off consumer check and root/isolated
consumer formatting pass. Added document links/fragments, lock TOML and whitespace checks
pass. No new compiler-floor execution, full Editor/framework matrix or remote CI is claimed.

The remaining approximately 64 ms compilation and 85 ms materialization P50, complete-source
query/preparation and previously measured allocation churn stay open. Further owner attribution
may examine actual reusable pure facts separately from receiving-World admission; mandatory
late validation must not be removed merely because it appears more than once. This result
selects no new Signal runtime, persistent UI layout, raster cache, default frontend or game UI
execution model and does not complete the foundation Goal or R11/R12.

### Materialization Owner Attribution

The next observation starts from `421f6880b34316b97178e34ccae8fff50ec52bac`, preserving the
stable-structure compiler and the same complete native consumer. Eight more optional static
Scene spans subdivide value admission/decoding, predecessor admission, retirement hierarchy,
identity preparation/adoption/registration and final hierarchy/epoch publication. The engine
still installs no subscriber; feature-off removes these observation declarations.

The same thread-scoped Registry records all 25 intervals. In this fixed successful projection
replacement, each property edit reaches predecessor admission twice, identity preparation
twice and hierarchy-retirement preparation four times; the other 22 scopes are reached once.
Each sample reports the sum of all invocations of its named interval, with complete call counts.
The P50/P95/max of repeated scopes describes that per-sample sum, not individual-call latency.
Parents and children remain inclusive and nonadditive. Epoch publication includes identity
indexes, old entities/domain retirement and the successor domain insertion; it is not pure
index-update time. Existing outer-source allocation observations remain distinct.

Independent source review confirms the original operation/error/borrow/drop order and exact
expected counts without finding P1/P2. This source review supplies no numeric result or
machine-code equality claim. Keep actual optimized native call-count/workflow qualification
and matched observer pins before choosing the next owning optimization. Necessary receiving
World/lifecycle checks cannot be replaced by earlier or merely pure admission.

The observation implementation is `9d1e667c3da159f885e6b7268e138b74214bd042`. Optimized
DX12 smoke/probe passes 2/2 (`e4d0c02a-d46a-4df2-ba5f-0c675e4c2401`); Vulkan passes 2/2
(`e49fdcfc-7eb6-4fcd-b335-25e633afd96e`). Both measured edit cases verify all 25 exact call
counts: 128 each for predecessor/identity preparation, 256 for retirement hierarchy and 64
each for the other 22 intervals. The original five workloads and native smoke remain intact.
The release rebuild takes 12m15s outside samples. No allocation observer is enabled.

DX12 elapsed milliseconds **P50 / P95 / max**:

| Inclusive interval, repeated intervals are per-sample sums | Local edit | Broad refresh |
| --- | --- | --- |
| Value admission and actual decoding | 11.783 / 13.082 / 16.990 | 11.708 / 13.306 / 22.749 |
| Predecessor admission, two calls | 11.415 / 12.928 / 16.667 | 12.016 / 14.108 / 18.866 |
| Retirement hierarchy, four calls | 5.321 / 6.466 / 7.895 | 6.409 / 7.355 / 9.206 |
| Identity preparation, two calls | 13.208 / 15.380 / 17.842 | 13.264 / 14.199 / 22.167 |
| Identity adoption | 1.873 / 2.192 / 4.404 | 1.882 / 1.973 / 2.131 |
| Bound identity registration preparation | 8.866 / 9.485 / 16.099 | 8.886 / 9.395 / 11.127 |
| Hierarchy publication | 1.175 / 1.373 / 3.312 | 1.197 / 1.333 / 1.405 |
| Identity-epoch publication | 10.608 / 11.113 / 12.992 | 10.599 / 11.538 / 17.248 |
| Entire preview materialization | 85.643 / 89.886 / 115.560 | 86.283 / 93.206 / 114.563 |
| Original Editor drive | 154.460 / 164.105 / 187.825 | 155.431 / 182.587 / 188.559 |

The Vulkan control observes decode 11.747/14.265/24.419 ms locally and
11.678/12.308/14.232 ms with broad refresh; materialization remains
85.095/96.223/103.868 and 86.077/89.383/95.650 ms. Adoption is about 1.9 ms P50 in each
backend. Source investigation identifies shareable support checks in adoption, but exact target
observers, marker ownership/incarnation, deferred work and successful-prefix/error order must
remain. Its measured scope does not justify a new batch API in this unit.

### Selected Compiled-Topology Reuse

Receiving validation rebuilds complete placement and namespace indexes for an immutable
`CompiledSceneDocument`. Its sole storage constructor follows full flat admission, including
unique ID sorting and complete rooted-forest validation; normal/Authoring sealing and Runtime
imports/cook all use that owner. World data edits change separate carriers. Reuse the original
closed topology and ID-sorted slice without a retained duplicate index, public proof token or
new compiler. Raw drafts/claims still require their complete original placement admission.

All receiving Schema/owner/version/capability, explicit default, shape/value limit, native
reference-flatness, SceneLocal membership, asset and actual codec checks remain. In particular,
Authoring sealing does not establish receiving Native Visibility projection eligibility; keep
the direct parent-child predicate and the original lowest-ID offending child diagnostic.
Check local references against every compiled entity, including empty records. Do not widen
the stronger Runtime exact-definition witness or bypass any receiving-World/lifecycle checks.

Before implementation, require `scene_decode` P95 below **5 ms** in both edit cases on DX12
and Vulkan, with identical complete source/render vectors and all 25 interval call counts.
Observe entire materialization/drive/sample alongside this scoped target; no whole-operation
SLA follows. Preserve the complete native smoke and public receiving failure-before-codec,
limits/owner/reference and source-immutability workflows. No allocation or GPU result is
inferred from removal of these temporary topology/namespace indexes.

### Compiled Topology Receiving Reuse

Implementation: `9b1376f75b5e5b4656153fb99a292962f564e515`, compared with the 25-span observer
at `9d1e667c3da159f885e6b7268e138b74214bd042`. Open-GPUI remains unchanged at
`f75c10b2a9bf7b4dcb2c8c2aa446203997601369`.

The receiving path now relies on the compiled document's existing sealed immutable rooted
forest. It queries the ID-sorted entity slice for SceneLocal membership instead of constructing
a duplicate namespace set. Native Visibility eligibility scans the existing parent-child edges
and retains the minimum offending child ID; it does not infer receiving support from Authoring
sealing. Raw draft/claims admission and the stronger Runtime exact-definition witness are
unchanged. All receiving value/definition/reference/asset/codec and late World checks remain.
No public API, retained index, cache, compiler or World owner is added.

Two new receiving controls exercise real Authoring preview sealing with crossed parent/child
ID order, zero-codec Native Visibility refusal and last-good preservation, plus shuffled source
records referencing empty entities at both ends of the sorted namespace. The latter also proves
receiving-limit refusal before codecs and one actual decode without re-execution at commit.
Both verify original source/receipt/provenance/occurrence behavior through the existing owner.

The first focused run (`d78c8a34-df50-44e6-a4c0-9d04ddba05b9`) passes 5/6. Its only failure
is the new limit fixture assuming total World population equals its one sentinel, overlooking
Bevy's internal resource entities. The correction captures the complete original Entity set
and requires exact equality after rejection; sentinel, zero-codec and absent identity-domain
assertions remain. Production is unchanged by this correction. The final focused receiving
cohort passes 6/6 (`d40e52b4-3229-41bc-878b-e2368b8a6d46`). Independent production/test source
review finds no P1/P2 in the sealed boundary, complete namespace, diagnostic ordering or raw
admission paths; it does not replace the complete owner/public/native qualification.

The complete Scene package library/public cohort passes **1180/1180** across 53 binaries,
with three existing ignored performance observations
(`d3391e5a-fce0-477f-af67-8a903ac50dcc`). It includes the original flat/raw admission,
Runtime import/cook, source/value/reference/owner/default/version/budget and late publication
controls, plus the two new receiving cases. This is correctness evidence, not an elapsed-time
gain or a complete Editor/framework/platform matrix.

The replacement's optimized smoke/probe pairs pass on DX12, **2/2**
(`4cf7eb98-21b1-467b-8874-91a127369f86`), and Vulkan, **2/2**
(`eec8adef-a0d2-4407-8bf5-5d11404ed7e0`). The complete 10,001-record source, eight warmups,
64 measured samples per case, 25-span observer, geometry/fonts and original five workloads
are unchanged. The release rebuild takes 12m10s outside samples. Neither allocation observation
nor measurement readback is enabled.

Elapsed milliseconds **P50 / P95 / max**:

| Backend / source-edit case | Value admission/decoding | Unbound preparation | Preview materialization | Original Editor drive | Entire owner sample |
| --- | --- | --- | --- | --- | --- |
| DX12 / local | 0.533 / 0.648 / 1.349 | 14.498 / 19.606 / 28.065 | 74.883 / 92.790 / 119.277 | 144.437 / 181.414 / 204.851 | 167.769 / 204.083 / 248.583 |
| DX12 / broad refresh | 0.531 / 0.663 / 0.748 | 14.655 / 15.994 / 19.655 | 75.795 / 84.300 / 112.255 | 144.961 / 159.991 / 208.839 | 168.428 / 182.604 / 237.343 |
| Vulkan / local | 0.539 / 0.684 / 0.800 | 14.359 / 18.805 / 21.137 | 74.620 / 82.322 / 120.045 | 144.188 / 180.817 / 224.036 | 166.931 / 206.093 / 248.951 |
| Vulkan / broad refresh | 0.520 / 0.671 / 1.178 | 14.483 / 16.272 / 26.248 | 75.146 / 88.151 / 113.394 | 144.574 / 168.643 / 184.411 | 167.138 / 193.545 / 208.849 |

All four decode P95 observations meet the predeclared target below 5 ms. Against the same
25-span DX12 baseline, local decode changes from 11.783/13.082 to 0.533/0.648 ms P50/P95;
broad decode changes from 11.708/13.306 to 0.531/0.663 ms. Both backend materialization P50
observations fall from approximately 85/86 ms to 75/76 ms; entire owner P50 is about 167/168 ms
instead of 178 ms. These are selected-workload observations on the shared reference machine,
not exclusive CPU accounting, a general speedup or a whole-operation tail guarantee.

The tail does not improve uniformly. DX12 local owner P95 increases from 186.861 to 204.083 ms;
Vulkan local increases from 193.168 to 206.093 ms. The unchanged source-compiler P95 is
80.739 ms on DX12 and 94.895 ms on Vulkan in those local-edit runs, compared with 71.383/
72.796 ms for their broad-refresh controls. The earlier below-80-ms compiler observations
therefore do not establish a stable SLA. Do not discard these runs, substitute the lower
median for tail acceptance or rerun solely to select greener samples. The scoped decode target
is met; shared-machine timing, entire operation and remaining compiler/identity work stay open.

All five complete source/query/notification/View/row/draw/record/submission vectors and accepted
draw counts match both 25-span baseline backends. Repeated span counts remain 128/128/256;
the other 22 are 64 per property case. Local edit still reuses Hierarchy/rows, broad refresh
still reconstructs its visible rows. Native sample-end occupancy reaches at most one; the
replacement current/final gauges and diagnostics are zero. The unchanged smoke retains exact
history/source/input/IME, Source/Cooked colors, geometry/scale, refused/panicking candidates
and independent native producer retirement. These facts are not peak/live GPU-memory,
physical input, scanout, allocation or foreign-platform qualification.

The independently locked public Scene spawn/preparation/durable-identity cohort passes
**21/21** (`72165929-1ba9-4b94-addd-5698ed1ae629`), retaining exact receiving definitions,
late lifecycle refusal, predecessor/retirement and touched-asset custody. No lockfile or
dependency version changes in this replacement. Further whole-source compilation/query,
identity/publication and allocation/retention costs remain; necessary late checks must not be
replaced by stale preparation merely to meet a larger target. Complete foundation Goal,
R11a/e budgets and Editor/game/default/framework-maintenance selection remain open.

Final all-target/all-feature correctness Clippy passes for Scene and the independently locked
GPUI engine consumer with `-D clippy::correctness`; existing other warning categories remain.
Scene all-target observation-off and independent consumer all-target feature-off checks pass,
as do root and isolated-consumer formatting, added document links/fragments and whitespace.
No new compiler-floor, allocation, foreign native/browser or complete Editor/framework matrix
is claimed. No remote CI or publication is performed.
