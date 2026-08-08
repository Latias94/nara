---
type: "Verification Evidence"
title: "NUI-U15A neutral UI conformance and public boundary freeze"
description: "Freezes the kernel, runtime adapter, and root presentation UI boundaries through three executable public-only examples."
timestamp: 2026-08-08T05:13:54Z
record_id: "32535c96ab3b49d3affa4ecdcbb22e39"
status: "complete"
producer_id: "codex-root"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3c81645"
verified_by: "codex-root"
---

# Verification

NUI-U15A is complete at `3c81645`. Three executable, neutral consumers freeze the shared UI
boundary before Reference Game product integration:

- `crates/nara_ui_kernel/examples/layout_gallery.rs` consumes the renderer-neutral kernel
  directly;
- `crates/nara_ui/examples/interaction_lab.rs` consumes the ECS/Scene runtime adapter and the
  normalized input timeline;
- `examples/ui_presentation_gallery.rs` consumes the root product facade and the owned packet /
  presentation-outcome boundary.

The examples contain no Reference Game dependency or vocabulary, no private Nara imports, no
shared example harness, and no provider, registry, component-catalogue, scenario-DSL, adaptive
layout, smoke, digest, or pixel-matrix abstraction. Their local `fixture`, `presenter`, and `probes`
modules are concrete example implementation details rather than a second retained UI model.

# Result

Passed. The neutral consumers exposed and closed four owning-contract defects before the freeze:

1. deterministic direct-App/headless input now enters the sole `InputTimeline` instead of a second
   UI ingress path;
2. packet UVs have one named top-left-origin contract and WGPU performs the sole coordinate flip;
3. the presentation proof retains and accepts the exact `Arc<UiPaintPacket>` before committing its
   matching receipt, while `ContentRejected` cannot advance presentation authority;
4. installed surface revision allocation belongs to `UiRuntimeState`. Installation is create-only,
   typed world-owned size/endpoint/mapping updates derive from the current installed descriptor,
   and copied descriptor snapshots cannot manufacture a successor revision.

The implementation review found no remaining P0/P1 issue after those corrections. The first
causality review rejected scalar revision comparison as lineage because a copied stale branch could
advance its counters past the current branch; `3c81645` removes that path rather than weakening the
test. Reference Game changes in this commit are mechanical compilation migrations only and are not
product evidence.

# Frozen Public Inventory

The following inventory is the admitted public surface consumed by NUI-U15B. A signature or shared
semantic change, or a change to the assertions below, reopens U15A under R45.

## Kernel Consumer

- `nara_text::bundled_font_face` supplies the concrete bounded first-party face.
- `nara_ui_kernel` supplies `UiAuthoringSnapshot`, `UiAuthoringSnapshotBuilder`, `UiCompiler`,
  `UiEnvironment`, `UiTheme`, and immutable `Arc<UiFrame>` compilation.
- Kernel authoring/layout vocabulary is `UiRoot`, `UiNode`, `UiStyle`, `UiDimension`, `UiEdges`,
  `UiFlexDirection`, `UiOverflow`, `UiVisual`, `UiText`, `UiInteraction`, `UiSemantics`,
  `UiSemanticRole`, `UiColorRole`, and `UiLayer`.
- Stable backend-neutral identity and observation are `UiSurfaceId`, `UiRootKey`, `UiLocalKey`,
  `UiPaintPrimitive`, and the read-only frame layout, paint, focus, semantics, diagnostics, and root
  outcome projections.

## Runtime And Scene Consumer

- `nara_ui::prelude` supplies ordinary authoring and action-journal types: `UiRoot`, `UiButton`,
  `UiLayer`, `UiActionId`, `UiActionJournal`, consumer IDs, and cursors.
- `nara_ui::advanced` supplies bounded integration observations and commands: `UiFrame`, `UiKey`,
  `UiActivationSource`, `UiNonActivation`, `UiRouteRejection`, `UiSemanticSnapshot`,
  `UiSurfaceDescriptor`, create-only `install_ui_surface`, runtime-owned
  `update_ui_surface_logical_size`, `update_ui_surface_input_endpoint`,
  `update_ui_surface_mapping`, `latest_ui_frame`, `commit_ui_presentation`,
  `ui_semantic_snapshot`, and `retire_ui_surface`.
- Direct-App/headless hosts use public `InputTimeline::admit_keyboard`, `admit_mouse_button`,
  `admit_pointer`, and `admit_window_event`; accessibility and gamepad admission remain on that same
  timeline. Native adapters continue to use their runtime-scoped drivers.
- The neutral fixture uses the owning public domain APIs from `nara_app`, `nara_ecs`,
  `nara_hierarchy`, `nara_scene`, `nara_reflect`, and `nara_input` to combine one Scene-authored root
  with ECS-authored roots. None of those APIs is re-exported as a new UI framework.

## Presentation Consumer

- Root `nara` composition uses `RuntimeUiPlugins`, `PresentationUiPlugins`, the ordinary prelude,
  `advanced_prelude`, and backend/window types from `backend_prelude`.
- `nara::ui_render::advanced` supplies `UiPaintPacketBuilder`, `UiPreparedImages`,
  `UiPaintPacket`, `UiPacketPrimitive`, and the closed `UiPresentationOutcome` algebra.
- Prepared image capture uses the public concrete Image/render-resource APIs; packet construction
  retains immutable image and glyph page snapshots and does not re-read live assets during
  presentation.
- The example-local `HeadlessTestPresenter` is intentionally concrete. It is not a public trait,
  backend registry, or replacement for the first-party presentation bridge.

# Named Shared Invariants

1. **One structural order:** ordered hierarchy projection is the sole root/sibling structural
   sequence; layout, paint, reverse hit testing, focus, and semantics do not fall back to entity or
   query order.
2. **One input timeline:** every external event is admitted to `InputTimeline`; routing has typed
   `PassThrough`, consumed journal delivery, or fail-closed rejection. Direct dispatch remains a
   test-only kernel/runtime mechanism.
3. **Exact presented-session authority:** an unpresented candidate cannot replace old visible
   behavior. Only an exact accepted packet/frame pair produces a receipt; interaction and semantic
   activation resolve against the receipt's retained immutable behavior and input mapping.
4. **Surface isolation and owner revisions:** state, focus, capture, reducers, routes, and
   presentation epochs are per surface. Runtime-owned typed updates alone advance route and
   presentation revisions; duplicate creation, retirement, stale geometry, disabled targets, modal
   underlays, and invalid routes fail closed.
5. **Pause-safe bounded delivery:** typed actions remain in the bounded journal until every explicit
   consumer cursor acknowledges them; no frame-local vector silently drops paused work.
6. **Owned presentation resources:** a packet owns the exact image and glyph snapshots used by its
   primitives. Packet UVs use top-left origin (`u` right, `v` down), and a backend converts that
   convention exactly once.
7. **Explicit composition:** runtime/headless content stays presentation-free; root presentation
   composition opts into image preparation, packet compilation, windowing, and WGPU. Product facts
   cannot alter these shared boundaries without independently reopening U15A.

# Exact Conformance Assertions

- The layout gallery compiles three ordered Base/Overlay/Modal roots with no diagnostics; preserves
  logical geometry across scales `1.0` and `1.25` while changing physical snapping; proves nested
  Flex plus Absolute layout, percentages, clipping shared by layout/paint, text paint, structural
  ordering, one focus target, semantic names, and immutable prior-frame facts.
- The interaction lab combines Scene and ECS authoring, proves primary-before-secondary structural
  order independent of entity allocation, and isolates two surfaces at scales `1.0` and `1.25`.
  It proves pointer capture/release outside, formal focused/pressed/enabled semantics, exact old
  action behavior after an uncommitted candidate, accessibility activation after the new frame is
  committed, real gameplay pass-through, disabled and modal fail-closed rejection, three retained
  paused-cursor actions, reclamation after acknowledgement, resize invalidation, and retired-route
  rejection.
- The presentation gallery proves independent route/presentation invalidation (`1/1 -> 2/2 ->
  3/2`), an owned clipped solid/image/glyph packet in kernel structural order, exact image snapshot
  equality, nonblank glyph pages, and `Arc::ptr_eq` for both frame and accepted packet. A wrong-target
  presenter returns `ContentRejected` and creates neither receipt nor semantic authority; the exact
  packet returns `Presented`, creates a matching epoch/revision receipt, and admits exactly one typed
  accessibility action. Same-surface resize replaces the candidate, retirement rejects identity
  reuse, a fresh surface recovers, and the retired packet remains readable.

# Evidence

- `cargo run --locked -p nara_ui_kernel --example layout_gallery`: passed; printed
  `roots=3, layouts=10, paint=14, semantics=6, scales=[1.0, 1.25]`.
- `cargo run --locked -p nara_ui --example interaction_lab`: passed; printed the complete
  scene/ECS, two-surface, timeline, capture/focus/modal, exact-behavior, accessibility,
  fail-closed, and retained-cursor summary.
- `cargo run -p nara --locked --no-default-features --features presentation-ui,desktop-winit,render-wgpu --example ui_presentation_gallery`:
  passed headlessly and executed the exact packet/outcome/receipt/action round trip.
- One representative Windows `--windowed` run opened the gallery, displayed the clipped
  solid/image/text/Button content with correctly oriented glyphs, and exited cleanly. This is a
  bounded visual observation, not a pixel/hash artifact.
- `cargo nextest run --locked -p nara_text -p nara_ui_kernel -p nara_ui_widgets -p nara_ui -p nara_input -p nara_gameplay --test-threads=1`:
  143 passed, 0 failed.
- `cargo nextest run --locked -p nara_ui_render -p nara_render_wgpu --all-features --test-threads=1`:
  58 passed, 0 failed, including nonblank glyph/image GPU readback and the one-time UV conversion.
- `cargo nextest run --manifest-path ui-module-consumer/Cargo.toml --locked --test-threads=1`:
  3 passed, 0 failed.
- `cargo nextest run --locked --test ci_policy --test product_capabilities --test-threads=1`:
  11 passed, 0 failed.
- `cargo check --workspace --all-targets --all-features --locked -j 1`: passed.
- `cargo check --manifest-path reference-game/Cargo.toml --all-targets --all-features --locked -j 1`:
  passed as mechanical compatibility evidence only.
- `cargo clippy --locked --no-deps -p nara_input -p nara_ui_kernel -p nara_ui -p nara_ui_render --all-targets --all-features -j 1 -- -D warnings`:
  passed. The broader workspace retains pre-existing lint debt outside U15A.
- `cargo fmt --all -- --check`: passed.

# Follow-up

NUI-U15B is the sole successor. It may integrate the six bounded Reference Game capture states,
the separate paused lifecycle condition, representative physical targets, input sources, package
launch, and headless parity only through this frozen inventory and these semantics. It must not
repeat U15A behavior across a Cartesian matrix or add smoke/hash/pixel evidence for an already
structured assertion.

A product-only need remains in Reference Game data or adapters. Any shared behavior, public
interface, or conformance-assertion change stops the affected path, reopens U15A, and requires the
independent provenance specified by R45. NUI-U16 later performs non-behavioral public/evidence
reconciliation against this exact baseline.

# Citations

- `3c81645 feat(ui)!: freeze conformance boundaries`
- `crates/nara_ui_kernel/examples/layout_gallery.rs`
- `crates/nara_ui/examples/interaction_lab.rs`
- `examples/ui_presentation_gallery.rs`
- `crates/nara_ui/src/runtime.rs#install_ui_surface`
- `crates/nara_ui_render/src/packet.rs#UiPacketUv`
- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u15a-prove-and-freeze-ui-foundation-interfaces`
