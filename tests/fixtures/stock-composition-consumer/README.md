# Independent Stock Composition Consumer

This is an independent Cargo workspace using Nara's public production APIs. It composes ordinary
Scene/Sprite content with a separately authored blur package and a separately authored grading
package. Neither effect package depends on the other. The consumer selects graph edges; Nara
continues to own Sprite extraction, View order, Canvas mapping, UI lowering, native submission,
original UI completion and retirement.

## Qualification and evidence limits

The complete independent workspace passes all nine explicitly selected ordinary/native tests on
Windows/DX12, run `7adfbeb5-e6b2-4d4f-b465-9512041a935c`: nine passed, zero skipped, 8.817s.
The Runtime example is prebuilt and launched on its process's platform main thread. Both effect
packages and this workspace also pass all-feature/all-target Clippy with `-D warnings`.

The actual native Editor trajectory passes in the final public/stock selection, run
`04e7026b-ab1a-49a8-82eb-fd3b4d03414d`: 79 passed, 1932 filtered/ignored, 15.491s.
That selection includes public plugin/UI capture and CPU stock helpers; it is not 79 native
Editor trajectories. A separate critical/affected Editor selection passes five actual native
cases, run `216985ec-58ad-4acb-8396-656ba45ec4fb`. Counts overlap and are not additive.

| Consumer | Qualified behavior | Actual evidence |
| --- | --- | --- |
| `tests/source.rs` | Materialize a source-bound ordinary Scene, run normal extraction and public collection, retain original input after Runtime close | Executed in the independent nine-test run |
| `tests/composition.rs` | Branch untouched world/Scene and blur/grade outputs, compare world versus resolved pixel radii, retain old outputs, replay original inputs, disable effects, and read the actual final UI composite with exact one-pixel HUD checks | Executed on the selected native GPU |
| `tests/effects.rs` | Actual RGBA8/BGRA8 unorm/sRGB ports, half-float blur scratch and premultiplied alpha grading | Native format matrix passed |
| `tests/translucent_scene.rs` | Ordinary half-alpha Sprite over a transparent Camera clear, original-input replay, four actual effect formats, exact alpha-zero regions and grade alpha preservation | Native translucent Scene case passed |
| `tests/ui_binding.rs` | Real public UI Feature accepts its original binding and rejects a foreign operation key, instance, equal-but-different capture Arc and copied equal-pixel final successor | Native binding cases passed |
| `examples/runtime_window.rs` and `tests/runtime_window.rs` | Ordinary source `DesktopRun`, Sprite and authored HUD, actual original UI completion facts, source retirement and numerical original-input offscreen replay | Prebuilt example and native test passed, including the 33x24 boundary oracle |
| Native Editor `stock_composition_tests` | Real displayed Scene pixels, exact composite join, picking, source edit/Undo, old outputs and physical retirement through the same recipe adapter | Executed in the final 79-test public/stock selection |
| `examples/editor_scene.rs` | Optional manual native Editor using that same recipe | All-feature/all-target build checked; no separate manual observation claimed |

The final post-UI texture uses the public stock `WgpuCompositeColor` semantic marker.
`tests/composition.rs` imports its exact output through the existing
`WgpuPublishedFrame::final_texture::<WgpuCompositeColor>()` into the declared typed readback
Feature. The complete image is compared to the predetermined effect/HUD oracle, every white
stripe pixel is exact, and every pixel outside the stripe must match the selected pre-UI Scene's
actual bytes exactly. Incorrect world/Scene markers reject with `ResourceSemantic`; without
captured UI, the original world output rejects the composite marker. No casts, manual
`submit_image_use`, direct queue submission or private API bypass are used. The explicit native
run above executes these assertions; screenshots and process exit alone are not their oracle.

The same retained-output case consumes each full publication into its image, retires and drops
the original producer, disposes the CPU inputs, and requires weak stock-capture references to
expire. `image.texture::<WgpuCompositeColor>()` must still return the exact original typed lease,
while world/Scene markers must reject. After dropping even the image wrapper, a new declared
readback producer in the original live Host epoch must observe the original bytes exactly. No
complete program or CPU capture is retained to recover the texture's semantic meaning.

The Editor example opens an actual Editor. Closing it successfully does not establish rendering
or picking acceptance. There is no fabricated automated Editor success test. Existing native
qualification remains required for the claims affected by the parent integration; see
[`native-editor-qualification.md`](../../../docs/verification/native-editor-qualification.md).
Device loss, monitor scanout, two-window scheduling, performance and other platforms remain
outside these finite consumer assertions.

## Composition

`CompositionPlugin` depends on `WGPU_STOCK_CAPTURE_PLUGIN_ID`. Its product recipe replaces the
default policy/composer while retaining explicit `WgpuStockCapturePlugin` selection and the stock
Sprite/UI contributors. During `WgpuStockCaptureSet::Compose`, it takes one complete
`WgpuStockFrame`, wraps the original immutable stock capture, and transfers the unchanged
move-only UI sidecar to `WgpuPolicyFrameWriter::capture_with_ui`. Capture refusal propagates a
Runtime fault; dropping an unaccepted original sidecar returns its pre-admitted Aborted outcomes.
`RetainedInputs` observes sealed CPU inputs without taking transport or presentation obligations.

The policy uses only the public `WgpuStockStages` helper:

1. `world(10)` draws the ordinary captured Scene at its actual world raster extent.
2. If a Canvas was captured, `resolve_canvas(20, &original_world)` retains the untouched,
   target-sized Scene branch.
3. `EffectSpace::World` wires blur then grade in `WgpuWorldColor`. A Canvas then resolves the
   processed world through the original mapping. Without Canvas the world already has the
   target dimensions; the policy does not invent a resolution operation.
4. `EffectSpace::Resolved` with Canvas wires the same independent effects in `WgpuSceneColor`,
   after the original Canvas resolution.
5. `finish(90, &selected_scene, &additional_outputs)` composes the original HUD at the complete
   target extent. Additional outputs retain exact original, blurred and graded versions.

Disabling effects selects the original Scene and passes its original output to `finish`.
The helper adds the final version only if absent. It does not manufacture a second before/after
version, and caller duplicate output entries retain ordinary graph rejection.

`recipe(configuration)` uses `ProductRecipe::select_wgpu_policy`. The Runtime plugin and fresh
Editor adapters use the same canonical configuration. Each Editor adapter owns a new original
installation; retaining an input retains that input's exact policy, configuration and stock CPU
snapshots. A later recipe or live Scene cannot retarget it.

The optional `CompositionCapture::inspected` observer records bounded CPU graph handles for the
numeric tests. Recording a handle confers no publication authority: every actual check obtains
the exact typed output through `WgpuProgramOutputs::texture`. Production captures do not install
this observer.

The fixture library also exports `oracle`, `readback` and `recipe_with_inspection` for the actual
Editor native owner test. They reuse the same independent implementations and stay on public
APIs. The [native Editor test](../../../editor/src/platform/imgui/stock_composition_tests.rs) joins
the actual Scene publication and native composite, reads its selected final pixels, and exercises
picking, source editing, Undo and physical retirement. Its execution result is recorded separately
above. The Editor dev-dependency must leave this fixture's optional `native-editor`
feature disabled; that feature exists only to build the manual Editor example.

## Predetermined numeric oracle

The source Scene contains an opaque red 4x2 Sprite at the center of an 8x6 blue Camera view.
The target is 32x24. Without Canvas this is a normal 32x24 raster. With an 8x6 Canvas, the original
nearest mapping produces an exact 4x fit. Integer-aligned edges avoid an antialiasing ambiguity.
No extracted Sprite batches, render packets or stock lowering are manufactured by the fixture.

A second ordinary Scene uses transparent black for its Camera clear and a straight linear Sprite
tint of `[0.8, 0.4, 0.2, 0.5]`. Normal stock blending produces premultiplied linear
`[0.4, 0.2, 0.1, 0.5]` before the original RGBA8 target quantizes it. The normal public collector
captures that Scene, and its original accepted input replays only after the source Runtime retires.
The independent identity Grade converts the retained stock output to each actual RGBA8/BGRA8
unorm/sRGB port, then blur and a nonidentity grade execute on that spatially varying translucent
image. The CPU oracle includes the original source quantization and subsequent format conversion.

Before preparing a candidate, the test constructs all expected pixels and fixes the tolerance:

- Native sRGB textures decode RGB to linear light when sampled and encode RGB on attachment
  writes. Alpha stays linear. BGRA readback is swizzled to canonical RGBA bytes.
- Blur clamps taps at the actual source extent. The horizontal result rounds through
  `half::f16`, matching the independent blur's `Rgba16Float` scratch. The vertical pass then
  quantizes into the actual destination format.
- Grade reads that quantized blur output, unpremultiplies nonzero-alpha RGB, applies exposure,
  Rec. 709 luminance saturation, contrast about linear 0.5, clamps, premultiplies and quantizes
  again. The translucent Scene has a nonzero-alpha Sprite, blurred transition pixels and
  alpha-zero regions. Those zero regions must remain exact transparent black through conversion,
  blur and grade; grade must preserve every actual blurred alpha byte exactly.
- Unchanged opaque source pixels and the standalone UI stripe use exact byte comparison.
  Effect comparisons admit **3 encoded byte values per channel**, fixed before GPU results,
  for native sRGB conversion, UNORM rounding and floating-point differences. This tolerance
  is not chosen from observed GPU errors.
- The new translucent stock source allows one encoded value for source UNORM rounding before
  effect comparison. Its transparent clear remains exact, its central alpha remains within that
  predetermined rounding bound, and re-reading the same retained source after the entire format
  matrix must match the original GPU bytes exactly. Existing opaque solid and HUD comparisons
  retain their zero tolerance.
- The world-space and resolved-space expected images must differ before native execution.
  Radius one at the 8x6 raster covers a larger physical neighborhood than radius one at 32x24.

The ordinary native Runtime project uses a 33x24 window and matching authored primary UI surface.
Its Schema composition is resolved from that same source manifest. At unit DPI the odd width
places the centered Sprite's left/right edges on pixel centers. The Runtime oracle computes
half-open coverage with inclusive top/left and exclusive bottom/right boundaries before reading
GPU pixels. The separate 32x24 CPU/Canvas and native format matrices retain their integer 4x fit.

Readback is a normal typed public Feature with an explicit `CopySource` read. It reserves its
private map buffer through `reserve_bytes`, copies only inside its declared encode context, and
maps after the Host's actual original prefix completes. Import/re-export checks retain the
original producer, source generation and frame. It neither takes a native submission reservation
nor submits command buffers directly.

## Serial verification commands

Run from the repository root after the parent has aligned this workspace's lock. Reuse the normal
repository target directory and never overlap Cargo invocations in this checkout.

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 check --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --tests --examples --all-features
cargo +1.95.0 nextest run --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --test source --test-threads=1
```

Run the ignored GPU cases explicitly on an actual adapter. Missing adapter/desktop prerequisites,
timeouts, an empty selection and unavailable evidence are failures, not passing skipped tests.

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --test composition --test effects --test translucent_scene --test ui_binding --run-ignored only --test-threads=1
```

The Runtime test launches a prebuilt example so the real EventLoop stays on the child process's
platform main thread. The test does not invoke Cargo itself. An optional absolute
`NARA_STOCK_RUNTIME_WINDOW_EXE` selects a prebuilt artifact when using another profile.

```powershell
cargo +1.95.0 build --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --example runtime_window
cargo +1.95.0 nextest run --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --test runtime_window --run-ignored only --test-threads=1
```

Run the actual hidden-window Editor trajectory explicitly from the repository workspace:

```powershell
$env:DEAR_IMGUI_REQUIRE_WGPU_ADAPTER = '1'
cargo +1.95.0 nextest run -p nara-editor --all-features --lib --locked --run-ignored only -E 'test(native_scene_stock_composition_presents_external_effects_picks_edits_undoes_and_retires)' --test-threads=1
```

The optional manual example also opens the same recipe in the real Scene View. It supports visual
inspection and ordinary controls; process exit alone is not numeric or native qualification.

```powershell
cargo +1.95.0 run --manifest-path tests/fixtures/stock-composition-consumer/Cargo.toml --locked --features native-editor --example editor_scene
```

The source-authoring and numeric fixture deliberately use finite dimensions. They do not claim
arbitrary content, arbitrary Canvas letterboxing, all UI layouts or a native platform release.
