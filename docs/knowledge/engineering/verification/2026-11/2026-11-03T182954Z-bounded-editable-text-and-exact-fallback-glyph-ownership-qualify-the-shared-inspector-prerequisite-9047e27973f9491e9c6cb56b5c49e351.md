---
type: "Verification Evidence"
title: "Bounded editable text and exact fallback glyph ownership qualify the shared Inspector prerequisite"
description: "Parley and ICU editing, exact admitted faces, preedit restoration, focus lifetimes, neutral consumers and actual GPU masks qualify the text prerequisite while the complete Inspector remains open."
timestamp: 2026-11-03T18:29:54Z
record_id: "9047e27973f9491e9c6cb56b5c49e351"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-04-owned-inspector-and-editable-text-plan.md"
git_branch: "refactor/foundation"
git_commit: "59c9edb"
---

# Verification

Windows 11, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, `RUSTUP_TOOLCHAIN=1.95.0`, one Cargo invocation at
a time and the root `target`. Base `59c9edb`; this record covers the shared text/field prerequisite
under the [owning unit](../../../../plans/2026-11-04-owned-inspector-and-editable-text-plan.md).
Uncommitted Editor controller/Host integration is excluded from this qualification.

| Gate | Result |
| --- | --- |
| Text, widgets, kernel, planner, Runtime UI/serde and recording accessibility | 367/367; `3d08d32d-2cf9-458e-ab03-058bbc436b77`, with 12 existing ignored cases excluded |
| Actual WGPU UI cohort with DX12 selected | 31/31; `b188bd34-cacc-4f75-aec9-e102b0ebfde5`, including clipped fallback alpha readback versus independent single-face rasters and a primary-only missing-glyph control |
| Independent public UI workspace | 7/7; `91459f80-109b-4ac4-8f90-2d151cf13c62`, including local editing and actual ordered fallback compilation without ECS, Scene or renderer dependencies |
| Neutral galleries | Layout, interaction and headless presentation galleries execute successfully with their existing assertions |
| Correctness Clippy | All targets of the six affected packages pass with existing unrelated warnings; final text/widget all-target `-D warnings` gate passes without warnings |
| Independent review | Text/font and widget reviewers close their actionable findings after focused source re-review |

Commands, run serially:

```text
cargo +1.95.0 nextest run --locked -p nara_text -p nara_ui_widgets -p nara_ui_kernel -p nara_ui_render -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --test-threads 1 --no-fail-fast
cargo +1.95.0 nextest run --locked -p nara_render_wgpu --all-features --lib -E 'test(ui::tests::)' --test-threads 1
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --target-dir target --test-threads 1
cargo +1.95.0 clippy --locked -p nara_text -p nara_ui_widgets -p nara_ui_kernel -p nara_ui_render -p nara_ui -p nara_winit --features nara_ui/serde,nara_winit/accessibility --all-targets -- -D clippy::correctness
cargo +1.95.0 clippy --locked -p nara_text -p nara_ui_widgets --all-targets -- -D warnings
cargo +1.95.0 run --locked -p nara_ui_kernel --example layout_gallery
cargo +1.95.0 run --locked -p nara_ui --example interaction_lab --features serde
cargo +1.95.0 run --locked --example ui_presentation_gallery --features presentation-ui,desktop-winit,render-wgpu
```

The Runtime UI suite needs its existing `serde` feature: the first narrower invocation could not
compile existing cooked-Scene tests. This qualification uses the corrected feature selection.

# Result

The finite font set admits at most eight faces, 32 MiB per container and 64 MiB across selected
faces. Collection selections preserve original bytes plus face index. Private family aliases
preserve ordered selection even for equal family names. Default-instance shaping suppresses
automatic weight/width/slant synthesis, verified with the real Mona Sans variable fixture.
Each glyph retains its actual face, and packet atlas bytes match independent raster masks.

The local single-line editor uses Parley layout/selection/bidi affinities and ICU grapheme
boundaries. All text/layout/selection/history/composition adoption is atomic. Preedit keeps the
complete original committed buffer and selection; cancel restores both, while commit replaces
the selection once. Local Undo/Redo retains at most 32 entries and never invokes document history.
An equal primary font does not authorize another fallback set/order or changed resource limits.

Each field has a unique live incarnation and a distinct admitted focus token. Blur, disable and
same-property recreation reject old input. The compiler and packet use one inset text/selection/
caret clip and shared pointer/candidate coordinates. TextInput values preserve empty committed
text without leaking preedit through an automatic child semantic node. The recording AccessKit
bridge publishes label/value/focus without claiming SetValue or complete text accessibility.

# Corrected failures and rationale

- The first text RED (`1e87481b-dc2f-48c4-89c8-d2396de08c5e`) stopped inside `e + U+0301` at byte
  one. Parley expands a shaped ligature into scalar components; those components also subdivide
  unsupported emoji ZWJ sequences. ICU bounds navigation, hit placement and deletion while
  Parley retains visual stepping and caret geometry. Deleting a separator can form a new flag
  grapheme, so complete-text changes recheck the resulting boundary as well.
- Review found the shaper's `u8` map counter and fixed 32-item cluster map. Full replacement
  proposals, including repeated combining-mark insertions and deletion joins, reject more than
  32 scalars in one grapheme before calling Parley. The rejected proposal preserves history and
  can be retried. Direct layout admission enforces the same ceiling.
- The first full owning run also reproduced a debug assertion on a malformed TTC face-count
  table. Container admission now bounds the complete offset table before entering the parser.
- Review found a missing key on the internal text node, empty-value refusal and inconsistent
  inset clipping. The field now owns a stable internal key, explicit committed semantic text and
  one content clip. Presentation may legitimately move focus to another eligible key when a
  target retires; this does not transfer its predecessor's text token.
- A selected font's variation instance must match its raster instance. This unit admits the
  default instance and suppresses automatic synthesis rather than silently shaping one weight
  and rasterizing another. Custom variation-axis authoring remains outside this interface.
- Nominal cmap coverage unions hid an actual missing combining mark. Scalar attribution then
  misidentified a ligature's carrier, and run-wide attribution falsely implicated successfully
  normalized text. The final policy uses actual missing shaped glyphs and visual ligature
  groups, including RTL order. Multi-scalar failures report their complete source sequence,
  without implementing a partial normalization/shaping engine. The exact four-sequence control
  passes as `c0778690-a336-4721-9a9d-aadd9086bb76` after separating the single-face RTL control
  from the intentionally two-face coverage control.

# Follow-up and limits

The shared checkpoint's changed public inventory is named in the owning plan and explained in the
[guide](../../../../guides/editable-text.md). Prior neutral assertions remain unchanged. The
complete Nara-owned Inspector, external semantic-tool trajectory, native Host image/composite
admission, native IME ownership/candidate placement and predecessor removal still need their
own executed workflows. The broader foundation Goal, R11 and R12 stay open.

The new CPU editor accepts Unicode strings but its selected fonts determine visible coverage.
The GPU test covers two fixed fonts at 24 px and 1x scale; it does not qualify every script or
native font fallback. Injected composition and recording accessibility do not establish actual
OS input-method delivery, assistive-technology parity, monitor scanout or Linux/macOS support.
Existing glyph-extent policy bounds published raster results; this unit does not establish a
new bound on every upstream rasterizer temporary allocation.
