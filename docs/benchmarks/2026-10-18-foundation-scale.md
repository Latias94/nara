# Foundation Hierarchy And Transform Scale Baseline

This is a headless investigation of the current algorithms, not a supported entity-count promise,
a native frame-time qualification, or evidence for an incremental implementation.

## Decision

Retain the current correct Transform implementation for the admitted consumers. Investigate the
Hierarchy writer before admitting a consumer with thousands of hierarchy members and several
independent mutation producers: the 4,096-member experiment exceeds the pre-recorded 1 ms P95
investigation flag in all three processes. An eight-operation atomic batch is materially cheaper
than eight independent transactions. Combining different producers must preserve their transaction
semantics; this measurement does not authorize silently merging them.

The 65,536-member hierarchy experiment exceeds the 15-second sampling cap. It establishes a stress
boundary, not a usable P99 or a reason to copy Bevy's parallel/unsafe traversal. No current reference
game consumer establishes this population as a product requirement. F11 remains measured technical
debt with a concrete admission trigger; F12 retains a measured consumer gate.

## Subject And Protocol

- Timing source: `327b86f`, after U2/U3. Allocation evidence follows `784f4ab`; the measured
  Hierarchy/Transform algorithms are unchanged. Instrumentation lives only in ignored test targets.
- Windows 11 Pro build 26300; Intel Core i9-13900KF, 24 cores / 32 logical processors; 68,400,455,680
  bytes installed memory; Lenovo default power scheme. No process affinity or machine isolation.
  Other desktop/background work may affect tails.
- Rust 1.99.0 (`b940084d7`, LLVM 23.1.1), x86_64-pc-windows-msvc, Cargo release profile.
- One Cargo invocation, one test thread and one managed Runtime at a time; `CARGO_BUILD_JOBS=1`.
- Populations 64 / 4,096 / 65,536, four ordered roots, deterministic 4-ary topology. Every member
  owns a Transform; sparse means exactly one leaf write. It does not model one Entity per map tile.
- 120 warm-up samples; up to 1,000 timed samples and 15 seconds of accumulated measured duration
  per row. Nearest-rank percentiles stay separate by process. Incomplete rows have no P99.
- Hierarchy rows move the same 1 / 8 / 32 leaves between two roots. The independent case submits
  eight one-operation transactions; the batch case submits one eight-operation transaction.
  Timing includes typed driver ports and a zero-step completion frame, not just writer internals.
- Transform rows time typed edit admission plus managed drive. Static has zero fixed steps; sparse
  and root-dirty have one. Interpolation begins with a half-step then maintains alpha approximately
  0.5. The four-step row consumes one pending edit in the first tick; the other three ticks do not
  write. It is not a four-edit or dense-motion test.
- Fixture construction, output formatting, topology/global oracles and percentile sorting are
  outside the timed span. Hierarchy validation checks every Parent, ordered Children, root and
  completed preorder after warm-up, each 64-sample block, and final restoration. Transform checks
  all completed translations against fixture-owned parent indexes. A 2,048-deep chain is an
  untimed correctness check.

## Hierarchy Results

All figures are milliseconds. Each 4,096-member row contains 1,000 samples per process.

| Population / work | Run 1 P95 / P99 | Run 2 P95 / P99 | Run 3 P95 / P99 |
| --- | --- | --- | --- |
| 4,096 / batch 1 | 1.09 / 1.20 | 1.13 / 1.38 | 1.07 / 1.22 |
| 4,096 / batch 8 | 1.04 / 1.21 | 1.05 / 1.26 | 1.06 / 1.18 |
| 4,096 / batch 32 | 1.07 / 1.25 | 1.03 / 1.13 | 1.00 / 1.09 |
| 4,096 / independent 8 | 4.69 / 4.97 | 5.13 / 6.64 | 4.41 / 4.67 |
| 65,536 / batch 8 | 19.60 / unavailable | 19.48 / unavailable | 28.42 / unavailable |
| 65,536 / independent 8 | 99.42 / unavailable | 125.26 / unavailable | 97.75 / unavailable |

Stress batch-eight samples are 811 / 815 / 736; stress independent-eight samples are 158 / 149 / 160.
They must not be pooled to manufacture a complete P99. The three hierarchy nextest runs are
`619fcfc8-e756-4c2d-b73d-1b6a11e0a9a5`, `c9ce1851-c3d1-44bd-9bda-2a249bd5b70b`, and
`10bf07b7-550b-43d1-b506-a132dcd3fe84`; all correctness checks passed.

## Transform Results

The first complete process has 1,000 samples per row, including stress. Figures are milliseconds.

| Population / work | P50 | P95 | P99 |
| --- | --- | --- | --- |
| 4,096 / static | 0.02 | 0.02 | 0.03 |
| 4,096 / one leaf | 0.16 | 0.20 | 0.30 |
| 4,096 / root dirty | 0.18 | 0.21 | 0.28 |
| 4,096 / one leaf, interpolation | 0.43 | 0.47 | 0.57 |
| 4,096 / four steps, one pending edit | 0.18 | 0.20 | 0.26 |
| 65,536 / static | 0.22 | 0.24 | 0.29 |
| 65,536 / one leaf | 2.48 | 2.84 | 3.10 |
| 65,536 / root dirty | 2.79 | 3.12 | 3.31 |

Run `0bd13836-c0b6-4307-875a-145a98685971` passed. A single complete process does not establish the
two-of-three Transform investigation threshold. The full summaries retain unrounded nanoseconds.

## Allocation And Requested-Heap Results

A separate executable delegates to Rust's `System` allocator and counts successful `GlobalAlloc`
requests. The original timing executable has no instrumented allocator; these samples cannot be
mixed with its latency population. Run `4b711344-1aff-4c2d-9616-badbb4164d5c` passed one explicitly
selected allocation test, with the two imported timing tests skipped.

Each scenario has 120 warm-up cycles followed by four blocks of 64 cycles. Fixture construction,
hierarchy input allocation, correctness oracles and output occur outside the counted windows.
Reallocation counts one call, the full new requested size as allocated and the old size as released.
The following per-cycle figures are identical across all four blocks; calls include reallocations.

| Population / work | Allocation calls / cycle | Requested bytes / cycle | Adjusted retained delta / block | Maximum peak above block start |
| --- | --- | --- | --- | --- |
| 64 / static or one leaf | 0 | 0 | 0 | 0 |
| 4,096 / static or one leaf | 0 | 0 | 0 | 0 |
| 65,536 / static or one leaf | 0 | 0 | 0 | 0 |
| 4,096 / batch 8 reparent | 1,212.5 | 1,487,768 | 0 | 553,952 bytes |
| 4,096 / independent 8 reparent | 9,441.5 | 11,428,052 | 0 | 554,008 bytes |

All 32 windows have matching requested-traffic and live-counter deltas. Each scenario's end-of-block
live requested bytes remain identical over its four blocks. Hierarchy consumes and drops 30,208
bytes of inputs allocated before each window; its raw live delta is therefore -30,208 bytes.
Adding back that transferred input produces the zero adjusted delta above. This is finite churn
and short-window retention evidence, not an engine memory optimization or a leak proof. Independent
transactions amplify allocation traffic as well as CPU cost, strengthening F11's admission trigger.

The counters are process-wide and do not filter activity by thread. Requested Rust heap bytes are
not allocator usable bytes, resident working set, GPU memory, or a total engine-memory bound. No
allocation percentiles, long-running soak, or cross-platform memory qualification is claimed.

## Reproduction And Limits

The harness and small summarizer live together in `crates/nara_transform/tests/`. From the workspace
root, set `CARGO_BUILD_JOBS=1`, set `NARA_SCALE_OUTPUT` to a distinct absolute JSONL path, and run:

```text
cargo nextest run --locked -p nara_transform --release --test scale_measurement --run-ignored only --test-threads 1 -E 'test(hierarchy_scale_baseline)'
cargo nextest run --locked -p nara_transform --release --test scale_measurement --run-ignored only --test-threads 1 -E 'test(transform_scale_baseline)'
python crates/nara_transform/tests/summarize_scale_measurement.py <raw.jsonl> --output <summary.jsonl>
cargo nextest run --locked -p nara_transform --release --test scale_allocations --run-ignored only --test-threads 1 -E 'test(allocation_and_memory_baseline)'
```

Without an override the harness locates the workspace `target` through `CARGO_MANIFEST_DIR`, so
nextest's crate working directory cannot redirect the evidence accidentally. Retained raw files
are compressed under `data/2026-10-18-foundation-scale/`, beside the per-process summaries.

Use a new output path for the allocation executable and select only the named allocation test;
it imports the timing fixture, including its two ignored timing tests. The summarizer processes
timing records only. Allocation raw records remain a separate dataset in the same archive directory.

Windows process samples during hierarchy timing observed a high-water working set near 70.7 MB;
that observation is separate from the requested-heap counters. Neither supplies complete
process-memory or leak qualification. GPU memory, startup cost, dense motion, wide sibling insertion
and native presentation are outside this experiment.

The next real large-map/attachment/streaming consumer must specify its loaded members, independent
producer count, active ratio, CPU and memory budgets before proceeding. Reproduce and attribute
the hierarchy threshold, then replace candidate copying only under the existing mutation authority
and invalidation contracts, retaining full validation as the correctness oracle. A real Transform
consumer exceeding 2 ms P99 at 4,096 sparse participants in two of three complete processes triggers
its own attribution and incremental-propagation investigation. Neither threshold is a retroactive
production SLA.
