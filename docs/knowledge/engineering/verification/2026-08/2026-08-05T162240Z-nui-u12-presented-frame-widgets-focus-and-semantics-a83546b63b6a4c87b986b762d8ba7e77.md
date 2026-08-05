---
type: "Verification Evidence"
title: "NUI-U12 presented-frame widgets focus and semantics"
description: "Verifies exact retained-frame interaction routing, controlled Button intents, modal focus and capture, complete semantic state, and the bounded optional AccessKit mapping."
timestamp: 2026-08-05T16:22:40Z
record_id: "a83546b63b6a4c87b986b762d8ba7e77"
tags: ["nui-u12", "ui", "widgets", "focus", "semantics", "accessibility", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f9abd9b4bbbc57e0da9fe08d015073797118308c"
verified_by: "serial focused nextest, optional-feature and default-feature checks, strict Clippy and rustdoc, dependency inspection, formatting, staged diff review, and inline reuse/quality/efficiency review"
---

# Verification

NUI-U12 landed in `f9abd9b4bbbc57e0da9fe08d015073797118308c`.

- `UiInteractionSession` resolves every event-time `UiRouteStamp` against the exact retained
  `UiFrame` and `UiPresentationReceipt`, while consulting the latest submitted frame only to
  fail closed for retired or hidden targets. Compiled-but-unsubmitted, retired-pair, stale-geometry,
  disabled, inert, outside-modal, and unavailable-action paths cannot activate a widget.
- The kernel owns bounded per-surface seat, pointer, capture, hover, focus, modal restoration, and
  navigation state. Dispatch is candidate-atomic, emits capture/target/bubble facts, cancels stale
  capture and focus, and keeps pointer, keyboard, gamepad, and accessibility activation on one
  typed path.
- `nara_ui_widgets` owns the controlled `UiButton` reducer and bounded `UiIntentBatch`. It stores
  no product value or callback authority, clears stale local state when disabled, and emits only a
  typed `Activate` intent for the owning Runtime or Editor adapter to interpret.
- The formal frame semantic tree and presented semantic snapshot share the same exact stable keys,
  parentage, roles, names, values, enabled/selected/expanded state, actions, logical geometry, and
  focus facts. Runtime pressed state cannot survive a newer disabled frame.
- `nara_winit` contains an optional, bounded AccessKit schema bridge. Stable `NodeId` allocation is
  retained for at most 4,096 exact keys, updates are failure-atomic, and only Click and Focus map
  back into renderer-neutral kernel events. AccessKit remains absent from kernel, widgets, packet,
  headless, and default `nara_winit` contracts; native OS adapter installation remains a later
  product decision.
- The interaction implementation is split by ownership into routing, pointer, focus, state,
  semantics, types, and session modules. The inline reuse/quality/efficiency pass found no reusable
  cross-domain abstraction; it removed duplicate preflights and a redundant fallible path, made the
  route-parent API explicit, and closed selected/expanded plus disabled-focus/pressed gaps.

# Result

Passed for the NUI-U12 completion signal.

- `cargo nextest run -p nara_ui_kernel -p nara_ui_widgets` passed 40 tests.
- `cargo nextest run -p nara_winit --features accessibility` passed 32 tests, including the
  selected/expanded semantic and AccessKit projection proof.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml` passed 3 independently locked
  consumer tests from authored Button through exact submission, focus, activation, and typed intent.
- `cargo nextest run -p nara_input` passed 30 normalized-timeline tests used by the dispatch boundary.
- `cargo check -p nara_winit --all-targets --no-default-features` proved that the default window
  adapter does not pull AccessKit or the UI kernel.
- Strict `-D warnings` Clippy passed for `nara_ui_kernel`, `nara_ui_widgets`, and accessibility-enabled
  `nara_winit`; `RUSTDOCFLAGS=-D warnings` documentation, workspace formatting, dependency-tree,
  staged-diff, and whitespace checks also passed.
- A serial workspace all-target check compiled the changed production packages and remains blocked
  only by concurrent unstaged user work in `crates/nara_ui_render/src/tests.rs`, which still imports
  the removed `HierarchyConstructionWriter` and calls the removed
  `UiNode::with_sibling_order`. NUI-U12 did not modify, stage, hide, or revert that WIP.

# Evidence

- `crates/nara_ui_kernel/src/interaction/session.rs#UiInteractionSession`
- `crates/nara_ui_kernel/src/interaction/routing.rs`
- `crates/nara_ui_kernel/src/interaction/semantics.rs#UiSemanticSnapshot`
- `crates/nara_ui_kernel/src/journal.rs#UiFrameJournal`
- `crates/nara_ui_kernel/tests/interaction.rs`
- `crates/nara_ui_widgets/src/button.rs#UiButtonState`
- `crates/nara_ui_widgets/src/intent.rs#UiIntentBatch`
- `crates/nara_ui_widgets/tests/button.rs`
- `crates/nara_winit/src/accessibility.rs#WinitAccessibilityBridge`
- `crates/nara_winit/tests/accessibility.rs`
- `ui-module-consumer/tests/text.rs#direct_consumer_reduces_a_presented_button_to_a_typed_intent`

# Follow-up

NUI-U13 must replace the legacy ECS execution-tree UI with one authoring adapter over ordered
hierarchy, exact projected keys, `UiFrameJournal`, `UiInteractionSession`, and controlled widget
state. It must delete ECS layout/hit-test/focus authority and renderer-bound root/panel fields rather
than wrapping them, preserve Scene/Prefab/Retry structural order and identity, reject legacy codec
shapes, and publish typed Runtime actions without exposing kernel compiler internals in ordinary
preludes.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-05T122212Z-nui-u10-renderer-neutral-immutable-ui-frame-kernel-524095a3d0614d7390aa2b2f8e8d3442.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-05T132051Z-nui-u11-normalized-input-timeline-and-pause-safe-routing-aba5a9f9a3664dfdaf3093313a43989f.md`
