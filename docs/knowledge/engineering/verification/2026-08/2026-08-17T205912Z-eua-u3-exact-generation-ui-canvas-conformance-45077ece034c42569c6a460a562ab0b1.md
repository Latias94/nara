---
type: "Verification Evidence"
title: "EUA-U3 exact-generation UI Canvas conformance"
description: "Verifies exact U2 projection reuse, project-owned Canvas environment, resource-complete WGPU image presentation, composite-receipt pick authority, and bounded recovery."
timestamp: 2026-08-17T20:59:12Z
record_id: "45077ece034c42569c6a460a562ab0b1"
status: "completed"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "bca3e62"
verified_by: "codex-root"
---

# Verification

EUA-U3 was verified at commit `bca3e62` after the U2 toolkit-neutral authoring-frame baseline.
Cargo commands were run serially from the repository root.

# Result

Passed. The private Editor Canvas now compiles the exact U2 authoring projection through the Nara
UI kernel and presentation planner, without constructing a second Prefab hierarchy or a Runtime UI
session. The accepted project UI profile supplies the logical extent and safe-area transform;
project images are prepared from the same immutable content snapshot used by the Host. The
authoring-only image is submitted through the existing one-device Editor execution owner, while
selection remains unavailable until the exact image, texture registration, device epoch, and
Editor-window presentation join into one composite receipt.

Review corrections bind picking to the exact safe-area transform, reuse Runtime controlled-button
visual lowering, rebuild a consumed or deferred candidate on the next Editor frame, and reject a
Canvas target that exceeds the live GPU two-dimensional texture limit. The implementation does not
mint a Runtime `UiPresentationReceipt` or add a public image Surface/RenderGraph abstraction.

# Evidence

- `cargo nextest run -p nara_ui -E 'test(persistent_prefab_compiles_exact_source_local_canvas_facts) | test(canvas_pick_uses_safe_area_clip_visibility_and_paint_order)' --test-threads=1` - 2 passed.
- `cargo nextest run -p nara --features serde,runtime-2d,tooling,presentation-ui editor_ui_authoring_frame_and_continuous_gesture_share_one_exact_prefab_revision --test-threads=1` - 1 passed.
- `cargo nextest run -p nara_render_wgpu --features ui-submitter image_target_ui_packets_match_only_their_exact_target` - 1 passed.
- `cargo check -p nara-editor` - passed after the final review corrections.
- `cargo fmt --all`, `cargo fmt --all -- --check`, and `git diff --check` - passed; Git reported only the repository's line-ending notices.

The focused Canvas pick test covers asymmetric safe-area mapping, clipping, hidden content,
topmost paint order, and outside-content rejection in one scenario. Broader UI field, resolution,
theme, platform, and GPU matrices were intentionally not added.

# Follow-up

EUA-U4 owns the visible egui workflow over these established facts: source navigation, ordered UI
Hierarchy, typed Inspector controls, Canvas overlays and gestures, revision-bound draft invalidation,
and one bounded native canary. Candidate/presentation ownership remains private to the Host and
WGPU execution owner.

# Citations

- [Editor UI Prefab Authoring and Canvas v1 plan](../../../../plans/2026-08-17-1930-feat-editor-ui-prefab-authoring-and-canvas-v1-plan.md)
- [ADR 0113](../../../../architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md)
- [EUA-U2 verification](2026-08-17T171453Z-eua-u2-toolkit-neutral-ui-prefab-authoring-and-gesture-conformance-19920c861d394a4fb43db6f60e4cf2a5.md)
