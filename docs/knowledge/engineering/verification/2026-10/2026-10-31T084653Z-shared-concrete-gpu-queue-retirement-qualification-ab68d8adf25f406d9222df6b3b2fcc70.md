---
type: "Verification Evidence"
title: "Shared concrete GPU queue retirement qualification"
description: "Verification Evidence for Shared concrete GPU queue retirement qualification."
timestamp: 2026-10-31T08:46:53Z
record_id: "ab68d8adf25f406d9222df6b3b2fcc70"
producer_id: "codex-foundation-main"
run_id: "render-shared-native-drain-2026-10-31"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-render-composition-and-host-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "96bd0bf4859b6a2e6d26799901503977fce52835"
---

# Verification

Implementation **`96bd0bf4859b6a2e6d26799901503977fce52835`** on `refactor/foundation`, receiving
baseline `9f6acf3`. Windows 11/DX12, Rust 1.95.0 MSVC, serial Cargo with one job and the root target.
This follows the [original-result prerequisite](2026-10-31T082117Z-original-ui-result-custody-and-logical-close-qualification-6e1965c6428841ec9138088bef20f90c.md)
without closing the full foundation Goal or complete render unit.

# Result

The actual device/queue drain moves from the Dear-specific module into
`nara_render_wgpu::retirement`. Hidden `WgpuGpuRetirement` is available without `editor-imgui`;
the former `WgpuEditorGpuRetirement` and implementation path are removed with no alias or second
drain. Existing Editor construction and retained graphics close consume the shared owner directly.

The token retains the exact Device/Queue pair. Normal retirement flushes queued initialization
uploads once; discard destroys that original device and still polls its accepted work. Only actual
QueueEmpty completes the drain. Poll errors retain retryable state; interrupted destroy/flush stays
poisoned. `WgpuGpuRetirementError` retains native poll provenance without attributing it to toolkit
paint. The actual Editor error boundary retains its GPU-drain diagnostic and completion policy.

Parent review confirms the same concrete state machine and real consumer migration. Euclid owned
the bounded patch and was closed after source-ready review. No mock drain or new generic GPU,
retirement manager or quarantine was introduced.

# Evidence

| Check | Actual result |
| --- | --- |
| WGPU adapter selection | **8 passed**, run `3b0c32e2-ee67-455a-b0a9-62af12d7776a`, build 1m02s/run 6.631s. Includes real pending initializer transfers, constructor/attach failures, discard with accepted work, loss before flush and idempotent renderer shutdown; one case is the CPU secondary-ID mapping. The other 151 library cases were not selected. |
| Native Editor retirement group | **6 passed**, run `9418e99f-2fb7-44d5-b0f0-afbc638b870a`, build 2m58s/run 12.383s. Actual callback drift/retry, close before first presentation, dirty Cancel, dirty replacement, partial construction/recovery and healthy worker return all use production owners. The other 297 cases were outside the selection. |
| Minimal feature check | `cargo check -p nara_render_wgpu --no-default-features --locked` passed in 6.47s. Existing feature-specific unused-mut warnings remain. |
| Correctness Clippy | `nara_render_wgpu` all features/all targets passed `-D clippy::correctness` in 22.73s, with non-correctness warnings. |
| Static checks | Root `cargo fmt --all --check` and `git diff --check` passed. No source changed after these checks. |

Native checks selected `WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`; absent hardware
was not accepted as a passing skip. This does not establish other-platform or scanout evidence.
The change does not alter capture, output, epoch or submission policy and has not connected a
Runtime-local Host to this drain yet.

# Follow-up

Continue the [owned native transition](../../../../architecture/render-host-ownership-design.md):
capture this original retirement owner while Ready and before initialization uploads/first accepted
submit, then retain it independently of backend error cleanup and telemetry. Complete the finite
capture/result connection and native run session together with UI/READY, exact target/provider
release and original event-pump custody. GPU ECS storage, the borrowed Winit run and scheduled
native submission still exist.

At this checkpoint parent drafts reconcile ADRs0032/0040/0078 and the actual code-first/diagnostic
consumers. Astra/max reviewer Nash (`01a19116-dc3e-7f22-92f9-6e843ce84212`) is still reviewing that
next interface decision; its pending result is not this module's qualification. The candidate Root
direction reuses DesktopRun for existing Runtime input and retains completed CPU observations with
their own identities, rather than rerunning Last or treating a new simulation frame as submitted.
Do not freeze the complete interface or count that design as implemented before review/integration.

# Citations

- [Shared concrete drain](../../../../../crates/nara_render_wgpu/src/retirement.rs)
- [Actual Editor consumer](../../../../../editor/src/platform/imgui/retirement.rs)
- [Adapter tests](../../../../../crates/nara_render_wgpu/src/editor_execution/imgui/tests.rs)
- [Complete render unit](../../../../plans/2026-10-31-render-composition-and-host-execution-plan.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
