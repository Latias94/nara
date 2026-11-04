# UI Work Observation Baseline

Status: the first R11a work-observation and dependency prerequisite, measured against `3bcb83e`
plus the accompanying implementation. This is not the complete R11a baseline or a performance
improvement. The [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
still requires native active-View, allocation, upload/GPU, latency and numeric
budget evidence before the measured-reuse work.

## Observation Contract

Each existing owner exposes a detached cumulative snapshot. Counters saturate, do not reset on
resource retirement and do not authorize more work, publication, input or presentation. They do
not change admission limits, revision allocation, last-good fallback or exact backend outcomes.

| Owner | Observed work | Important exclusions |
| --- | --- | --- |
| `TextEngine` / `TextWorkStats` | Direct layout attempts and submitted UTF-8 bytes, complete shaped glyphs before result admission, raster attempts and produced image bytes, paint-window attempts and visited glyphs | Direct Parley work inside `TextEditor` construction/edits/projection, font setup, validation-only calls, retained memory and GPU work |
| `UiCompiler` / `UiCompileWorkStats` | Compile/root attempts, committed frames, created Taffy nodes, layout passes/callbacks, last-good fallback selections and its own text-engine work | Description construction, allocation, active incremental layout reuse, other engines and rendering |
| `UiPresentationPlanner` / `UiPresentationWorkStats` | Plan attempts, successfully retained packets/output primitives and its own text-engine work | Partial lowering of refused plans, compiler shaping, backend expansion, uploads and GPU work |
| Private Inspector/Hierarchy `OwnedPanelWork` | Controller calls, successfully built descriptions, stored source-node counts and same-frame returns | Description attempts refused before snapshot construction, byte allocations, Session work and native presentation |

Rejected operations count only the work reached at the phase documented by the corresponding
field. Submitted source bytes are not scanned or retained bytes. Produced raster bytes can be
discarded later. Successfully lowered primitives are not an accounting of all lowering work.
Do not add overlapping nested text snapshots as independent work.

`UiAuthoringSnapshot::source_node_count()` sums the stored envelope counts, including original
counts for rejected root bodies, without walking the trees again. A retained root is an inert
failure fallback; it does not demonstrate active incremental reuse. The public tests keep those
distinctions observable through successful/rejected candidates and exact presentation outcomes.

Counting every shaped glyph before projection includes layouts rejected by line/glyph admission.
Parley 0.11.0 has no public constant-time full glyph count: `Layout::len()` counts lines and
`Run::len()` counts clusters. The observation adds one line/run/cluster/glyph traversal, O(L + R +
C + G) time and O(1) auxiliary space. Its overhead is included below. Folding it into the
short-circuiting projection loop would change the documented count for rejected layouts.

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

## Native Window Protocol

The native probe uses the existing hidden-window Windows/DX12 Host and trusted compiled Game
input fixture. It builds no external project and performs no code reload. It selects the actual
`image-source` entity, starts Play, captures the Game image, and holds D for 16 warmup plus 128
sampled 20 ms owner drives. The source Inspector is read-only during Play. Every sample requires
the sprite to move within the visible Game image, the Runtime frame and image version to advance,
and the original Game and unchanged Inspector occurrences to be composited in that same current
primary-window frame. Source values, selection, revisions, history and dirty state cannot change.

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
provide the separate completion/presentation proof. Process/Rust allocations, complete native
backend upload bytes, driver residency, GPU timestamps and OS input/display latency remain open.

The two original Game/Runtime-UI correctness trajectories also pass after sharing their existing
setup/retirement harness (`becf4af8-f519-4145-89d8-68ae842cd914`, 2/2). The focused follow-up
review finds no remaining P1/P2 after the final-health correction. This native baseline records
remaining description work and measurement limits; it does not select a cache implementation or
claim R11a's remaining metrics/budgets, an independent self-drawn Editor or complete R11/R12.

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
