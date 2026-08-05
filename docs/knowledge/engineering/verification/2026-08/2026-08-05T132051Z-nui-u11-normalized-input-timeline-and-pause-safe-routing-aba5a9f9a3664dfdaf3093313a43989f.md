---
type: "Verification Evidence"
title: "NUI-U11 normalized input timeline and pause-safe routing"
description: "Verifies the bounded globally ordered normalized input timeline, event-time route stamps, atomic lifecycle cancellation, independent consumer cursors, and pause-safe UI-before-gameplay handling."
timestamp: 2026-08-05T13:20:51Z
record_id: "aba5a9f9a3664dfdaf3093313a43989f"
tags: ["nui-u11", "input", "timeline", "routing", "pause", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "c54d3667367f91dec1ad66118b2943e2922ce196"
verified_by: "serial focused nextest, Rust 1.95 all-target checks, strict Clippy, dependency inspection, formatting, and staged diff review"
---

# Verification

NUI-U11 landed in `827ccb2ce37e895987d6e775d9b413b69423d3fb`; the contract-only module split
landed in `c54d3667367f91dec1ad66118b2943e2922ce196`.

- `InputTimeline` is the renderer- and UI-independent platform-event authority. It retains at most
  4,096 fixed-shape entries in one global, monotonically increasing, non-wrapping `u64` sequence.
  Capacity and sequence admission are preflighted before event state or compatibility projections
  change.
- Every entry captures an opaque event-time route stamp: window, optional surface, seat, optional
  pointer, optional presentation epoch, and route generation. Pointer events use their direct
  window route; keyboard, gamepad, and accessibility events use the seat's focused-window snapshot.
  Unpublished routes remain observable and actionable by gameplay but are not UI-routable.
- Observation, UI routing, action scanning, and oldest deferred action progress are independent.
  Observation and UI routing run in `CoreStage::First`; action-bearing edges remain deferred while
  `PreUpdate` is paused, and a UI-consumed edge is skipped by the first resumed action pass.
- Winit converts physical cursor coordinates to logical coordinates at ingress and records focus,
  resize, scale-factor, close, external destruction, and controlled surface retirement before
  forwarding window-domain events. Route retirement is fail-closed even if the event timeline is
  full.
- Pressed state is isolated by `(window, seat)` for keys and `(window, seat, pointer)` for pointer
  buttons. Focus loss, pointer leave, resize, scale change, close, and surface retirement emit
  deterministic cancellation edges. Pointer leave releases only the exact pointer; losing an
  unfocused window cannot release the focused window's keys.
- `ButtonInput` and `PointerState` remain compatibility observations, not routing authority.
  Timeline-origin edges carry their global source sequence so legacy action resolution cannot
  deliver them twice.
- The implementation is physically split between `timeline/types.rs` for stable event/route/error
  contracts and `timeline.rs` for state, atomic ingress, retention, and consumer progress.
  `nara_input` depends only on ECS/App/Core foundations and has no text, UI, window-backend, render,
  or WGPU dependency.

# Result

Passed for the NUI-U11 completion signal.

- `cargo nextest run --locked -p nara_input -p nara_winit --all-features
  --test-threads=1 --no-fail-fast` passed 63 tests. Coverage includes cross-device event order,
  event-time presentation stamps, multi-window/seat/pointer separation, exact pointer leave,
  non-focused-window isolation, capacity and sequence exhaustion, route retirement, Winit focus
  ingress, and paused UI consumption.
- `cargo nextest run --locked -p nara_window -p nara_gameplay --all-features
  --test-threads=1 --no-fail-fast` passed 46 tests. The focused RuntimeInstance pause/resume
  regression also passed independently.
- Rust 1.95.0 all-target/all-feature checks passed for `nara_input` and `nara_winit`. Strict
  package-local `-D warnings` Clippy passed for both, along with formatting, dependency-tree, and
  staged-diff checks.
- A serial Rust 1.95 workspace all-target/all-feature check compiled every production package and
  reached the test-target tail. It remains blocked only by concurrent unstaged edits in
  `crates/nara_ui_render/src/tests.rs`, which import the removed
  `HierarchyConstructionWriter` and call the removed `UiNode::with_sibling_order`. NUI-U11 did not
  modify, stage, or hide that user-owned WIP.

# Evidence

- `crates/nara_input/src/timeline.rs#InputTimeline`
- `crates/nara_input/src/timeline/types.rs#InputRouteStamp`
- `crates/nara_input/tests/timeline_runtime.rs`
- `crates/nara_winit/src/lib.rs#WinitApp`
- `crates/nara_winit/src/tests.rs#winit_ingress_captures_the_exact_route_before_focus_loss_invalidation`
- `tests/runtime_instance.rs#paused_input_edges_are_retained_until_the_next_resolving_frame`

# Follow-up

NUI-U12 should consume only pending UI-routable entries, resolve each stamp against the exact
retained `UiFrame`/submission receipt, and own capture, focus, modal, navigation, and semantic
dispatch in the kernel. `nara_ui_widgets` should remain a controlled reducer layer that emits
bounded typed intents without World, Host, Editor, Winit, or AccessKit authority.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u11-establish-one-normalized-input-timeline`
- `docs/architecture/adr/0036-event-message-and-resource-queue-lifetime.md`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
