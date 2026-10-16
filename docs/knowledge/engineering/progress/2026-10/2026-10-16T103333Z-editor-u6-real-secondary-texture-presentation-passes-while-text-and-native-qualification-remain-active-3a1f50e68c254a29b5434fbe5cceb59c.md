---
type: "Work Progress"
title: "Editor U6 real secondary texture presentation passes while text and native qualification remain active"
description: "Work Progress for Editor U6 real secondary texture presentation passes while text and native qualification remain active."
timestamp: 2026-10-16T10:33:33Z
record_id: "3a1f50e68c254a29b5434fbe5cceb59c"
producer_id: "codex-root"
run_id: "editor-foundation-20261016"
related_plan: "docs/plans/2026-10-16-1229-refactor-editor-foundation-and-frontends-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bb6314e"
---

# Summary

U9 and U1-U5 remain committed through `bb6314e`. U6 is active and uncommitted. The real Windows
reference-game slice passed primary Scene/Game -> detached Game -> primary return, with four exact
frames in each placement, rejection of an overwritten-image receipt and normal ordered shutdown.
This is not an adoption pass; U7-U8 remain pending and the production frontend remains egui.

# Details

- Direct registry Dear core/Winit 0.20.0 dependencies compile under Rust 1.95. The WGPU crate is a
  local, provenance-documented copy of the released 0.20.0 source with a bounded observability patch
  under `vendor/dear-imgui-wgpu/`; `repo-ref/` was not changed.
- The released route exposed viewport ID lists only. The patch reports actual successful Elements
  `TextureView` samples and exact backend-constructed secondary presents. Nara's optional private
  bridge prepares a fresh FrameToken synchronously, validates same Device/Queue and the latest
  successful image submission, and joins before primary acquire. It does not submit secondary work.
- Root corrected primary acquired extent, tracked latest successful writes per image allocation,
  and made secondary identity mapping exclude any configured primary WindowId.
- Native run `41542` passed with `primary_frames=4`, `secondary_frames=4`, `returned_frames=4`,
  `first_present_ms=6`, old receipt rejected and renderer-then-platform shutdown. First-present starts
  after GPU setup, so it is not a cold-launch or comparative benchmark. IME remained unverified.
- Vendor nextest run `e5ff46d3-7456-4dd8-b9e7-ec4e82be1516`: 100 passed, 0 skipped, actual adapter
  required. Root integrated nextest run `b6cda40e-d9fb-4d4d-b926-87323cb99c12`: 204 passed, 0 skipped
  across editor and WGPU libraries, including input owner isolation and secondary identity mapping.
- Focused vendor review found a possible arbitrary-primary identity collision; root corrected it.
  Focused input review subsequently found that Dear saw the initial capture press but not its
  release. Root has just corrected release forwarding; this last change is not yet reverified.
- `editor/src/platform/imgui/input.rs` is frozen worker output now owned by root and wired into
  the Host. It reuses the existing input router, real native viewport mapping and live Context
  hittest. Transient preedit never becomes committed text.
- Native initial failure attempted a Game candidate before Runtime admission; the slice now checks
  active start/target first. A D3D12 debug-layer exception during early-error Vulkan/device teardown
  was observed with CDB; normal trajectory passes, early-error teardown remains a separate gap.

# Next Action

Root is the sole Cargo/formatter/index/commit owner. No Cargo process remains active at this
checkpoint. Preserve all shared edits. Complete U6 text/CJK font and IME verification, actual
clipboard/navigation, persistent layout, view close/reopen/resize, DPI, device recovery and measured
same-workload comparison before adoption. Add meaningful input release regression evidence and
rerun the native slice after final integration. Perform simplification and focused/full reviews at
their actual gates. Do not port U7 or delete egui while U6 is pending.

Reference native command: set `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`,
`CARGO_TARGET_DIR=F:/SourceCodes/Rust/nara/target`, `NARA_IMGUI_QUALIFICATION_CANARY=1`, then
`cargo +1.95 run --manifest-path reference-game/Cargo.toml --locked --features editor-imgui-qualification --bin editor_imgui_qualification`.
Reference-game's `cc` lock was narrowly updated to 1.8.0 for Dear's required build family.
The machine has Chinese and English keyboard layouts (`0x8040804`, `0x4090409`), and Windows
SimHei/MS YaHei files exist; those read-only facts do not establish real IME behavior.

An empty untracked `myeasylog.log` appeared during native/debugger execution. Do not stage it or
delete it without establishing ownership. All U6 implementation/research files are still uncommitted.

# Citations

- [Active adoption evidence](../../../../research/2026-10-16-editor-foundation/adoption-evidence.md)
- [Renderer contract](../../../../research/2026-10-16-editor-foundation/u6-renderer-contract.md)
- [Text baseline](../../../../research/2026-10-16-editor-foundation/u6-text-baseline.md)
- [Previous registration](../../registry/2026-10/2026-10-16T094849Z-nara-editor-foundation-implementation-codex-root-14c36d005b0c4171afccdaaa32720e10.md)
