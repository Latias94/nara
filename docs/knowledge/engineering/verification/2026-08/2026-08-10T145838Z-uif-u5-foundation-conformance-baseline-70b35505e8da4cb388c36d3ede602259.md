---
type: "Verification Evidence"
title: "UIF-U5 Foundation Conformance Baseline"
description: "Pins the renderer-neutral UI public inventory, neutral examples, independent consumer, and exact backend-accepted presentation assertions before product validation."
timestamp: 2026-08-10T14:58:38Z
record_id: "70b35505e8da4cb388c36d3ede602259"
tags: ["ui", "foundation", "conformance", "public-api", "uif-u5"]
status: "complete"
producer_id: "codex-root"
run_id: "uif-u5"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e307cb3"
verified_by: "codex-root"
supersedes: "9c48d2de48184e7e973c9871b7e32b6c"
---

# Verification

- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui_widgets -p nara_ui
  --test-threads=1` passed all 146 tests with run ID
  `01b4d96a-b38d-499b-86be-23f047eb28b6`.
- `cargo nextest run --locked -p nara_ui_kernel --test compile_order --test failure_atomic
  --test validation --test profile --test text_budget --test-threads=1` passed all 31 tests with
  run ID `4ef604d7-82f8-431d-9ab8-6e5d728383b0`.
- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui -p nara_ui_render
  -p nara_render_wgpu --all-features --test-threads=1` passed all 225 tests with run ID
  `e4c1990b-b9f7-4b43-a722-295527f59689`.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1`
  passed all three tests with run ID `eb5dfad4-3a88-4049-ab14-d1bc8fe1e2a5`.
- `cargo run --locked -p nara_ui_kernel --example layout_gallery` passed and reported
  `roots=3, layouts=10, paint=14, semantics=6, scales=[1.0, 1.25]`.
- `cargo run --locked -p nara_ui --example interaction_lab` passed after its presenter was changed
  to require packet planning, backend submission permission, exact backend generation binding and
  a backend-issued one-shot commit before Host receipt publication.
- `cargo run -p nara --locked --no-default-features
  --features presentation-ui,desktop-winit,render-wgpu --example ui_presentation_gallery` passed
  the owned packet, matching receipt and accessibility-action round trip.
- `cargo fmt --all -- --check` and strict no-dependency clippy for `nara_ui_kernel`, `nara_ui` and
  `nara_ui_render` across all targets and features passed. Strict root clippy still reports only the
  pre-existing unrelated `result_large_err` findings in `src/project_host/ingest.rs` and the
  `needless_lifetimes` finding in `src/startup_scene.rs`.

# Result

UIF-U5 is complete at commit `e307cb3`. This record publishes a **Foundation Conformance
Baseline**. It does not claim that the Reference Game product workflow is validated.

## Frozen Public Inventory

- `nara_ui_kernel` owns renderer-neutral authoring admission, immutable compilation, layout,
  logical paint, hit testing, focus, semantics, root outcomes and modal-barrier facts. Its public
  consumer boundary is `UiAuthoringSnapshotBuilder`, `UiCompiler`, `UiEnvironment`, `UiTheme` and
  immutable `Arc<UiFrame>` facts. ECS, Host, WGPU and toolkit identities remain outside it.
- `nara_ui` owns the Scene/ECS adapter, Surface Slot lifecycle and presented input projection. The
  ordinary prelude exposes surface installation/update/suspend/restore/retire operations, immutable
  frame and semantic snapshots, and exact presented input dispatch. It does not expose receipt
  minting or a fake presenter.
- `nara_ui_render` owns resource-complete presentation planning and immutable
  `Arc<UiPaintPacket>` values with closed content/backend outcomes and exact generations. The root
  facade composes this boundary without reopening a public render graph or live asset read.
- Ordinary product and independent-consumer code imports no `__private` presentation authority.
  The only hidden Host authority used by first-party conformance is confined to
  `crates/nara_ui/examples/interaction_lab/presenter.rs` and
  `examples/ui_presentation_gallery/presenter.rs`; both prove actual backend acceptance before
  receipt commit.

## Exact Assertions

- The layout gallery proves ordered multi-root Flex and Absolute layout, clipping shared by paint,
  logical geometry stability across scale, structural focus/semantics order and immutable prior
  frame facts.
- The interaction lab proves Scene/ECS authoring, two isolated surfaces, timeline arbitration,
  capture/focus/modal behavior, exact presented behavior, accessibility activation, fail-closed
  routing and retained journal cursors. Backend acceptance is a required precondition for receipt
  publication.
- The presentation gallery proves owned solid/image/glyph packet data, exact packet identity,
  backend-issued one-shot commit, matching receipt and semantic action. A retired packet remains
  readable without a live World or asset store.
- The independently locked `ui-module-consumer` proves layout, text and semantic consumption without
  importing the product facade or hidden Host authority.

## Reference Pressure

The conformance boundary was checked against the pinned matrix in
`docs/architecture/ui-foundation-design.md`: Open-GPUI `67f0048d`, Dear ImGui Rust `b5244c46`,
egui `46ba6405`, Godot `4173760f` and Bevy `e8b3598f`. Unity remains a secondary analogy only. The
comparison supports a shared retained Foundation with independent Runtime and Editor product layers;
it does not admit a docking framework, generic binding DSL, toolkit-neutral widget trait or public
multi-window policy.

# Review

- Three independent simplification lenses reported no reuse, quality or efficiency findings.
- Standards and specification review each found the same P1: `interaction_lab` could prepare and
  commit a presentation without proving backend acceptance. Commit `e307cb3` corrected the owning
  example boundary; both reviews then passed with no remaining findings.
- The orphan legacy `crates/nara_ui/src/tests.rs` and `crates/nara_ui_render/src/tests.rs` files were
  removed instead of being counted as coverage. No Cartesian product matrix or opaque digest smoke
  was added.
- The earlier NUI-U15A statement that all examples were public-only is narrowed: ordinary consumers
  are public-only, while named first-party presenter glue may use hidden non-forgeable authority only
  when it proves the real backend acceptance path.

# Follow-up

The Runtime UI Product Slice successor must pin this record and prove the real product trajectory,
authoring roundtrip, desktop/backend presentation, headless parity and packaged arbitrary-CWD path.
If that work exposes a shared defect, the product plan is suspended while a focused UIF correction
publishes a successor baseline; neither this record nor earlier evidence is rewritten.

# Citations

- `docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/architecture/adr/0109-root-local-ui-compilation-and-presentation-closure.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-08T051354Z-nui-u15a-neutral-ui-conformance-and-public-boundary-freeze-32535c96ab3b49d3affa4ecdcbb22e39.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T055148Z-uif-u4c-surface-suspension-and-platform-authority-closure-9c48d2de48184e7e973c9871b7e32b6c.md`
