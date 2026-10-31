# Independent Public Render Program Consumer

This standalone workspace authors native Features against the production
`nara_render_wgpu` program API, with default features disabled. Its other direct engine
dependency is `nara_render`, also without default features. WGPU is pinned to 30.0.1 for
native Feature implementations. There is no root workspace member or root test target.

The consumer creates no ECS World, gameplay Runtime, native window, surface or event loop.
It does not import an engine-private module, the old composition fixture's executor, graph
or history implementation, or a core package-specific lowering branch. All dependency
compilation, native allocation, initialization, ordering, submission, output publication and
retirement use the existing production Host. Ordinary Feature code creates pipelines and
bind groups, prepares private uploads, and encodes commands. It never receives a Queue and
never submits a native command buffer itself.

## Tests

`tests/candidate.rs` contains five CPU cases:

- Different semantic texture/buffer ports admit explicit preservation copies. Their
  declarations and stable keys run against dependency order, and their complete output
  group uses another order. Source/frame identity and cloneable immutable input are checked.
- Foreign-candidate handles and a second writer reject at declaration. A refused writer
  leaves the original complete candidate usable.
- An unwritten output and a two-resource preservation cycle reject the whole candidate.
- Resource count, modeled resource bytes, and overflowing metadata allowances reject
  without native work.
- A real blur Feature definition can be cloned, installed and dropped before device
  acquisition. The candidate retains its capture/adapter pair. Producer retirement closes
  further instance admission without issuing a native prefix or allocating GPU storage.

Two `compile_fail,E0308` examples in `src/lib.rs` check that a composite texture cannot enter
a Scene-color port and raw counts cannot enter a normalized-frequency port. Run doctests
separately: nextest does not execute them. These probes use the actual public typed ports.

`tests/offscreen.rs` contains three ignored Windows/DX12 cases:

- `invalid_capture_preflight_refuses_before_known_native_resource_budget` supplies invalid pixels
  with a zero-byte native payload budget. Pure preflight returns the capture-shape refusal before
  resource allocation or preparation, then a valid capture with the same index succeeds.

- `independent_features_preserve_exact_a1_while_a2_and_b_execute` uploads a real 64-by-64
  linear premultiplied RGBA32Float image, including transparent, half-alpha and alpha-zero
  pixels. Two installed blur definitions of the same Rust Feature type select horizontal
  and vertical passes. A separately implemented complement Feature consumes the blurred
  output. Luminance atomics count the original image into four raw integer bins. A distinct
  frequency buffer is initialized by copying the raw predecessor and then normalized in
  place. Its STORAGE-to-VERTEX dependency feeds a real instanced histogram draw through
  `set_vertex_buffer`; that render pipeline has no storage or uniform binding. A separate
  histogram texture is initialized from the complement through an explicit copy across
  different semantic port types, then loaded and stored by the draw.
  Operations are declared out of dependency order, with dependent operations having lower
  keys than their predecessors. Publication order differs again. Reports are looked up
  through their exact original instance; another instance of the same Rust type rejects.
  A1's complete output group stays retained while A2 and independently clocked B execute.
  A is closed before any mapping or blocking drain, and B then imports the exact A1/A2
  texture and buffer leases for new readback submissions. Every original image, blur,
  complement, histogram pixel and raw/frequency value is checked after that interleaving.
  B renders another full group after A's prefix settles. Closed A rejects resubmission.
- `private_allocation_refusal_keeps_old_outputs_and_the_same_candidate_can_retry` settles
  A1, then leaves enough retained-byte capacity for all A2 graph resources but one byte less
  than its private upload reservation requires. Host submission refuses inside Feature
  preparation, publishing no group. Increasing the limit admits the exact same immutable
  A2 program and capture index. Fresh exact-output imports then check both the old A1 and
  new A2 numerically. Original submission custody and last-lease accounting return charged
  bytes to zero before final Host retirement.

All image channels and normalized frequencies use an absolute tolerance of `1e-6` against
independent CPU equations in `tests/support/oracle.rs`. Raw counts and integer totals are
exact. Histogram comparisons cover every pixel against the CPU raster rectangle oracle;
the chosen capture offsets avoid a bar edge exactly on a pixel center. The original private
composition fixture's WGSL and CPU equations supplied mathematical evidence only. The
consumer's Feature adapters, public program construction and readback are independent.

Readback is itself a normal installed Feature. It declares six CopySource reads and copies
four textures and two buffers into one private 262,176-byte MAP_READ buffer. Its Prepared
and typed Report retain the original Device, buffer and `WgpuFeatureAllocation` guard.
Mapping starts only after successful Host admission. Polling services that original Device,
without an extra native submit or a current-output lookup. Both map completion and the
original submitted prefix are checked. Native polling loops have ten-second deadlines.
The workload uses seven graph resources, seven operations and six published outputs;
candidate allowances cap resources/operations at 12, accesses at 48, outputs at 8, modeled
resource bytes at 2 MiB and modeled metadata bytes at 256 KiB. Private native allocations
share the production Host's retained-byte budget.

## Run

Run each Cargo operation serially with every other Cargo command in this checkout. The
integration owner generated this standalone workspace's committed `Cargo.lock`; use it for
normal locked qualification.
Use the parent runner's target-directory policy; no build output is supplied by this fixture.

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location) 'target')
cargo +1.95.0 check --locked --manifest-path tests/fixtures/render-program-consumer/Cargo.toml --tests
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-program-consumer/Cargo.toml --test candidate --test-threads=1
cargo +1.95.0 test --locked --manifest-path tests/fixtures/render-program-consumer/Cargo.toml --doc
```

Explicitly select DX12 and the production adapter policy before running the native cases:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-program-consumer/Cargo.toml --test offscreen --run-ignored only --test-threads=1 --no-fail-fast
```

Choose `NARA_WGPU_FORCE_FALLBACK=1` explicitly for fallback-adapter evidence instead. The
production Host selects the concrete adapter within that policy; this consumer does not
provide a separate adapter picker. Missing explicit selection, an unsupported OS, adapter
or device acquisition failure, validation failure, mapping failure and timeout all fail the
native prerequisite or test. They never return a passing skip. A default test run leaves
all native cases ignored and is not GPU qualification.

## Evidence Limits

Source presence is not execution evidence. The
[qualification record](../../../docs/knowledge/engineering/verification/2026-10/2026-10-31T133536Z-finite-typed-render-programs-through-the-original-host-production-qualification-a32bacec9d8b4572801df1f79fc31c01.md)
records actual owner/CPU/doctest/Windows-DX12 results and their final source coverage. Further
changes need evidence matching their behavior and native scope.

The cases qualify neither browser behavior nor window presentation, UI/input completion,
stock Sprite/UI migration, history slots, actual device loss, recovery, native failure
injection, performance or retained RSS. Producer-local close is observed with B submitting
after A closes; the test does not force a hardware-dependent pending interval or assert a
continuously nonempty queue. The CPU metadata and resource limits are modeled admission
bounds. Pipeline/driver allocation overhead is outside that model. The complete production
migration remains owned by the
[render unit](../../../docs/plans/2026-10-31-render-composition-and-host-execution-plan.md)
and [production design](../../../docs/architecture/render-program-production-design.md).
