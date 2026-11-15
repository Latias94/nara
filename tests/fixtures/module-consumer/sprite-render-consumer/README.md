# Sprite And Tile Module Consumer

This independent package consumes the domain crates directly. An ordinary managed Runtime
extracts a colored Tilemap and an overlapping Sprite through one or two real Cameras. The
selected stock policy publishes its original render-only batch once; an offscreen Host owns
all native acquisition, preparation, submission and completion. No product facade, platform
EventLoop or dummy window is required.

The default CPU controls check fresh capture identity, direct edits with cleared dirty hints,
one/two View extraction and adapter observation before native initialization. The ignored
native control compares complete pixels against a separate f64 camera/map oracle, retains
published output across later frames and producer retirement, and first realizes an older CPU
input after the original Runtime and World have retired. Assertion unwind still retires the
original Runtime and Host before resuming the original failure.

The ignored release measurement separates 64, 4,096 and 65,536 cells; static, single-cell and
moving-camera scenarios; one/two Views; and plain versus bound Material values. The two Material
paths deliberately draw the same pixels. A fixed 320x180 Canvas resolves to 960x540. Eight
warmups precede 64 recorded samples per group. Full-pixel comparisons, readback, source/reference
construction and post-retirement replay occur outside ordinary sample intervals.

Each sample records complete managed drive plus capture handoff, previous-frame release/Host
frame setup, target adaptation,
`host.prepare`, `host.submit_candidate` and original-prefix waiting separately. Host submission
includes actual native preparation, initial mapped-buffer copies, encoding and publication; it
is not just a Queue call. Native work snapshots record issued API requests and initialized
payload/storage bytes, not measured bus transfers or GPU execution time. Native-prefix waiting
serializes samples and includes polling delay, so this is not pipelined throughput or scanout.
Reported retained bytes exclude driver allocations and complete CPU/RSS residency.

The geometry-only measurements exclude the historical changing HUD workload. Keep each
process's distributions separate, report finite-sample P99 limitations and compare changes
using the same current public substrate. The owning
[measured geometry successor](../../../../docs/plans/2026-11-14-durable-material-and-shader-plan.md#measured-sprite-and-tile-geometry-successor)
records selection criteria, actual results and remaining scope. No retained geometry cache or
supported population/frame-time SLO follows from the existence of this collector.

Run serially from the repository root, reusing its target:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-sprite-render-module-consumer --test geometry --test-threads=1 --no-fail-fast --no-tests fail
$env:WGPU_BACKEND = 'vulkan'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-sprite-render-module-consumer --test geometry --run-ignored only -E 'test(original_tile_captures_and_pixels_survive_edits_views_and_runtime_retirement)' --test-threads=1 --no-fail-fast --no-tests fail
```

For measurements, select the committed source revision and a distinct fresh absolute JSONL
path for every process. The collector uses `create_new` and never overwrites previous evidence.
Use DX12 or Vulkan explicitly on Windows, or the actual native backend under qualification.
Missing requested native hardware fails rather than silently passing a skip.

```powershell
$env:NARA_GEOMETRY_COST_REVISION = git rev-parse HEAD
$env:NARA_GEOMETRY_COST_OUTPUT = Join-Path $env:TEMP 'nara-geometry-current-run-1.jsonl'
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --locked --offline --release -p nara-sprite-render-module-consumer --test geometry --run-ignored only -E 'test(measure_current_public_geometry_costs)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```
