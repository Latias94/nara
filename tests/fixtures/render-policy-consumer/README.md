# Independent Renderer Policy Consumer

This standalone workspace replaces the stock renderer with an independently authored
screen-space scalar-field renderer through the production policy selection and Host APIs.
The root `nara` dependency disables default features and enables only `desktop-winit` and
`render-wgpu`. WGPU is a development dependency pinned to `30.0.1`; the shared policy is
compiled by integration tests and the window example. No engine source contains a fixture
branch, and this workspace supplies no executor or native submission authority.

## Concrete renderer

`tests/support/heatmap.rs` owns a sixteen-sample scalar domain, immutable `HeatmapCapture`,
`HeatmapPolicy`, producer-local Feature instances, and a native Feature. The Feature uploads
an 80-byte uniform under the Host's original private-allocation guard and encodes one
full-screen triangle using the independent `heatmap.wgsl`. Four scalar bands select blue,
cyan, yellow and red. A palette rotation belongs to the original Feature definition; changing
another installation of the same Rust type cannot reinterpret an accepted input.

The policy supports exactly one full-target logical View, no fixed world canvas, and at most
1024 by 1024 pixels. Camera transforms do not reposition this screen-space domain. It builds
one typed final texture using the actual destination format, with explicit initialization,
one writer and complete output publication. No Sprite/UI stock Feature, stock quad path,
`WgpuBackendPlugins`, `WgpuStockPolicyPlugin`, or direct `submit_program` path is used.
This scalar renderer has no temporal, UI, source/processed-product or performance claim. The
separate four-chain temporal consumer below has its own workload and qualification scope.

## Actual Runtime window and original input reconstruction

`examples/heatmap_window.rs` configures `MinimalPlugins`, a real `WindowPlugin`, public
Render plugins, `WgpuCapturePlugin` and `WgpuWindowPresentationPlugin`. It installs its
original definition through `install_wgpu_policy` and runs an ordinary ECS SystemParam
capture system in `WgpuPolicyCaptureSet::Capture`. Scalar data comes from `Res<ScalarField>`;
`WgpuPolicyFrameWriter` selects the typed capture through normal Runtime transport.

The capture system also retains one bounded sibling `WgpuRenderInput` from the **same
installation and exact Capture Arc**, using the public neutral packet builder over that
frame's original extracted Views/windows, generation and frame index. It does not retrieve
or fabricate the connection's private transport input. The post-Host observer joins this
sibling input to the actual presented receipt by Runtime generation, frame, window and
installation identity, checks the policy's drawing observations, then requests successful
exit. A missing native adapter, capture, presentation or receipt fails the run.

After `DesktopRun` completes original Runtime and native retirement, the example adapts
that accepted sibling input with `with_image_target`. It asserts unchanged installation,
source, frame, logical Views and fixed-canvas facts. A fresh offscreen Host calls
`prepare_policy` before acquisition, reserves/activates its image target, and calls `submit`
on that same retained policy input. Every output pixel is read back against the scalar-field
CPU oracle, then the image producer and Host retire. The final success marker requires all
three steps: actual window presentation, original-policy image reconstruction and retirement.

`tests/window.rs` runs the prebuilt example as one finite subprocess so Windows gets a real
main-thread event loop. It checks the exit status after all child assertions, kills/reaps a child
after 90 seconds, and permits an explicit executable path override. It does not launch Cargo.
The observer allows 20 seconds for presentation; retained desktop cleanup has a ten-second
retry envelope and two-second native drives. Native prefix, map and offscreen Host polling
each have ten-second deadlines. Child diagnostics inherit the test's output streams.

## Tests

Three CPU tests in `tests/capture.rs` verify that installation close refuses new captures
while retained input keeps its original identity/topology, scalar capture copies domain
data and rejects invalid values before GPU work, and a closed definition's exact device
request can be prepared without native acquisition or Feature creation. This renderer needs
only the default device request; it does not claim optional-feature hardware coverage.

Two ignored native tests in `tests/offscreen.rs` verify:

- An accepted input builds after installation close, reconstructs after its original Host
  retires, and retains its original palette while a replacement definition of the same
  policy type produces a different result. Old output leases retain their exact pixels.
- Producer A closes before any readback or blocking drain while independently clocked B
  keeps submitting its own policy. A rejects further work; all retained A/B outputs match
  their original captures and definitions. Surface acquisition/presentation counts stay zero.

Readback uses the public `submit_image_use` boundary on the exact retained output. The CPU
oracle checks every pixel, including all four bands and scalar-cell boundaries. RGBA8 values
are exact: all palette channels are either zero or one, with opaque alpha. No tolerance or
adapter-failure skip can turn missing native evidence into success. The window observer also
rejects a CPU adapter. Run these cases on a real hardware adapter and an interactive desktop.

## Run

Run Cargo operations serially with the rest of the checkout, reuse the root `target`, and
limit build jobs. The standalone `Cargo.lock` is checked in; ordinary verification uses it.

From the repository root, qualify the locked source:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location) 'target')
cargo +1.95.0 check --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --all-targets
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test capture --test-threads=1
cargo +1.95.0 build --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --example heatmap_window
```

Explicitly select one native backend and disable forced fallback before native acceptance.
For the selected Windows/DX12 path:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test offscreen --run-ignored only --test-threads=1
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test window --run-ignored only --test-threads=1
```

The example may also be run directly using `cargo run --locked --manifest-path
tests/fixtures/render-policy-consumer/Cargo.toml --example heatmap_window`. For an explicit
target triple, nonstandard profile or copied example, set `NARA_RENDER_POLICY_WINDOW_EXE`
to its absolute executable path before running the window test. The default lookup is
relative to the test executable's profile directory and supports the shared root target.
Vulkan/Metal selection is available for a future native run; it has no evidence in this
source-only handoff. The README describes intended assertions, not results of executed tests.

## Temporal Policy And Original View Ownership

The additional `tests/history.rs` consumer uses `tests/support/accumulation.rs` and
`accumulation.wgsl` for two Views with two independent accumulation declarations per View.
One production Host transaction publishes all four exact history textures and the selected final
image. The shader computes `Hn = 0.25 * current + 0.75 * predecessor`; an independent `f64` CPU
oracle checks every linear-float history pixel and the composed RGBA8 image. Ordinary program
imports and Host-owned image use provide readback, without an independent queue submission owner.

Original topology retains `WgpuHistoryView` alongside each neutral captured ID. Immutable captures
clone the topology and effect declarations. Binding creates candidate-local slot tokens; the Host
pins complete predecessors before graph construction. Tests cover View/effect/output reorder,
one-effect replacement with the other three histories continuous, sibling/reset rejection,
deduplication after reset, public budget refusal returning the same candidate, and independent B
progress while A waits or retires. Retained A outputs keep their original pixels. A sixth test
keeps one Feature declaration alive across 513 retired View owners, proving active slot reclamation
rather than a lifetime registration ceiling. It also retrieves the final typed output for ordinary
imports. Fresh-Host recovery explicitly starts empty `DeviceRecreated` history, without claiming
reconstruction of the old temporal pixels.

`examples/history_window.rs` runs that same policy through ordinary managed Runtime capture and
native presentation. Two ordinary Camera components supply actual extracted View IDs. The first
capture fixes original topology and lineage. Three actual presented receipts supply the next
predecessor expectations, allowing unrelated skipped frames. The example checks original capture
identity, installation, View continuity, native acquire/submit/present and complete graph draws,
then completes original Runtime/native retirement. Window sRGB/BGRA output is selected from actual
destination facts; the offscreen numeric oracle remains linear RGBA8. The window probe adds
execution-path evidence, not another window pixel-quality or performance claim.

Run serially with the root target and explicit hardware selection already described above:

```powershell
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test history --run-ignored only --test-threads=1
cargo +1.95.0 build --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --example history_window
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test history_window --run-ignored only --test-threads=1
```

The history window test launches the prebuilt executable on a real main thread and never invokes
Cargo. `NARA_RENDER_POLICY_HISTORY_WINDOW_EXE` selects an explicit executable path when needed.
Missing hardware, expected presentation or retirement fails; no adapter-failure skip qualifies it.
