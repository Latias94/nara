---
type: "Verification Evidence"
title: "NUI-U10 renderer-neutral immutable UI frame kernel"
description: "Verifies the bounded renderer-neutral compiler, exact structured identity, stable layout and presentation order, root-local failure atomicity, and exact submitted-frame receipts."
timestamp: 2026-08-05T12:22:12Z
record_id: "524095a3d0614d7390aa2b2f8e8d3442"
tags: ["nui-u10", "ui-kernel", "layout", "identity", "presentation", "verification"]
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3fc486a016c7c894d7a11413e2df769be6f6b13e"
verified_by: "serial focused nextest, independently locked consumer, Rust 1.95 compilation, strict Clippy, dependency-feature inspection, rustdoc, formatting, and diff checks"
---

# Verification

NUI-U10 landed at `3fc486a016c7c894d7a11413e2df769be6f6b13e`.

- `nara_ui_kernel` is split into eight ownership modules: authoring, compiler, frame, geometry,
  identity, journal, limits, and style. It depends only on `nara_text`, exact Taffy 0.10.0, and
  `thiserror`; Taffy remains private with only `std`, `taffy_tree`, `flexbox`, and `content_size`.
- Root-scoped stable keys distinguish runtime identity from an opaque source entity plus a
  structured prefab projection path. Keys are exact, bounded to 256 encoded bytes, and never parse
  the transitional expanded Scene anchor string.
- One immutable `UiFrame` owns layout, paint, reverse hit order, focus order, semantics, root
  outcomes, and bounded diagnostics for one exact logical environment. Flex and absolute layout
  share the same logical geometry; nearest-edge physical snapping is recorded separately for scale
  factors 1.0, 1.25, 1.5, and 2.0.
- Presentation order is layer, explicit root sequence, hierarchical stacking path of local z and
  structural index, then fragment order. Keys, entity allocation, and query order are not ordering
  fallbacks. Opaque `UiImageId` values carry content identity without exposing an asset or backend
  handle.
- Compilation is atomic per root. A rejected root may retain only an exact-environment last-good
  visual; retained visuals are inert and cannot contribute hit, focus, or semantic authority.
  Removed roots retire their cache state.
- `UiFrameJournal` retains the current and two prior exact submitted frame/packet pairs. It rejects
  compiled-but-unsubmitted, mismatched, retired-frame, and retired-surface receipts. A receipt means
  only that the backend accepted and submitted the exact packet; it does not claim player-visible
  presentation.

# Result

Passed for the NUI-U10 completion signal.

- `cargo nextest run --locked -p nara_ui_kernel --test-threads=1 --no-fail-fast` passed 29 tests.
  Coverage includes nested Flex/Absolute layout, percent units, four scale factors, explicit root
  order, stacking contexts, local z plus structural order, clip/hit behavior, exact image and
  projected identities, key and resource ceilings, duplicate and missing keys, non-finite input,
  root-local fallback, root retirement, deterministic headless fixtures, bounded diagnostics, and
  current/prior/retired receipt matching.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1`
  passed two direct-consumer tests without the root facade, ECS, Scene, window, or render crates.
- Strict `-D warnings` Clippy passed for the kernel and independent consumer. Rust 1.95.0 checks
  passed for both. `RUSTDOCFLAGS=-Dwarnings cargo doc --locked -p nara_ui_kernel --no-deps -j 1`,
  formatting checks, dependency inspection, and staged diff checks passed.
- A serial full-workspace all-target check compiled every production package and the new kernel.
  Its test-target tail remains blocked only by concurrent unstaged edits in
  `crates/nara_ui_render/src/tests.rs`, which still import the removed
  `HierarchyConstructionWriter` and call the removed `UiNode::with_sibling_order`. NUI-U10 did not
  modify, stage, or hide that user-owned WIP.

# Follow-Up

NUI-U11 should add a bounded normalized input timeline with event-time route stamps, atomic
admission, independent observation/UI/action cursors, and pause-safe action retention. NUI-U12 can
then dispatch widgets and semantics against the exact presented frame without re-reading live
layout or route state.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u10-build-the-renderer-neutral-immutable-ui-frame-kernel`
- `docs/architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `crates/nara_ui_kernel/src/lib.rs`
- `ui-module-consumer/tests/text.rs`
