---
type: "Verification Evidence"
title: "Coherent WGPU device ownership and render cost comparison qualification"
description: "Verification Evidence for Coherent WGPU device ownership and render cost comparison qualification."
timestamp: 2026-10-31T06:46:58Z
record_id: "18760d4fdea7435b90fb9bd2bee64510"
producer_id: "codex-foundation-main"
run_id: "render-device-owner-2026-10-31"
---

# Verification

Foundation R8/R9a on `refactor/foundation`, Windows 11/DX12, Rust 1.95.0 MSVC. The private
comparison checkpoint is `cfdf3a9`, instrumented cost/selected ownership design is `cd84a48`, and
the production internal device-owner refactor is **`48cf9ce`**. Cargo was strictly serial, one
job, reusing the root target. No remote publication occurred; the full foundation Goal is active.

The supplied ZIP was rehashed in this receiving checkpoint and still matches
`99589eef3287cb9e353501bf1d27d1f4287ec825dbaa45c97abee38033ed2c36`. Its imported research remains
reconciled by the existing disposition; no archive extraction or roadmap reset was repeated.

# Result

`WgpuDeviceOwner` now owns the existing stock Runtime/Editor device namespace. Production Ready
is a complete Adapter/Device/Queue/loss aggregate, while an Instance can preexist it for surface
selection. Default device requests and fallback validation use one private acquisition path with
an optional compatible surface. Backend surfaces/caches drop before the device owner, explicitly
and on direct field destruction. Cleanup preserves non-reused incarnation and epoch watermarks;
terminal Unavailable cannot reopen through clear.

Pure epoch/fallback admission precedes native construction. An ended synchronous acquisition
failure returns Uninitialized and its original error, retaining any Instance needed by an
already-created surface. It no longer reports an initialization with no pending operation.
The actual native loss callback retains its original signal; late predecessor loss cannot poison
the successor. Existing stock Runtime/Editor APIs and encoding remain their current real consumers.
No second backend, public raw-GPU callback, compatibility alias or migration reader was introduced.

The optimized private comparison measures the same image/buffer kernels with fresh resources and
equivalent success custody through graph, ordered, slot and reused-topology ECS paths. Three
processes execute 36,000 measured candidates. Large repeated P95 variation prevents a stable
speed/tail-cost winner; the [report](../../../../benchmarks/2026-10-31-render-composition.md) owns
the method, tables, modeled payloads and limits. Success timings do not qualify equivalent fault
protocols, real game P99 or retained GPU memory.

# Evidence

| Check | Actual final result |
| --- | --- |
| WGPU all-feature library | **147 passed**, six skipped: four preexisting ignored cases plus the two explicitly selected native device cases below. Run `de58c84b-1a6e-47aa-8bf8-672153534c37`, build 1.17s/run 69.650s. Actual adapter requirement was enabled. |
| Device-owner focused selection, including ignored native cases | **6 passed**, run `556043ad-87cf-4a8f-b7d3-6382687109d1`, build 17.07s/run 2.372s. Four cases overlap the default library; two are additional actual surface-less native tests. This is not 153 distinct library cases. |
| Native Editor critical group | **3 passed**, run `d5aa6558-ba4a-42c4-97a6-0a7d5c47734d`, build 2m36s/run 6.970s. Paused redraw rebase, Game input/refocus/resize/Step, and independent View detach/minimize/reopen/recover/retire all execute through production paths. The 300 other cases were outside this selection. |
| Minimal WGPU feature check | `cargo check -p nara_render_wgpu --no-default-features --locked` passed in 44.22s. Existing feature-specific unused-mut warnings remain. |
| WGPU correctness Clippy | All features/all targets passed in 51.34s with `-D clippy::correctness`; this is not `-D warnings`. |
| Private optimized cost comparison | One ignored measurement test passed in each of three independent processes, with actual numeric checks; run times 5.321s, 4.839s, 4.867s. Measurement-only correctness Clippy passed in 2.13s. |
| Formatting/static integration | Root and independent fixture `cargo fmt --all --check` passed. `git diff --check` and affected local document-path checks passed. |

Native environment selected `WGPU_BACKEND=dx12` and
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`; acquisition failure was not silently skipped. The two new
native cases are ignored by default to keep ordinary CPU/no-adapter CI portable. Their explicit
entry is:

```powershell
cargo +1.95.0 nextest run -p nara_render_wgpu --all-features --locked --lib -E 'test(device::tests::)' --run-ignored all --test-threads=1
```

Predecessor failure and correction: the first all-feature run
`4596bcf1-95df-4085-b0d5-18f15e3b78a5` passed 148/149 cases and failed the new assertion that
devices from independent Instances must compare unequal. The observed native DeviceId repeated;
exact WGPU 30.0.1 source's `CoreDevice` equality proxies its native ID. That invalid test premise
was removed, not the isolation requirement. The final case asserts the issued owner/epoch,
destroys/polls the retained predecessor, and actually submits a successor clear-buffer command,
waits its original prefix and checks native validation. The production owner already authenticates
with its own namespace. No source change followed the final focused/library/native evidence.

Independent review and dispositions:

- Copernicus mapped the concrete full Host migration and implemented only the bounded device
  prerequisite, preserving the stock consumers rather than installing another renderer.
- Averroes challenged the selected production transition. Connection close now explicitly blocks
  Runtime Stopped until original logical completions settle; incomplete EventLoop/timeout paths
  retain a local run-session/retirement owner. Host-issued display occurrence joins the designated
  UI completer, and post-Host maintenance advances with optional `None` results for deadlines and
  READY. These are selected design and future production oracles, not implemented Host claims.
- Its final measurement-source review found no fairness blocker. Final production-source review
  found no remaining material P1/P2 correctness issue after synchronous-error, native-test
  portability and handle-identity test corrections. All agents are closed.

# Follow-up

Continue the [complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
from step 2 of its [production ownership design](../../../../architecture/render-host-ownership-design.md):
owned stock capture/result connection, Runtime logical close barrier, local runner presentation
and incomplete native run custody. Migrate exact first-present/UI observations with that owner;
delete GPU ECS storage, direct scheduled submit and missing-resource retirement inference only
when all actual consumers use the replacement.

Then separate target configuration from immutable physical output versions, add producer-prefix
retirement and explicit original reconstruction input, and integrate external graph/Feature,
alternate policy, independently paced streams and Nara-drawn clipped composition. Actual
Runtime/Scene View/offscreen, source/processed interleaving, designated UI completion/passive
mirrors, two windows/image, loss/recovery and finite close still require the complete production
workflow. This internal acquisition test is not a public window-free GPU Host. Browser, other
native platforms, material R9b, authored Schema/Prefab evolution, joint motion/execution and full
editable Editor work remain open. Proposed0121 remains Proposed; the overall Goal stays active.

# Citations

- [Production ownership design](../../../../architecture/render-host-ownership-design.md)
- [Private cost method/results](../../../../benchmarks/2026-10-31-render-composition.md)
- [Owning ADR0078](../../../../architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md)
- [Private device owner](../../../../../crates/nara_render_wgpu/src/device.rs)
- [Predecessor graph/history/native evidence](2026-10-31T060722Z-private-render-graph-buffer-history-and-viable-controls-comparison-on-dx12-d6aca1996b2342659bd07933b33e0e64.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
