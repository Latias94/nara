# Nara Editor Module Seams

**Date:** 2026-10-16

**Source baseline:** `072a0da`

**Scope:** Source research for an editor refactor plan; no implementation or native-platform validation is claimed.

## Finding

Keep the existing document, history, authoring, and execution owners. Refactor the private Editor
product around them. Nara already separates durable tooling semantics from egui, but its product
controller, native host, presentation bridge, and fixed panel composition still assume egui and one
central viewport. Replacing widget calls alone cannot establish docking, detached native windows,
or simultaneous independent engine views.

This matches [STRATEGY.md](../../../STRATEGY.md): independently understandable modules serve a
coherent production workflow, and abstraction needs a concrete consumer. [ADR 0015](../../architecture/adr/0015-editor-tooling-and-dogfooding-boundary.md)
already assigns docking/layout to the shell and window/GPU authority to the Host. Its explicit
warning also applies: one second-toolkit panel does not prove complete toolkit replacement.
[ADR 0114](../../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
keeps ordinary project authoring non-executable; a UI migration must preserve that product.

## Confirmed source structure

### A useful semantic boundary already exists

`crates/nara_tooling/Cargo.toml` depends on assets, identity, reflection, Scene, filesystem,
diagnostics and ECS, with no egui, Winit or WGPU dependency. Its immutable `EditorFrameModel`
(`src/frame.rs:230`) carries workspace, project, diagnostic, persistence and import projections.
`EditorActionBatch` (`frame.rs:572`) returns ordered semantic actions. Document mutation uses an
explicit base revision, while geometry-sensitive actions retain displayed-frame authority.

The actual owner is `EditorProjectSession::dispatch_editor_actions`
(`src/project_host/runtime/editor/actions.rs:27`). It checks the current frame only when
`batch.requires_current_frame()` requires it, then dispatches against the published model.
`dispatch_document_mutation` verifies live document revision (`actions.rs:339`). This implements
ADR 0117's distinction between document transactions and displayed geometry. Reinstating a
blanket same-frame rule during UI migration would regress existing programmatic editing.

The UI adapter is not currently a second document store. `EguiEditorPanel`
(`crates/nara_tooling_egui/src/editor.rs:63`) owns Inspector buffers, browser presentation,
scene/game/UI panels, project-open dialog state and sidebar choice. Its output contains semantic
actions plus transient view interaction (`editor.rs:150`). Preserve that distinction: a text
buffer or dock selection is toolkit-local; selection/history/dirty state remain authoritative
workspace facts.

### The product controller still imports toolkit concepts

`editor/src/app.rs:29` imports the egui adapter's shell inputs, viewport enums, project actions and
mutation wrappers. `EditorApplication` owns `EguiProjectShellPanel` (`app.rs:229`), and its public
`show` method takes `&mut egui::Ui` (`app.rs:773`). Its project-replacement progress window is
constructed with `egui::Window` inside `show_with_viewport` (`app.rs:905`).

Likewise, `PublishedEditorProject` owns both `EditorProjectSession` and `EguiEditorPanel`
(`editor/src/shell.rs:66`). Project manager, retry and recovery states carry
`EguiProjectOpenDraft` (`shell.rs:94`); `EditorProjectRequest::from_draft` and `draft` convert
between product paths and the adapter type (`shell.rs:35,48`). These are concrete migration
seams, not evidence that the underlying state machines are wrong.

UI Prefab rendering additionally constructs closures that lower continuous, discrete, direct and
structural intents through `EditorUiAuthoringFrame` (`app.rs:825-879`). Keep that admitted semantic
lowering in its domain owner. Move adapter wrapper construction out of the application controller;
do not duplicate its mutation rules inside a new toolkit or invent a universal widget language.

### Native and GPU topology is deliberately singular

`NativeEditorHost` (`editor/src/platform.rs:297`) owns one `Option<Arc<Window>>`, one egui platform
state, one renderer, one `EditorViewportSlot`, and one selected central viewport. Its event filter
ignores window IDs other than that single native window (`platform.rs:829`).
`editor_window_packet` always emits `NaraWindowId::PRIMARY` (`platform.rs:2749`). Closing that
window requests product exit (`platform.rs:858`). Detached-window close needs different semantics.

`WgpuEditorExecution` explicitly describes itself as a private fixed-topology owner
(`crates/nara_render_wgpu/src/editor_execution.rs:591`). It retains a single
`viewport_target: Option<WgpuEditorViewportTarget>` (`editor_execution.rs:599`). The underlying
backend already has per-window surface bookkeeping (`backend.rs:445`), but that does not make the
Editor execution facade multi-window or multi-view.

`EditorViewportSlot` also retains one optional texture binding (`editor/src/platform/viewport.rs:71`).
Its source enum distinguishes Scene, trusted in-process Game and UI Prefab (`viewport.rs:35`).
The slot rejects changing producer before retiring the previous source (`viewport.rs:252`).
Consequently docking an Inspector is a smaller change than displaying independent Scene and Game
views together, and neither is implied by a toolkit's multi-viewport flag.

The GPU ownership pattern is valuable: the Host creates one `WgpuEditorExecution`, borrows raw
device resources through a scoped callback, and initializes the toolkit renderer inside it
(`platform.rs:2720`). The adapter registers its native texture in
`EditorViewportSlot::prepare_target` (`viewport.rs:157,187`); commit and cancellation free the
right registrations (`viewport.rs:218,225`). A replacement toolkit should use this lifetime seam,
not create a second device, queue, event loop or competing submission authority.

### Input and presentation carry real ownership facts

`EditorGameInputRouter` owns capture, held keys/buttons and generation-bound routing
(`editor/src/platform/game_input.rs:118`). Geometry mapping currently accepts `egui::Rect`
(`game_input.rs:54`), although its calculation already delegates to engine `DisplayMapping2d`.
Move only that geometry boundary to the existing neutral `DisplayRect`; preserve the input lease,
release and stale-presentation behavior.

`handle_window_event` decides whether events reach the toolkit after Game capture
(`platform.rs:842`). It releases/invalidate input on transitions, focus loss, DPI changes and
resizing. New native windows require routing by actual window and panel instance, including a
defined keyboard-focus owner. An unfocused Inspector must not inject Game input; moving a captured
view to another window must release or explicitly transfer its route.

The source has no explicit Editor-level IME/accessibility conformance owner in the inspected
paths. Winit events feed the egui platform adapter and its platform output returns through
`handle_platform_output` (`platform.rs:1734`). This is an evidence gap, not a confirmed missing IME
implementation. A dear-imgui trial needs actual text composition, clipboard, DPI and focus tests;
successful ASCII input or a screenshot cannot substitute for them. Accessibility limitations need
an explicit product statement rather than an assumed toolkit feature.

## Proposed module changes

Keep this first refactor inside existing owners unless independent consumption requires a crate:

| Current location | Target responsibility and removal |
|---|---|
| `editor/src/app.rs`, `shell.rs` | Toolkit-free product controller: open/retry/replacement/close, neutral launch draft and shell projection, semantic actions. Remove egui `show` methods and adapter fields from product states. |
| `nara_tooling` | Preserve document/history/frame/action ownership; add only concrete missing semantic panel projections. Do not migrate it into a new Editor kernel. |
| `nara_tooling_egui` | Temporary migration adapter. Keep field buffers and drawing code here until full parity; remove the obsolete default shell/fixed composition after replacement, with explicit retained debug consumers if any. |
| Private toolkit adapter module | Implement dear-imgui shell and panels against the same product/tooling projections. Own widget IDs, draft buffers and toolkit texture IDs. No project filesystem or Runtime execution authority. |
| `editor/src/platform.rs` | Split actual responsibilities: native window lifecycle/event routing, UI integration, and frame driving. The native Host remains the sole event-loop/window/GPU owner. Avoid several traits that merely forward these fields. |
| `platform/viewport.rs` | Separate engine presentation and per-view identity from toolkit texture registration. Replace singular slots only for the bounded multi-view consumer admitted by the plan. |
| `nara_render_wgpu::editor_execution` | Extend private execution to a bounded set of real window surfaces and view targets when detached windows/parallel views require it; retain one backend/device authority and exact completion identities. |

A small trusted-Rust panel catalog can describe stable panel type/instance identity, title,
availability and toolkit-specific renderer construction. Shell layout records contain only those
identities, placement and local layout data. A panel receives immutable context and emits existing
commands. Prove it with one independently registered diagnostics or asset-information panel.
Do not accept executable project contributions merely because the project manifest names them.
Dynamic plugin ABI, universal extension package kernels, all-toolkit widget schemas and a parallel
command bus remain outside this consumer.

Future custom UI can reuse semantic projections, commands, assets, engine presentation and Host
integration. Its widget drawing, input adapter and panel layout implementation will still need
work. Claiming automatic toolkit portability would exceed the current contracts and evidence.

## Checks to retain and checks to consolidate

Backend namespace, texture registration, target, source generation, displayed geometry and input
route express different mutable lifetimes. Their checks are not interchangeable duplicate
validation. For example `EditorEguiRenderer::matches` checks backend/device identity
(`platform.rs:2714`), while `EditorViewportSlot::matches_prepared` also checks texture-registration
generation (`viewport.rs:120`). Removing one just because both mention a generation is unsafe.

Conversely, `redraw_project_shell` and `redraw_workspace` each perform toolkit input/output,
texture upload, render-pass creation and deferred texture freeing (`platform.rs:1169,1337`).
Extract this common toolkit frame/submission operation with the concrete new adapter. Preserve
their distinct product surfaces and completion behavior; do not build a general render graph or
source-parsing proof system to deduplicate these functions.

## Acceptance scenarios

1. Exercise project-manager open, rejected open, retained replacement, recovery and dirty-close
   directly against the toolkit-free controller. Preserve `application_contract.rs` scenarios,
   including `retry_and_replacement_close_are_failure_atomic` and
   `replacement_cancel_restores_the_exact_retained_workspace`. Move semantic assertions out of
   egui harness setup while keeping adapter interaction coverage.
2. Run an ordinary external project with unavailable project Rust: browse, custom-field edit,
   Undo/Redo, Save/Reopen, image import and compiler repair remain available. Preserve
   `first_game_workflow.rs`, `generic_open_recovery.rs`, `authoring_loop.rs` and supervised Play
   clean-room trajectories rather than replacing them with a docking demo.
3. Register the bounded external panel without touching a central panel enum; dock, float, close
   and reopen it. Restore layout with missing panel IDs by retaining valid layout and reporting or
   ignoring unavailable entries. Closing a detached tool window must not close the project.
4. Move one real engine view between windows, resize, change DPI, minimize, close and recreate it.
   Verify texture retirement, GPU recovery, input release, correct pixel mapping and stale pick
   rejection. Prove simultaneous views separately if the selected scope claims them.
5. Preserve WGPU tests for exact sampled image/window identity, producer handoff, stale target
   reservations and terminal UI completion (`editor_execution.rs:1134-1414`), plus content mapping
   (`game_input.rs:528`). Add behavioral multi-window coverage; do not emulate a Rust compiler in
   validation scripts.

The inspected source confirms architectural constraints, not a new correctness defect. Native
Windows/Linux/macOS multi-window, IME, accessibility and final toolkit feature parity require
their own measured implementation evidence.
