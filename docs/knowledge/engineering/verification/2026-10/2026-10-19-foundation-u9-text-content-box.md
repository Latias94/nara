---
type: Verification Evidence
title: Foundation U9 shared text content geometry
description: Padded text measures and paints one content box while outer interaction geometry remains intact.
timestamp: 2026-10-19T09:09:00Z
record_id: f086368050f6437e898751dd27158290
producer_id: codex-root
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Design Review

Taffy's text measure callback receives available content space with padding/borders removed;
known dimensions can describe the outer box. Measuring at the known outer width caused too few
wrapped lines and an undersized automatic height. The callback now returns intrinsic content
dimensions, leaving outer sizing and insets to Taffy.

Final shaping and `UiPaintPrimitive::Text` use the compiler-resolved content box. Text Clip
intersects that content box with inherited clipping. Empty or entirely clipped content omits text
without shrinking background, hit, focus or semantic bounds. Presentation lowering uses the
published content origin and never applies padding again.

The independent review found that changing paint geometry alone left the auto-height measurement
defect. That finding led to the callback correction and an asymmetric Flow/Flex regression proving
parent height and the following sibling's position. ADR 0102's neutral checkpoint was reopened
against the existing gallery and pinned Bevy/Godot B10 comparison.

# Serial Verification

Windows, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, repository lockfile and target:

- Initial regression run `224ab24f-4251-437b-94fc-1c28a1cd11fa` showed outer bounds used for padded
  button text. A separate missing-key fixture panic was corrected and is not counted as defect proof.
- Intermediate cohort `63682f50-1c67-4adb-859e-152157939659`: 120 passed, two failed. The real
  auto-height mismatch was 76.8 versus 134.40001 content height. The unrelated snapshot failure was
  a CRLF fixture comparison; only line-ending normalization was added, without changing golden data.
- Corrected full cohort `b2f7426c-42b0-4839-bc3d-f92daff12e7b`: all 123 UI kernel/render tests passed.
  It includes Absolute and Flow auto-height, asymmetric padding, nested clipping, fractional scale,
  empty content, unchanged interaction bounds, equivalent glyph packets and existing frame/order/
  retention/presentation rejection cases.

# Remaining Evidence

The amended neutral `layout_gallery` must run before the successor conformance checkpoint closes.
The final workspace gate remains separate. No native Editor layout or screen presentation claim
is inferred from the backend-neutral tests.
