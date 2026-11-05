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

Run serially from the repository root, reusing its `target`:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test-threads=1 --no-fail-fast
cargo +1.95.0 build --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --examples
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/editor-ui-host-consumer/Cargo.toml --features native --test native_panel --test conformance_native --run-ignored only --test-threads=1 --no-fail-fast
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
