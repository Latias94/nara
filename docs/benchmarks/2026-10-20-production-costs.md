# Ordinary Authoring And Runtime Costs

Status: Baselines complete; a directly observed schema-lookup correction is in progress.

This decision-local experiment follows the
[module review](../audits/2026-10-20-module-goal-alignment-review.md) and
[measurement plan](../plans/2026-10-20-production-cost-measurement-plan.md).
The starting product source is `33248b79b069e4d9023601e38e4c76515ea8e937`.
Historical Save, Reopen, startup and reference-game protocols remain unchanged.

## Questions And Boundaries

| Workflow | Observation | Excluded interpretation |
| --- | --- | --- |
| Scalar edit and Undo | Public Editor action admission and owner frame drive | Native input-to-display latency |
| Browser refresh and document Open | Public owner call occupancy, scan phases and completion | Pure storage time or a consistent filesystem snapshot |
| Static/changing Tilemap and HUD | Real ECS drive, UI planning, WGPU preparation, submission and completion wait | Surface presentation, monitor scanout or frame-rate qualification |
| Rust function body / runtime structure | Saved source through dev build and successful headless consumer exit | In-place hot reload, arbitrary state migration or first presented frame |
| Resource accounting | Precisely identified owned textures and buffers | Complete process heap or driver GPU residency |

## Local Protocol

Windows 11 Pro, Intel Core i9-13900KF (24 cores / 32 logical processors), 64 GiB
installed RAM, NVIDIA GeForce RTX 4090. Ordinary background desktop activity is present;
no affinity, machine isolation, cold-cache or cross-platform claim is made.
Root executes one Cargo command and one consumer at a time, with one build job and the
checkout's existing shared target. Runtime and interaction measurements use optimized
release code. Rust iteration uses the ordinary dev profile with incremental compilation
explicitly enabled, Rust 1.95.0 and cached offline dependencies.

Keep process populations separate. For two warm-ups and sixteen retained runtime/interaction
observations, report nearest-rank P50 and P95; P95 equals the observed maximum and is not a
stable tail estimate. Rust iteration has five retained observations per class and reports
median and range. Never subtract independent percentiles to manufacture a phase cost.
Fixture setup, semantic assertions, row formatting and aggregation remain outside timed calls.
Each workload retains bounded correctness and physical retirement checks.

The existing 8 ms owner-call investigation trigger is a local prioritization aid, not a
performance assertion, product support ceiling or release SLO. A miss requires attribution
before selecting async preparation, incremental document candidates, culling or parallelism.

## Results And Decisions

### Rust Iteration

The independent Collect Game copy retains its real 62-tick headless workflow, three collected
targets and victory result. Function-body edits alternate the movement multiplier between 1.0
and 1.1. Structural edits add/remove an actual `f32` field on the private runtime `GameState`
Resource, initialize it and use it for movement; no persistent schema changes. The exact player
result alternates between x=108.0 and x=108.8. Every observation asserts that result after timing.

One initial warm-up, five no-edit invocations, five alternating body edits, one structural
baseline warm-up and five structural edits run in that fixed order. The initial build/run took
35.707 s and is excluded from retained results; it is not a cold build.

| Path / saved source to successful process exit | Median seconds | Observed range seconds | Samples |
| --- | --- | --- | --- |
| No-edit Cargo invocation plus fresh execution | 1.313 | 1.251-1.780 | 5 |
| Function-body edit | 6.633 | 5.676-7.252 | 5 |
| Runtime Resource layout edit | 7.633 | 7.107-10.224 | 5 |

These observations identify a seconds-scale rebuild/restart cost, not a stable P95 or proof
that structural compilation is inherently slower than every body edit. The class order, small
population and existing shared cache prevent a causal compiler comparison. Rust build invocations
include Cargo, frontend, code generation and linking; further attribution is required before a
compiler/linker or dependency-splitting decision. A second engine language or native hot-patch ABI
does not follow from these figures. Retain ADR 0093's correct rebuild path and measure the actual
Editor Play/first-presentation endpoint separately before selecting a faster path.

The [collector](../../tests/support/rust_iteration_cost_measurement.py) edits only a retained
disposable game copy; ordinary sample sources remain unchanged. It records compiler artifacts
to identify the actual executable, captures separate build and process intervals, and publishes
logs outside the measured endpoint. [Raw observations](data/2026-10-20-production-costs/rust-iteration.json)
retain all seventeen observations, variants' hashes, toolchain and explicit build environment.
Machine paths in the committed copy are replaced with descriptive placeholders; the local
original and per-command logs remain under the shared target.

```powershell
$env:RUSTUP_TOOLCHAIN='1.95.0'
python tests/support/rust_iteration_cost_measurement.py --samples 5 --output target/rust-iteration-cost-current.json
```

### Interaction And Runtime

All three optimized processes pass in each lane. Editor retains 768 numeric rows per process;
runtime retains 654, including labelled warm-ups and six one-off close observations. Runtime
has 36 retained timing groups of sixteen samples. The actual offscreen adapter is RTX 4090,
NVIDIA 617.14, **Vulkan**. The environment variable used by other native tests does not select
the backend for this `Instance::default()` fixture; these results do not qualify DX12 timing.

Figures below are per-process P50 / P95 milliseconds. P95 is the sample maximum.

| Editor / 4,096 entities or Browser entries | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Scalar edit dispatch | 120.043 / 126.704 | 116.953 / 171.079 | 119.111 / 129.948 |
| Frame drive after edit | 59.586 / 66.723 | 60.693 / 64.989 | 60.969 / 67.522 |
| Undo dispatch | 120.026 / 130.516 | 116.439 / 137.187 | 119.171 / 127.690 |
| Frame drive after Undo | 59.525 / 69.288 | 59.634 / 64.470 | 61.297 / 67.742 |
| Browser Refresh maximum drive | 63.617 / 188.584 | 50.355 / 119.561 | 70.092 / 129.433 |
| Browser terminal publication drive | 1.135 / 1.517 | 1.011 / 1.450 | 0.996 / 3.473 |
| Browser Refresh completion | 781.063 / 1,650.725 | 690.669 / 949.378 | 879.214 / 1,178.156 |
| Browser Scene Open dispatch | 64.059 / 68.662 | 63.147 / 74.633 | 61.859 / 62.930 |
| Browser Scene Open maximum drive | 62.108 / 70.527 | 65.592 / 72.130 | 62.549 / 67.827 |

| Runtime / map cells and operation | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| 4,096 static / managed drive | 1.145 / 1.503 | 1.446 / 1.809 | 1.233 / 1.501 |
| 4,096 changing / managed drive | 1.135 / 1.534 | 1.499 / 1.979 | 1.228 / 1.587 |
| 65,536 static / managed drive | 27.866 / 30.785 | 32.942 / 34.925 | 32.109 / 34.278 |
| 65,536 changing / managed drive | 25.882 / 27.056 | 34.020 / 36.604 | 32.073 / 35.365 |
| 65,536 static / WGPU prepare | 2.346 / 2.681 | 2.773 / 3.083 | 2.562 / 2.925 |
| 65,536 changing / UI packet plan | 0.069 / 0.408 | 0.049 / 0.894 | 0.047 / 0.451 |

All numeric observations and 252 per-process summary groups are retained under
[baseline summary](data/2026-10-20-production-costs/baseline-summary.json), including filenames,
nextest identities and uncompressed digests. The six gzip JSONL files retain every raw row.
To reproduce a group, sort its sixteen `elapsed_ns` values; P50 is element 8 and P95 is
element 16 (one-based). Exclude runtime samples -2/-1 and the separate sample-16 close row.

The Editor workload uses 64 / 1,024 / 4,096 shallow Scene entities with one built-in
Transform2d record each. A selected rotation changes to 0.125 and Undo restores 0.0,
history depth, dirty state and unchanged saved bytes. Browser refresh alternates one marker
among 64 / 1,024 / 4,096 small unrecognized text sources. Its terminal publication interval
includes index finalization, safe projection, the first query slice and frame publication.
Zero later-query time means no separate query call was needed. Scene Open alternates two
equally sized clean sources, forces actual admission/replacement, and acknowledges each
completed CloseDocument result outside timing. Completion includes bounded 1 ms polling.

The first optimized interaction attempt failed after one replacement because the collector
omitted that ordinary terminal-result acknowledgement. Its partial samples are excluded.
The fixture is corrected at the consumer boundary; no product admission rule is weakened.

Runtime workloads contain one Tilemap entity with 64 / 4,096 / 65,536 unit cells, one camera
with world height 32 and width approximately 56.89, and eight HUD labels plus a native-size
blue marker. The geometric intersection is 64 cells for the small map and approximately
1,856 for either larger map. Static and combined one-cell/one-label mutation are separate
cases. All queued-cell counts, actual semantic label, nonzero glyph ink, final changed-cell
and UI-overlay readback are asserted outside the timed calls.

The real managed drive includes ECS mutation, fresh UI compilation, transforms and sprite
extraction/queue/sort. UI packet planning and WGPU preparation follow separately. Preparation
includes GPU allocation/upload; encode/submit includes CPU command encoding and registration.
Every sample drains the prior submission. The encode-start-to-completion-callback interval
overlaps submission and drain; never sum these distributions. Packet construction, immutable
payload capture and admission between timers mean these rows are not a complete frame budget.

Resource columns repeat one post-drain snapshot across phases. Tracked current bytes exclude
completion-retired instance payload, even while local prepared objects retain handles; peak
includes prepared/in-flight logical instance payload. Canvas bytes
are already included in tracked texture totals; glyph snapshot bytes are a separate CPU byte
count. The destination texture, material/composite uniforms, driver padding and many other
allocations are outside tracked totals. Retirement timing covers the managed close-to-Retired
transition only; the retained Runtime owner/World is dropped later, so CPU destruction and heap
reclamation are not measured. The
the zero resource snapshot follows later explicit planner/backend cleanup and is not evidence
of instantaneous driver deallocation.

For 65,536 cells, each sample prepares 5,250,000 logical instance bytes: 65,536 world quads
and 89 UI quads at 80 bytes each. The fixed canvas is 230,400 bytes. Static peak tracked payload
is 6,528,980 bytes; changing text peaks at 8,626,132 while old glyph-page versions remain within
the existing grace window. That history later falls back to one 1 MiB glyph page. It is not a
demonstrated leak. All samples have zero pending/lost retirements; all six explicit backend
cleanup snapshots per process have zero tracked current payload.

## Owning Boundaries And Next Changes

1. **Remove proven quadratic schema lookup first.** Generic Inspector projection queries an
   Available component once per entity. The current Scene/Prefab accessor scans all entities
   and classification facts for each query. Use one private index built with the admitted
   classification, preserving exact identity, readiness, version and schema basis. Do not
   expose an unchecked schema-by-ID route. Repeat the public workload after the correction.
2. **Attribute remaining scalar preparation.** A field edit and Undo clone the complete document,
   validate publication shape, canonicalize and classify. Format validation currently encodes and
   shape-parses both pretty JSON and RON. Keep these acceptance rules until a bounded equivalent
   is proved; isolate their costs before choosing touched-record preparation or an async commit
   contract. Whole-document work must not be disguised as a need for a general Future trait.
3. **Move measured Browser transport work behind its existing owner.** Enumeration dominates
   the scan calls; terminal publication is smaller in this fixture. A finite owned scan worker
   should preserve attempt identity, last-good snapshot, capability/lineage checks, coalescing,
   cancellation and physical cleanup. Scene Open needs separate immutable preparation and exact
   owner admission; its second digest observation is an existing safety check, not automatically
   redundant I/O. Never publish successful editing before its owner commits.
4. **Measure Tile extraction/queue/sort, then select view/chunk locality.** All 65,536 cells are
   processed despite approximately 1,856 intersecting the view. The measured drive exceeds one
   60 Hz frame before other frame work, whereas this small HUD does not establish similar
   pressure. Existing Bevy visibility and Godot dirty-quadrant designs are useful references;
   they do not select a cache implementation or justify another job kernel by themselves.
5. **Keep Rust rebuild attribution separate.** The measured headless result is seconds away.
   Inspect dependency/codegen/link and actual Play-to-present intervals before a hot-patch ABI,
   second language or speculative crate split. Extend the next complete game workflow after
   these costs have explicit owners; Tile authoring tools remain deferred.

Pinned source comparisons: Bevy `85503941` consumes `RenderVisibleEntities` in
[queue_sprites](../../repo-ref/bevy/crates/bevy_sprite_render/src/render/mod.rs); Godot
`81ab961f` processes dirty rendering quadrants/cells in
[TileMapLayer](../../repo-ref/godot/modules/tilemap/tile_map_layer.cpp). These are source design
references, not comparative engine benchmarks or latest-upstream claims.
