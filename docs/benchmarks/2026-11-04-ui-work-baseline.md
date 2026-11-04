# UI Work Observation Baseline

Status: the first R11a work-observation and dependency prerequisite, measured against `3bcb83e`
plus the accompanying implementation. This is not the complete R11a baseline or a performance
improvement. The [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
still requires native active-View, allocation, upload/GPU, latency, optimized-build and numeric
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
```

The current composition still hosts Nara panels in Dear. Neither probe is an intact pure-ImGui
reference comparison or an independent self-drawn Editor. Neither includes an active render
View, native input/display, cold graphics construction, allocation/retained bytes, upload/GPU,
theme/typography changes or multiple native windows. No numeric SLA, cache architecture or
optimization is selected from these debug observations. R11a remains open until its matched
optimized/native workloads and fixed budgets are recorded; R11b-f and complete R11/R12 remain
separate implementation work.
