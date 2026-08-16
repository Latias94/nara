---
title: Editor Scene Composition and Scene View v1 - Plan
type: feat
date: 2026-08-16
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
depends_on: docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md
plan_id: editor-scene-view-2026-08
unit_namespace: ESV
---

# Editor Scene Composition and Scene View v1 - Plan

## Goal Capsule

| Field | Contract |
|---|---|
| Objective | Extend the completed Editor Authoring Loop with a bounded Scene Composition workflow and an embedded edit-world Scene View: create or modify a visible scene object, arrange it in the canonical hierarchy, see the same authored scene in an editor-owned render target, pick it from the presented frame, save/reopen it, and let existing project consumers use the changed source. |
| Product owner | The private `nara-editor` application remains the product Host. `nara_tooling` and `nara_scene` own document facts and actions; the private Editor platform owns windows, GPU, render targets, and toolkit integration. |
| Architecture authority | Accepted ADRs 0015, 0026, 0047, 0078, 0094, 0101, 0105, and 0107 remain authoritative. OQ-001 and OQ-022 are closed only by the focused execution tracer in ESV-U1; this plan does not authorize a general RenderGraph or a public Viewport framework. |
| Execution profile | Five dependency-ordered units, one focused render-execution tracer, owner-level tests, one non-GUI authoring trajectory, and one local native Scene View canary. Verification is deliberately not a platform or state Cartesian product. |
| Stop conditions | Stop the Scene View path if the tracer cannot share one device execution authority without allowing raw GPU handles to escape the scoped private platform callback or enter domain/adapter models, if picking cannot bind to an exact composite presented generation, or if a required schema/target decision would become a new general-purpose framework. Record the rejection and split a successor instead of weakening the contract. |
| Tail ownership | ESV-U5 updates the ADR implementation ledger and one engineering-memory verification record. The plan itself carries no progress state; Git and the evidence record remain the source of completion. |

---

## Product Contract

### Summary

The first Editor product can open and edit a startup Scene, but it cannot yet answer the question a level designer asks first: “Did the object I authored actually appear where I expected?” This plan adds the smallest visual authoring loop that answers that question without turning the Editor into a second runtime or prematurely designing Game View, docking, or a universal render graph.

The product slice is an edit-world Scene View inside the private Editor window. It renders one bounded scene target through the existing first-party render path, displays that target through the egui adapter, and converts pointer selection back into a typed, presented-generation-bound Editor action. The same source document remains the authority for persistence and for later check/run/export consumers.

Product Contract preservation: newly bootstrapped from the user-approved scope in this session; no prior Product Contract IDs are being rewritten. The completed Editor Authoring Loop remains immutable predecessor evidence.

### Problem Frame

The existing Editor workflow proves document mutation and persistence, while the existing renderer deliberately supports a narrow window-oriented packet topology. Adding a full Viewport subsystem now would either hide that limitation behind a speculative abstraction or leak WGPU/egui identity into the Editor Domain. A small, explicit Scene View is the next useful consumer: it creates concrete pressure for an offscreen target, camera/input mapping, and presented-generation picking, while keeping ownership and failure semantics measurable.

### Key Decisions

- KD1. **Build Scene Composition and edit-world Scene View before Game View.** (session-settled: user-approved — chosen over implementing both views now because Game View adds Play placement and runtime lifecycle.) The Scene View closes the authoring feedback loop; Game View remains a separate successor. Governs R1-R18.
- KD2. **Use one private Editor Host GPU authority.** (session-settled: user-approved — chosen over separate Editor/game devices because window, target, input, and recovery need one owner.) The Scene target and the Editor window target share one serialized backend-instance/device-epoch domain. `EditorProjectSession` owns project and preview-world facts, `NativeEditorHost` owns Winit/egui orchestration, and one `WgpuEditorExecution` owned by the native Host owns the instance, device, queue, targets, and fixed submission transaction. No second device or toolkit-owned queue is introduced. Raw WGPU values may be borrowed only inside the scoped first-party Editor integration callback and may not escape into engine, domain, or adapter models. Governs R8-R14.
- KD3. **Keep Scene View private and egui-first.** (session-settled: user-approved — chosen over forcing immediate Nara UI convergence before a complete panel workload exists.) egui renders the first product panel, but receives only a texture lease and immutable frame facts. A future Nara UI adapter must consume the same model/action seam rather than changing document or render ownership. Governs R11, R15, R17.
- KD4. **Reuse Scene patches and Editor actions.** Composition uses existing `ScenePatchOperation` and the completed revision/history/persistence owners. No generic binding DSL, component registry, or separate Scene Composition crate is created. Governs R1-R7, R16.
- KD5. **Treat target and pointer identity as presented facts.** A pick is valid only for the exact target lease, presentation generation, geometry revision, backend instance, and device epoch that produced the displayed frame. Stale input is rejected or cancelled; it is never applied to the newest scene by inference. Governs R9, R13, R14.

### Requirements

#### Scene composition authority

- R1. `nara_scene` remains the persistence and patch authority. Composition operations use its existing add/remove/restore entity, add/remove/replace component, field, and move operations; no parallel mutable scene graph is introduced.
- R2. The Editor exposes a bounded, schema-backed composition surface: explicit Scene Tree commands create an Empty Entity or an untextured Colored Sprite as a root or child, remove an entity subtree with confirmation, and move/reparent it through a cycle-safe parent/order chooser; Inspector commands add or remove the admitted `Transform2d` and untextured `Sprite` components through typed first-party creation drafts. Colored Sprite creation publishes the entity and its complete components as one atomic patch/action/undo record. The Editor Domain allocates the stable `SceneEntityId`; adapters do not derive identity from labels or toolkit IDs. Creation selects the new entity, reparent preserves it, and removing the selected subtree moves selection to its surviving parent or clears it. Drag-and-drop composition and a generic component browser are not required in v1. The surface does not promise arbitrary reflection, a plugin registry, or a universal component authoring language.
- R3. Every composition mutation carries a non-optional document identity and exact base `SceneAuthoringRevision`. Missing, stale, reopened, or mismatched targets reject before scene, selection, history, or preview state changes.
- R4. The workspace document slot remains the single selection authority. Inspector, Scene Tree, and Scene View consume that selection; none stores a second durable selected entity.
- R5. Scene Tree rows are produced from the canonical ordered roots/children projection with depth and parent facts. Adapters never reconstruct or independently sort the hierarchy.
- R6. Existing bounded, clean-aware history and persistence contracts remain in force. A composition gesture produces one ordered Editor action and one undo record; failed or oversized patches are atomic and do not publish a partial scene.
- R7. The authoring trajectory creates a schema-valid untextured colored `Sprite` with `Transform2d` through those Editor controls. Its complete values are serialized as ordinary project content and consumed by the same loader as the product, not a test-private presenter or hard-coded runtime reconstruction.

#### Scene View execution and presentation

- R8. Scene View renders an edit-world snapshot derived from the current document. It never mutates or owns the Play world, gameplay state, persistence receipt, or durable document data.
- R9. The private Host owns a target lease containing exact `EditorDocumentId`/`SceneAuthoringRevision`, target identity, logical/physical extent, content transform, target generation, presentation generation, geometry revision, non-reused backend instance identity, and device epoch. Resize, target replacement, document replacement, backend replacement, and device loss retire old leases before a new one becomes usable; matching an epoch without its backend instance is invalid.
- R10. One serialized `WgpuEditorExecution` authority owns the Editor window surface, Scene image texture, instance/device/queue, resource caches, and the fixed Scene-write/egui-sample/window-present transaction. `NativeEditorHost` owns that execution object and the toolkit renderer state bound to its backend instance/epoch; `EditorProjectSession` owns no GPU state. Scene View and egui submission cannot silently create independent device/queue lifetimes.
- R11. The first target extension supports exactly one editor-owned offscreen Scene target followed by the existing Editor window target in a fixed Host-owned order. `RenderFramePacket` becomes a one-target tagged packet with either the existing `RenderWindowPacket` or one `RenderImagePacket`; the image form carries the exact `Handle<RenderImage2d>` and non-zero physical extent. The first-party WGPU implementation fixes Editor Scene images to `Rgba8Unorm` with `RENDER_ATTACHMENT | TEXTURE_BINDING`, allocates and retires them inside the backend instance, and returns a submitted-image result without pretending that an offscreen texture was presented. The implementation does not turn the pair into a general multi-target packet, RenderGraph, dependency scheduler, or public render-target registry.
- R12. Scene View has one independent transient editor camera with an explicit image target/viewport and no preview-world window surface. Authored window-target cameras remain document data but are not selected as the Scene View camera. Editor camera state is not persisted into the Scene document and is not shared with a future Game View camera by accidental aliasing.
- R13. Pointer picking uses bounded CPU hit facts emitted with the exact presented 2D Scene frame: stable entity identity, projected visible quad, and presented draw order from the real sprite queue. Primary-click selects the topmost containing quad, primary-click on empty content clears selection, and input outside the content rect does nothing. A stale frame, retired target, changed geometry revision, replaced backend instance, or retired device epoch yields a typed stale/rejected result and cannot select a different entity; GPU ID-buffer readback and general picking shapes are deferred.
- R14. Target unavailable, zero extent, resize, submission deferral, and device failure produce bounded UI states (`unavailable`, `retrying`, or `recovering`) without presenting stale pixels as current or accepting stale picks. Device failure retires the current composite lease and replaces the terminal `WgpuEditorExecution`; one automatic replacement attempt is allowed, after which the panel remains unavailable until an explicit Retry action creates another fresh backend instance. The native window, document, workspace, and unsaved authoring state survive this replacement.

#### Product and adapter boundary

- R15. The fixed egui shell places Scene Tree on the left, Scene View as the central primary work area, Inspector on the right, and Diagnostics below. The Scene View adapter fits the current presented lease or current-frame candidate token into an aspect-preserving content rect, displays immutable `EditorFrameModel` facts, emits typed navigation/selection/composition actions, and owns only transient widget state such as camera drag and texture IDs. Input always resolves against the prior composite presented lease; a current-frame candidate token is display-only until the surface present succeeds. WGPU, Winit, egui IDs, and backend receipts do not enter durable Editor Domain state.
- R16. One serial trajectory proves: open project → compose a visible entity/component and hierarchy placement → observe it in Scene View → pick it → undo/redo → save → close/reopen → check/run/export consumers read the same source.
- R17. Game View is explicitly deferred. This plan records only the compatibility boundary: a future Game View must use a runtime-owned target/session and may reuse target/receipt primitives, but must not reuse Scene View's edit camera, selection authority, or document mutation path.
- R18. Verification remains proportional: owner-level contract tests, one end-to-end domain trajectory, one real native Scene View canary, and focused render recovery tests. No opaque hash catalog, full OS/GPU matrix, or script-side Rust source interpretation is added.

### Actors

- **Editor author:** edits the Scene and expects visual feedback and reliable selection.
- **Editor Domain:** owns document identity/revision, canonical hierarchy, selection, actions, history, and persistence.
- **`EditorProjectSession`:** owns project content, schema admission, document/workspace lifecycle, the dedicated preview `World`, and renderer-neutral preview candidates; it owns no native or GPU object.
- **`NativeEditorHost`:** owns the native window/event loop, egui context/input/renderer state, frame ordering, panel extent, and the current `WgpuEditorExecution`; it is the only caller that joins domain candidates with platform presentation.
- **`WgpuEditorExecution`:** owns the WGPU backend instance, device/queue, Editor surface, offscreen Scene textures, caches, and fixed two-target execution transaction; it publishes unforgeable submission/presentation facts but no domain action.
- **Scene View adapter:** translates transient pointer/camera gestures to typed actions and presents the Host-owned target.
- **Existing project consumers:** check, headless/desktop product runs, and source-carrying export consume the saved project source; they do not call Scene View internals.

### Key Flows

#### F1. Compose and inspect

1. Open a complete-schema project and startup Scene.
2. Create or restore a schema-valid visible entity, add its admitted component, and place it under a selected canonical parent.
3. `EditorProjectSession` materializes the edit-world preview; `NativeEditorHost` publishes a Scene target lease only after the composite GPU transaction succeeds.
4. The Scene View displays the presented target and the Scene Tree/Inspector show the same document revision.

#### F2. Pick and recover

1. The author clicks a visible entity in the displayed Scene frame.
2. The Host resolves the click against that exact presentation/target generation and emits a typed selection action.
3. Resize, target invalidation, deferred submission, or device loss retires the old lease and prevents stale selection.
4. Target-only failure recreates the target inside the same backend domain; device failure replaces the execution owner with a new backend instance before a new accepted presentation restores the Scene View without changing the document.

#### F3. Persist and consume

1. Undo and Redo move the document through the existing bounded history.
2. Save advances the saved checkpoint only after the existing persistence receipt is verified.
3. Close/reopen reloads the same source and restores the authored composition.
4. Existing check, generated-product run, and source-carrying export consume that source; Scene View is not a hidden alternate loader.

### Acceptance Examples

- AE1. A stale composition action from before close/reopen is rejected, and the reopened document, selection, preview, and history remain unchanged.
- AE2. The Scene Tree and Scene View identify the same entity after a reparent/reorder operation; adapter-side sorting or duplicate selection state is not needed.
- AE3. A click from Scene presentation generation `G` and backend instance `B` cannot select an entity using generation `G+1` or replacement instance `B+1` after resize, scene edit, or device replacement; it returns a typed stale result.
- AE4. A target failure retires and replaces only the Scene texture. A device failure retires the composite lease, texture registration, toolkit renderer, target/cache state, and terminal backend in that order; one fresh execution owner may restore the view with a different backend instance without displaying old pixels as current.
- AE5. The authored visible entity survives Undo → Redo → Save → Close → Reopen and is consumed by the existing project check/run/export path.
- AE6. A Game View is not present in this product slice, and no Play world or runtime camera is created merely to satisfy Scene View.
- AE7. Editing one deterministic `Transform2d` position through the Inspector moves the entity to the corresponding presented Scene logical coordinate; the object is pickable at that coordinate and remains there after Save/Reopen.

### Scope Boundaries

#### In scope

- Scene composition actions backed by existing Scene patch operations.
- Typed first-party creation drafts for `Transform2d` and an untextured colored `Sprite`; a generic component browser remains deferred.
- Canonical Scene Tree projection and single workspace selection authority.
- One edit-world Scene View in the private `nara-editor` application.
- One editor-owned offscreen target sharing the Host's serialized GPU authority with the Editor window.
- Independent transient editor camera, resize/content transform, exact-generation picking, bounded failure/recovery states.
- egui adapter integration and one real authoring trajectory.

#### Deferred to successors

- Game View and Play-world placement/transport.
- Transform gizmos, drag handles, snapping, multi-selection, and advanced picking shapes.
- Docking, detached windows, multiple native surfaces, and multi-viewport layout persistence.
- Asset Browser, generic structured-data assets, importer registry, and unknown-schema/lossless editing policy.
- Nara UI Inspector/Scene View convergence, custom widget catalog, IME, and full OS accessibility parity.
- General RenderGraph, arbitrary render-target graphs or dependency schedulers, and multi-GPU orchestration. This plan admits only the fixed `Scene offscreen target -> Editor window/egui sampling` order edge required by R11.

#### Explicitly rejected for this plan

- A public `EditorViewport`/`SceneView` trait or new shared viewport crate.
- A second async runtime, background render worker, or unbounded channel between Editor and renderer.
- Copying Godot's Node/Object ownership, Bevy's App/Render sub-app topology, or OpenGPUI's docking graph into Nara's foundation.

### Success Criteria

- The editor author can see and pick a real authored Scene object in one launchable native Editor window.
- The same object identity, revision, hierarchy placement, authored transform, presented logical position, and selection are coherent across Scene Tree, Inspector, Scene View, save/reopen, and downstream source consumers.
- Resize and device/target failure are observable and recoverable without stale input or stale pixels.
- The new target support remains private/minimal and leaves existing runtime and headless consumers unchanged.

### Outstanding Questions

- **No launch-blocking question remains.** This plan selects one Host-owned device/queue domain, one existing renderer-neutral image target, and one fixed `Scene target -> Editor window` submission edge. ESV-U1 is the evidence/admission gate that records this decision in ADR 0111 and closes OQ-001/OQ-022; falsifying evidence triggers the Goal Capsule stop condition rather than an alternative guessed during implementation.
- **Deferred, non-blocking:** OQ-010 (Nara UI Editor dogfooding) remains gated on a fuller text/IME/accessibility workload.
- **Deferred, non-blocking:** OQ-039 (Play placement/local transport) remains a Game View/Play successor concern.

### Sources and Research

- `docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md` — completed Editor Domain, frame model, ordered actions, private egui application, and explicit Viewport deferral.
- `docs/architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md` — one serialized GPU authority and device-epoch recovery.
- `docs/architecture/adr/0094-minimal-render-execution-boundary-and-evidence-gated-extensions.md` — closed first-party render boundary and OQ-001 trigger.
- `docs/architecture/adr/0105-closed-first-party-render-plan.md` — immutable packet/target ownership and no premature RenderGraph.
- `docs/architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md` — Editor Domain, Host, toolkit adapter, and future Nara UI separation.
- `docs/architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md` — canonical root/child ordering and Scene placement authority.
- `crates/nara_render/src/lib.rs` — current single-view/window packet topology and explicit image-target rejection.
- `crates/nara_render_wgpu/src/backend.rs` — existing device/queue/surface/cache ownership and device epoch.
- `crates/nara_scene/src/patch.rs` and `crates/nara_tooling/src/inspector.rs` — existing composition patch operations and Inspector command boundary.
- `repo-ref/godot` — reference for Scene Tree/Inspector/Viewport workflow completeness, not ownership topology.
- `repo-ref/bevy` — reference for ECS extraction and render separation, not App/Render lifecycle reuse.
- `repo-ref/open-gpui` — reference for future panel/viewport pressure, not a docking or lifecycle dependency.
- `repo-ref/egui` — reference for low-level WGPU renderer texture registration; the private Editor Host remains the authority.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Close the render question with a tracer before broadening the renderer.** ESV-U1 exercises one Editor window target plus one offscreen Scene target on one device/queue authority. The result becomes the focused Scene View ADR and either admits the bounded topology or records a clean stop condition.
- KTD2. **Keep the target extension narrow.** Refactor `RenderFramePacket` into a single-target tagged packet: `Window(RenderWindowPacket)` preserves the existing path, while `Image(RenderImagePacket)` carries one existing `RenderTarget::Image(Handle<RenderImage2d>)` plus its admitted physical extent. The WGPU Editor path derives a fixed `Rgba8Unorm` `RENDER_ATTACHMENT | TEXTURE_BINDING` descriptor, allocates the texture inside the backend instance, and returns a submitted-image receipt rather than a surface-present receipt. Submit the Scene target before the Editor window in one fixed private transaction; do not create a graph scheduler, registry, general texture descriptor, or multi-target public packet.
- KTD3. **Use a composite presented lease as the deep module.** `NativeEditorHost` may render a candidate texture in the current egui frame, but it promotes pickable facts only after one unforgeable composite receipt proves all three events: the exact Scene image was submitted, that image generation was sampled by the exact egui paint frame, and the Editor window surface was submitted and presented. The resulting lease binds document/revision, image target/generation, geometry revision, presentation generation, backend instance/device epoch, egui texture-registration generation, and window surface generation. The app does not place this lease or its transient texture token inside `nara_tooling`.
- KTD4. **Materialize preview from a document snapshot, not from Play.** `EditorProjectSession` owns one dedicated edit-preview `World` because it already owns the accepted document and schema/asset authorities. It reuses `SceneAuthoringSession::sync_world*` after a document action is accepted, installs one transient editor camera with an explicit image target/viewport, and exposes only an opaque renderer-neutral preview candidate to the private application. No preview-world window is installed, so authored window-target cameras remain inert for this extraction without being removed from the document. The preview is replaceable on every revision; Play remains a separate runtime lifecycle and is not a hidden dependency of Scene View.
- KTD5. **Replace the high-level Painter with a low-level egui WGPU renderer.** The current `egui_wgpu::winit::Painter` owns and hides a separate device and surface outcome, so it cannot satisfy R10 or KTD3. `NativeEditorHost` retains `egui_winit` input/window integration but owns a low-level `egui_wgpu::Renderer` bound to the current backend instance/device epoch. A scoped `nara_render_wgpu::__private` Editor callback may lend borrowed device/queue/texture-view facts for that frame; those borrows cannot be stored or re-exported, and the integration is not added to any gameplay prelude or public renderer-neutral API.
- KTD6. **Bind CPU picking to presented facts from the real queue order.** `nara_sprite_render` preserves the runtime entity on each queued sprite item and emits a parallel ordered hit-candidate list before batching discards per-entity identity. `EditorProjectSession` maps that runtime entity through `SceneEntitySource` to the stable `SceneEntityId`; only the composite receipt binds candidate quads/order to the presented target lease. Pointer coordinates are inverse-mapped through the target/content transform and interpreted only while target, presentation, geometry, backend instance, and device epoch match. The adapter never reconstructs picks from the current document or introspects GPU packets. Reconciliation retires capture and pending picks on mismatch; no GPU readback path is introduced in v1.
- KTD7. **Extend existing action and history owners instead of adding a composition framework.** Scene Composition uses typed commands and `ScenePatchOperation`; schema admission and component construction remain with existing Scene/Project owners.
- KTD8. **No new async architecture.** Existing bounded platform initialization may remain asynchronous where the library requires it. Per-frame preview extraction, target publication, and action dispatch stay serialized and bounded; no executor, worker queue, or cross-thread mutable domain state is introduced.
- KTD9. **Recover by replacing the execution owner, not reviving a terminal backend.** The existing `WgpuRenderBackend` treats device failure as terminal. `NativeEditorHost` therefore retires the composite lease, unregisters the Scene texture, drops the epoch-bound egui renderer, drops the failed execution/caches, and constructs a fresh `WgpuEditorExecution` around the surviving native window. One automatic replacement is attempted on the next redraw; further attempts require the explicit Retry command. A replacement always has a new non-reused backend instance identity, even if its device epoch starts from the same numeric value.

### Ownership Map

| Concern | Owner | Forbidden leakage |
|---|---|---|
| Scene document, patch, revision, history, saved checkpoint | `nara_scene` + `nara_tooling` | WGPU, Winit, egui IDs, texture handles |
| Canonical tree and selection | `nara_tooling` workspace/model | Adapter-side hierarchy reconstruction or second selection owner |
| Preview `World`, transient camera, stable entity mapping, renderer-neutral candidate | `EditorProjectSession` private preview module | Play-world mutation, persistence side effects, native/GPU handles |
| Native window/event loop, egui input/context/renderer, panel extent, transaction ordering | `NativeEditorHost` in the private `nara-editor` package | Owning a second device/queue or reconstructing document facts |
| Backend instance, device/queue, surface, offscreen texture/cache, fixed GPU transaction | `WgpuEditorExecution` inside `nara_render_wgpu`, owned by `NativeEditorHost` | Toolkit/domain ownership, general target registry, hidden second Runner |
| Single-target packet and submitted/presented receipts | `nara_render` plus the WGPU execution owner | Image submission described as surface presentation, forged receipt, unbound pointer action |
| Composite lease and accepted hit facts | `NativeEditorHost` | Publication before Scene submit + egui sample + window present all succeed |
| Scene View texture ID, pan/zoom, drag state | private app/egui adapter | Durable document state or stored borrowed backend handles |
| Future Game View | successor runtime product | Reusing edit camera/selection/document mutation implicitly |

### High-Level Technical Design

```text
Scene document + revision
          |
          v
EditorProjectSession
  workspace + preview World + transient camera
          |
          v
renderer-neutral Scene packet + ordered hit candidates
          |
          v
NativeEditorHost -> WgpuEditorExecution
          |
          +-- submit exact Scene image
          +-- egui samples that image generation
          +-- submit + present Editor window
          |
          v
composite receipt -> presented lease + accepted hit facts
          |
          v
Scene View adapter + pointer transform
          |
          v
exact-generation selection action
          |
          v
EditorWorkspace

Scene Tree / Inspector adapter
                    |
                    v
 document/base-revision-bound composition action
                    |
                    v
             EditorWorkspace
```

The preview path is replaceable and revision-bound. A new document revision does not mutate an old target in place; it produces a new attempt or a bounded unavailable state. Pointer input arriving before redraw is interpreted only against the prior composite presented lease, which is what the author actually saw. The current redraw may use an ephemeral candidate texture token, but it cannot publish that candidate as pickable until the Editor window present succeeds. The Editor frame model joins document facts and presentation status once per frame, while the adapter remains a consumer.

### Assumptions and Constraints

- A schema-valid visible 2D fixture can be created or supplied without adding a new importer or binding language.
- The first offscreen target uses fixed `Rgba8Unorm` sampled/render-attachment storage. General image formats, mip chains, readback, storage binding, and externally supplied texture views remain outside this plan.
- Existing runtime/headless packet consumers must compile and behave unchanged when Scene View support is not installed.
- The current editor remains single-window and single-document for this plan; no attempt is made to solve future multi-document or detached-window authority.
- One native canary is enough to prove the product seam; performance baselines and platform matrices belong to later evidence work.

### Sequencing

1. ESV-U1 closes the highest-risk one-device/offscreen-target execution question and records the durable boundary.
2. ESV-U2 independently extends Scene/Tooling composition and selection contracts without rendering dependencies; it is ordered after U1 for risk control, not because it consumes the render contract.
3. ESV-U3 implements private preview target materialization, lease retirement, and recovery.
4. ESV-U4 presents the target through egui and binds input/picking to exact presented facts.
5. ESV-U5 runs one real authoring trajectory and updates evidence/ledger; Game View remains deferred.

### System-Wide Impact

- `nara_scene` and `nara_tooling` gain a small set of revision-bound composition actions and canonical tree facts; their public surface should remain focused on existing document/action concepts.
- `nara_render` gains the single-target window/image packet sum; `nara_render_wgpu` gains a private Editor execution transaction, fixed Scene texture storage, submitted-image facts, and composite presentation facts. Existing window-only consumers remain a valid subset.
- The private `nara-editor` package enables `presentation-2d` and `render-wgpu`, replaces the convenience Painter with a low-level egui WGPU renderer, and consumes the doc-hidden scoped first-party integration seam. No public crate depends on `editor`, and the seam is not re-exported from a gameplay prelude.
- `nara_sprite_render` retains per-item runtime identity long enough to emit ordered hit candidates; batching and draw submission remain unchanged for consumers that do not request these facts.
- Runtime UI, Reference Game gameplay, and Winit runtime ownership are not changed by this plan.
- The architecture ledger gains one focused ADR and one completion record; no roadmap or current-state document becomes a second implementation ledger.

### Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Existing renderer cannot accept an image/offscreen target without a broad rewrite | U1 tracer is a hard gate; implement the smallest target extension or stop and record a successor, never add a generic graph as a workaround. |
| egui Painter creates a second device/queue and hides window-present outcome | Remove Painter in U1; bind the low-level renderer to the execution owner's backend instance/epoch and require the composite receipt before publishing a lease. |
| Preview world drifts from the document or accidentally shares Play state | Rebuild from an immutable document snapshot and explicitly keep Play out of the Scene View path. |
| Pointer events select a newer frame after resize/edit or backend replacement | Carry target/presentation/geometry/backend-instance/device-epoch identities through the pick request and reject the entire mismatch tuple. |
| Device recovery is assumed but the current backend is terminal | Replace the whole private execution owner with one automatic attempt, then require explicit Retry; never claim in-place backend recovery. |
| Composition work becomes an arbitrary component editor | Limit v1 to admitted schema-backed operations and one visible fixture; defer generic data assets and unknown schemas. |
| Tests grow into expensive matrices | Prefer owner invariants plus one serial trajectory and one native canary; use synthetic failure injection for recovery. |

---

## Implementation Units

### U1. Scene View execution tracer and ownership contract

**Goal:** Close OQ-001/OQ-022 for the first Scene View consumer and record the durable one-device, one-offscreen-target, exact-presentation boundary before renderer or Editor code expands.

**Requirements:** R8-R11, R13-R14; KD1-KD3, KTD1-KTD3, KTD5, KTD8-KTD9.

**Dependencies:** Completed EFI-U1 through EFI-U5; ADRs 0078, 0094, 0105, and 0107.

**Files:**

- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking.md`
- `docs/architecture/open-questions.md`
- `editor/Cargo.toml`
- `editor/src/platform.rs` and its private test module for the one-device Editor tracer
- `crates/nara_render/src/lib.rs` or a private `packet.rs` split owned by that crate
- `crates/nara_render_wgpu/src/{lib.rs,backend.rs,frame.rs,presented.rs}` plus private `editor_execution.rs`/`offscreen.rs` splits if the admitted transaction warrants them
- Do not create a public viewport crate; any additional file must be a private split of one of these owners and justified by the admitted tracer.

**Approach:**

1. Freeze the three-owner call direction: `EditorProjectSession` produces a renderer-neutral preview candidate; `NativeEditorHost` orders input, model, egui, and presentation; its `WgpuEditorExecution` owns all GPU resources and returns unforgeable terminal facts. No reverse dependency or GPU access enters the project session.
2. Replace `RenderFramePacket`'s mandatory window field with the KTD2 one-target tagged packet. Preserve the exact existing window behavior and admit only the fixed Editor image descriptor; image submission has its own result and never increments surface-present facts.
3. Enable `presentation-2d` and `render-wgpu` for `nara-editor`, replace Painter with the low-level renderer, and exercise the fixed transaction. The execution owner first selects/initializes a surface-compatible device from the existing Editor window authority without presenting it, then encodes/submits the Scene image, registers/samples that exact image generation in egui, and finally submits/presents the Editor window on the same backend instance/device epoch.
4. Construct the Host-private composite receipt only after all three steps succeed. Any image, registration, surface-acquire, submit, or present failure leaves the candidate non-pickable and does not advance presentation generation.
5. Exercise target resize/retirement and terminal device failure. Target replacement stays inside the execution owner; device failure follows KTD9 whole-owner replacement and proves the old instance/epoch lease cannot accept a pick.
6. Write ADR 0111 as a focused refinement of existing render/Editor ADRs. Update OQ-001/OQ-022 only with the observed decision and evidence; do not rewrite unrelated open questions.

**Patterns to follow:** `WgpuRenderBackend` non-reused instance identity plus device epoch; immutable `RenderFramePacket`; existing surface-present receipt construction; completed Editor Host frame driving. The current backend's `Unavailable` state is terminal, so follow KTD9 rather than citing nonexistent in-place recovery. Use `repo-ref/egui` only for low-level texture registration guidance.

**Test scenarios:**

- Existing window-only packets retain their behavior, while one image packet submits without acquiring or presenting a surface.
- One-device tracer submits one Scene target followed by one egui window frame without a second device/queue identity or a combined public multi-target packet.
- The composite receipt is absent when the Scene image was not submitted, when egui did not sample the exact registered image generation, or when the Editor surface did not present; only the all-success path advances the presented lease once.
- Retiring the target invalidates its texture/pick lease and a replacement target gets a new generation.
- Replacing a failed execution owner yields a new backend instance identity, rejects the old lease even if epoch numbers coincide, and accepts a later all-success transaction.

**Verification:** Focused tracer tests and ADR/OQ consistency checks pass. If the one-device topology cannot be proven, the unit records a bounded rejection, marks this plan blocked, and does not proceed to later Scene View units; rejection is not completion evidence.

### U2. Revision-bound Scene Composition and canonical tree actions

**Goal:** Add the smallest useful Scene Composition surface to the existing Scene/Tooling owners: create/restore a visible entity, admit or remove a component, and move/reparent it with exact document/revision identity and bounded history.

**Requirements:** R1-R7; KD4; KTD4, KTD7.

**Dependencies:** Completed EFI exact document/base-revision and clean-aware history work. This unit is technically independent of ESV-U1.

**Files:**

- `crates/nara_scene/src/patch.rs` and related authoring/history owner files only where existing patch cost/atomicity needs a focused extension.
- `crates/nara_tooling/src/workspace.rs`
- `crates/nara_tooling/src/inspector.rs` and canonical tree model files
- `crates/nara_tooling_egui/src/*` only for typed action plumbing required by the later adapter unit
- Focused Scene/Tooling tests and one schema-valid visible-content fixture under existing test support.

**Approach:**

1. Reuse `ScenePatchOperation` and existing revision-bound dispatch; do not add a second mutable hierarchy or a generic component registry.
2. Represent composition as ordered UI-neutral actions. Navigation/selection actions remain revision-free; composition actions carry document and base revision.
3. Make the workspace slot the only selection authority and project canonical roots/children into Scene Tree rows with depth and parent facts.
4. Ensure patch validation, history admission, and selection updates are atomic on failure. A rejected component or placement does not publish a new accepted document snapshot.
5. Keep v1 component construction schema-backed and bounded to the fixture's admitted types; leave arbitrary reflection and unknown-schema policy out.

**Patterns to follow:** Existing `ScenePatchDocument`, `SceneInspectorCommand`, `EditorWorkspaceCommand`, `SceneAuthoringSession`, and ADR 0101 canonical placement. Avoid stringly component names outside the existing schema/diagnostic boundary.

**Test scenarios:**

- Add/restore, component admission, and reparent/reorder each publish one revision-bound action and one undo record.
- Wrong document or stale revision rejects with no document, selection, history, or accepted-snapshot mutation.
- Canonical roots/children order remains stable after composition and reopen.
- Undo/Redo and save-checkpoint cleanliness remain correct for a structural composition edit.
- An over-limit or schema-invalid composition is rejected atomically.

**Verification:** Focused `nara_scene` and `nara_tooling` tests cover these owner contracts; no egui or WGPU fixture is used to prove domain correctness.

### U3. Private edit-preview target and Host transaction

**Goal:** Materialize the accepted Scene document into a private edit-world preview and publish it through the U1 target lease, with resize, unavailable, deferred, and whole-execution-owner replacement handling.

**Requirements:** R8-R11, R13-R14; KD2; KTD2-KTD4, KTD6, KTD8-KTD9.

**Dependencies:** ESV-U1 target contract; ESV-U2 composition actions and canonical snapshot.

**Files:**

- `src/project_host/runtime/editor.rs` plus a private `src/project_host/runtime/editor/preview.rs` owner split for the preview `World`, transient camera, and renderer-neutral candidate.
- `editor/src/app.rs` and `editor/src/platform.rs` for native orchestration, candidate handoff, composite receipt publication, and execution-owner replacement.
- Minimal admitted changes in `crates/nara_render/src/{lib.rs,packet.rs}` and `crates/nara_render_wgpu/src/{backend.rs,frame.rs,presented.rs,editor_execution.rs,offscreen.rs}` as actually created/admitted by U1.
- `crates/nara_sprite_render/src/{types.rs,queue.rs}` and focused tests for preserving per-item runtime identity and emitting ordered hit candidates before batching.
- Focused preview/target transaction tests.

**Approach:**

1. Add `EditorPreviewSession` as a private field of `EditorProjectSession`. Reuse `SceneAuthoringSession::sync_world*` to materialize its immutable accepted Scene snapshot into that dedicated edit-preview `World`; never borrow or mutate the Play world. Install one transient editor camera after synchronization and keep window surfaces out of the preview world so extraction contains exactly one image-target view.
2. Preserve runtime entity identity through sprite queue ordering, then map each queued item through the preview World's `SceneEntitySource` to one stable `SceneEntityId`. Publish renderer-neutral candidate hit facts with the same projected quad and sorted draw ordinal used by the submitted sprite work; do not copy or reimplement sorting in the Editor adapter.
3. Return one opaque `EditorPreviewCandidate` from the project session to `EditorApplication`: exact document/revision, one image packet, geometry revision, and candidate hit facts. The cross-package seam is first-party/doc-hidden and contains no WGPU, Winit, or egui value.
4. Let `NativeEditorHost` pass the candidate to its U1 `WgpuEditorExecution` and use the current candidate texture only inside that redraw. Promote it to the presented Scene lease/hit facts only from the all-success composite receipt; no “latest candidate” join is permitted.
5. Keep the target resource and texture-registration lifetime behind the opaque private lease. On resize or replacement, retire the old lease and registration before exposing the new one. Target generation, geometry revision, backend instance, and device epoch all participate in identity.
6. On deferred submission, preserve only bounded retry facts and show unavailable/retrying. On device loss, perform KTD9's ordered whole-owner replacement; do not try to revive terminal `WgpuRenderBackend` state or reuse the old texture as the new document revision.
7. Keep fixed target ordering inside the execution owner. Do not expose a RenderGraph or public target registry.

**Patterns to follow:** `EditorProjectSession` current-frame/dispatch/drive order; `SceneAuthoringSession::sync_world*`; `SceneEntitySource`; `WgpuRenderBackend` instance/epoch identity; existing `UiPresentationBridge` receipt discipline; private Editor platform ownership from EFI-U4.

**Test scenarios:**

- A document revision produces a matching preview candidate, but no accepted target lease exists until the composite Scene-submit/egui-sample/window-present receipt succeeds.
- A rejected/stale composition action leaves the currently presented preview bound to its prior accepted document snapshot.
- Resize retires the old target, rejects a pick against it, and accepts the replacement only after presentation.
- Queue order and hit order remain identical for overlapping sprites, and runtime entities map to stable `SceneEntityId` values through `SceneEntitySource` before publication.
- Deferred/zero-extent targets produce bounded non-presented outcomes. Device loss retires the old composite state, performs at most one automatic whole-owner replacement, and requires explicit Retry after a failed replacement.
- Runtime/headless paths remain valid when the private preview target is not installed.

**Verification:** Owner-level render/Host tests plus one synthetic recovery sequence. Do not add a matrix of windows, GPUs, or target sizes.

### U4. egui Scene View, camera, and presented-generation picking

**Goal:** Make the private Editor window show the Host-owned Scene target and turn pointer input into exact-generation-bound selection actions.

**Requirements:** R2-R5, R9, R12-R15; KD3-KD5; KTD3, KTD5-KTD7.

**Dependencies:** ESV-U2 canonical tree/actions; ESV-U3 target lease and preview facts.

**Files:**

- `editor/src/app.rs`
- `editor/src/platform.rs` and any private platform submodule needed to share the WGPU device with egui
- `crates/nara_tooling_egui/src/editor.rs`, `inspector.rs`, and focused tests
- `crates/nara_tooling/src/*` only for immutable Scene View frame facts/action types that belong to the domain seam

**Approach:**

1. Add a fixed Scene View panel to the existing shell. Before each redraw, the private app reads one immutable post-drive `EditorFrameModel` (Scene Tree, Inspector, Diagnostics, selection) and either the matching reusable presented texture or a same-revision candidate token. A mismatch renders unavailable and cannot be joined by “latest” inference. Only the composite receipt promotes the candidate to the next input-authoritative presented lease.
2. Add the bounded composition controls promised by R2: Scene Tree commands for Add Root, Add Child, Remove, Move/Reparent, and Inspector commands for Add/Remove admitted Component. These controls must emit the same exact document/revision-bound ordered actions as owner tests; the native trajectory must not call the domain Interface directly as a substitute for author interaction.
   Removal confirms the affected subtree and restores selection to the surviving parent or clears it; the parent chooser excludes the entity and its descendants and preserves canonical sibling order.
3. Report the Scene View panel's desired physical extent to the private Host after layout. The Host applies resize on the next drive; until a matching lease is presented, the panel shows a bounded `resizing` state and disables picking instead of clamping input into an old texture.
4. Store camera pan/zoom and egui texture registration as transient app/adapter state. Middle-button drag pans, wheel input zooms around the pointer, and primary-click is reserved for selection; camera gestures do not dirty the document. Persist none of these facts into Scene documents or workspace authoring state.
5. Use the low-level egui WGPU renderer selected by KTD5. Recreate its epoch-bound resources and texture registration whenever the execution owner changes; never retain borrowed device/queue/texture-view values beyond the scoped frame callback.
6. Treat the previous composite lease as input authority for events arriving before redraw. Convert pointer coordinates through that lease's target/content transform and attach target generation, presentation generation, geometry revision, backend instance identity, and device epoch to the pick request.
7. Accept selection only when the request matches the entire presented Scene tuple; otherwise show a bounded stale/unavailable diagnostic and leave selection unchanged. Candidate texture use during the current redraw cannot retroactively validate input against an unpresented frame.

**Patterns to follow:** Existing `EditorFrameModel` and ordered action ingress; `egui_wgpu` texture registration; renderer-neutral UI receipt/presentation identity; no toolkit IDs in durable state.

**Test scenarios:**

- Scene View displays the accepted target and the Scene Tree/Inspector selection remains the same authority.
- Scene Tree/Inspector composition controls emit exact document/revision-bound actions for create, component admission/removal, and hierarchy placement; stale controls cannot mutate a reopened document.
- Camera movement changes only transient view state and does not dirty the document.
- A current-generation primary-click selects the topmost presented hit, an empty-content click clears selection, outside-content input does nothing, and a pick after resize/revision/backend-instance/device retirement is rejected.
- A Scene image submission without matching egui sampling or Editor-window present never becomes pickable; the next input cycle continues to use only the last valid composite lease or an explicit unavailable state.
- Target unavailable/recovering states do not display stale pixels or emit selection actions.

**Verification:** Focused adapter/model tests and one local native-window canary. Visual inspection verifies only the bounded flow and failure state, not pixel-perfect layout across platforms.

### U5. Authoring trajectory, downstream consumption, and handoff

**Goal:** Prove the full user-visible loop on one ordinary project source and close the documentation/evidence boundary without promoting Scene View into a new general foundation.

**Requirements:** R1-R7, R12-R18; all KDs and KTDs.

**Dependencies:** ESV-U1 through ESV-U4.

**Files:**

- `editor/tests/authoring_scene_view.rs` or the smallest existing integration-test owner selected during implementation
- `tests/scene_inspector.rs`, `tests/editor_persistence.rs`, and existing project workflow support only where the trajectory needs a focused assertion
- `docs/architecture/adr/implementation-status.md`
- `docs/knowledge/engineering/verification/` completion record
- `docs/architecture/open-questions.md`/ADR 0111 only for final evidence links, not duplicated status tables

**Approach:**

1. Use a generated or guarded project copy with ordinary schema-valid content so the trajectory edits source data instead of invoking a private presenter.
2. Run one serial path through real Editor controls: open → compose visible entity/component and placement → edit one deterministic `Transform2d` position → Scene View present/pick at the expected logical coordinate → Undo/Redo → Save → close/reopen → check/run/export source consumers.
3. Compare structured authored identity, hierarchy placement, transform position, presented logical bounds, selection, and saved source facts across the trajectory. Do not join unrelated telemetry by frame counters or opaque hashes.
4. Record which assertions are headless/domain, which are native Scene View, and which are downstream product/export evidence. Do not call this a Game View or full Editor Product-Validated baseline.
5. Remove abandoned tracer branches, temporary target adapters, and unused compatibility paths before declaring the plan complete.

**Patterns to follow:** Completed EFI-U5 trajectory/evidence format; existing source-carrying export and project check workflows; repository ledger ownership rules.

**Test scenarios:**

- The edited source round-trips through Scene View and the existing check/run/export consumers.
- A deterministic Inspector `Transform2d` edit appears at the matching presented logical coordinate, is pickable there, and remains consistent after Save/Reopen.
- Reopen reconstructs the same canonical tree and visible entity without relying on process-local selection or texture state.
- A negative target/pick/recovery case is retained as one focused regression, not a Cartesian matrix.
- The final ledger points to the new completion record and does not leave the predecessor plan or current-state index as a second active authority.

**Verification:** One serial integration trajectory, one native canary, affected focused nextest suites, and documentation consistency checks. No broad release matrix is required for this plan.

---

## Verification Contract

### Required gates

1. **Documentation and authority:** `git diff --check`; ADR 0111, OQ-001/OQ-022, implementation status, and the completion record agree on the same scope and predecessor.
2. **Domain contracts:** focused `cargo nextest` suites for `nara_scene`, `nara_tooling`, and `nara_tooling_egui` cover revision-bound composition, canonical tree, selection, history, and action ordering.
3. **Render/Host contracts:** focused tests cover the window/image packet sum, one-device target identity, composite receipt, lease retirement, resize, deferred/unavailable, and whole-owner backend replacement. They must not require a full OS/GPU matrix.
4. **Editor package:** `cargo check -p nara-editor` (or the repository's final package-specific check) proves the private application and its feature closure; no public crate depends on `editor`.
5. **Product trajectory:** one serial guarded-project trajectory proves compose → view → pick → undo/redo → save/reopen → check/run/export. The evidence names the exact source copy and distinguishes real native presentation from headless/component fixtures.
6. **Native canary:** one local Scene View launch confirms the target is nonblank, the camera is independent, the deterministic object appears and can be selected at its expected logical coordinate, and unavailable/whole-owner-replacement states do not accept stale input. The captured evidence names the composite receipt identity used for the pick. This is a bounded manual/native check, not a screenshot matrix.
7. **Formatting:** `cargo fmt --all -- --check` after inspecting the final diff. No Cargo commands are run concurrently in the implementation checkout.

### Evidence rules

- A candidate frame is not a presented frame. Pick and visual evidence must reference the accepted target/presentation lease.
- A domain fixture is not desktop evidence. The final record labels component, headless, native, and export assertions separately.
- Do not manufacture coverage through state×target Cartesian fixtures or opaque digests. Prefer exact structured identities and one failure example per failure class.
- Existing runtime/headless consumers must remain a valid requested subset when Scene View/private Editor capabilities are absent.

---

## Definition of Done

### Global

- [ ] ESV-U1 has closed OQ-001/OQ-022 with a measured one-device Scene View boundary. A bounded rejection blocks this plan and cannot satisfy this completion item.
- [ ] Scene Composition uses existing Scene patch/history/persistence authorities; no duplicate scene graph, selection owner, binding DSL, registry, public viewport crate, RenderGraph, or second async runtime was added.
- [ ] The three-owner boundary is preserved: `EditorProjectSession` owns preview/domain facts, `NativeEditorHost` owns native/egui orchestration and the execution object, and `WgpuEditorExecution` owns GPU resources and terminal facts. No raw GPU/Winit value enters domain or adapter models, and no borrowed frame handle escapes the scoped private integration callback.
- [ ] Scene View displays a real authored edit-world object at its expected logical position and exact-generation picking works there; stale/retired target input cannot mutate selection.
- [ ] Save/reopen and existing check/run/export consumers use the same source data proven by the trajectory.
- [ ] Game View is explicitly deferred and no Play-world placement or runtime camera code is smuggled into Scene View.
- [ ] Abandoned experiments, dead compatibility paths, and temporary test presenters are removed or clearly confined to their intended test owner.
- [ ] ADR implementation status and one verification record are updated; no conflicting active-plan/current-state claim remains.

### Per-unit

- **ESV-U1:** tracer/ADR/OQ evidence proves the target and device ownership boundary or records a clean stop.
- **ESV-U2:** composition, canonical hierarchy, exact revision, selection, and bounded history tests pass without UI/WGPU fixtures.
- **ESV-U3:** private preview target transaction, ordered hit facts, lease retirement, resize, deferral, and whole-owner replacement tests pass.
- **ESV-U4:** egui Scene View, independent camera, texture lifecycle, exact-generation pick, and stale-input tests pass; native canary is nonblank.
- **ESV-U5:** the serial source trajectory and downstream consumers pass, evidence is accurately classified, and documentation authority is reconciled.
