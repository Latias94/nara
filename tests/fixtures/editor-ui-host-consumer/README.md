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

The optional `native` example uses the same independently authored panel and
public semantic Host in a real Winit window. `WgpuRenderHost` creates the device,
owns every submission and retires the producer. `UiSurfacePresenter` publishes
input geometry only after the original image/window composite result is
returned and drained. The example reuses the public sampling `ImagePainter`
helper from the existing render-composition fixture by an explicit source path.
It does not use Editor-private source or rendering hooks.

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

Input is injected through the example's pointer/IME adapter against real
presented routes. This does not qualify OS event delivery, accessibility,
clipboard, native docking, multi-window hosting, dynamic loading, rendering
performance or a complete independent Editor. Semantic panel lifetime and
Surface/input lifetime remain distinct. No ImGui feature or extension is enabled.

Run serially from the repository root, reusing its `target`:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --test public_host --test-threads=1 --no-fail-fast
cargo +1.95.0 build --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --example panel_surface
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test native_panel --run-ignored only --test-threads=1 --no-fail-fast
cargo +1.95.0 fmt --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --all -- --check
```

Select the appropriate explicit hardware backend on other platforms. The native
test launches the prebuilt example on its main thread; it never launches Cargo.
`NARA_EDITOR_PANEL_SURFACE_EXE` may name an absolute prebuilt executable path.

The lockfile is normalized from the repository's existing resolved versions and
qualified with serial locked/offline runs. The November 5 Windows/DX12 native
trajectory passes with six actual presentations and five journal commits. The
[owning plan](../../../docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md#toolkit-independent-panel-custody-prerequisite)
records the final CPU/native results, corrections and scope limits.
