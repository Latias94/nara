---
type: "Subagent Finding"
title: "Nara diagnostics and debugging productization"
description: "Audits Nara's implemented observability surfaces against current Godot, Unity, Unreal, and Bevy primary sources and routes product work without inventing a second diagnostics core."
timestamp: 2026-07-20T05:04:49Z
record_id: "8e7063fdcfa54bac831ba17f0bc7304c"
tags: ["nara", "diagnostics", "debugging", "profiling", "tooling", "observability"]
status: "complete"
producer_id: "codex-diagnostics-product-research"
run_id: "session-019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
subagent_id: "diagnostics_product_research"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bca2eeb86f086989bbe1df2fe09a836c414f43ee"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
---

# Finding

Nara does not need another diagnostics core. It already has a stronger low-frequency diagnostic
contract than the compared engines expose as one common API: typed control-flow errors, bounded and
privacy-classified operation reports, a bounded ordered runtime bus, and a separate numeric pressure
resource. The product gap is that almost no production domain publishes into the runtime surfaces
and no tooling view consumes them.

Productization should preserve four distinct data planes:

| Plane | Data shape | Nara owner | Intended product surface |
|---|---|---|---|
| Low-frequency diagnostics | Stable code, severity, producer/domain, classified fields, repeat and loss accounting | `nara_diagnostic::RuntimeDiagnostics` plus operation-local `DiagnosticReport` | Problems/runtime diagnostics console and headless inspection |
| Numeric pressure and profiling | Latest gauges/counters now; sampled timings and histories only after a measured need | `RuntimePressureSnapshots` now; OQ-025 for profiler capture | Monitors and profiler charts |
| High-frequency trace/timeline | Ordered spans/events, correlation, channel selection, drop evidence, capture session | Not implemented; OQ-025 and OQ-041 | Timing/gameplay timeline and later remote sessions |
| Visual/render debugging | One selected frame's packet, phase, draw, resource, and graphical-state evidence | Not implemented; render owners plus a concrete OQ-025/OQ-041 consumer | Frame debugger or visual gameplay log |

Sharing an Editor shell does not make these one storage model. Godot, Unity, and Unreal all present
several tools together while retaining different capture costs, lifetimes, and semantics.

## Verified Nara State

This audit used the live worktree at `bca2eeb86f086989bbe1df2fe09a836c414f43ee`; concurrent dirty
files were not modified. The implementation ledger truthfully marks ADRs 0009, 0048, 0068, and
0076 `partial`.

### Implemented foundations

- ADR 0009 and `crates/nara_diagnostic/src/{report,field,identity}.rs` implement bounded
  `DiagnosticReport` entries with stable codes, static summaries, classified fields, sticky
  severity, byte/count limits, deterministic eviction, and explicit tracing sinks. Typed errors
  remain control flow; reports are observations, not arbitrary error strings.
- ADR 0048 and `crates/nara_diagnostic/src/runtime.rs` implement `RuntimeDiagnostics` with ordered
  sequence numbers, producer/domain/code/severity identity, explicit dedupe, first/last frame and
  repeat count, filters, bounded snapshots, deterministic count/byte/frame retention, saturating
  loss statistics, and a caller-owned tracing cursor.
- ADR 0068 and `crates/nara_diagnostic/src/pressure.rs` implement a separate latest-snapshot store.
  Each source publishes bounded `u64` gauges or counters with stable IDs and typed units. It is an
  observation surface, not a global budget manager, histogram store, or overload policy engine.
- `DiagnosticsPlugin` installs both resources and retention maintenance in `CoreStage::First`.
  Root product plugin groups install it for ordinary product compositions, and project profiles can
  select validated runtime diagnostic limits.
- `nara_tooling` already emits operation-local `DiagnosticReport` values for Inspector, workspace,
  and Play commands. These are useful command results, but they are not an accumulated runtime
  timeline.

### Producer and consumer coverage

- `crates/nara_asset_watch/src/observability.rs` is the only production bridge found that publishes
  `RuntimeDiagnosticDraft` and `RuntimePressureSnapshotDraft`. It lowers six watcher failure kinds
  to deduplicated warnings and publishes bounded queue occupancy, high-water, admission, discard,
  and failure counters. Its tests exercise both runtime resources.
- Asset import/reload retains `AssetReloadDiagnostics` locally. Task pools expose `TaskStats`;
  rendering exposes `RenderBackendStatus`, `FrameStats`, `WgpuFrameTransactionStats`, and cache
  stats; App/render paths also report terminal `RuntimeFault` values. None currently has a production
  bridge into both shared observation resources.
- The root facade re-exports and configures the resources but is not another producer. Outside
  `nara_diagnostic`, only asset-watch production code constructs runtime drafts.
- `nara_tooling_egui` currently renders the Scene Editor and Inspector. It has no model or panel for
  `RuntimeDiagnostics` or `RuntimePressureSnapshots`, and `nara_tooling` exposes no UI-neutral
  runtime diagnostic console model.

The immediate deficiency is therefore producer coverage and product consumption, not the absence
of a diagnostic bus. A panel populated only by synthetic entries would hide that deficiency.

## Primary-Source Comparison

### Godot

Godot's stable documentation separates Output, the Debugger panel, runtime debug visualization,
and Remote scene inspection. Within the Debugger it still distinguishes errors, script profiling,
a rendering-specific Visual Profiler, network counters, automatically retained Monitors, and VRAM
inspection. The Remote scene dock can inspect and change running node properties. See the official
[debugging-tools overview](https://docs.godotengine.org/en/stable/tutorials/scripting/debug/overview_of_debugging_tools.html)
and [Debugger panel](https://docs.godotengine.org/en/stable/tutorials/scripting/debug/debugger_panel.html).

Relevant lesson: Nara should offer one discoverable debugging workspace, but retain separate data
models for actionable failures, continuously sampled metrics, opt-in profiling, and runtime object
inspection. Godot's Remote node mutation is a workflow reference, not a reason to replace Nara's
stable-identity and safe-point edit contracts with a general mutable object inspector.

### Unity

Unity's Profiler connects to Editor, local players, or target devices and organizes performance
capture into modules, frame navigation, saved sessions, markers, counters, and optional deeper
instrumentation. Its GPU module is not enabled by default because of overhead. See the official
[Unity Profiler](https://docs.unity3d.com/Manual/Profiler.html) and
[Profiler window reference](https://docs.unity3d.com/Manual/ProfilerWindow.html).

Unity keeps the [Frame Debugger](https://docs.unity3d.com/Manual/FrameDebugger.html) separate: it
selects a frame, lists rendering events, steps through them, and shows the graphical state at each
point. Relevant lesson: a Nara render debugger should consume an on-demand frame execution capture,
not diagnostic entries or pressure gauges, and high-overhead instrumentation must be explicitly
enabled.

### Unreal Engine

Unreal's [Visual Logger](https://dev.epicgames.com/documentation/unreal-engine/visual-logger-in-unreal-engine)
records actor snapshots, categorized messages, visual state, and a scrub-able timeline for live or
post-session gameplay debugging. This is distinct from
[Unreal Insights](https://dev.epicgames.com/documentation/unreal-engine/unreal-insights-in-unreal-engine),
which captures high-rate trace events and analyzes CPU/GPU timing, counters, memory, and other tracks.
The official [Trace documentation](https://dev.epicgames.com/documentation/en-us/unreal-engine/trace-in-unreal-engine-5)
uses selectable channels to control data rate and a separate Trace Server/store for recorded data.

Relevant lesson: a gameplay timeline and a performance trace can share correlation vocabulary
without sharing retention or capture machinery. Always-on `RuntimeDiagnostics` must not absorb
per-system spans, per-entity state, allocations, GPU timing, screenshots, or frame packets.

### Bevy

The checked-in Bevy `0.20.0-dev` reference at commit
[`f6c6e6e`](https://github.com/bevyengine/bevy/tree/f6c6e6eebb94e81c090614f19039319e9acb3c85/crates/bevy_diagnostic)
implements a numeric `DiagnosticsStore`: named `f64` measurements retain a configurable history and
provide latest, average, and exponentially smoothed values. Separate plugins produce frame time,
FPS, entity count, and system information, while `LogDiagnosticsPlugin` is only one sink. See
[`diagnostic.rs`](https://github.com/bevyengine/bevy/blob/f6c6e6eebb94e81c090614f19039319e9acb3c85/crates/bevy_diagnostic/src/diagnostic.rs),
[`frame_time_diagnostics_plugin.rs`](https://github.com/bevyengine/bevy/blob/f6c6e6eebb94e81c090614f19039319e9acb3c85/crates/bevy_diagnostic/src/frame_time_diagnostics_plugin.rs),
and [`log_diagnostics_plugin.rs`](https://github.com/bevyengine/bevy/blob/f6c6e6eebb94e81c090614f19039319e9acb3c85/crates/bevy_diagnostic/src/log_diagnostics_plugin.rs).

Relevant lesson: Nara can copy the small producer-plugin ergonomics and lazy/optional measurement
pattern. It should not merge Bevy-style floating-point metric history into its privacy-safe fault
bus, adopt `bevy_app`, or treat periodic logging as the product UI.

## Recommended Routing

### U17: first low-frequency product consumer

Keep U17's Save/reopen/Play lifecycle as its critical path. Add only the diagnostic surface needed
to make that flow explain failures:

- A UI-neutral, read-only tooling view over bounded active-runtime entries and operation-local
  reports. Preserve their provenance instead of republishing command reports as runtime history.
- An egui Problems/Runtime Diagnostics view showing severity, code, producer/domain, first/last
  frame, repeat count, safe classified fields, and rejected/evicted/truncated accounting.
- Focused bridges only for faults U17 directly exercises and owns at composition boundaries, such
  as Editor preparation, persistence, Play start/control, and retirement outcomes. The rich typed
  error/status remains with its domain.
- A compact pressure section may display latest snapshots and units, but U17 should not add graph
  history, sampling policy, timers, remote transport, or a profiler.

This is a product consumer and a narrow producer-coverage slice. It does not reactivate the entire
legacy U31 backlog and should not block honest Save/Play completion if the panel cannot be completed
without broadening U17.

### U14: measurement evidence, not a profiler framework

U14 should record its already-required frame-time, memory, build, and render-packet cost samples in
the compact offline baseline owned by the plan. It may read existing numeric status/counter sources,
but it must not publish percentiles or product decisions back into `RuntimePressureSnapshots`.
Record which production producers are absent as a measured tooling bottleneck; do not implement a
generic capture session, exporter, remote profiler, or diagnostic convergence project in U14.

For rendering, U14 should retain packet batch/instance/byte and observed clone/allocation counters.
It should not build a frame debugger. A concrete unexplained rendering defect or measured GPU/CPU
timing bottleneck is the evidence needed to admit the next capture slice.

### OQ-025: opt-in profiler, trace, crash, and telemetry contracts

Use OQ-025 only after its trigger: a measured regression or production crash requires information
the bounded buses cannot represent. The future decision must separately specify channels, capture
session ownership, span/counter correlation, clock domains, producer overhead, loss evidence,
privacy, bounded in-memory/file retention, export, and crash-safe handoff. CPU/GPU timing, task and
system spans, allocation/call-stack sampling, and high-rate render execution belong here, not in
`RuntimeDiagnostics`.

### OQ-041: incremental/remote observation session

Use OQ-041 when Editor, child-process Play, AI tools, or a remote target needs an initial baseline
plus incremental updates and commands. Its contract must own monotonic sequence, subscription
lifetime, coalescing/drop and resynchronization, generation identity, command/result correlation,
backpressure, disconnect, and reconnect. A Visual Logger-like gameplay timeline can become one
consumer, but temporal adjacency must not be labeled causality. Remote transport must not be chosen
before the local bounded model and disclosure policy are proven.

An on-demand render-frame snapshot may remain local. Streaming frame events or controlling a remote
capture invokes OQ-041; high-rate CPU/GPU instrumentation or trace export invokes OQ-025. A feature
that needs both must satisfy both contracts rather than creating a fifth universal bus.

## Explicit Non-goals

- No second engine-wide diagnostic bus, free-text event map, or tracing-as-source-of-truth design.
- No gameplay, admission, overload, or replay decision based on diagnostics or pressure snapshots.
- No always-on per-system, per-entity, per-draw, allocation, call-stack, screenshot, or pixel capture.
- No process-global panic hook, telemetry exporter, network protocol, authentication scheme, or
  stable public trace ABI in U17 or U14.
- No dynamic mutation of arbitrary runtime ECS state from the diagnostic console.
- No wgpu device/surface/encoder handles, runtime `Entity`, host paths, error strings, secrets, or
  unclassified component payloads in diagnostic, trace, or render-debug records.
- No claim that a diagnostic console, profiler, timeline, remote Inspector, and frame debugger are
  one feature merely because mature engines place them in one Editor.

## Disposition

Preserve ADRs 0009, 0048, and 0068 without a new ADR or public API decision. Treat the current work
as productization of an existing accepted foundation: first expose useful low-frequency evidence in
U17, measure real bottlenecks in U14, and admit OQ-025/OQ-041 work only from those concrete results.
The first success criterion is not panel breadth; it is that a reference-game author can see why a
Save, Play start, runtime service, watcher, or render operation failed without reading logs or engine
source, while headless consumers can inspect the same stable evidence.
