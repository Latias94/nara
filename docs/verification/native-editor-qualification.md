# Native Editor Qualification

This is the manual Windows/DX12 qualification gate for the selected Dear frontend.
Ordinary workspace tests exclude these ignored cases and do not establish native acceptance.
For a Windows/DX12 release qualification, run the three critical trajectories and the additional
groups required by the claimed capabilities. For development changes to native view/input, device
replacement, frame cadence or supervised lifecycle ownership, select the trajectories covering
the affected behavior. A focused result establishes only that tested scope.
Documentation-only changes do not require another native qualification run.

The independent-development route in ADR0118 builds a complete self-drawn Editor beside the
ImGui reference, then switches/removes the reference after complete acceptance. Existing tests
below cover the actual Dear-based composition, including hosted Nara panels; they do not prove
an independent self-drawn shell or an intact pure-ImGui reference. Record the exact reference
revision and coverage. Future successor trajectories must launch its own composition, exercise
the agreed full workflow matrix and prove no required Dear dependency/context/renderer/input/
docking service. Reciprocal embedding and mixed first-party panels are not prerequisites.

## UI Architecture Successor Scope

The [R11a-f sequence](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11a-execution-model-dependency-closure-and-work-baseline)
and [UI design](../architecture/ui-foundation-design.md) add unimplemented public panel-hosting,
custom-drawing, observation/operation and measured-reuse targets. Existing results below retain
their original scope. Select actual successor native trajectories when those capabilities land:

- Independently compile/run the Nara-drawn composition and a separately selected non-Nara Host
  conformance consumer. Each uses public drawing, text, popup, lifecycle and exact-result ports.
- When claiming optional mixed-content hosting, use a separate extension fixture to transfer
  focus, IME/capture and popup scope, then close/recreate with pending input/output. This does
  not prescribe a mixed default or require ImGui embedding during successor development.
- Two logical Views remain independent through panel relocation, fractional scale, clipping,
  resize and output retirement. Multiple Render Views alone do not qualify UI Surface aggregation.
- Public semantic targeting and real pointer/key/text routes reach the ordinary controllers;
  distinguish those paths and match the original domain result plus later presented observation.
- Static/dynamic, collection and lifecycle costs meet the workload/hardware budgets fixed by
  R11a. Record construction, layout, shaping, raster, upload, GPU and latency/memory separately.
- An independently authored composite/custom canvas measures children, draws clipped geometry,
  handles motion/wheel/keyboard and exposes semantic operations through public interfaces.
  Repeat sizing/composition without duplicate commands/tasks/subscriptions; reorder keyed state
  without moving drafts to another item. Scoped style changes do not alter another library.
- R11f actually changes source-built panel code, builds and reconstructs the Editor, then observes
  the changed UI and declared document/history/draft recovery. Exercise compile, capture, startup
  and restore failures, unsupported state refusal, fresh input incarnations and last-good recovery.
  Record change-to-correct-presentation separately from build success. Existing GPU reconstruction
  and supervised Play tests do not qualify this currently unimplemented workflow.

A finite Host conformance frontend does not establish ImGui/egui compatibility. A named toolkit
needs its actual adapter; an official Nara extension package is not required. Injected IME,
semantic actions and screenshots do not establish actual OS candidate delivery, pointer hit
testing or native accessibility respectively. Native detachment keeps separate platform evidence.

The finite [independent surface consumer](../../tests/fixtures/render-output-consumer/README.md#native-ui-surface-presentation)
qualifies the public Nara packet-to-presented-journal prerequisite separately. Build its
`ui_surface` example, then run the ignored `ui_surface` test target serially with the explicit
native backend and `NARA_WGPU_FORCE_FALLBACK=0`. It uses a hidden real window, original image
submission, designated occurrence and matching composite before draining presentation into its
read-only journal. Pointer/IME events and logical keys are injected through its adapter. Require
the complete field/Undo, presented modal popup, one action, old route/token refusal,
close/reconstruction and original Host retirement assertions. Separate error/panic controls
must settle an actual pending submission before reporting cleanup success. These results do not
qualify a non-Nara frontend, general Editor attachment, native OS event delivery or an independent
Editor Shell. The ordinary Editor trajectories below retain their distinct composition scope.

The independently authored keyed composite additionally runs under its public `MeasuredColumn`
layout, using current title/child intrinsic sizes and scoped exact placement rather than
row-index absolute coordinates. Preserve the full 11-presentation/ten-commit trajectory,
including preedit/selection/history during reorder, focused-row filtering, same-visible-position
replacement, explicit model/schema reset and rejection of old input after the replacement
receives focus. The trajectory then imports the external Lyon-backed `CurveCanvas`, presenting
custom geometry in the same stream as ordinary node/port controls. Require presented pointer
selection, Tab/Enter port activation, Shift-Tab/Enter selection, fractional authored pan with
matching geometry/semantics, old-route refusal and final retirement. The extended total is
14 presentations and 13 journal commits. Error/panic child modes must also exercise a pending
mesh submission. This is finite child-layout, custom geometry and presented-input acceptance;
curve hit testing, canvas motion/wheel and the complete R11b/R11c consumers remain separate.

## UI Work Probes

The [R11a work baseline](../benchmarks/2026-11-04-ui-work-baseline.md) records owner-local work
and separate debug/release CPU distributions. Its opt-in native probe reuses the trusted compiled
Game input fixture and the existing hidden-window Host; it performs no external Cargo build or
project-code reload. Run it separately from the correctness trajectories when measuring work:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
$env:WGPU_BACKEND='dx12'
$env:NARA_EDITOR_CJK_FONT='C:\Windows\Fonts\simhei.ttf'
cargo +1.95.0 nextest run --locked --cargo-profile release -p nara-editor --lib --run-ignored only -E 'test(measure_native_static_inspector_beside_advancing_game)' --test-threads=1 --nocapture
```

The probe keeps 16 warmup and 128 sampled moving-Game frames. Every sampled original Game image
and retained Inspector image must have matching composite receipts in that same primary-window
frame. Inspector frame/image identity, compilation/planning work and authoring values/history
stay unchanged. Timed owner drive, cursor ingress and native redraw cover the complete selected
window, not isolated UI work. Completion spans owner-drive start through the original queue's
callback, including polling delay; it is neither GPU timestamp duration nor display latency.
Observe actual native/Dear/panel scale and extent separately and reject geometry drift. After
completion drain, recheck the original device through the Host health boundary. Completed queue
callbacks and unchanged epochs are insufficient on terminal loss. The probe then destroys that
drained device outside all measured intervals and requires exact `DeviceLost` refusal with no
namespace replacement; original graphics/application retirement must still complete.
Logical Host retention/backend payload observations do not measure process allocations or complete
driver residency. Report cold setup, warm distributions, instrumentation/maintenance and physical
cleanup separately. The actual composition remains Dear-based with hosted Nara panels; this does
not qualify an independent self-drawn shell, pure-ImGui reference, OS input or cross-platform costs.

Native probes also print the existing Host transaction's `native_work()` snapshot: returned
Nara preparation Queue requests, successful nonempty mapped initialization and encoded graph
Preserve copies. Keep payload versus aligned storage bytes distinct. Later failure or cleanup
does not refund reached work; the next transaction resets it. These counts exclude Dear/private
native callbacks and arbitrary Feature commands, and do not measure hardware transfer, GPU
execution or resident bytes. Pair them with the original exact presentation/workload assertions.

The separate `measure_native_primary_gpu_timestamps_beside_advancing_game` lane requests
`TIMESTAMP_QUERY | TIMESTAMP_QUERY_INSIDE_ENCODERS` through the original Host before device
initialization. It retains the same 16 warmup and 128 complete sampled frames, exact receipts,
original-device health, actual terminal-loss refusal and physical close. It brackets primary
paint in the same encoder, adding no sampled pass or submission; resolving queries is observer
work. Mapping must report actual success and the original copy prefix must complete before
reading all 128 pairs. Query/buffer ownership survives failures until the original graphics drain.

Run that lane separately using the same ordinary Cargo features and native environment above.
Its CPU/completion times are instrumented. GPU markers exclude Game/image-producer rendering,
acquire/present/scanout, other windows and Dear's private upload bytes. WGPU 30 permits encoder
command reordering around timestamp writes; a marker interval is not a strict isolated execution
guarantee or a substitute for presentation receipts. Missing capability or invalid/missing sample
fails the complete lane rather than dropping samples or silently disabling the queries.

The separate `measure_native_repeated_lifetime_memory` probe repeats the complete ordinary
Inspector/Game workload across 4 warmup and 12 measured native lifetimes in one process. Each
lifetime retains its own 16 warmup / 128 sampled frames and original receipt, source/history,
geometry and health assertions. Alternate healthy graphics retirement with destruction of that
same drained device and exact terminal-loss refusal. Require a fresh Host identity, original
graphics/Session cleanup and release of the Rust window in every cycle. Neither path restarts
the process or relaxes the workload.

Use the same environment and release command above with the memory probe's name, in separate
serial processes when collecting a baseline. `K32GetProcessMemoryInfo` samples the whole process
before native construction, after the workload but before graphics cleanup, and immediately after
the original harness has dropped Native/window/Application and the platform window is destroyed.
Return each lifetime to Winit, require its exact `Destroyed` event, and capture at the following
about-to-wait boundary only after `IsWindow` confirms that the HWND is absent. Rust weak-reference
release alone is insufficient because Winit posts the Windows destruction request. Record the
dispatch interval separately and check a finite missing-event deadline when the event loop
returns; it does not preempt a blocked native callback. Drop per-frame observer vectors before
the live observation; retain only a preallocated, bounded set of detached cycle facts.
Report process private **commit** and current Working Set separately from lifetime peaks and
Host/backend logical payloads. Working Set includes shared pages, and peaks include setup and
warmups. The probe adds no post-cleanup sleep, allocator trim or working-set reduction. These
short repeated lifetimes do not qualify exclusive UI memory, complete GPU residency, a plateau,
a leak bound or long-running behavior. The device-loss live boundary follows the explicit loss
control, whereas the healthy boundary still owns a healthy device; retain that label in comparisons.

## Environment

Use Windows with an interactive desktop, Rust 1.95.0 and an actual DX12-capable adapter.
Run Cargo serially in the checkout with its existing target directory.
The three critical trajectories below use hidden native windows and owned Context input;
they submit and present through production paths without moving the user's pointer or activating a visible window.
The separate external-process trajectory uses a visible, initially inactive Editor window as specified below.
They do not establish monitor scanout, accessibility parity, or Linux/macOS qualification.

An unavailable adapter/desktop is a failed qualification prerequisite, not a passing skipped test.
Keep the previous platform claim unchanged until this gate can run.
A permanently provisioned native CI runner is not currently claimed.

## Required Trajectory

Inspect the intended diff and record the source revision first.
Then execute the existing critical tests explicitly:

```powershell
git rev-parse HEAD
$env:CARGO_BUILD_JOBS='1'
$env:WGPU_BACKEND='dx12'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) | test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) | test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts)' --test-threads=1
```

For the critical qualification group, all three tests must execute and pass; an empty selection
is not acceptance. A narrower development selection must identify the affected behavior and
the tests actually executed, without claiming the complete group passed.
They cover same-output Game resize and authoring input, pause/resume cadence, exact Game input and Step, independent view close/reopen, detachment/minimization, device recovery, and retirement.
The ordinary non-native Editor suite separately protects cadence policy and source-diverged Restart admission.

For toolkit/input upgrades, also exercise the real Nara Project path/Profile fields through the
registered native backend and core IME composition:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_project_fields_use_upstream_ime_composition_and_reject_stale_owner_commit)' --test-threads=1
```

This proves preedit/draft isolation, one commit and rejection of a late commit after the active
field changes. It constructs Winit events directly and does not qualify OS candidate-window
placement, actual input-method event delivery, or manual cross-window typing.

## Nara Inspector Integration

The ordinary Scene/Prefab Inspector uses Nara controls in its default Document tab. Select a
standalone TrueType CJK fallback
accepted by the current workspace shell and Nara text owner, then run the owned trajectories explicitly:

```powershell
$env:NARA_EDITOR_CJK_FONT='C:\Windows\Fonts\simhei.ttf'
cargo +1.95.0 nextest run -p nara-editor -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --lib --locked --run-ignored only -E 'test(native_owned_inspector_)' --test-threads=1
```

Use an actual installed compatible font; an unavailable path or missing glyph diagnostics fail
this trajectory. Retain the environment and DX12 prerequisites above. The hidden window uses the
ordinary Document tab, maps pointer input through confirmed image geometry, edits Chinese/string and
numeric buffers through Winit IME events, holds Apply across a new presentation, and exercises
document Undo/Redo and disk Save/Reopen. It also verifies composition isolation/cancellation,
fresh focus after an outside click, rejection of a commit before the new IME handshake, retained
input across Inspector tabs and preservation of original unavailable source records.

The keyboard trajectory uses the production logical-key entry called by Winit ingestion, with
the shared clipboard replaced by an isolated memory backend. It covers Unicode Copy/Cut/Paste,
failed Cut and invalid Paste preservation, local Undo/Redo, AltGr and modifier handoff, composition
guards, Tab/Shift+Tab, explicit Enter/Space activation, cancelled pointer gestures and queued-input
refusal before document history and disk Save/Reopen. Its boolean value-button controls also
check keyboard ownership without a text token, repeated-activation refusal, document history and
saved/reopened values. It does not read or alter the user's actual clipboard or synthesize physical
OS keyboard events.

The reference trajectory selects and cancels actual source-local target choices before explicit
Apply, ordinary Undo/Redo and disk Save/Reopen. The scrolling trajectory uses an actual 18-field
schema and production Winit wheel ingress. It covers line/physical-pixel motion, invalid/cancelled
input, clipped hit points, pressed-button cancellation, retained dirty buffers, content shrink,
ordinary history and original-record preservation through Save/Reopen. No image receipt or
kernel viewport is forged by these native tests.

The structured-field trajectory opens both an ordinary Scene and Prefab through the same shell.
It edits Optional, Record, scalar List and Map drafts through owned controls and native pointer/IME
ingress. Invalid and pending values, child cancellation and confirmation preserve the document
until whole-field Apply. The trajectory then checks exact history, ordinary Undo/Redo, typed disk
values, original unavailable records and a clean Save/Reopen. The predecessor recursive trajectory
separately protects the shared Optional child identity change.

The component trajectory creates an ordinary Scene entity and repeats the complete path in a
Prefab. Native owned-image clicks open the creation choices, add/remove the component and
fold/unfold its dirty field without changing source/history. Explicit Apply and document
Undo/Redo precede disk Save/Reopen. Actual typed saved values and the original unknown records
must survive. Entity creation uses the Nara source-declaration Hierarchy; document navigation
and persistence retain the surrounding shell. CPU controls also
refuse stale revisions, repeated menu activation, disabled duplicate creation and read-only or
unknown-record removal, and retain local text Undo while cancelling hidden composition.

The nested-reference trajectory uses the same source-local picker for Record, Optional, List and
Map leaves in actual Scene and Prefab documents. It checks duplicate-name choices, menu cancellation,
pending-child confirmation and whole-field Apply, then ordinary Undo/Redo, typed saved references
and original unavailable records through clean Reopen. Its pointer and locating wheel events use
the production owned image path; OS input-method delivery retains the separate limits below.

The retained-input recovery trajectory uses an isolated memory clipboard for Chinese/string and
invalid numeric drafts. It checks preedit exclusion, refused writes and retry, local Undo/Redo,
unprojected fields and incompatible fields after actual component removal. A separate 65-field
schema reaches the 64-draft limit through native pointer and IME ingress, activates the presented
global recovery button, rejects retired input and edits the previously refused field. Recovery
preserves source, document history, selection, original unavailable records and project files.
CPU controls separately cover exact recovery snapshot refusal and queued/pending completion IDs;
this native trajectory does not manufacture those asynchronous states or touch the OS clipboard.

An unchanged forced repaint must reuse the compiled frame and retained image receipt without
requesting a continuous follow-up. That observation is not an idle CPU benchmark. These trajectories
do not read back the native framebuffer, deliver OS input-method events or qualify candidate-window
placement, physical OS key/clipboard delivery, keyboard-only entry from the surrounding shell,
detached-window/DPI transfer or accessibility.
The ordinary panel's default replacement and remaining specialized-panel and workspace
scope are recorded under the
[owning unit](../plans/2026-11-04-owned-inspector-and-editable-text-plan.md).

## Nara Source-declaration Hierarchy Integration

For ordinary Scene/Prefab Hierarchy input, source mutations or hosted-panel routing, execute:

```powershell
$env:NARA_EDITOR_CJK_FONT='C:\Windows\Fonts\simhei.ttf'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_owned_hierarchy_) | test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive)' --test-threads=1
```

The structure trajectory repeats source-local Root/Child creation, selection, Parent and Order
choices, removal cancellation/confirmation, Undo/Redo and disk Save/Reopen in Scene and Prefab.
It preserves source IDs, routed placement predicates and original declarations. The displayed
list represents authored source declarations rather than the effective expanded Prefab tree.

The large-list trajectory uses at least 1,024 declarations per source, actual unknown records
and an unrelated dirty Inspector field. Committed filtering, cancelled preedit, long-filter
refocus at its displayed prefix, line/pixel wheel input and multiple deltas before repaint must
preserve exact Session selection, history, source bytes and the Inspector draft. Show selection
reveals the existing logical selection. Explicit field Apply and disk Save/Reopen retain all
declarations; document replacement retires the old filter's native text lifetime and refuses late
input. These tests map real Winit events through the confirmed hosted image. The cadence control
collects a real owned Hierarchy selection with Game resize in the same output before testing
paused-idle Resume time rebasing.

CPU controls separately qualify stale revision/selection/Schema/read-only refusal at activation
and final queue admission, exact result routing, small-viewport confirmation, long legal IDs,
shared display budgets in tall viewports, oversized-filter refusal, specialized projection retirement
and visible-row construction for source and choice lists.
Visible widget/layout counts do not establish bounded matching scans, allocation, idle CPU or
GPU cost. Specialized UI trees, keyboard traversal to uninstantiated rows, the workspace shell,
actual OS IME/candidate placement, assistive technology and broader platform/DPI evidence remain
separate requirements.

For the specialized UI tree and creation forms, also run the existing complete Canvas trajectory:

```powershell
$env:NARA_EDITOR_CJK_FONT='C:\Windows\Fonts\simhei.ttf'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_canvas_widgets_pick_move_resize_and_save_reopen_real_source)' --test-threads=1
```

Its structure controls now use the confirmed Nara Hierarchy image and Winit pointer/text ingress.
The test opens and cancels typed forms, creates Container/Text/Button recipes with exact parameter
values and undoes each, then creates the selected Image, deletes/undoes/redoes it and saves/reopens
the original Prefab with that one new Image. Text and Button label creation include the complete
4,096-byte capacity through native committed input. Chooser Escape retains keyboard ownership for
Tab/Enter or another Escape. Reopen switches the same document in both directions between ordinary
and specialized projections with a genuinely unpresented native candidate; neither replacement
may present or route input through the retired controller's frame.
The same trajectory retains actual Canvas picking,
move/resize, continuous Inspector Escape cancellation, ordinary history, unrelated source bytes
and project-content refresh assertions. The specialized Inspector and Canvas themselves are
separate frontend consumers; this tree/form result does not imply their Nara replacement.

## Nara Scene Integration

For the Nara-drawn ordinary Scene toolbar and viewport, run the direct-sampling trajectories:

```powershell
$env:WGPU_BACKEND='dx12'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_owned_scene_)' --test-threads=1
```

The original Scene output and Nara chrome must be sampled in the same actual native window
frame. The Scene placement has a distinct display occurrence; a same-output mirror cannot
substitute its immutable mapping. Winit wheel and middle-button input exercise pointer-anchored
zoom, pan and the Nara Reset View button. Outside-content and toolbar input do not move the
camera, and replaced mappings cannot authorize the old pick.
The same-sized floating-panel move additionally preserves the exact CPU frame and chrome route
while invalidating the Scene sample. A click during that refusal cannot borrow the next frame's
confirmation; a new click after the actual replacement sample succeeds.

The source trajectory clicks a real Sprite, applies an ordinary source edit and Undo, resizes
the native window and closes the Scene panel. It reads the exact GPU output before and after
these operations and verifies that retained predecessor pixels remain immutable after retirement.
Those readbacks are test observations; the product's viewport does not copy pixels through CPU
memory. The original Scene/Canvas priority, independent Scene effects, image-reimport and native
view-lifecycle trajectories below remain required for changes to their affected boundaries.

These tests qualify their injected native input and GPU/window receipts. Full Inspector parity,
virtualization, measured idle and retirement costs, actual OS IME/accessibility and broader
platform claims retain their separate gates.

## External Process Scheduling

For changes to external Play cadence, maintenance wake or child-exit presentation, also run:

```powershell
$env:CARGO_TARGET_DIR=Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_external_running_maintenance_stays_quiet_and_presents_child_exit)' --test-threads=1
```

This trajectory performs a real supervised Cargo build and READY handshake, then checks at least
three due process-maintenance cycles without another Editor redraw. A test-only exit trigger in
the guarded project copy ends that child with exit code zero; production Host observation must
publish and present the exit. It tests process-exit observation, not graceful Runtime teardown.

This trajectory uses a visible, initially inactive Editor window so Windows delivers its real
redraw requests; the ordinary game child can also show its desktop window.
No OS pointer or keyboard input is injected. The copied fixture, child and native graphics are
retired by their existing owners. Allow up to ten minutes for build/READY on a cold target.

## Broader Qualification

For changes to same-window Runtime interaction and public Host composition, execute the owning
renderer regressions with the same explicit DX12 adapter prerequisites:

```powershell
cargo +1.95.0 nextest run -p nara_render_wgpu --all-features --lib --locked --run-ignored only -E 'test(native_window_clear_revokes_only_displaced_runtime_after_success) | test(native_unpickable_successor_revokes_displaced_world_without_issuing_permission)' --test-threads=1
```

These use actual Host presentation. Failed paint preserves the incumbent, a successful window
clear revokes the displaced Runtime's pointer authority, another window stays current, and an
immutable retained image remains readable. A successor without picking data still displaces
its producer's old presentation. This establishes the selected Host interaction transitions,
not monitor scanout or complete Editor frontend qualification.

When the change affects image/Canvas workflows, project opening, docking/layout, or supervised builds, also execute the corresponding ignored native tests.
For selected stock-stage policies, Scene rendering adapters or picking independent of the concrete
policy, execute the independent effects through the actual native Scene View:

```powershell
$env:NARA_WGPU_FORCE_FALLBACK='0'
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER='1'
cargo +1.95.0 nextest run -p nara-editor --all-features --lib --locked --run-ignored only -E 'test(native_scene_stock_composition_presents_external_effects_picks_edits_undoes_and_retires)' --test-threads=1
```

This hidden-window trajectory selects the external blur/grade packages through a product recipe,
joins the displayed Scene's exact image/composite receipt, and reads its actual final pixels.
It also checks original before-effects branches, ordinary picking, source editing, Undo, retained
old images and physical retirement. A separate same-source replay supplies branch observations;
it does not substitute for the actual displayed output. This is qualification of the working
Dear frontend and the shared render seam, not the future editable Nara-drawn Inspector.

For retained Scene source selection and ordinary Prefab/Canvas navigation, execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_retained_scene_receipt_survives_prefab_navigation_and_bound_close) | test(native_ui_canvas_priority_survives_closing_retained_scene)' --test-threads=1
```

These hidden-window trajectories qualify the retained Scene's exact presentation receipt while
an ordinary Prefab is active, disabled inactive-Scene picking, explicit return to the bound Scene,
and additive second-Scene publication with stale first-Scene picking rejection. Closing the bound
second Scene does not fall back to the retained first Scene. The second trajectory retains ordinary
and UI Prefabs together, preserves Canvas priority through unrelated Prefab and Scene Close, and
retires it after its own source closes. These are bounded same-kind Open/navigation trajectories,
not qualification of multiple simultaneous preview Runtimes or every persistence combination.
The retained-Scene trajectory also performs four real empty-entity creation/Undo pairs. Its
same-owner/extent Scene epochs must retain target configuration identity while each newly rendered
output owns distinct pixels and its own display registration. Old presentation tokens fail; an
unchanged repaint can reuse its retained output and registration. It explicitly invalidates a presentation with
an incompatible source revision, then requests the unchanged accepted Host source and requires
a fresh submitted/composite token. That recovery probe simulates rejected-source observation;
the owner tests separately exercise actual materialization rejection and exact source restoration.

For complete Prefab-to-local conversion, also execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --all-features --lib --locked --run-ignored only -E 'test(native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab)' --test-threads=1
```

This hidden-window trajectory uses actual Inspector mouse and wheel input to prepare, review and
commit the same conversion for both Scene and Prefab sources. It checks fixed remaps, exact local
hierarchy, ordinary local children, another linked instance, one Undo/Redo and actual Save/Reopen.
It compares Sprite geometry, material and source texture pixels through the presentation owner and
requires actual native composite receipts. It does not read back the GPU framebuffer or qualify
monitor scanout. Host tests separately cover stale source/working-copy refusal and physical
cancellation/Close; a passing native success path does not establish those failure paths.

For reviewed branch extraction, also execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_prefab_extraction_reviews_cancels_and_publishes_exact_outputs)' --test-threads=1
```

This hidden-window DX12 trajectory opens the actual Prefab through Inspector, captures a branch
draft and changes the active tab before preparing. It verifies the original source remains the
target, revises an invalid SaveCurrent selection through the real checkboxes, cancels without
writes, confirms the exact reviewed outputs and reopens the saved original Prefab. The ordinary
`extraction_maintenance_observes_async_transitions_and_keeps_review_quiet` test separately proves
that asynchronous operation changes request presentation through Application observation and an
unchanged Review remains quiet. Explicit widget paints do not measure input-idle native repaint
incidence. The independent consumer and real starter strict Check/supervised Play tests retain
their separate complete source and child-process acceptance.

For finite source-tool facets, panel/Inspector result routing or parameter retirement, build the
actual C# consumer and run its explicitly selected native workflow:

```powershell
python tests/fixtures/behavior-languages/run.py --native-editor
```

The runner builds the real child artifacts, executes the window-free language/Session consumers
and the selected Application consumer, then runs
`native_source_tool_inline_property_and_panel_share_review_history_and_persistence` and
`native_owned_inspector_source_tool_property_and_panel_share_review_history_and_persistence`
with DX12.
These hidden-window trajectories prepare through the health property control and accept through
the shared Source Tool panel, verifies a single Undo/Redo, the ordinary armor control, Attach,
activation/parameter retirement, generic fallback, Save, physical Close and fresh Open. Rust
materializes the saved component data through public Scene bindings. Both native scopes must
present. The Nara-owned trajectory additionally accepts a fresh review through its own controls
and preserves an ordinary dirty health draft across facet replacement and disabled fallback.
It locates visible controls by their Schema paths (`wire_health`/`wire_armor`) while proposals
retain the declared field IDs (`health`/`armor`). Terminal Session completion and the following
confirmed Nara image presentation are observed separately.
The current fixture begins with a native-only bundle and preserved unknown EnemyStats
records. It prepares/reviews/adopts the installed owner through the public Application before
enabling the tool, verifies source/history/checkpoint preservation and old-basis rejection, then
runs existing Inspector/panel interactions. This qualifies native Application adoption, not a
newly drawn installation/adoption control. It publishes and inspects a real project-local tool artifact and
explicitly approves its managed Host. Application preparation and durable enablement publish the
selection record before route promotion. The native panel exercises Disable for an active installed
origin, then Save and physical Close. A fresh Open observes an enabled desired record without
probing or activating it, and the panel's Disable saved selection persists Disabled without code
execution. Installed DLLs remain in place in this durable workflow; the independent session-only
consumer separately proves invocation from captured bytes after removing the installed entry.
This is a bounded installed-selection trajectory, not the full install/update/remove UI, a language
SDK, executable gameplay publications or the complete custom-drawn UI runtime. Record actual results
for this source revision; earlier evidence retains its original development/captured-image scope.
Integrated installations now use `.nara/source-tools/artifacts/<name>`. The independent Session
consumer additionally imports a declared text template into an ordinary project copy and verifies
history, Save/Reopen and withdrawal preservation. Its separate native-format Prefab consumers
check complete closed-collection identity/reference remapping, two independent copies, ordinary
editing/persistence and preservation after actual managed-artifact unlink. Window-free Application
tests exercise the public import operations and unavailable inventory without document persistence
gates. The native trajectory above retains its tool/facet/selection scope. Those window-free
source-format results do not qualify newly drawn import/reclamation controls, arbitrary Scene or
asset template collections, or the future custom-drawn frontend.

The runner's separate window-free Application selection also includes
`csharp_source_tool_creation_metadata_evolution_preserves_existing_data_and_history`. It uses
an actual generation-two embedded tool catalog and captured predecessor to review/adopt a
creation-only owner successor, preserve existing data/history, Close/Open before separately
approving V2, then execute and persist a real proposal. Independent Session consumers add
inactive dirty Scene/Prefab captures, unsupported successor refusal and failed-probe recovery.
These source/metadata workflows do not establish graphical upgrade controls or general stored
field/value migration. The native selection above retains its existing actual interaction scope.

For trusted Play source association and source-local document Close, also execute:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_unrelated_prefab_close_preserves_presented_play_source_until_source_close)' --test-threads=1
```

This hidden-window DX12 trajectory opens and closes an anchored Prefab while Game continues
through exact submitted/composite receipts with the same source, Runtime generation and target.
Closing the associated Scene retains it while Stop is pending; completed native frames then
observe exact Stop and target retirement before reporting source removal. It does not prove
intra-frame ordering or the held cancellation/failed-start cleanup paths; public Host tests own
those separate trajectories.

The full native platform group can be selected with:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(platform::imgui::)' --test-threads=1
```

Supervised-build trajectories invoke real Cargo builds in their own fixtures and can take substantially longer than the three critical tests.
Do not start another Cargo invocation while they run.

## Evidence and Release Use

Record the exact tested source revision, dirty-file scope if any, Rust version, Windows/DX12 profile, nextest run ID, selected tests, result, and any environment failure in engineering-memory verification evidence.
Bind the final release claim to committed source; a passing test of another worktree or a differently resolved dependency graph is insufficient.
Keep this document as the stable procedure and put execution results in evidence records.

The first-party Dear renderer adapter has an additional adapter-required gate using the root
workspace lock and the published 0.21.0 backend:

```powershell
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER='1'
cargo +1.95.0 nextest run -p nara_render_wgpu --locked --features editor-imgui --lib --test-threads=1
```

This gate tests Nara's integration with the production WGPU 30 dependency resolution.
Its offscreen tests do not replace the actual native trajectories above.
For changes to native retirement, partial graphics construction or shutdown/replacement gating,
also execute the focused retirement trajectories:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(platform::imgui::retirement_tests::)' --test-threads=1
```

They cover exit before first presentation without unrelated events, a real dirty Scene edit and
Cancel, dirty Exit arriving during queued reconstruction, SDK callback drift reported after
renderer release, actual SDK attach refusal after renderer construction, and healthy Open Host
worker return before graphics/process Exit. The healthy-Host trajectory uses a staged real event
handler: it dismisses the manager's initially focused text field with Escape through the owned
Context input. After initial and CloseRequested presentation settles, at least three actual
maintenance deadline wakes must deliver no new redraw or presentation while the worker remains held. Only
after release may physical Tasks return and native destruction finish. It starts with a hidden,
inactive window; if Windows withholds presentation, it shows the same inactive window after
500 ms and records that fallback. It does not inject OS pointer or keyboard input. The other
retirement trajectories remain hidden. They observe retained
Context/backend identity and destruction before a
replacement. The adapter gate separately checks initialization-upload readback, retained
device/queue drain after destruction, failed attach custody, and shutdown/binding idempotence.
These checks do not qualify arbitrary SDK exceptions, detached outstanding epochs, foreign
sidecar recovery or OS window destruction. The complete lifecycle trajectory above owns actual
secondary presentation and device replacement.

For renderer identity/binding changes, also execute the real delegated-window bridge:

```powershell
cargo +1.95.0 nextest run -p nara-editor -p nara_render_wgpu --features nara_render_wgpu/editor-imgui --lib --locked --run-ignored only -E 'test(secondary_bridge_rejects_foreign_replay_stale_and_unsampled_images)' --test-threads=1
```

It verifies actual secondary presentation before primary acquisition, exact native registration,
foreign device/queue rejection, report replay, binding replacement and stale image-content receipts.

For the optional fixed world canvas, also run the same actual Game input/resize/Step workflow with
the canvas profile:

```powershell
cargo +1.95.0 nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent)' --test-threads=1
```

Exact world pixels and bars are separately checked through the shared target encoder in
`nara_render_wgpu::canvas::tests`; native presentation alone does not establish pixel readback.
