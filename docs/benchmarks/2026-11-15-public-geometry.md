# Public Sprite And Tile Geometry Costs

Status: the private immutable Material value correction at `c796594` is selected after
correctness review and three independent candidate measurement processes. The baseline is
`10cd1094900fb56639e2b3afe5cde197d4882ca6`; the measured candidate is
`94fc2190272a752721f7b5c3764165154c7795c0`. The
[owning plan](../plans/2026-11-14-durable-material-and-shader-plan.md#measured-sprite-and-tile-geometry-successor)
keeps the remaining R9c geometry decisions separate. The subsequent visible-queue candidate
at `6c88c87` is rejected under its independent 10% gate; the original queue is retained.

## Workload And Source

The independently locked [module consumer](../../tests/fixtures/module-consumer/sprite-render-consumer/README.md)
builds an ordinary managed Runtime from domain crates. Its actual Publish system takes the
original stock policy transport once. One offscreen Host executes the original input through
public preparation/submission and original-prefix completion. It observes its own acquired
adapter without creating a probe device. Captured drawing and resource retirement use the same
production owners as other stock consumers.

Each process runs 36 groups: 64/4,096/65,536 cells, one/two Cameras, plain/bound Material values
and static/one-cell/moving-camera scenarios. An overlapping blue Sprite checks painter order.
The bound path uses four white parameters, so both Material paths have identical expected
pixels. A 320x180 WorldCanvas resolves to 960x540. The larger maps deliberately extend beyond
the visible region; complete extraction and authorization still execute. Eight warmups precede
64 recorded samples per group. Each process records 2,304 numeric rows.

All six processes use Windows 11, an i9-13900KF, Rust 1.95.0, one Cargo build job and the same
independently locked workspace's ordinary release profile. The actual original Host reports
Vulkan, RTX 4090 and NVIDIA driver 621.14. Lock SHA-256, full revisions, process IDs, nextest IDs,
raw-file digests and every group's distributions are in the
[summary](data/2026-11-15-public-geometry/summary.json). The six compressed raw JSONL files beside
it preserve all 13,824 sample rows. No dependency version, workload, native resource limit or
profile setting changes between variants.

## Intervals And Controls

Seven intervals separate complete managed drive/capture, previous-output release/Host frame
setup, target adaptation, Host preparation, native preparation/encoding/submission/publication,
total CPU time through submission and original-prefix polling wait. Initial mapped-buffer
copies count as initialized native work. Queue write calls alone would miss that upload work.
Oracle snapshots, complete pixel references, readbacks, retained-input replay and cleanup stay
outside sample intervals. JSONL writing also occurs after the measured intervals.

The original Runtime and World retire before an old, previously unprepared CPU input is first
realized. Published A remains readable after distinguishable B, later publication and producer
retirement. Every measurement process checks independent full pixels, source/queue diagnostics,
original completion and final zero Host resources, candidate metadata and cleanup faults.
Failure paths retire the original Runtime/Host before resuming the same assertion panic.

The six process identities and durations are:

| Variant / run | Nextest ID | Test seconds |
| --- | --- | ---: |
| Baseline 1 | `9dffb501-6091-45df-8c39-b5b43ad5b437` | 46.439 |
| Baseline 2 | `507538f3-aec1-4985-a87d-dd3f920080bd` | 45.023 |
| Baseline 3 | `680c76ef-a1ec-43f8-ac8b-66df9faf2a8e` | 46.264 |
| Shared values 1 | `b25b88ab-4aac-45d2-b777-2ab00557ee38` | 12.227 |
| Shared values 2 | `3af31c5e-3dc2-4c9a-af3f-7b3884511f29` | 12.062 |
| Shared values 3 | `d4dfa981-1787-4109-934d-bb7c9ef946f4` | 14.417 |

These durations include all groups, warmups and correctness/retirement work. They are separate
from ordinary sample intervals.

## Correction And Results

At the baseline, `Material2dBinding::clone` copies parameter and texture maps during cell
extraction, queue resolution and later lowering. The selected correction keeps the original
generation directly in each binding and shares its immutable validated values in one private
`Arc<BindingValues>`. Encoded bytes are owned by that value block. Public constructors/getters,
value equality, signed-zero bytes, Image version capture and generation lifetime remain intact.
The binding also becomes smaller inside extracted/captured structures. This comparison changes
both clone allocation and representation size; it does not isolate their individual shares.

For 65,536 cells, static bound values and the complete managed-drive/capture interval, nearest-rank
P50 / P95 milliseconds are:

| Views | Baseline 1 | Baseline 2 | Baseline 3 | Shared 1 | Shared 2 | Shared 3 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 75.248 / 89.775 | 70.811 / 76.450 | 72.628 / 85.021 | 6.279 / 8.747 | 5.693 / 7.296 | 5.867 / 9.314 |
| 2 | 72.461 / 85.175 | 69.226 / 73.570 | 68.683 / 71.809 | 6.915 / 9.227 | 6.078 / 6.468 | 10.429 / 11.666 |

Total CPU-through-submission P50 for the same one-View groups falls from
76.203/71.445/73.237 ms to 6.686/6.125/6.270 ms. Two-View values fall from
73.191/69.839/69.317 ms to 7.412/6.454/11.175 ms. The slower third candidate process is retained;
its machine-wide timing variation has not been attributed to a specific cause.

Additional managed-drive/capture P50 controls, in milliseconds:

| Cells / Views / Material / scenario | Baseline runs 1 / 2 / 3 | Shared runs 1 / 2 / 3 |
| --- | ---: | ---: |
| 64 / 1 / plain / static | 0.036 / 0.032 / 0.041 | 0.033 / 0.036 / 0.031 |
| 64 / 1 / bound / static | 0.076 / 0.074 / 0.083 | 0.036 / 0.046 / 0.036 |
| 4,096 / 1 / bound / static | 2.419 / 2.159 / 2.627 | 0.487 / 0.481 / 0.480 |
| 65,536 / 1 / plain / static | 8.155 / 7.674 / 8.718 | 4.484 / 4.341 / 4.073 |
| 65,536 / 1 / bound / one-cell | 71.213 / 68.858 / 70.610 | 6.371 / 6.066 / 9.742 |
| 65,536 / 2 / bound / moving-camera | 72.183 / 71.408 / 71.116 | 6.032 / 6.047 / 10.323 |

Across all 18 large bound candidate groups, P50 improves by 84.740--91.960% against the fastest
matching baseline process. This exceeds the preselected 25% criterion in every candidate
process, View count and scenario. Across every 64-cell group, the largest P50 increase against
the fastest matching baseline is 0.0124 ms, below the 0.1 ms investigation threshold.

All thirteen sampled native/resource/count fields match every corresponding before/after
sample. Queue counts remain 65--1,982, initialized-buffer payload is at most 159,040 bytes,
and sampled retained native resources reach 4,147,204 bytes in both variants. Candidate metadata
is zero at the post-completion sampling point. These values describe the recorded observation
points and exclude total CPU/RSS, driver allocations and an unobserved preparation peak.

For the large static one-View bound groups, native submission P50 is 0.488--0.516 ms before and
0.321--0.353 ms after. Polling-wait P50 is 1.385--1.509 ms before and 1.265--1.420 ms after.
The primary measured correction is CPU work. Identical native bytes do not establish identical
physical bus traffic, and polling completion is not a GPU timestamp.

## Qualification And Decision

The existing Material/Sprite cohort passes 136/136 in
`0d92a4b4-1f6a-40a5-8673-f6937b373855`. Original Host/stock Material controls pass 21/21 in
`beb0aa23-c7c0-403c-8813-3020ed341c91`, including Shader generations, both layouts, last-good,
late original captures, graph-shared textures, admission and original-prefix cleanup. Final
independent CPU controls pass 2/2 in `944968d1-4901-4433-a44f-1912958a921c`. The complete public
pixel/retirement trajectory passes on asserted Vulkan RTX 4090 in
`ef829583-2932-4d9d-8347-5a011513a851` and asserted DX12 Microsoft Basic Render Driver in
`5c39fb56-a1de-466e-bddc-e0b407139952`. Each native trajectory covers all twelve small-scene
View/Material/scenario combinations. These overlapping scopes are not an aggregate test count.

Independent correctness review finds no P1/P2 regression. Owning correctness Clippy, independent
all-target strict Clippy, root/independent format checks and local document/manifest checks pass.
Separate read-only data review verifies all six raw/compressed digests, complete sample groups,
4,536 per-process percentiles, decision arithmetic and observation limits without finding a
P1/P2 discrepancy.
Local CI selects the new CPU and DX12 software pixel controls; remote CI has not run.

Keep the private value-sharing correction. A cross-frame geometry cache needs separate measured
pressure, complete version facts and a CPU-retention budget under
[ADR 0096](../architecture/adr/0096-evidence-gated-render-scaling-and-upload-policy.md).
The [historical Tile/HUD report](2026-10-20-tile-visibility.md) retains its original raw data and
revision-specific reproduction. Its retired private-executor collector excludes different work
and includes changing HUD, so it cannot establish a direct speedup against this geometry-only
substrate. Deliberately retained private numerical oracles remain separate from current costs.

## Visible Queue Material Capture Follow-Up

This separate comparison starts at `814899e3e9e2d075aa27b49cf0db91d91621876e`, after the
selected value-sharing correction and subsequent runtime work. Its three fresh baseline
processes precede any queue implementation edits. The earlier deep-copy measurements above
are historical evidence, not this follow-up's baseline.

The private candidate borrows each extracted Material binding during resolution and clones it
only for visible queue entries. It still resolves and captures every declared texture before
visibility, including when there are no Views. This removes one temporary binding clone per
material-bearing extracted item; it does not remove texture resolution, extraction, per-View
projection, sorting or native work. Public capture types, value equality, batching and resource
ownership stay unchanged.

The preselected gate requires every one of the six 65,536-cell bound groups in each of three
candidate processes to reduce managed-drive/capture P50 by at least 10% against the fastest
matching fresh baseline. All thirteen count fields must match corresponding samples. A
64-cell P50 regression greater than 0.1 ms requires investigation. An unsuccessful comparison
removes only this candidate and keeps the new independent semantic controls. The
[owning plan](../plans/2026-11-14-durable-material-and-shader-plan.md#visible-queue-material-capture-comparison)
records the scope and stop condition before candidate measurement.

All six new processes use the same independently locked release workload, Rust 1.95.0,
Vulkan RTX 4090 and driver 621.14. Each passes all 36 groups, 2,304 recorded rows, complete
independent pixels, original-input first realization after World retirement, retained outputs
and final zero Host resources/candidate metadata/cleanup faults. The
[queue summary](data/2026-11-15-public-geometry/queue-capture-summary.json) and six
`queue-*.jsonl.gz` artifacts preserve all 13,824 rows, exact source/environment identities,
raw/compressed digests and every group's seven separate interval distributions.

| Variant / run | Nextest ID | Test seconds |
| --- | --- | ---: |
| Current baseline 1 | `89fea249-0e0e-4c2a-9d92-84cc4c09d439` | 16.467 |
| Current baseline 2 | `7800e15a-3a71-4fd7-a5e1-6aa0085881c4` | 17.174 |
| Current baseline 3 | `b9d5404b-411d-450a-8923-316ca81bfee0` | 16.346 |
| Borrowed queue 1 | `00cf89ee-9019-4574-b521-1354c03ceac2` | 15.532 |
| Borrowed queue 2 | `2f5cfaa9-a1bd-4f28-ac94-7ba21369453b` | 15.615 |
| Borrowed queue 3 | `f5c44516-2df2-4e41-91ed-d01fa398f611` | 15.578 |

For 65,536 bound cells, managed-drive/capture P50 / P95 milliseconds are:

| Views / scenario | Baseline 1 | Baseline 2 | Baseline 3 | Borrowed 1 | Borrowed 2 | Borrowed 3 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 / static | 10.115 / 21.268 | 11.056 / 17.425 | 9.727 / 11.077 | 8.688 / 12.323 | 8.270 / 10.025 | 8.306 / 9.020 |
| 1 / one-cell | 9.569 / 11.253 | 10.495 / 13.337 | 9.500 / 14.121 | 8.558 / 11.871 | 8.444 / 15.679 | 8.124 / 14.340 |
| 1 / moving-camera | 9.883 / 11.834 | 10.669 / 13.819 | 11.130 / 22.550 | 8.207 / 9.736 | 8.512 / 9.408 | 8.537 / 11.072 |
| 2 / static | 10.707 / 15.509 | 11.450 / 12.944 | 10.458 / 16.404 | 9.363 / 12.346 | 9.454 / 12.189 | 9.081 / 14.165 |
| 2 / one-cell | 10.669 / 16.332 | 11.933 / 17.754 | 11.080 / 15.316 | 9.239 / 10.310 | 9.744 / 12.359 | 9.524 / 12.705 |
| 2 / moving-camera | 9.829 / 15.164 | 11.802 / 17.653 | 10.870 / 12.247 | 9.050 / 11.822 | 9.508 / 11.071 | 8.825 / 15.520 |

Only 13 of 18 large bound comparisons pass the 10% gate. The five failures improve P50 by
9.919%, 7.922%, 3.262%, 8.665% and 9.601%, respectively. Across all eighteen comparisons,
the measured reduction is 3.262--16.959%. The largest small-scene P50 increase is 0.0102 ms,
and all thirteen sampled count fields match corresponding samples across all six processes.
Neither correctness nor small-scene regression causes rejection: the candidate does not satisfy
the required repeatability across View counts and scenarios. No extra run or variant is added
to change the outcome. The original queue is restored; the multi-View, offscreen and exact
resource-generation tests added with the candidate are retained.

These measurements cover the complete managed drive and capture, rather than queue cost alone.
The bound workload has four white parameters and no extension textures or Shader edits;
separate semantic tests cover multiple texture captures and generation changes. Visible-item
counts and native work are unchanged. The observed timing difference does not isolate the
reference-count operation from generated code, structure layout or uncontrolled machine-wide
variation. All baselines precede all candidates, and several P95 values remain noisy. This
negative selection does not prove that the borrowed representation is always slower or cannot
help another workload. The earlier immutable-value correction remains selected.

On the final selected source, Material/Sprite tests pass 138/138 in
`9b7bce53-7bf3-4cfb-8966-1ed8d8b2d6e4` (6.534 seconds), including both newly retained
semantic controls. The independent public CPU consumer passes 2/2 in
`d1fbb9cd-3366-4df7-adf1-364dff88455a` (0.286 seconds). Its complete pixel/retirement
control passes on asserted DX12 Microsoft Basic Render Driver in
`90470d41-d14c-4d1e-b308-1940dd2c38f1` (1.608 seconds). The six Vulkan hardware processes
above retain their separate measurement and correctness scope. These are overlapping
selections, not an aggregate test count.

Independent data review recomputes all six compressed/raw digests, 4,536 interval percentiles,
149,760 corresponding count-field comparisons, the 13/18 gate result and observation limits
without a P1/P2 discrepancy. Queue correctness review separately finds no P1/P2; it does not
establish a performance gain. The source collector, scene and independently locked dependency
graph are unchanged between baseline and candidate.
Owning all-target/all-feature correctness Clippy, root format and local document/data checks
pass on the final selected source. The production queue matches the pre-experiment baseline;
the retained tests are additional correctness coverage. Existing software CI already selects
the ordinary public pixel control; no CI configuration or remote run is added for this outcome.

## Reproduction And Limits

Use an isolated checkout of the selected baseline or candidate revision, the committed module
workspace lock and a fresh absolute output path for each process:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
$env:WGPU_BACKEND = 'vulkan'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
$env:NARA_GEOMETRY_COST_REVISION = git rev-parse HEAD
$env:NARA_GEOMETRY_COST_OUTPUT = '<fresh absolute JSONL path>'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-sprite-render-module-consumer --test geometry --run-ignored only -E 'test(measure_current_public_geometry_costs)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

Distributions stay separate by process; 64-sample nearest-rank P99 is the maximum and supplies
no stable production tail estimate. All baseline processes precede all candidate processes;
the runs are neither randomized nor isolated from every machine workload. Original-prefix
waiting serializes frames and includes polling delay. These measurements do not qualify
pipelined throughput, physical scanout, GPU duration, general Material workloads, a supported
map population or a product frame-time SLO. Actual foreign-platform execution and complete
R8/R9/R12/Foundation acceptance remain separate. Editor/Dialogue product work remains paused.
