# Private Render Composition CPU Stages

Date: 2026-10-31. Scope: disposable R8/R9a comparison, not a production renderer benchmark.
Baseline: private graph/kernels at `cfdf3a9`, plus the new instrumented
[measurement entry](../../tests/fixtures/render-composition/tests/measure.rs). No production
render interface or performance winner is selected by this report.

## Method

Windows 11 Pro (10.0.26300), Intel Core i9-13900KF (24 cores/32 logical processors), NVIDIA GeForce
RTX 4090/DX12, Rust 1.95.0 MSVC, exact WGPU 30.0.1. Cargo used one process/job and the existing root
target. The instrumented binary was built with `--release`; `debug-assertions=false` was observed.
Three independent processes each execute three repeated series. Each series has 128 warmups and
1000 measured candidates per variant, for 36,000 measured and 4608 warmup candidates in total.
Actual instrumented outputs pass independent numeric checks before sampling and at each series'
final output. The same ignored test passed in all three processes (5.321s, 4.839s, 4.867s); these
are repeated runs of one measurement case, not three distinct correctness tests.

All variants share one initialized immutable uploaded input, prepared pipelines, five fresh local
RGBA32Float 64-by-64 textures, two fresh 16-byte buffers, one fresh encoder and the same seven
kernels in identical native order. Initialization is five clears, one buffer zero and one 16-byte
preserve copy. All eight resource versions remain held through the submitted prefix. The ECS
control reuses its World and initialized serial schedule topology; its construction is setup,
not per-sample work. The two-pass slot owns its private blur choreography. Ordered/slot controls
do not perform equivalent dynamic graph admission; their missing admission stage is N/A.

The graph model is rebuilt per candidate. Native allocation/initialization/encoding/error scopes
remain one intact `native::prepare` call; its allocation and encoding cannot be independently
timed through that interface. Control-only split timings are diagnostic, not equivalent graph
stages. Shared Effects timing excludes graph lookups and view construction and includes its
bind-group/pass encoding. Every variant checks native Validation/OOM/Internal scopes at creation,
encoding and submission and retains an accepted-prefix custodian. Their failure implementations
are not claimed equivalent by success timings.

Each variant runs at most eight pending candidates before waiting on that exact prefix. Variants
rotate order across batches/series. Device/pipeline/input/schedule setup, input upload/flush, batch
wait, output-owner release and numeric readback remain outside sample intervals. Queue submission
can still incur driver backpressure. No GPU timestamps, GPU kernel latency, scan-out, retained
driver-heap allocation or RSS was measured. Timer instrumentation is identical where applicable
and is not subtracted. This workstation's background load/power policy was not isolated.

## Results

Times are microseconds. The first table reports the median of nine series' P50/P95 values,
alongside their P95 range. It is not a pooled percentile or an independent-machine confidence
interval. Never add stage P95 values to reconstruct total P95.

| Variant | Candidate CPU through submit P50 | Candidate CPU through submit P95 | Series P95 range |
| --- | ---: | ---: | ---: |
| Graph, fresh model | 198.3 | 330.0 | 270.7–600.9 |
| Ordered static plan | 189.7 | 300.0 | 266.8–581.1 |
| Slot, private blur passes | 192.1 | 311.2 | 255.9–597.8 |
| ECS, reused topology | 193.8 | 316.6 | 266.6–604.6 |

| Stage, median series P50/P95 | Graph | Ordered | Slot | ECS |
| --- | ---: | ---: | ---: | ---: |
| Model declaration/admission | 3.4 / 7.0 | N/A | N/A | N/A |
| Native prepare, inclusive | 158.1 / 265.4 | 153.5 / 240.2 | 156.0 / 251.8 | 157.4 / 261.5 |
| Shared Effects encoding sum | 10.7 / 16.7 | 10.3 / 15.4 | 10.3 / 15.7 | 10.4 / 16.2 |
| Submit/scopes/prefix custody | 35.7 / 59.9 | 34.3 / 57.7 | 34.8 / 59.3 | 34.8 / 59.7 |

The total P95 process medians also vary substantially:

| Process | Graph | Ordered | Slot | ECS |
| --- | ---: | ---: | ---: | ---: |
| 1 | 558.2 | 513.0 | 501.7 | 544.7 |
| 2 | 330.0 | 300.0 | 311.2 | 300.3 |
| 3 | 280.1 | 267.9 | 271.1 | 271.6 |

All nine graph/control comparisons remained below the plan's pause-and-attribute trigger,
`max(10 percent of control P95, 2 times repeated control P95 span)`. Their paired median-series
P95 deltas range 8.5–56.5us, while the large control variation raises those triggers to 483.8–683.8us.
That result is inconclusive about a stable tail-cost difference; it does not establish cost
equivalence, a speedup or freedom from production overhead. Do not discard the first process or
attribute the variation to a specific subsystem without another controlled experiment.

The modeled common local resource payload is 327,712 bytes; one imported input is 65,536 bytes.
Eight candidates plus the shared input model 2,687,232 bytes before readback/driver/allocator
overhead. The graph's conservative default-capacity metadata envelope is 104,752 bytes; its two
native resource tables model 256 bytes. The control's output-handle struct is 544 bytes and excludes
native handles' backing allocations and ECS heaps. These quantities describe different bounded
payloads; they are not measured retained memory or an architectural memory winner.

## Consequence And Reproduction

Continue the selected private coherent device-owner extraction and production ownership transition.
This comparison has not exposed a measured cost requiring selection of a different resource model.
Keep graph/Feature interface freeze behind its real external Runtime/Scene View/offscreen and
ownership consumers. Private native preparation, fresh outputs, scopes and driver work dominate
the observed interval; this method does not independently attribute their internal components.
Production caching, geometry/material work and real game P99 require their own relevant evidence.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location).Path 'target'
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/render-composition/Cargo.toml --test measure --run-ignored only --test-threads=1 --no-capture
```

Correctness Clippy for the new measurement binary passed in 2.13s with `-D clippy::correctness`.
Shared support-module dead-code/large-error warnings remain; this is not `-D warnings` coverage.
The [earlier private verification](../knowledge/engineering/verification/2026-10/2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md)
owns the graph/history/native fault evidence. The [unit plan](../plans/2026-10-31-render-composition-and-host-execution-plan.md)
and [production ownership design](../architecture/render-host-ownership-design.md) retain the
unimplemented complete production outcome; Proposed0121 remains Proposed.
