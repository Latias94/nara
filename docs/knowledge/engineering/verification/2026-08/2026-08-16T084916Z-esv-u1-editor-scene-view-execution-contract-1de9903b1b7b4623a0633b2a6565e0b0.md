---
type: "Verification Evidence"
title: "ESV-U1 Editor Scene View execution contract"
description: "Verifies the private one-device Editor image-to-window execution, exact composite receipt, target retirement, and whole-owner replacement boundary."
timestamp: 2026-08-16T08:49:16Z
record_id: "1de9903b1b7b4623a0633b2a6565e0b0"
resource: "editor-scene-composition-view-v1"
tags: ["editor", "scene-view", "render", "esv-u1"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "1f97c48"
verified_by: "focused-nextest-check-build-native-canary"
---

# Verification

ESV-U1 is implemented by commit `1f97c48`. The private Editor now uses one
`WgpuEditorExecution` for its WGPU instance, device, queue, Editor window surface, and one exact
offscreen image target. The low-level egui renderer borrows that owner through scoped private
callbacks instead of creating a second hidden WGPU lifetime.

`RenderFramePacket` remains a one-target value with closed Window and Image forms. Managed
runtimes retain their opaque runtime generation; the private Editor uses a distinct process-local
render source generation rather than constructing a fake Play runtime. The ordinary runtime WGPU
path remains window-only and rejects the private Editor producer and image target.

# Result

- The fixed transaction submits one Scene image, verifies that egui samples the exact registered
  texture generation, then submits and presents the Editor window.
- A composite receipt is created only when image submission, exact egui sampling, and window
  presentation share one backend instance and device epoch.
- Image submission is not reported as surface presentation, and the packet keeps one exact target.
- Resize retires the old image target and texture registration before replacement. Terminal device
  failure replaces the complete execution owner and toolkit GPU state while leaving the Editor
  document/session outside that lifetime.
- ADR 0111 resolves OQ-001 only for this fixed edge and OQ-022 only for edit-world Scene View
  ownership. No RenderGraph, public Viewport, target registry, Game View, or second async runtime is
  admitted.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check -p nara-editor`
  - passed.
- `CARGO_BUILD_JOBS=1 cargo check -p nara_render_wgpu --no-default-features`
  - passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_render -p nara_render_wgpu --all-features -E
  'test(/(frame_packet|composite_receipt|target_matching)/)'`
  - passed: 7 tests; 103 tests skipped by the focused filter.
- `CARGO_BUILD_JOBS=1 cargo build -p nara-editor`
  - passed.
- Windows native canary:
  - launched `target/debug/nara-editor.exe tests/fixtures/runtime-runner/renamed-root/project`;
  - after five seconds the titled `Nara Editor` window remained live and responsive;
  - the process was then explicitly stopped;
  - the application would terminate its frame path if the exact egui texture identity or composite
    receipt join failed.
- `cargo fmt --all`
  - completed.
- `git diff --check`
  - passed before the implementation commit.

# Evidence Boundary

The native canary proves the private GPU execution and composite-receipt tracer. It does not prove
a real authored Scene preview, Scene composition controls, visible object placement, picking, or
the final source-to-check/run/export trajectory. Those remain ESV-U2 through ESV-U5 and prevent ADR
0111 from being marked fully implemented.

# Follow-up

Activate ESV-U2 only. Add revision-bound Scene Composition and canonical tree actions in the
existing Scene/Tooling owners without UI or WGPU fixtures. Keep preview materialization, egui Scene
View, picking, Game View, docking, and general render scheduling outside U2.

# Citations

- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md`
- `crates/nara_render/src/lib.rs`
- `crates/nara_render_wgpu/src/editor_execution.rs`
- `crates/nara_render_wgpu/src/presented.rs`
- `editor/src/platform.rs`
- Commit `1f97c48`
