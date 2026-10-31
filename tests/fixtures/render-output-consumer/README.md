# Independent Public Render Output Consumer

This standalone workspace consumes the production image-output Host through public APIs only.
Its direct engine dependencies are `nara_render_wgpu` without default features, `nara_render`,
`nara_core` and `nara_ecs`, also without default features. WGPU 30.0.1 is used only by the native
tests for trusted readback of an output's borrowed texture view. No toolkit, submitter, root
product, scene serialization or platform-runner feature is enabled.

The consumer creates no gameplay Runtime, ECS World, native window, surface or event loop.
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

Compile the independent consumer, then explicitly execute all three native cases. For the selected
Windows/DX12 qualification:

```powershell
$env:WGPU_BACKEND = 'dx12'
cargo +1.95.0 check --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --tests
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-output-consumer/Cargo.toml --test offscreen --run-ignored only --test-threads=1 --no-fail-fast
```

All three tests are ignored by default because they require an actual native GPU. A default test
run or compilation alone is not native acceptance. The explicit selection must execute all three
cases; adapter/device acquisition, polling, readback or timeout failure fails the test rather than
returning a passing skip. The Host uses its production backend-selection and fallback settings.
An execution qualifies only the actual backend/platform recorded by its caller. This fixture is
not a browser or window-presentation qualification.

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
  successful-publication watermark. After the original prefix settles, a retained display keeps
  the allocation until its last lease drops, with exact retained-byte assertions.
- `original_cpu_input_reconstructs_after_the_original_host_has_retired` keeps the original input
  independently, closes and drops its installation, completes producer and final Host retirement,
  drops that Host, and submits the
  same input object to a new normal offscreen Host. It checks red pixels and new issued identities,
  and rejects the foreign old output for both display and native access. It neither injects nor
  claims actual device-loss recovery, and does not test released-input recovery unavailability.

Readback imports the exact output through `submit_image_use`. Its callback only allocates the
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
