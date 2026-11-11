# Nara-Owned GPUI Rendering Consumer

This independently locked workspace consumes the maintained sibling `open-gpui` checkout through
relative path dependencies. Keep the two repositories next to each other and use the framework
revision recorded in the [integration study](../../../docs/research/2026-11-11-open-gpui-engine-integration-layers.md).
The engine and ordinary renderer fixtures do not resolve these framework paths. This is a public
integration consumer, not the selected full Editor frontend.

```text
cargo +1.95.0 build --locked --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --example gpui_view
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/gpui-engine-consumer/Cargo.toml --test gpui_view --run-ignored only --test-threads=1 --no-fail-fast
```

Set `WGPU_BACKEND=dx12` or `vulkan` and `NARA_WGPU_FORCE_FALLBACK=0`. Reuse the engine workspace's
target directory and serialize Cargo. Launch the prebuilt `gpui_view` executable without
`--smoke` to inspect the Scene View, controls and draft field interactively; `--smoke` runs the
bounded native workflow and closes. The child-process test never starts Cargo.

After building from the Nara root with its shared target directory, inspect the Windows example:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
.\target\debug\examples\gpui_view.exe
```

The Nara adapter implements `HostedSurfaceRenderer` and records the GPUI Scene directly into
the original Host's acquired window attachment. It creates no additional UI window, device,
surface or submitter. The real `EditorProjectSession` produces the Scene preview; its immutable
output is imported without a pixel copy. Actual Host composite receipts and GPUI accepted-frame
publication jointly admit picking. UI recordings remain local until the exact window attempt
prefix completes, including refusal and panic paths.

Smoke assertions cover glyphs/buttons and actual Scene pixels, overlay input exclusion, Editor
selection, source translation and Undo, 4096-byte draft presentation, IME commit/cancellation,
last-submitted input while a successor waits, stale picking, paint refusal/panic recovery,
resize/ABA/zero extent, fractional logical-root scale and closing with a pending candidate while
another engine producer remains live. Qualification-only readback renders the same frozen Scene
to an offscreen target in the original window submission; it is not a swapchain readback, scanout
proof or steady-state performance path. Physical IME, physical DPI migration, full frontend/game
adoption, device loss and numeric costs retain separate acceptance requirements.

The manifests share one fixed official WGPU Git revision containing the qualified surface-drop
and wasm-timeout corrections. Registry `30.0.1` does not contain those later fixes. An application
combining the reference Dear renderer must also patch its registry WGPU edge at its own root,
as described in the [render-output guide](../../../docs/guides/render-image-outputs.md).
This consumer enables stock UI alongside the Editor's render-only path to exercise additive
Cargo feature unification without requiring the Runtime UI product in the framework.

Source presence and compilation do not establish native acceptance. Exact runs, revisions,
failures and limits belong to the integration study and foundation plan.
