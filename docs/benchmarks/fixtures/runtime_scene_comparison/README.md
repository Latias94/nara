# Local Runtime Module Comparison

These ordinary CPU consumers compare corresponding data work, not complete engines.
They use one collection of 64, 4,096 or 16,384 records containing
`x=i%side`, `y=i/side`, `tile=i%64`, `flags=i%4`, where `side=sqrt(rows)`.
Every retained sample checks every resulting field outside the timers.
No renderer, frame loop, external asset, Prefab or per-cell entity is involved.

| Consumer | Source and phase boundary |
| --- | --- |
| Nara | Current public Scene JSON/RON decode, component compilation, fresh World preparation and commit. Generic tagged values retain the actual public wire format and receiving checks. |
| Bevy typed Scene | Pinned `ScenePatch::resolve` and fresh-World `spawn`. Complete typed patch construction is outside timing. This pin has no native Scene file loader. |
| Bevy serialized World | Separate official `DynamicWorld` `.scn.ron` encode/decode/write APIs, enabled by `native-world-ron`. Its decode includes reflected native conversion; it has no Scene compile phase. |
| Godot | Official `PackedScene` text load with `CACHE_MODE_IGNORE`, then instantiate outside the tree. Generic Dictionary metadata, packed integer metadata and restricted native TileMapLayer are separate cases. |

Nara's one component is `TileRows(Vec<TileRow>)`, decoded through its public codec;
the generic wire uses signed integers, and the codec checks the native i32/u32 ranges.
Both Rust consumers' native rows contain actual i32/u32 fields occupying 16 bytes per row.
Godot Dictionary integers are native Variant integers. Equal logical data therefore
do not imply equal encoded bytes, storage, validation or ownership guarantees.
Source entity/component counts describe payload declarations; Nara's original identity
and instance bookkeeping remain included in prepare/commit and its resulting World counts.
Nara additionally reads an externally minified copy of its exact public JSON wire;
that control removes whitespace before timing and is not its default writer.
Godot metadata can share containers; native TileMapLayer instead builds a mutable
cell map. Its no-TileSet fixture omits deferred rendering/collision/navigation work.

Use an existing root `target`, one Cargo process and one build job. The Nara fixture
lock was seeded from the root lock and retains its dependency versions/sources.
The independently locked Bevy consumer uses the read-only reference checkout at
`03ed5a27c92715c8fa2188f31f04e454ec39942f`; it requires Rust 1.97.1 or later.
The measured reference compiler is 1.100.0; the Nara compiler is 1.95.0.
Reference source must be present; no build or update modifies `repo-ref/`.

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location) 'target')
cargo +1.95.0 build --locked --offline --release --manifest-path docs/benchmarks/fixtures/runtime_scene_comparison/nara/Cargo.toml
cargo +1.100.0 build --locked --release --features native-world-ron --manifest-path docs/benchmarks/fixtures/runtime_scene_comparison/bevy/Cargo.toml
python docs/benchmarks/fixtures/runtime_scene_comparison/run.py --nara target/release/nara-runtime-scene-comparison.exe --bevy target/release/bevy-runtime-scene-comparison.exe --godot F:/MySoftware/Godot-4.7.2-stable/Godot_v4.7.2-stable_win64_console.exe --output C:/absolute/new/measurement-directory
```

Build both consumers before measuring. `run.py` does not run Cargo. It runs three
process rounds in varied engine order, each with three warmups and nine retained
samples per case. A new output directory is required. Generated scenes, individual
stdout/stderr, process walls, executable/fixture/source hashes and raw sample arrays
remain there. The Godot project is copied there so its cache does not touch the
source fixture. See the [Godot consumer notes](godot/README.md) for its exact scope.

Nara's pretty RON output at 16K exceeds its default 16MiB encoded-file allowance.
That case explicitly verifies the default refusal, then selects the public 32MiB
encoded allowance for saving/receiving; every other original limit is unchanged.
The record reports both limits. This is a benchmark setting, not a production
default change or a claim that packed data have the same cost.

Warm file reads include allocation/open/read/close. Rust decode consumes resident
bytes. Godot's decode performs its own additional file read; do not subtract the
separate read samples or silently call it an isolated parser measurement.
Godot's microsecond clock is reported as nanoseconds with 1,000ns resolution.
Cleanup, validation and process startup are outside module timers. Process wall is
only diagnostic and includes generation, verification and repeated samples.
No cold-cache, exclusive CPU, memory, GPU, steady-frame or engine-ranking claim follows.

For a subsequent Nara-only attribution, enable `scene-work-observation`. The consumer reuses
the original game's binary-local observer rather than adding another profiler. Each case's
`work_observation` covers its three warmups and nine samples, and distinguishes effective value
admission, asset interpretation and entity-plan finalization. These nested wall spans overlap;
they cannot be added to their containing compile/preparation envelopes. The option is disabled
by default. Capture exact build sources/executables separately when comparing frozen variants;
the printed baseline implementation revision is historical context, not a claim about a later
build. The published cross-engine observations use the exact fixture version at `70140a3`.

The [source-boundary study](../../../research/2026-11-17-runtime-module-performance-comparison.md)
owns the official implementation findings. Actual observations and proposed budgets
belong in the [benchmark report](../../2026-11-17-runtime-module-comparison.md) rather than these reusable consumers.
