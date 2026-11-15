# Independent Renderer Policy Consumer

This standalone workspace exercises independently authored renderer policies and an ordinary
stock Material through the production capture, policy selection and Host APIs. The root
`nara` dependency disables default features and enables `desktop-winit`, `presentation-2d`
and `render-wgpu`. The WGPU development dependency follows the maintained git revision
`cddb2f5d80dc0009b2ef6767635140cacf90a007` (package version `30.0.0`) with resource counters
enabled. Integration tests and window examples compile the shared policies. No engine source
contains a fixture branch, and this workspace supplies no executor or native submission authority.

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

The joint motion regression reuses the independent animation and Rapier fixture authors from
`../motion-execution`. It freezes the actual Presented spatial stamp, poses, virtual shader time,
real time, interpolation fraction and pause state together. Two fixed ticks plus a half-step,
zero-tick presentation, Pause and exact Step supply different captured samples. Shader phase is
quantized from that original virtual time; delayed Host execution never samples the live clock.
KeepLocal reparent and a solver-owned kinematic correction run before the next fixed history pair.

It retains both an unprepared input and an already prepared candidate before `discontinue()`.
Both refuse after the cut, as does late admission of the original raw typed capture. Only the
first View's two histories reset; the other View continues its exact predecessor chain. The
successor and same-tick presentation match all four float histories and final pixels against the
CPU oracle. An independent lineage also continues, old output reads remain exact, and the original
submission prefix remains observable through retirement. This is offscreen Windows/DX12 evidence;
it adds no native-window, general prediction, camera-cut or material-generation claim.

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

## Ordinary Material Captured View And Time

`tests/material_view_time.rs` is an independent ordinary stock Material consumer. It uses the
same real Animation/Rapier App as the motion/history control, with a fixture-local 16 ms fixed
timestep. Two actual Cameras have distinct physical viewports and mappings. A small Sprite
is attached directly to the solver's original kinematic entity. Its Material reads only the
engine-owned `nara_view` for View/time facts; the sole authored parameter is a constant gain.

The consumer compares the original packet sample with App clocks and the actual Presented
spatial publication. A separate CPU oracle derives the complete 64 by 24 linear RGBA8 image
from source poses, known Camera geometry, exact Duration arithmetic and public clock facts.
The Sprite edges fall on integer pixel boundaries. Four UV bands encode real/virtual
elapsed time, deltas, fixed time/ticks, View mapping/viewport and pause/interpolation state.
The horizontal halves independently probe `clip_to_world * (0.5, -0.25, 1)` and
`world_to_clip * (0.75, 2.375, 1)`, exercising both linear columns with asymmetric nonzero
coordinates. The CPU derives the probe bytes from the known Camera parameters rather than
the uploaded matrices. Covered RGB allows one UNORM byte of rounding; alpha and every clear
pixel are exact.

The trajectory covers initial and advancing zero-tick frames, multiple fixed ticks, a 0.5
time scale and Pause with real time advancing while virtual time freezes. Paused presentation
retains a nonzero alpha but reports no applied spatial interpolation. Exact Step advances
the original fixed state without another capture; only the following ordinary Extract
publishes the new Presented sample. App frame words remain distinct from render packet indices.

An original A input is held before later B is published. Between their captures, a fixture-local
request changes the original Cameras' translations and viewport heights through the App's
ordinary Update system. Each Sample freezes its known Camera configuration. View IDs remain
stable while actual extracted mappings, raster coverage and both probe bytes change beyond
the RGB tolerance. A is first prepared after B, then replayed after further scaled/paused/stepped
frames through a separate producer on the same Host. Adaptation preserves the exact installation,
source/frame, Views, time and stock capture pointer. Full image comparisons retain A's old
projection, pose and time and prove B's output stays unchanged. The original device's enabled
WGPU render-pipeline counter must be positive and stable.

The test keeps Runtime and Host owners outside its assertion-unwind boundary. It physically
closes the original Runtime, observes the original World drop, retires original producers,
checks retained outputs and submission prefixes, and verifies zero retained native/candidate
bytes and cleanup faults. Assertion failures also run that explicit cleanup before unwinding.

This correctness test accepts either explicit `NARA_WGPU_FORCE_FALLBACK=0` for local hardware
or `1` for a requested software adapter in CI. It reports the actual adapter and checks the
requested backend. A missing requested adapter fails rather than skips. Existing history and
window qualification retain their own hardware prerequisites. The
[owning qualification](../../../docs/plans/2026-11-14-durable-material-and-shader-plan.md#captured-view-and-shader-time-successor)
records actual Windows Vulkan/DX12 hardware and DX12 software results. These are correctness
and lifecycle observations; the consumer does not establish a performance budget or test the
3,600-second wrap boundary.

The manifest enables the facade's `presentation-2d` API and WGPU `counters`. Run serially
against the checked-in standalone lock:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location) 'target')
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --test material_view_time --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

The exact test is
`ordinary_material_captures_view_time_motion_and_replays_original_frames`.

## Joint Material And Temporal History

`tests/joint_history.rs` drives the original Animation/Rapier Runtime and two real Cameras.
View 0 draws its kinematic actor through an ordinary Material reading captured real/virtual
time and pause/interpolation facts; View 1 sees a separate constant-blue control Sprite.
The existing `HistoryPolicy` builds one stock world operation, then explicitly samples its
exact graph texture into four float history slots. The original manual procedural path and
its independent history oracle remain unchanged.

The selected effect treats Camera cuts, solver correction, parameter adoption and same-ABI
Shader body adoption as incompatible with View 0 history. Each transition preserves original
lineage, View and Feature identities, discontinues the old semantic revision and resets only
the two affected slots. An old raw capture, an admitted input first prepared after the cut
and an already prepared candidate all refuse through their original History checks. The new
B uses Reset for View 0 and actual preceding outputs for View 1. Invalid Shader C retains
last-good and continuous predecessors; it cannot add a native pipeline.

An independent CPU oracle derives full source images from known Camera geometry, exact
Duration arithmetic and verified motion poses. Source colors select integer UNORM byte
centers, avoiding ambiguous half-byte conversion ties. Double-precision recurrence checks
all four Rgba32Float histories within `3e-6`; every final image pixel is checked separately.
Each cut also executes old non-temporal stock A on another producer, compares exact original
pixels and verifies reversed-frame refusal on the current producer's original target.

The trajectory covers advancing zero-tick frames, multiple fixed ticks, scaled time, Pause
and exact Step without Extract. Solver correction independently requires tick 4 at `(1.25, 2)`
before comparing the next presentation. Live native pipeline counts remain constant through
time, scale, Pause, camera and value changes; only the adopted Shader body adds one. The
Runtime physically closes and both drawing producers retire before retained histories and
images are read again. Reader retirement, output release and Host drain then require zero
retained bytes, candidate metadata and cleanup faults, including assertion-unwind cleanup.

Run on an explicitly selected backend, with `NARA_WGPU_FORCE_FALLBACK=0` for hardware or
`1` for the separately reported software path. Missing requested adapters fail. Existing
`history` and `history_window` tests retain their own hardware prerequisite.

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --release --test joint_history --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

This is a concrete accumulator's compatibility policy, not automatic camera/material edit
classification, general TAA/reprojection, cross-device history restoration or a performance
budget. Exact run results and correction history belong to the
[owning unit](../../../docs/plans/2026-11-14-durable-material-and-shader-plan.md#joint-material-and-history-discontinuity-successor).

The exact test is
`real_material_motion_and_camera_cuts_preserve_history_ownership_and_last_good`.
