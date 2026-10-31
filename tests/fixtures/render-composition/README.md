# Private Render Composition Comparison

This independent workspace contains disposable tests for the
[render composition unit](../../../docs/plans/2026-10-31-render-composition-and-host-execution-plan.md).
It exports no library API. A private resource model and exact-wgpu harness compare whole-resource
texture/buffer dependencies, physical versions and concrete effects before a production interface
is selected. They do not qualify Runtime/Editor integration, production Host initialization,
stream retirement or presentation/input authority.

Use the installed Rust1.95.0 toolchain, one Cargo process/job, and the repository's existing target:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location).Path 'target')
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-composition/Cargo.toml --tests --test-threads=1
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-composition/Cargo.toml --test effects --test gpu_harness --test graph_gpu --test native_admission --run-ignored only --test-threads=1
```

The first command runs CPU model checks. Real-GPU cases are ignored by default and execute only
with the second explicit selection; unavailable adapters fail the prerequisite rather than pass
through a silent skip. The private harness requests DX12 on Windows and Vulkan elsewhere, without
creating a surface/window. Execution on one backend does not qualify any other backend/platform.
Readback comparisons use explicit linear premultiplied RGBA32Float and independently computed
numeric oracles. The complete unit requires graph, viable controls, buffer/compute and history
consumers plus the later production/public workflow; this scaffold alone is not that evidence.

The [private comparison evidence](../../../docs/knowledge/engineering/verification/2026-10/2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md)
records61 CPU cases,7 real DX12 cases and a subsequent real post-Ready submission-failure case.
The image/buffer kernels execute through graph, ordered, slot and ECS-schedule paths. Exact output
versions, fixed history groups, late failures/canaries and accepted-prefix retention have direct
tests. Performance comparison, production GPU Host/epoch authority and full native Editor/Runtime
integration remain open; this private workspace is not their replacement or a public extension API.

The separate [CPU-stage comparison](../../../docs/benchmarks/2026-10-31-render-composition.md)
runs the same kernels with fresh outputs for every variant and a reused ECS schedule topology.
It excludes setup/readback/drain from samples and reports repeated-process variation without
selecting a speed or memory winner. Run its optimized entry explicitly:

```powershell
cargo +1.95.0 nextest run --locked --release --manifest-path tests/fixtures/render-composition/Cargo.toml --test measure --run-ignored only --test-threads=1 --no-capture
```
