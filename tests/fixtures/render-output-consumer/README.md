# Independent Public Render Output Consumer

This standalone workspace consumes the production image-output Host through public APIs only.
Its direct engine dependencies are `nara_render_wgpu` without default features, `nara_render`,
`nara_core` and `nara_ecs`, also without default features. WGPU 30.0.1 is used only by the native
tests for trusted readback and native-adapter timestamp capability checks. The separate native
UI example enables the stock UI render feature and consumes text/kernel/widgets/presentation plus
Winit through test dependencies. It requires no Dear, game UI ECS adapter, Editor or root product.

The offscreen workflows create no gameplay Runtime, ECS World, native window, surface or event loop.
The render Host creates each move-only `WgpuRenderProducer`; configuration, submission and
retirement borrow that owned producer. Its copyable `.id()` is used only to compare nominal facts,
never as execution authority. Independent source identities and logical image handles use their
public constructors. Each image is a real stock clear draw built from one manually supplied
`ExtractedView`. A retained stock policy installation captures those immutable facts into each
`WgpuRenderInput`; the Host uses that input's original definition. There is no private implementation import, fake Runtime generation, alternative
device initialization path or mock GPU.

## Run

Run from the repository root, serially with every other Cargo operation in this checkout. Reuse
the root `target` and the installed Rust 1.95.0 toolchain. Generate the standalone lockfile once
if it has not already been supplied:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location).Path 'target')
cargo +1.95.0 generate-lockfile --manifest-path tests/fixtures/render-output-consumer/Cargo.toml
```

Compile the independent consumer, then explicitly execute its four offscreen native cases. For the selected
Windows/DX12 qualification:

```powershell
$env:WGPU_BACKEND = 'dx12'
cargo +1.95.0 check --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --tests
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --test offscreen --run-ignored only --test-threads=1 --no-fail-fast
```

All four tests are ignored by default because they require an actual native GPU. A default test
run or compilation alone is not native acceptance. The explicit selection must execute all four
cases; adapter/device acquisition, polling, readback or timeout failure fails the test rather than
returning a passing skip. The Host uses its production backend-selection and fallback settings.
An execution qualifies only the actual backend/platform recorded by its caller. This fixture is
not a browser qualification. The offscreen cases do not qualify window presentation; the separate
native UI trajectory below has its own actual window/composite and routing assertions.

## Native UI surface presentation

`examples/ui_surface.rs` consumes `UiSurfacePresenter` through public APIs, prepares an immutable
field/button/popup frame and returns its original backend outcomes after actual window drawing.
The existing `WgpuRenderHost` owns the device, producer, output and physical native-use retirement.
The original `WgpuUiImageSubmission::display()` occurrence is the only one used to complete its
presentation; mirror displays cannot substitute for its composite receipt.

Only after the presenter drains that original one-shot acceptance may the caller route input
through its read-only frame journal. Compilation, planning and offscreen submission alone grant
no route. The caller still owns its `UiInteractionSession`, widget reducers and native placement.
Its modal popup must reach real presentation before the popup action can run. Retirement revokes
the old route before late completion, and replacement receives a fresh surface/input lifetime.

The same normal trajectory then imports `KeyedEditors` from the independently compiled
`ui-module-consumer` library. Two same-label fields draw through the original presenter. Actual
presented focus admits captured scoped input; reorder/insert preserve the same draft, combining
preedit, selection/caret and local Undo/Redo. Filtering the active row discards its state, then a
different item occupies its exact earlier presented rectangle. Old scoped input cannot mutate
any replacement. Explicit document/schema replacement clears the old local draft/history;
compatible composition retains the new handles and focus token. This portion of the trajectory
observes 11 native presentations and ten journal commits, retaining the original single popup
action, reconstruction and original-Host retirement assertions. Logical glyph positions and
glyph/face identities are checked without requiring target-scale raster keys to equal the source
editor's keys. This is not independent pixel readback or a typography accuracy benchmark.

The trajectory then presents `CurveCanvas` from that same external library. Its fixture-local
Lyon tessellator supplies actual cubic geometry between ordinary background and foreground
controls. Presented pointer selection and Tab/Enter port activation use the original journal
and button reducers; Shift-Tab/Enter changes selection again. Explicit fractional pan moves
curve vertices and matching node semantics together on the next accepted frame. The previous
route and the final retired surface cannot activate controls. This checkpoint observes
14 actual presentations and 13 journal commits before the continuous-input portion below.
It preserves every earlier field/popup/keyed state and original-Host assertion.

The same original surface then admits a middle-button drag on the fixed viewport. Changed
descendant geometry and disappearance do not cancel this explicitly opted-in viewport capture;
motion outside the viewport still uses the original authored anchor. An unrelated button
release preserves capture, and its matching release ends it. Wheel events over an ordinary
child button select the viewport's scroll interest once. Both Lines and converted pixel-event
units preserve the same world anchor through actual presentations. A drag at the pan limit,
Shift-wheel adjustment and stationary release exercise the bounded-view rebase; native focus
loss cancels a final held gesture. The complete trajectory is 19 presentations and 18 journal
commits, retaining the earlier keyboard, field/IME and original retirement proof. This is
injected native-adapter input, not OS delivery, cross-window capture or a graph document owner.

The ignored `ui_surface` integration test launches the example in a child process so its event
loop runs on the actual main thread. A second test injects error and panic after a real image
submission but before composite, both for the initial controls and after custom mesh submission;
success requires original permit settlement,
presenter quiescence and Host retirement, not merely the expected failure string. Run these
separately, serially, under the same native environment:

```powershell
cargo +1.95.0 build --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --example ui_surface
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --test ui_surface --run-ignored only --test-threads=1
```

Injected native events and normalized routing qualify this consumer's input adapter; they do not
establish physical OS input-method/candidate delivery, accessibility or an independent Editor.
The presentation crate's combined App/ECS/render/image dependency closure remains explicit even
though this adapter installs no game UI plugin or World. This is a finite presentation prerequisite,
not the whole R11b panel attachment or a completed neutral presentation split.

## Workflows And Assertions

- `retained_pixels_survive_resize_and_a_close_while_b_keeps_submitting` publishes red A1 and green
  A2 under one unchanged configuration, then a blue resized A3. It retains the exact leases and
  distinct display occurrences, retires A before any blocking readback, and submits another B
  capture before each A-prefix poll. B remains live and publishes after A completes. Every pixel
  of A1/A2/A3 is read back after A retirement; a retained A1 display never follows A2. Final
  producer settlement and lease release return output accounting to zero before Host retirement.
  Reusing closed A for submission or target reservation returns `ProducerClosed`; repeated
  retirement cannot issue a second prefix or recreate the producer through its nominal ID.
- `exact_output_budget_preserves_leases_and_retries_the_same_refused_capture` refuses a
  configuration one byte below a 16-by-16 RGBA8 allocation, then refuses A2 one byte below the
  two-output budget while A1 remains red and charged. Raising the limit to the exact budget lets
  the same A2 input object/frame publish green, proving the refused attempt did not consume its
  successful-publication watermark. At the exact two-output cap, an additional read-use custody
  table refuses before its callback. The numeric oracle then separately admits that bounded native
  observation. After the original prefix settles, a retained display keeps
  the allocation until its last lease drops, with exact retained-byte assertions.
- `original_cpu_input_reconstructs_after_the_original_host_has_retired` keeps the original input
  independently, closes and drops its installation, completes producer and final Host retirement,
  drops that Host, and submits the
  same input object to a new normal offscreen Host. It checks red pixels and new issued identities,
  and rejects the foreign old output for both display and native access. It neither injects nor
  claims actual device-loss recovery, and does not test released-input recovery unavailability.

`native_adapter_requirements_enable_queries_without_replacing_the_device_or_outputs` additionally
contributes timestamp capabilities before acquisition, executes and maps real queries under the
original Host, admits a late subset and refuses a broader request without replacing the device.
Red output pixels survive those native uses and final retirement rejects new declarations.
This is capability, exact output and readback acceptance, not a timing benchmark or full GPU
profiling claim; the selected adapter must support both encoder timestamp capabilities.

Pixel readback imports the exact output through `submit_image_use`. Its callback only allocates the
readback buffer and encodes a texture-to-buffer copy; the Host submits and retains imported output
leases on the returned native prefix. The consumer never calls `Queue::submit`, mutates or destroys
the imported texture, or uses a producer's current target to find old pixels. It starts mapping
only after submission returns, polls the original prefix, and explicitly services the real device
through `with_image` if that prefix has settled but the mapping callback remains pending. Submission
settlement and map completion are checked separately, without lease custody in the map callback.
Each polling loop has a ten-second deadline; the interleaving probe also caps itself at 256
additional B captures. Small exact RGBA8 clears need no floating-point tolerance, and comparisons
cover all pixels while excluding row padding.

The interleaving probe keeps submitting B while polling A's frozen prefix; it does not claim the
queue is continuously nonempty or force a hardware-dependent pending interval. Issuing display
occurrences without a window does not prove that a toolkit sampled them, that a composite presented,
or that Runtime UI/input authority completed. Those joins, actual device loss, native failure
injection and the full graph/Feature workflow remain separate qualification in the
[current render unit](../../../docs/plans/2026-10-31-render-composition-and-host-execution-plan.md)
and [ownership design](../../../docs/architecture/render-host-ownership-design.md).

Source presence is not execution evidence. Record the exact engine revision, toolchain,
backend/adapter and actual test results when qualifying this fixture.
