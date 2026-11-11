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

The same composition also paints actual Source and Cooked PNG content. A public recipe plugin
observes sealed stock captures during two managed headless runs. The Source imports a real sRGB
PNG; the Processed run consumes an admitted two-member Scene/Image catalog and explicit producer
expectation after the temporary source/cooked backing has been destroyed. Both Runtime owners
retire before those original inputs render through the same native Host. Extra public prepared
Image observations are dropped before returning the captures; the consumer uses no private Sprite
batch representation. Rendering the escaped inputs after retirement is the replay evidence.

The adapter obtains transfer/alpha from each exact retained output's `descriptor()`. It selects
`WgpuSceneColorSpace::LinearSrgb` explicitly and records directly to the Nara sRGB attachment;
the framework's default numeric-sRGB composition remains unchanged. The smoke verifies all four
alpha bands, a translucent UI overlay, clipping and fractional-root scaling for both content
modes. [ADR0092](../../../docs/architecture/adr/0092-sdr-color-space-alpha-and-output-encoding.md)
owns the linear working-space and associated-output contract. These SDR controls do not select
HDR display output, gamut mapping or a tone mapper.

The manifests share one fixed official WGPU Git revision containing the qualified surface-drop
and wasm-timeout corrections. Registry `30.0.1` does not contain those later fixes. An application
combining the reference Dear renderer must also patch its registry WGPU edge at its own root,
as described in the [render-output guide](../../../docs/guides/render-image-outputs.md).
This consumer enables stock UI alongside the Editor's render-only path to exercise additive
Cargo feature unification without requiring the Runtime UI product in the framework.

Source presence and compilation do not establish native acceptance. Exact runs, revisions,
failures and limits belong to the integration study and foundation plan.

The current native smoke also mounts a source-owner Hierarchy and two independently retained
Inspector projections. They consume one observation from the selection-independent entity query, while each draft,
IME/preedit state and conflict lifetime remains panel-local. A selected X edit is prepared and
committed through the original Editor basis/history owner; a stale second draft refuses until
Reload, and the original Undo restores the source and both clean projections. Scene preview
updates use a separate cached View, so the native output records isolated Hierarchy reuse rather
than counting a parent refresh as local invalidation. The caller-owned render counters are
diagnostic only; they do not qualify layout, upload, GPU or latency budgets. The exact native
qualification and public CPU fixture results remain in the foundation plan. Further Undo may
remove the Sprite itself: the panels show an absent property and keep the Viewer alive for Redo.

The Hierarchy now uses GPUI's original variable-height `ListState`. Its temporary source contains
10,000 real empty declarations plus the constructed Sprite. The Sprite is pinned first as a
fixture display policy; other declarations retain source order. Public entity-query pages are
consumed completely, while the list builds only its visible/overdraw rows. The local Filter field
applies committed text explicitly, and Details changes row extents through original list
remeasurement. Surviving row keys retain their scroll offset across filtering; an absent anchor
falls back to the first row with zero offset. This is a finite consumer, not a generic tree editor.

The full native smoke scrolls near the end, filters and clears the source, changes row heights,
selects an exact virtual row and restores the Sprite before the original recovery/close path.
Injected Winit PixelDelta/LineDelta events exercise the ordinary wheel adapter; at logical scale
1.25, 6.25 physical delta pixels become five logical pixels. Actual monitor DPI migration and
physical wheel/trackpad behavior remain unqualified. `NARA_GPUI_COLLECTION` separates query pages/
records, filtering/reconciliation and row callbacks at its named checkpoint. Those counts omit
allocation, equality/string comparisons, framework layout and GPU work; they are not a total
O(visible) or latency claim. Source snapshots, optional filter indices and ListState retain
O(logical rows) metadata; old frame/resource leases can outlive a replacement.
