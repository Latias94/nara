# Independent Editor UI Host consumer

This is a separate workspace using `nara-editor` with `default-features = false`.
It registers its own `DraftPanel` through public `EditorPanelHost` APIs. The panel
retains a real shared `UiTextField`, editing history, focus token, controlled
buttons and modal state. It composes public UI descriptions without owning an
Application, document, history, input router or renderer.

`tests/public_host.rs` covers a complete data-only project workflow: bounded
non-active Scene enumeration and metadata/property reads, prepared property edit,
exact ordered callbacks, Undo/Redo, Save/Reopen and preservation of opaque source
records and placements. Further trajectories cover relocation, hidden/closed
round trips between Host calls, layout reset/restore, project replacement,
foreign/superseded/dropped candidates, action-slot filtering and explicit retry
after capacity refusal returns the original edit/construction candidate. These
are semantic CPU tests; compiling a UI frame is not presentation or OS input.

Both optional `native` examples use public `EditorPanelImageHost` attachment in
a real Winit window. This Host borrows the original window and accepts complete
image candidates only through exact original native outcomes. `WgpuRenderHost`
creates the device, owns every submission and retires the producer. The examples
reuse the sampling `ImagePainter` helper from the existing render-composition
fixture by an explicit source path, without Editor-private source/render hooks.

`panel_surface` keeps the same independently authored Nara panel and original
`UiSurfacePresenter`, journal and interaction owners. Its native provider adapter
reuses their content transform, field input token and presented text geometry.
The shared native Host supplies keyboard/IME scope, mapped candidate area,
injected clipboard, cursor and explicit refresh services; it creates no new
source mutation, text-coordinate or GPU authority.

The native trajectory edits text, opens an actual modal, verifies underlay
blocking, submits one exact panel edit, and executes public Undo/Redo and
Save/Reopen, correlating each persistence callback and terminal with its exact
attempt. With a UI image submission pending, it hides and reopens the panel,
filters an otherwise valid prepared property edit, preserves the exact document
basis/history/property/disk, revokes the old Surface route and drains
late native acceptance as discarded. A new panel has a fresh retained UI root
incarnation and rejects the old field input token after receiving actual focus.
Its replacement Surface must actually present before input can be admitted.
Exit cancels active preedit through the reducer, clears pointer/focus facts,
releases both panel states and retires the original GPU Host. The six native
presentations include one retired candidate; five publish journal entries.

`conformance_surface` uses `src/conformance.rs`, a separate frontend with its own
controls, local focus, field lifetime, modal and software painter. That module
imports shared text shaping/editing but no Nara widget tree or UI renderer.
Its captured opaque linear RGBA8 pixels pass through one original native GPU
Feature/upload/submission path. Readback compares all 144,000 pixels of the
initial and modal images with their software-painted captures. This proves
those original image pixels, not final swapchain sampling or a performance gain.

The 18-presentation trajectory preserves the same seven exact source/history/
persistence results, valid queued-edit retirement, replacement tokens and GPU
cleanup. It additionally rejects foreign/superseded frames and successful
windows missing required composites. Two panels under one semantic Host have
distinct original display occurrences: sampling only one rejects both routes;
complete presentation admits both. Focus transfers cancel old preedit before
the next panel receives focus, fence IME readiness and release the hidden panel.
Further controls cover keyboard-only focus, foreign semantic-owner refusal
without draining cancellation, out-of-bounds capture, wrong-button release,
modal underlay blocking, fractional placement/clip, clipboard write failure
and coalesced deadlines. Invisible zero-width carets refuse; nonzero exclusion
areas are clipped by the native mapping owner.

A deterministic missing-outcome control injects no native receipt after retiring
the pending panel. It then retries the same original image through an actual
native presentation, without recreating the panel. This is refusal/retry
coverage, not an OS surface-deferral claim. The Nara adapter also rejects Point
against unpresented edited text and active preedit without advancing its six
presentations or five journal commits.

Input is injected through the adapters against real presented output. Clipboard
tests inject a provider and preserve its errors without writing the user's
clipboard. These checks do not qualify OS input/candidate delivery, system
clipboard interoperability, accessibility, named-toolkit compatibility, native
docking, multi-window hosting, dynamic loading, rendering performance or the
complete independent Editor. Semantic panel lifetime and Surface/input lifetime
remain distinct. No ImGui feature or extension is enabled.

The `ui_operations` successor uses read-only `EditorPanelHost::inspect` and the original
submission's `submissions()` iterator without superseding pending edits. Its two CPU cases
retain distinct same-control scopes for the Nara and independently authored frontend.
Its ignored native test launches each existing complete example with `--observed-operations`:
accepted `EditorPanelImageObservation` values supply exact endpoint/snapshot/mapping and text
scope; observed platform input and typed provider requests return through the same reducers.
The examples retain their original presentation counts, source/history/persistence results,
image readback and retirement assertions. The foreign frontend additionally covers modal
outside-click routing, outside wheel refusal, wrong-button capture and the no-text-scope IME
Disabled acknowledgment. The
[operations unit](../../../docs/plans/2026-11-05-presented-ui-operations-plan.md)
records qualification separately from OS input, accessibility and future remote transport.

`drawn_editor` is a normal executable consumer of the public
`nara_editor::run_drawn(EditorLaunchOptions)` entry. It requires exactly one
explicit project-root argument and leaves the native event loop, graphics,
input, Application and Session to the production Editor. It has no test-only
success mode, private Editor hooks, custom runtime owner or Dear dependency.
The fixture's optional `native` feature is unnecessary for this example.

Its scope is the independently drawn source workspace: source opening,
document tabs, ordinary and specialized UI Hierarchy controls, structural
editing, the complete shared property Inspector, Undo/Redo and Save/Reopen
through the existing owners. The styled successor adds scoped typography and
control recipes plus an anchored document-action menu. It does not establish
complete Editor parity, Scene/Game tools, docking, multi-window support,
performance or native acceptance by itself. Build and
dependency checks establish public-entry consumption and the selected dependency
graph; interactive workflow qualification belongs to the
[independent drawn workspace plan](../../../docs/plans/2026-11-05-independent-drawn-source-workspace-plan.md)
and its [styled successor](../../../docs/plans/2026-11-05-styled-source-workspace-plan.md).

Build, inspect dependencies and run the entry serially from the repository root
with the same `target`:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 build --locked --offline --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --no-default-features --example drawn_editor
$drawnDependencyTree = cargo +1.95.0 tree --locked --offline --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --no-default-features --edges normal,build,features --prefix none
if ($LASTEXITCODE -ne 0) { throw 'Drawn Editor dependency inspection failed.' }
if ($drawnDependencyTree -match 'dear-imgui|imgui-reference|editor-imgui|^imgui(?:[-\s]|$)') { throw 'Unexpected ImGui dependency or feature in the drawn Editor graph.' }
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 run --locked --offline --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --no-default-features --example drawn_editor -- tests/fixtures/generic-editor/known-unbound
```

The exclusion check must find no Dear/ImGui package, `imgui-reference` feature or
`editor-imgui` feature. Replace the final fixture path with an ordinary local
project root to edit that project. This starts the production window and runs
until normal Editor shutdown; it does not inject input or terminate after a
synthetic success signal. DX12 selects the Windows backend; choose the appropriate
backend on other platforms. The existing lockfile and workspace are reused.

Run serially from the repository root, reusing its `target`:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test-threads=1 --no-fail-fast
cargo +1.95.0 build --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --examples
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test native_panel --test conformance_native --run-ignored only --test-threads=1 --no-fail-fast
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test ui_operations --run-ignored only --test-threads=1 --no-fail-fast
cargo +1.95.0 fmt --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --all -- --check
```

Select the appropriate explicit hardware backend on other platforms. The native
test launches the prebuilt example on its main thread; it never launches Cargo.
`NARA_EDITOR_PANEL_SURFACE_EXE` and `NARA_EDITOR_CONFORMANCE_SURFACE_EXE` may name
absolute prebuilt executable paths.

The lockfile is normalized from the repository's existing resolved versions and
qualified with serial locked/offline runs. The November 5 Windows/DX12 native
trajectory preserves six actual presentations and five journal commits; the
non-Nara frontend has its separate 18-presentation scope. The
[owning plan](../../../docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md#image-backed-native-panel-attachment)
records the final CPU/native results, corrections and scope limits.
