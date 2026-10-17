# Nara U6 presentation observability patch

## Provenance

This directory vendors the released `dear-imgui-wgpu` **0.20.0** crate from the local Cargo
registry. Its packaged `.cargo_vcs_info.json` identifies the upstream source as
`Latias94/dear-imgui-rs`, tag `v0.20.0`, commit
`af2942c78651b280d18af621de55b5462bb1e10a`, path `backends/dear-imgui-wgpu`.
The normalized release manifest and the original `Cargo.toml.orig`, README, changelog,
existing source and tests were copied before this patch. No dependency was upgraded or replaced
inside this crate.

Upstream's MIT and Apache-2.0 license texts are retained as `LICENSE-MIT` and `LICENSE-APACHE`.
They were copied from that exact upstream tag. This patch remains under the same dual license.
The registry copy's source identity is unchanged; this document describes the local delta.

## Reason and scope

Nara delegates secondary surface ownership, recording, queue submission, presentation and teardown
to the existing `WinitViewportRoute`. Its U6 bridge must identify the actual Nara texture sampled
in each presented secondary surface. The released report contains viewport ID sets only, so it
cannot establish that fact. The patch adds backend-produced observations at the existing draw and
presentation operations. It does not install a different renderer or transfer submission ownership.

## Exact source delta

- `src/renderer/draw.rs`: add the privately constructed `WgpuRenderReport`; retain each accepted
  element command's resolved `TextureView`, then collect unique views after its `draw_indexed`
  call. Zero-count, invalid-range and fully clipped commands cannot contribute a sampled view.
  Callback failure returns no successful report. Raw callback resources remain outside this
  standard-element observation contract.
- `src/renderer/render.rs`, `src/renderer/mod.rs`, `src/lib.rs`: export the render report and add
  `render_reconciled_with_samples`. The original reconciled rendering method delegates and
  discards its report without changing submission or presentation behavior.
- `src/renderer/multi_viewport_runtime/trace.rs`: under `wgpu-30`, add immutable
  `WgpuViewportPresentedFrame` values and `WgpuViewportFrameReport::presented_frames`. Records
  expose only read-only accessors for the exact device, queue, ImGui viewport ID, Winit window ID
  when enabled, surface lifetime/configuration identities, unique presentation identity, actual
  acquired texture extent and sampled texture views. There is no public successful-record
  constructor or `Default` implementation. Process-wide identity counters fail closed instead of
  wrapping. Presentation identity is allocated after the actual WGPU presentation call.
- `src/renderer/multi_viewport_runtime/surface.rs`: pair pending acquired textures and pending
  evidence in one `Option`; resize, release and recreation discard both together. Give each
  actual surface lifetime a fresh identity and each nonempty configuration a checked generation.
  Remove the unused retained configuration field after rendering switched to acquired extent.
- `src/renderer/multi_viewport_runtime/callbacks.rs`: discard an abandoned pending frame before a
  new render attempt; derive render/report extent from the acquired texture; retain evidence only
  after successful recording and queue submission; publish it only after presentation. Keep the
  existing render/present ID sets and ownership order.
- `src/renderer/multi_viewport_winit_adapter.rs`: read the native `WindowId` inside the existing
  live platform-window borrow; do not expose or reconstruct an owning window handle.
- `src/renderer/multi_viewport_runtime/runtime.rs`, `src/renderer/multi_viewport_runtime.rs`,
  `src/renderer/multi_viewport.rs`, `src/renderer/multi_viewport_sdl3.rs`: expose safe borrowed
  `WgpuPreparedViewportFrame::main_draw_data` and route `render_main_with_samples`; preserve the
  original `render_main` signature and its consume-once runtime validation. Re-export the WGPU 30
  presented-record type through each existing platform route.
- `tests/integration_test.rs`, `src/renderer/multi_viewport_runtime/tests.rs`, trace-local tests:
  check actual element execution with two active images, duplicate sampling, zero-count/clipped
  filtering and view replacement; check paired pending evidence disposal and identity overflow;
  assert that successful observation types cannot be fabricated through `Default`.

Detailed secondary records are available for WGPU 30. Existing WGPU 27/28/29 routes keep their
presentation APIs; primary sampled reports reuse the same draw executor in every existing cohort.
Winit and SDL3 viewport features remain mutually exclusive as upstream requires.

## Consumer obligations and evidence limit

The report certifies calls to WGPU, not monitor scanout or GPU completion. An empty sampled set is
valid for a cleared or fully clipped secondary frame. A later preparation error may discard an
already-completed partial report; consumers must leave those presentations uncertified rather
than infer them from a primary-window result. A skipped primary acquisition does not undo a
successfully returned secondary report.

Nara's private bridge synchronously prepares the route, validates exact `Device`/`Queue`/view
identity and current image-submission receipts, rejects reused presentation IDs, and performs this
join before main-surface acquisition. Texture-view equality alone does not identify the content
generation of a repeatedly written allocation. No plain-data success assertion is accepted by
the bridge. Consumers must retain normal GPU resource retirement and route-before-platform
shutdown ordering.

## Verification ownership

Nara's coordinating execution owner runs Cargo and formatting serially.
The renderer is a root workspace member so its tests use the same `Cargo.lock` as the production Editor.
CI explicitly tests Nara's WGPU 30 cohort with an adapter required; dependency compilation alone does not execute these patch tests.
Native detachment, resize, redocking, secondary close and shutdown also require the
[native Editor qualification](../../docs/verification/native-editor-qualification.md); renderer tests do not establish native support.

Run from the Nara root, serially, reusing its target directory:

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo +1.95.0 fmt --all
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER = '1'
cargo +1.95.0 nextest run -p dear-imgui-wgpu --locked --no-default-features --features wgpu-30,multi-viewport-winit --lib --test integration_test --test-threads=1
cargo +1.95.0 check --workspace --exclude dear-imgui-wgpu --locked --all-features --all-targets
```

Do not enable this package's `--all-features`: upstream intentionally rejects combined WGPU/backend cohorts.
Nara's blanket all-feature commands exclude this package; the explicit production-cohort test above covers it instead.
Other upstream cohorts remain outside Nara's continuous support claim.
An old standalone lockfile beside this manifest is not the production resolution and is not used by workspace commands.
Execution results belong in Nara's verification evidence, including historical U6 qualification.
