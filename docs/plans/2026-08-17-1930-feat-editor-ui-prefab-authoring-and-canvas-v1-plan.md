---
title: Editor UI Prefab Authoring and Canvas v1 - Plan
type: feat
date: 2026-08-17
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
origin: STRATEGY.md
execution: code
execution_state: active
active_unit: EUA-U2
activation_record: a234ba7c987f4e22b2e5756269470b3d
depends_on: docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md
plan_id: editor-ui-prefab-authoring-canvas-2026-08
unit_namespace: EUA
---

# Editor UI Prefab Authoring and Canvas v1 - Plan

## Goal Capsule

| Field | Contract |
|---|---|
| Objective | Let an author open one real UI Prefab source from the existing Editor, select its retained UI nodes, edit common layout/text/visual/button/theme values through typed Inspector controls, preview the accepted unsaved result through Nara UI in an exact-generation Canvas, manipulate supported absolute pixel geometry with one-record gestures, Undo/Redo, Save/Reopen, and then make the existing Game View consume the same saved source after an explicit Restart. |
| Product owner | The private `nara-editor` application remains the product Host. `EditorWorkspace` owns document slots, selection, history, dirty state, and ordered actions; `EditorProjectSession` owns source opening, persistence, project-content refresh, preview materialization, and Play freshness; `NativeEditorHost` owns the native window, egui, GPU execution, Canvas target, and physical input arbitration. |
| Architecture authority | Accepted ADRs 0006, 0015, 0026, 0047, 0101, 0104, 0107, 0108, 0111, and 0112 remain authoritative. EUA-U1 must accept one focused ADR 0113 before Prefab document, Canvas presentation, or continuous UI-authoring behavior becomes durable. OQ-010 remains open: this plan does not select Nara UI as the Editor shell toolkit. |
| Execution profile | Five dependency-ordered units, owner-level domain tests, one serial real-project UI authoring trajectory, and one bounded native Canvas canary. No platform, field, layout-mode, or window-state Cartesian matrix is required. |
| Stop conditions | Stop the affected path if Prefab editing requires a synthetic Scene owner, if persistence cannot bind exact document kind/source/revision, if Canvas picking cannot bind the exact final composite receipt, if gesture previews must mutate authoritative history, if source refresh requires mutating an existing immutable `ProjectContentSnapshot`, or if the implementation requires a public Canvas framework, RenderGraph, toolkit-neutral widget trait, Binding DSL, docking system, or second GPU device. |
| Tail ownership | EUA-U5 updates ADR/OQ links, the implementation ledger, current-state derivation inputs, and one engineering-memory verification record. This inactive plan does not activate itself or change current execution authority. |

---

## Product Contract

### Summary

Nara can now open a project, edit its startup Scene, use Scene Tree and Inspector, preview the edit
world, run an isolated Game View, and persist Scene changes. It cannot yet perform the ordinary UI
authoring loop users expect from Godot or Unity: open the source UI Prefab, inspect the retained UI
hierarchy, edit UI-aware fields, see the unsaved result at the real runtime layout, select nodes from
the Canvas, drag a supported element, save the source, and restart the game from that same change.

This plan delivers that bounded loop without changing the Editor toolkit strategy. egui remains the
near-term Editor Adapter and shell. Nara UI renders the authored game UI inside a private Canvas;
the Canvas is not itself a Nara UI Editor panel and therefore is not evidence for OQ-010 toolkit
convergence.

Product Contract preservation: bootstrapped from the user-confirmed scope in this session and the
completed Editor Authoring Loop, Scene View, Game View, Runtime UI Product Slice, and Fresh Project
workflows. Those predecessor records remain immutable evidence.

### Problem Frame

Most required mechanisms already exist, but they stop at different authority boundaries:

- `PrefabDocument` has canonical readers/writers and stable ordered entity structure, while
  `SceneAuthoringSession`, `EditorWorkspace`, and Editor persistence are Scene-only.
- Runtime UI components are schema-authorable, while the generic Inspector exposes many enum,
  dimension, color, brush, and nested fields as raw scalar rows.
- Nara UI produces immutable layout, paint, and presented-session facts, while the Editor has no
  source-node map or Canvas interaction model for those facts.
- Scene View and Game View already prove one private Editor GPU authority and exact composite
  receipts, while their viewport source model has no UI-authoring placement.
- `ProjectContentSnapshot` correctly freezes Prefab content, while saving a Prefab currently has no
  Host-owned replacement path for the next Scene preview or Play generation.

A shallow solution would instantiate a Prefab into a temporary Scene and save it back, let egui
directly mutate component values, infer selected nodes from current layout, or replace Prefab data
inside a live project snapshot. Each shortcut creates a second source of truth and fails under
Undo/Redo, Reopen, stale drafts, runtime isolation, or future toolkit replacement.

### Key Decisions

- KD1. **Open and edit the real Prefab source document.** (session-settled: user-approved — chosen over wrapping the Prefab in a synthetic Scene because source kind, canonical format, history, and persistence must remain exact.) Scene anchors and instance overrides remain separate Scene-owned facts. Governs R1-R5, R16-R18.
- KD2. **Keep egui as the Editor shell and use Nara UI only for the authored preview.** (session-settled: user-approved — chosen over immediate Godot-style toolkit unification and over permanently isolating Nara UI from Editor pressure.) The stable future parity seam is the immutable authoring frame plus ordered action trace, not a shared widget trait. Governs R6-R15, R19.
- KD3. **Limit v1 to existing UI nodes and component fields.** (session-settled: user-confirmed scope — chosen to close value editing, Canvas manipulation, persistence, and runtime consumption before source-node creation/removal/reparenting introduces cross-document override validation.) The hierarchy is navigable and selectable; structural composition is a successor. Governs R4-R8, R20.
- KD4. **Render the accepted unsaved Prefab through the real Nara UI pipeline.** (session-settled: user-approved — chosen over approximating runtime layout with egui controls.) Canvas geometry, clipping, z-order, text, paint, and pick facts derive from one exact UI authoring candidate and final Editor-window composite receipt. Governs R9-R15.
- KD5. **Model direct manipulation as a revision-bound continuous transaction.** (session-settled: user-approved — chosen over per-pointer-move document mutation or toolkit-owned undo.) Begin captures one document/target/revision baseline, Preview changes only transient authoring presentation, Commit publishes one patch and one history record, and Cancel publishes none. Governs R12-R15.
- KD6. **Publish saved Prefab content by replacing the project snapshot, then require explicit Game Restart.** (session-settled: user-approved — chosen over live-edit mutation of the running Runtime and over silently keeping an old snapshot after Save.) Receipt-backed Save advances the source checkpoint; a separate Host refresh creates a new immutable content generation. Existing Play keeps its old generation and becomes out of date. Governs R16-R18.
- KD7. **Use one real Runtime UI product as the end-to-end carrier, not as architecture authority.** The Reference Game supplies an existing UI Prefab and typed game action for the native trajectory, while generic owner tests prove the document, action, and Canvas seams. No Reference Game-specific symbol enters shared APIs. Governs R18-R19.

### Requirements

#### Source opening and document authority

- R1. Selecting a Scene entity whose structured provenance or local component contains a path-based `PrefabInstance` exposes an `Open Prefab Source` action. The Host resolves the exact normalized path under the configured Prefab root and opens or activates one Prefab document slot. No Asset Browser, filesystem search, expanded-ID parsing, or string-split locator is introduced.
- R2. A Prefab slot has an explicit document kind, workspace-local `EditorDocumentId`, source binding, authoring session lineage, monotonic revision, branch-safe content state, selection, saved checkpoint, external-change state, and persistence lane. `PrefabDocument` is the persistent truth. `PrefabDocument::instantiate()` may serve preview/materialization but never becomes the authoring or save owner.
- R3. v1 supports path-addressed Prefabs present in the admitted project-content closure. Stable-ID sources, missing source mappings, nested projected records that cannot be tied to one local source document, non-UI Prefabs, and unknown schemas reject with typed diagnostics. They do not fall back to the active document or scan another root.
- R4. The workspace owns one ordered document collection with closed Scene and Prefab slot variants. Scene and Prefab slots isolate identical `SceneEntityId` values by `EditorDocumentId`; each slot owns exactly one selection. Reopen preserves the slot identity only when document kind and normalized source binding are unchanged; otherwise the Host closes the old slot through ordinary arbitration and opens a new slot. Every successful Reopen publishes a new authoring-session lineage, clears history/selection/drafts, and makes every prior revision-bound action stale.
- R5. Scene-only commands remain explicit. Scene Composition, Play-source mutation, and Apply Changes never reinterpret the active Prefab as a Scene. The v1 shell keeps the startup Scene plus at most one directly opened Prefab source. Navigating between Authoring and Game, or between the retained Scene and Prefab producers, does not close that Prefab slot; replacing the open Prefab, closing it, closing the project, or exiting with accepted dirty content follows Save/Discard/Cancel arbitration.

#### UI authoring projection and typed Inspector

- R6. One root-private `EditorUiAuthoringFrame` binds the exact tooling-owned Prefab document/session/revision/selection stamp plus an optional transient gesture generation to one `nara_ui`-owned authoring projection payload. Durable authoring facts are always present: source-relative ordered UI hierarchy, stable source node identity, component capabilities, declared layout fields, editable/read-only reasons, field descriptors, and validation diagnostics. A closed Canvas projection outcome is either `Unavailable(reason)`, `Rejected(diagnostics)`, or `Ready` with resolved logical bounds, clip/order facts, source mapping, and gizmo capabilities. Hierarchy and Inspector remain usable when Canvas projection is unavailable or rejected; Canvas paint, hit testing, and overlays consume only the matching `Ready` facts. Last-good pixels may be displayed with an inert stale overlay but never masquerade as current pick authority. `nara_ui` never imports `EditorDocumentId` or other tooling identities, and `nara_tooling` never imports UI projection types.
- R7. The UI-specific authoring descriptor set is closed and typed. It carries stable component/field identity, display grouping, value kind, enum options, unit, finite range/step/precision where applicable, discrete versus continuous commit policy, validation state, and an optional bounded composite-field definition. A discrete composite action updates all correlated canonical fields through one atomically validated `ScenePatchDocument` and creates one Undo record; it is required for dimension kind/value, brush mode/payload, color channels, and other presence/value groups that have no valid single-field intermediate state. Toolkit labels and egui IDs are never field identity.
- R8. v1 provides useful controls for the runtime vocabulary already authored by `UiRoot`, `UiNode`, `UiVisual`, `UiText`, `UiButton`, `UiButtonVisuals`, and `UiTheme`: layer; position/layout mode; width/height/left/top dimension kind and value; flex direction/alignment/justification/gap/padding/grow/shrink; z/visible/overflow/stacking; solid/role/image brush facts; border and radii; text content/size/alignment/overflow/semantic text; button label/action/enabled/text size; button state roles; and theme palette colors. `UiSemantics`, unsupported complex lists, asset selection, and unsupported value combinations remain visible but read-only with an explanation rather than falling back to an unvalidated text editor. Editable semantic/accessibility authoring requires a later end-to-end product workflow.
- R9. Every discrete field commit carries exact document, Prefab kind, base revision, entity, component schema, and either one canonical field identity or one descriptor-declared bounded composite field set. The entire update lowers to one failure-atomic patch and one history record. Domain/schema validation and patch/history admission happen before document publication. Invalid enum, non-finite/out-of-range value, incomplete compound value, codec mismatch, stale revision, missing target, or budget failure leaves document, history, selection, saved checkpoint, accepted authoring frame, and Canvas presentation authority unchanged.

#### UI Canvas presentation and selection

- R10. The private Editor viewport remains one top-level `Authoring` or `GameRuntime(runtime_generation)` source. `Authoring` has one active producer, `ScenePreview` or `UiPrefab(document)`, but never parallel targets. The selected producer supplies the only source association eligible for the current target and receipt; `WgpuEditorExecution` retains target, texture-registration, and composite-receipt lifecycle ownership, while `NativeEditorHost` retains physical input arbitration. Opening a Prefab selects the UI Canvas; leaving it retires Canvas presentation and gesture authority before another producer or Game activates.
- R11. `EditorProjectSession` deepens the existing edit-preview owner so it materializes the active accepted Scene or Prefab plus any transient gesture overlay into one private World. The UI producer runs the existing Nara UI projection/compiler/planner to create an authoring-only image candidate and a source map from runtime UI keys to exact Prefab-local `SceneEntityId` values. It does not publish a Runtime `UiPresentationReceipt`, Runtime interaction session, focus/reducer state, accessibility actions, or backend outcome permits. The candidate contains no native, WGPU, egui, or durable workspace identity beyond the explicit document/revision stamp.
- R12. `NativeEditorHost` submits the authoring-only Canvas image through the existing one-device Editor execution owner, lets egui sample that exact texture registration, and publishes Canvas input authority only after the Editor window presents. The composite receipt binds document/revision, transient generation, UI frame/paint-packet identity, logical surface/content transform, image target/generation, texture-registration generation, backend instance, device epoch, window-surface generation, and Editor composite generation. It is an Editor Canvas presented stamp, not a Runtime UI presentation receipt.
- R13. Canvas picking uses the prior exact visible authoring frame and source map. It considers all visible authored UI layout facts, not only runtime-interactive hit regions, respects clip and structural/paint order, and emits the same workspace selection action as the hierarchy. Candidate-only, resized, stale-revision, foreign-document, retired-target, or old-device facts cannot change selection.
- R14. Hover, selection outline, direct-manipulation handles, drag threshold, the fixed fit-to-panel transform, and toolkit IDs are transient Editor state. Canvas mapping keeps two distinct transforms: `UiSurfaceContentTransform` maps authored logical space to the offscreen image, while a private `CanvasViewTransform` deterministically fits that image into the egui panel. Blank primary click clears selection; primary click on a visible node selects it; primary drag on the selected body starts move only when position is writable; primary drag on an admitted edge/corner handle starts resize; blank drag is a no-op in v1. An active drag retains pointer ownership outside the panel until release or cancellation. Escape cancels; focus loss, source switch, Reopen, target deletion, revision conflict, zero extent, fit-transform change during drag, or GPU retirement cancels or retires the gesture explicitly. User pan/zoom, marquee, snapping, guides, modifier transforms, and touch navigation are deferred. Canvas navigation never dirties the Prefab.
- R15. Direct move/resize is enabled only when the descriptor proves that the relevant position and size are authored as absolute pixel fields and are not parent-container controlled. Auto, percent, flex/container-managed, missing, locked, or invalid geometry remains Inspector-editable where supported but exposes no misleading direct handle. The Editor never silently converts a layout mode or writes resolved bounds back as authored values. Begin freezes the exact visible source/content/view transform, base revision, target fields, and transaction generation; later preview frames from that transaction may present without cancelling it. Move values remain within their typed finite domain; resize previews clamp only to descriptor-provided minimum/maximum bounds and cancel on non-finite mapping. Commit validates the transaction/base revision/schema authority and does not require the latest GPU preview to have presented. One committed gesture creates exactly one patch and one Undo record; Preview creates zero records and Cancel restores the accepted frame.

#### Save, project refresh, and runtime consumption

- R16. Save captures an exact Prefab checkpoint and writes through the existing Host-owned atomic replacement protocol and canonical Prefab writer. Only an accepted filesystem receipt advances the saved content state. Save failure, external conflict, uncertain publication, wrong lane/kind/source, or stale checkpoint preserves dirty/history/document state and exposes the reason-specific Retry, Reconcile, or corrective action admitted by that outcome.
- R17. After a Prefab Save receipt succeeds, the Host carries that receipt's normalized Prefab path and published content digest into successor construction, rebuilds a complete `ProjectContentSnapshot` through ordinary project capabilities, verifies the exact saved digest while loading the closure, and validates startup Scene expansion against it. The candidate charges the same aggregate `ProjectContentBudgetHost` as the Editor current snapshot and all Runtime leases, and admission permits at most two distinct retained project-content generations across that union. Success publishes a new non-reused project-content generation and refreshes Scene preview/source freshness without mutating any old snapshot. Digest mismatch becomes a typed external conflict; capacity failure reports the required state/limit change rather than an unconditional Retry. Any failure retains the last-good snapshot, marks the saved source as project-content-stale, and disables new Play/Restart; the file remains honestly saved.
- R18. A running Play generation never observes Prefab edits in place. Accepted unsaved changes and saved-but-not-yet-refreshed content mark the current Runtime out of date. New Play/Restart is disabled while the active Prefab has accepted unsaved changes or project-content refresh is pending/stale; an already-running Runtime may remain visible with an out-of-date badge and may always Stop. After successful source refresh and a clean active document, explicit Restart or a fresh Play start binds the successor content generation and renders the saved UI. Stop, Restart, or refresh never writes runtime state back to the Prefab.
- R19. One serial real-product trajectory proves: open project → select the UI Prefab anchor → open source → select the Retry control from hierarchy and Canvas → edit text/visual/layout/button values → perform one supported absolute-pixel gesture → Undo/Redo → Save → publish the successor project-content generation → Reopen the same source → return to Scene/Game → explicit Restart → observe the same saved action/label/geometry in the real Runtime UI. One native canary proves nonblank exact Canvas presentation and one current-generation pick/gesture; it is not a platform or pixel matrix.
- R20. v1 explicitly defers UI node/component add/remove/reparent, Prefab instance override authoring, nested-source editing, Asset Browser, user Canvas pan/zoom, marquee, snapping/guides, responsive breakpoints, animation timeline, interactive Runtime UI preview, editable semantic/accessibility authoring, data binding, docking, detached windows, native multi-window, plugin registration, full unknown-schema round-trip, and Nara UI Editor shell convergence.

#### Product UI environment authority

- R21. Effective project settings admit at most one bounded primary UI surface profile containing authored logical width/height and safe-area policy. It is project/product authority, never Prefab content or an Editor-only preference. Root product lowering publishes one immutable accepted profile before either the Runtime UI contribution or Editor preview is installed; each consumer then derives its own target-specific `UiSurfaceContentTransform`. Theme facts continue to come from authored UI content and physical target size/scale continue to come from the current Host. A missing or invalid profile makes Canvas projection `Unavailable` without disabling Hierarchy/Inspector. The Reference Game migrates its existing 960×540 constants to this profile; no product-specific fallback enters shared code. Multiple named surface profiles and a general surface catalog remain deferred.

### Interaction State Contracts

#### Orthogonal Prefab lifecycle facts

The product does not encode document cleanliness, persistence work, project-content freshness, and
Runtime freshness as one enum. The immutable Editor model exposes four orthogonal facts so valid
combinations such as `Dirty + Saving` and `Dirty + ProjectContentCurrent` remain representable:

| Dimension | States | Authority and action rule |
|---|---|---|
| Document | `Closed`, `Opening`, `Clean`, `Dirty`, `Reopening` | The workspace owns current accepted content. Editing/Undo/Redo may continue while a captured checkpoint is saving. Reopen failure preserves the current slot; successful Reopen creates a new session lineage. |
| Persistence | `Idle`, `Saving(checkpoint)`, `Uncertain(reconcile)` | The Host owns file work. A receipt advances only the captured content state. Save is disabled while another save or reconciliation is active. |
| Project content | `Current(generation)`, `RefreshPending(saved_state)`, `Stale(last_good, diagnostics)` | The Host owns snapshot replacement. Last-good Scene/Game content remains visible on failure, but new Play/Restart is disabled until reason-specific remediation publishes a successor. |
| Runtime | `Stopped`, `Current(content_generation)`, `OutOfDate(content_generation)` | Play owns an immutable content generation. Dirty content or pending/stale refresh marks an existing Runtime out of date; Stop remains available, while new Play/Restart waits for clean/current source authority. |

The shell derives badges and actions in this priority order: uncertain persistence, project-content
stale/pending, invalid Inspector draft, dirty document, out-of-date Runtime. Back to Scene or Game is
navigation and does not close the Prefab slot. Replacing the one open Prefab, closing it, closing the
project, or exiting the Editor uses Save/Discard/Cancel when the accepted document is dirty.

#### Authoring navigation

| Current surface | Action | Result and focus rule |
|---|---|---|
| `Authoring(ScenePreview)` | `Open Prefab Source` | Opens/activates the one Prefab slot, switches to `Authoring(UiPrefab)`, selects the mapped source node when exact provenance exists (otherwise the first root), and focuses UI Hierarchy. |
| `Authoring(UiPrefab)` | `Back to Scene` | Keeps the Prefab slot and dirty state open, retires Canvas input, restores the prior Scene selection, and focuses Scene Tree. It does not Save, Discard, or Restart. |
| Either Authoring producer | `Game` | Switches only the top-level source. An existing Runtime may remain visible; Play/Restart availability derives from the orthogonal lifecycle facts above. |
| `Game` | `Authoring` | Returns to the last selected Authoring producer and restores that producer's selection/focus if its session lineage is unchanged. |
| `Authoring(UiPrefab)` | close/replace Prefab or exit | Resolves invalid drafts, then applies Save/Discard/Cancel for accepted dirty content before retiring the slot. |

#### Inspector draft lifecycle

- Discrete controls keep an adapter-local draft keyed by document, session lineage/revision, entity,
  component, and field. Enter or focus loss attempts one exact commit.
- Validation failure retains the draft and an inline typed diagnostic while leaving accepted document,
  history, selection, and Canvas authority unchanged. Save is disabled until the active invalid draft
  is corrected or explicitly reverted.
- Escape/revert restores the accepted value and clears the diagnostic. Selection/source switch,
  Reopen, target deletion, or session-lineage change discards the stale draft with a visible reason;
  it never retargets the draft.
- A compound control such as dimension kind plus value owns one draft and commits atomically.
  Continuous controls use the gesture lifecycle below; an invalid preview cancels instead of
  publishing a partially valid value.

#### Canvas projection lifecycle

Hierarchy and Inspector continue to use the current authoring frame when Canvas projection is
`Unavailable` or `Rejected`. `Ready` facts become pickable only after the matching final composite
receipt. Last-good pixels after rejection/deferral are visibly stale and inert; recovery publishes a
new receipt rather than re-enabling old input facts.

#### Canvas gesture lifecycle

```text
Idle
  -> Pressed(exact visible target)
  -> Dragging(document, base revision, target, baseline, transient generation)
       -> Preview*          (no document/history mutation)
       -> Commit            (one patch, one revision, one undo record)
       -> Cancel(reason)    (zero patch/history records)
```

A primary release below the drag threshold remains a selection click. A direct handle is absent when
layout ownership is not writable. Adapter-local widget signals only translate into these states; the
adapter cannot publish a document mutation by editing an `&mut` domain value.

### Acceptance Examples

- AE1. An author selects the Scene's `reference-ui.prefab.json` anchor, chooses `Open Prefab Source`, and the Editor shows the source-relative UI hierarchy rather than expanded runtime IDs.
- AE2. Changing Retry label and a theme color updates the unsaved Canvas after one accepted discrete action. Source bytes and the running Runtime image remain unchanged, while Game View immediately reports that its content generation is out of date. Runtime content changes only after Save, successful project refresh, and explicit Restart.
- AE3. Dragging an absolute pixel-positioned control across several pointer events produces many transient previews but one committed patch and one Undo record. Escape instead restores the accepted layout and produces no record.
- AE4. Selecting a flex-managed child shows its resolved bounds but disables direct move/resize with a layout-owner explanation; no pixel values are fabricated.
- AE5. Save succeeds, project-content refresh fails, and the Editor reports `saved source / stale project content`; the old Game image may remain visible, but Play/Restart cannot claim the new Prefab until refresh succeeds.
- AE6. After successful refresh and explicit Restart, the Reference Game shows the same saved Retry label/action/geometry observed in the Prefab source and Canvas.

---

## Context and Research

### Existing Nara patterns

- `crates/nara_scene/src/{prefab.rs,patch.rs,authoring.rs,format.rs}` already owns ordered Prefab data, exact patches/inverses, bounded clean-aware history, and canonical Prefab encoding. The missing seam is a direct `PrefabAuthoringSession`, not another document format.
- `crates/nara_tooling/src/{workspace.rs,frame.rs,inspector.rs}` already owns exact document/revision actions, single-owner selection, immutable Editor frames, and receipt-backed dirty state. The current Scene-only slot/checkpoint shape must become a closed Scene/Prefab document model.
- `src/project_host/{persistence.rs,runtime/editor.rs}` already owns capabilities, atomic Save receipts, Play isolation, and project content. It must add document-bound persistence lanes and immutable snapshot replacement rather than moving filesystem authority into an Adapter.
- `crates/nara_ui/src/{codec.rs,runtime.rs}` and `nara_ui_kernel` already own the authorable component vocabulary, runtime validation, resolved layout, paint, semantics, and exact presentation facts. The authoring model should project those facts rather than duplicate a layout engine.
- `editor/src/platform/viewport.rs`, ADR 0111, and ADR 0112 already prove one private offscreen target, one GPU owner, final composite receipts, and exact-generation picking. UI Canvas extends that private source sum; it does not create a public viewport framework.

### Reference pressure

- Godot `Control`, Container, Canvas editor, Inspector, and UndoRedo patterns support separating declared layout from resolved bounds, disabling direct transforms for container-owned children, using typed property editors, and committing one continuous action. Nara retains its own document/patch authority and does not copy Godot's Object/Variant or mutable Node ownership.
- Open GPUI's Canvas frame/session split supports one immutable frame for paint, hit, overlay, and feedback. Nara does not copy its store, history, docking, or persistence owners.
- egui and Dear ImGui response lifecycles are sufficient to translate adapter-local start/change/end signals into the toolkit-neutral gesture transaction. Their convenience undo/reflection models are not document authority.
- No repository `docs/solutions/` or `CONCEPTS.md` learning exists for this workflow. This plan therefore cites current Accepted ADRs, landed code, completed verification, and pinned local reference sources without claiming prior institutional proof.

---

## Planning Contract

### Key Technical Decisions

- KTD1. **Accept a focused ADR 0113 before implementation broadens document or Canvas authority.** It refines ADR 0047 with true Prefab document slots, ADR 0026 with continuous UI-authoring transactions, ADR 0107 with the egui-shell/Nara-preview boundary, and ADRs 0111/0112 by adding UI as a mutually exclusive Authoring producer within the existing single-slot Authoring/Game topology. It does not close OQ-010, instance overrides, or live edit.
- KTD2. **Share authoring mechanics privately, preserve nominal public documents.** `nara_scene` may extract one private entity-document patch/history core used by `SceneAuthoringSession` and a new `PrefabAuthoringSession`. Public Scene and Prefab documents/sessions remain distinct; no generic public document trait, synthetic Scene conversion, or duplicated history implementation is added.
- KTD3. **Make the workspace document-kind aware without making it an asset database.** Fearlessly replace Scene-only slot/action/checkpoint names with closed document variants where the behavior is truly shared. Host product policy admits the startup Scene plus one path-bound Prefab source in v1. Stable source path, slot identity, authoring lineage, and revision remain separate facts.
- KTD4. **Keep UI-specific authoring knowledge in `nara_ui`, not generic tooling.** A private/publicly narrow `nara_ui::authoring` Module owns the closed UI component/field descriptors, layout ownership classification, source-to-runtime key map, and immutable projection payload without importing Editor identities. `nara_tooling` owns generic document/selection/gesture/action state without importing UI types. A root-private Host envelope pairs those exact facts into `EditorUiAuthoringFrame`; `nara_tooling_egui` receives borrowed tooling and UI views through an Adapter call but owns neither. No new crate, public generic frame trait, or dependency reversal is added.
- KTD5. **Reuse the existing private Editor GPU transaction without creating a Runtime presented session.** Keep the top-level Authoring/Game source boundary and add UI as one mutually exclusive Authoring producer. Nara UI retains logical-window/content-transform semantics, but the private preview planner lowers resource-complete UI paint packets directly into the existing Editor image transaction. It returns only Editor image/composite facts: no Runtime `UiPresentationReceipt`, outcome permit, input route, focus, reducer, or accessibility publication. Do not add `UiSurfaceEndpoint::Image`, a public render target catalog, or simultaneous Scene/UI/Game rendering.
- KTD6. **Carry a source map beside exact presented layout facts.** Projection records the Prefab-local source entity for each UI key before batching/layout obscures author identity. The map is admitted with the same candidate and promoted only with the matching composite receipt. The Editor never reverse-parses UI keys or maps current document state onto old pixels.
- KTD7. **Use one explicit gesture state machine for Inspector continuous fields and Canvas handles.** The domain stores only baseline identity and bounded preview deltas/values, not a cloned document or toolkit response. Commit lowers to the existing patch/inverse/history path exactly once. Discrete fields continue through ordinary exact-revision mutation.
- KTD8. **Replace project content only at a Host-owned generation boundary.** Save receipt and project snapshot refresh are separate outcomes. Refresh creates and validates a complete successor snapshot, then atomically swaps the Editor's content authority. Old edit previews and Play runtimes retain their old generation until orderly retirement/restart.
- KTD9. **No new async architecture.** Existing bounded platform/GPU initialization may remain asynchronous where its libraries require it. Document actions, gesture preview, Canvas compilation, Save, content refresh, and runtime restart remain serialized Host transactions; no executor, worker pool, background mutable document, or cross-thread UI store is introduced.
- KTD10. **Make the primary UI authored surface a project fact.** Add one optional effective project setting for authored logical extent and safe area. Root product lowering validates it and publishes one immutable profile before Runtime UI or Editor preview contribution installation; those consumers lower it into UI types. Prefabs and toolkit state do not own it. This is not a multi-surface registry.
- KTD11. **Budget successor project content as an explicit two-generation overlap.** Refresh charges the candidate through the same `ProjectContentBudgetHost` as the Editor current snapshot and every Runtime lease. The aggregate ceiling admits at most two distinct project-content generations across that complete retained set. No separate loader or budget host may hide retention. If an older Runtime already differs from the current Editor generation, constructing a successor would create a third distinct generation and Refresh rejects with a typed `runtime-retirement-required` outcome until that Runtime is stopped or restarted onto current content. Candidate capacity failure drops only candidate charges, preserves last-good authority, and names the limit or retained generation that must change; blind Retry is not an available action.

### Ownership Map

| Concern | Owner | Forbidden leakage |
|---|---|---|
| Scene/Prefab documents, patches, inverse, authoring revision, history | `nara_scene` | WGPU, egui IDs, project filesystem authority |
| Document slots, selection, dirty/savepoint, ordered actions, gesture state | `nara_tooling` | Codec-specific filesystem writes, runtime ECS mutation, toolkit memory |
| UI field descriptors, layout ownership, source map, immutable projection payload | `nara_ui` authoring Module | Editor document IDs, egui widgets, Host capabilities, persistence receipts |
| Exact Editor/UI authoring envelope, Prefab source capability, persistence lane, project snapshot refresh, preview World | `EditorProjectSession` / root Host | Direct adapter writes, mutation of old snapshots, native/GPU values |
| Primary UI authored logical extent and safe-area policy | effective project settings / root lowering | Prefab content, toolkit preferences, Reference Game hardcodes |
| Native window, egui shell, target size, physical pointer/focus, composite sequencing | private `nara-editor` Host | Durable document/selection/history authority |
| Offscreen image, backend/device epoch, texture registration, final composite receipt | existing private `WgpuEditorExecution` | UI document ownership, public target registry, second GPU owner |
| Runtime UI product behavior after Restart | isolated Play `RuntimeInstance` | Live mutation from the Editor Prefab session |

### High-Level Technical Design

```text
Scene Tree selection
  -> structured Prefab source path
  -> EditorProjectSession opens Prefab candidate
  -> EditorWorkspace publishes PrefabDocumentSlot

Prefab document + revision + selection
  -> nara_ui::authoring projection
  -> root-private EditorUiAuthoringFrame
       hierarchy + typed fields + declared layout
       resolved layout + source map + gizmo capabilities
  -> egui hierarchy / Inspector / Canvas overlay

Accepted Prefab + transient gesture overlay
  -> private preview World + logical UI surface
  -> Nara UI layout/paint facts
  -> authoring-only resource-complete UI paint packets
  -> WgpuEditorExecution offscreen image
  -> egui sample + Editor window present
  -> exact Canvas composite receipt
  -> current-generation pick / gesture action

Prefab checkpoint
  -> canonical writer + atomic ReplaceReceipt
  -> saved content state
  -> rebuild complete ProjectContentSnapshot
  -> publish successor content generation
  -> explicit Game View Restart
  -> isolated Runtime consumes the saved UI source
```

### Assumptions and Constraints

- The first real carrier has an existing path-addressed UI Prefab with schema-complete Nara UI components.
- v1 edits existing source-local entity/component fields. Structural source operations and instance overrides are deliberately excluded.
- One Prefab source is open at a time in the product shell, though shared workspace internals may use a closed document map.
- The Canvas uses the Prefab's admitted logical UI surface/environment but remains non-interactive authoring preview. Runtime button activation/focus/accessibility preview, responsive breakpoint authoring, and arbitrary multi-surface preview are deferred.
- One native canary and one serial product trajectory are sufficient. Performance budgets remain the existing UI/Editor budgets; this plan may record observations but does not establish a new SLA.

### Sequencing

1. EUA-U1 establishes true Prefab document, workspace, history, and persistence authority and accepts ADR 0113.
2. EUA-U2 defines the toolkit-neutral UI authoring frame, typed fields, layout ownership, and continuous gesture transaction.
3. EUA-U3 adds the private exact-generation UI Canvas candidate/presentation/picking path.
4. EUA-U4 delivers the egui workflow over those established owners.
5. EUA-U5 closes Save → content refresh → Game Restart and records one real product trajectory.

This owner-first order is intentional. Each unit must expose the earliest real carrier observation its
owner can support, but the shell never ships a provisional mutation or Canvas API ahead of document,
action, and presentation authority. Reordering a thin vertical UI slice before U1-U3 would harden the
very Scene-only and candidate-only shortcuts this plan exists to remove.

### System-Wide Impact

- `nara_scene` gains direct Prefab authoring while preserving nominal Scene/Prefab public types and one shared private patch/history implementation.
- `nara_tooling` becomes document-kind aware and gains a focused continuous authoring transaction; generic tooling remains independent of Nara UI component semantics.
- `nara_ui` gains a bounded authoring projection over its existing persistent components and runtime layout facts. `nara_ui_kernel` public boundaries do not expand unless exact source mapping cannot be expressed by the current adapter.
- Root Project Host persistence changes from one Scene-only lane to explicit document-bound lanes. Project content remains immutable and generation-replaced.
- The private Editor deepens its existing Authoring source with a mutually exclusive Scene/UI producer and UI-specific egui panels/controls. It retains one active offscreen target, one native window, and one GPU owner.
- Game View and Runtime UI behavior remain isolated; only the source content generation used by a fresh Restart changes.
- No shared Runtime UI widget catalog, docking graph, binding language, plugin system, Asset Browser, or multi-window policy is introduced.

### Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Shared authoring refactor accidentally weakens mature Scene behavior | Keep public Scene/Prefab sessions nominal, extract only private common mechanics, and run existing Scene owner tests unchanged before adding Prefab cases. |
| Generic workspace becomes a speculative asset framework | Use a closed Scene/Prefab slot enum and one real Prefab consumer; reject unsupported kinds instead of adding a registry. |
| Canvas disagrees with runtime layout | Render through the existing Nara UI pipeline and use presented layout/source-map facts; never use egui layout as authored geometry. |
| Direct handles corrupt flex/percent layout | Derive handle capability from declared layout ownership and expose no handle when authored pixel fields are not authoritative. |
| Preview events create many Undo records | Use Begin/Preview/Commit/Cancel; only Commit enters patch/history. |
| Save succeeds but project refresh fails | Separate saved-source truth from project-content generation, retain last-good snapshot, block new Play/Restart, and expose only the typed remediation admitted by the failure. |
| Editor Canvas and Runtime use different logical UI geometry | Admit one project-owned primary UI surface profile and require both consumers to lower that same authored extent/safe-area fact; missing profile makes only Canvas unavailable. |
| Snapshot refresh exceeds bounded retention | Charge current and candidate through one budget host, admit at most two distinct generations, and require old Runtime retirement before any third generation. |
| Reference Game leaks product-specific vocabulary into shared code | Keep its symbols in the trajectory/canary; shared APIs use document, source, UI key, field, and action identities only. |
| Test scope expands into field/layout/platform matrices | Cover owner invariants, one representative discrete field, one continuous field/gesture, one unsupported layout, one failure per owner boundary, one serial trajectory, and one native canary. |

### Resolved During Planning

- Direct source Prefab editing is required; synthetic Scene authoring is rejected.
- Existing-node field editing is the v1 structural boundary; add/remove/reparent is deferred.
- egui remains the shell; Nara UI renders the authored Canvas but does not yet render a complete Editor panel.
- Direct manipulation is limited to absolute pixel-owned geometry.
- Save and project-content refresh are distinct; Game consumption requires a successful refresh and explicit Restart.

### Deferred Questions

- Prefab instance override editing and conflict visualization.
- Structural UI composition and reusable UI element presets.
- Stable-ID/nested Prefab source navigation and a real Asset Browser.
- User Canvas pan/zoom, marquee, snapping/guides, responsive/multi-surface preview, localization/font/IME authoring, editable semantics/accessibility, animation, and binding workflows.
- Interactive runtime preview versus authoring-only Canvas.
- Nara UI Inspector parity, Editor toolkit convergence, docking, detached windows, and native multi-window.

---

## Implementation Units

### EUA-U1. True Prefab document, history, workspace, and persistence authority

**Goal:** Make one path-bound Prefab a first-class authoring document with exact revision/history,
selection, Save/Reopen, and Host-owned persistence while preserving existing Scene behavior.

**Requirements:** R1-R5, R16, R21; KD1, KD3; KTD1-KTD3, KTD8, KTD10.

**Dependencies:** Completed EFI, ESV, EGV, and FPER plans; Accepted ADRs 0006, 0026, 0047,
0101, and 0104.

**Files:**

- `docs/architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md`
- `crates/nara_scene/src/{authoring.rs,patch.rs,prefab.rs,format.rs}` and a private authoring-core split if justified
- `crates/nara_tooling/src/{workspace.rs,frame.rs,persistence.rs,inspector.rs}`
- `crates/nara_project/src/{manifest.rs,sections.rs,effective.rs,profile.rs}` only for the bounded primary UI surface profile
- `src/project_host/persistence.rs`
- `src/project_host/runtime/editor/{actions.rs,model.rs,persistence.rs}`
- Focused existing Scene/Prefab/tooling/persistence tests; no new public crate.

**Approach:**

1. Accept ADR 0113 with the ownership, identity, persistence, project-owned primary UI surface, two-generation content budget, Canvas, and deferred-scope decisions from this plan. Refine the predecessor ADR links without changing unrelated OQs.
2. Add the bounded optional primary UI surface profile to project settings and effective-profile lowering. Validate finite positive authored extent and safe-area bounds, keep the stored fact independent of `nara_ui` types, and publish no fallback from the Editor window or Reference Game constants.
3. Extract the smallest private document-neutral patch/history mechanics from `SceneAuthoringSession`. Add a nominal `PrefabAuthoringSession` that applies `ScenePatchDocument` directly to `PrefabDocument`, retains canonical source kind/order, and reuses bounded history/content-state semantics. Do not round-trip through `instantiate()`.
4. Replace Scene-only workspace storage/checkpoint assumptions with a closed Scene/Prefab document slot and tagged immutable persistence checkpoint. Every mutation still requires exact document kind and base revision; no active-document fallback remains.
5. Add Host-owned document persistence lanes keyed by the opened document and source binding. The Prefab lane opens through `PrefabDocumentCandidate`, encodes through the existing canonical writer, and commits saved state only from an accepted replacement receipt.
6. Add the `Open Prefab Source`, activate, close, Reopen, and dirty-close Host actions for one path-bound UI Prefab. Preserve structured source and slot identity where R4 permits it. Successful Reopen publishes a new authoring-session lineage and clears history, selection, and drafts; stable IDs, nested ambiguous projection, unknown schema, and non-UI source reject without workspace mutation.
7. Keep Scene-only composition/play/apply actions explicitly typed and reject them for a Prefab target.

**Patterns to follow:** Existing `SceneAuthoringSession` history and content-state behavior,
`EditorPersistenceAuthority`, `ScenePersistenceHost` receipt reconciliation, `PrefabDocumentCandidate`,
canonical Prefab writers, and structured `SceneEntityProvenance`/`PrefabProjectionPath`.

**Test scenarios:**

- A direct Prefab patch, Undo, Redo, branch edit, and Undo-to-saved-state preserve monotonic revision and bounded history while the canonical document remains a Prefab.
- Scene and Prefab slots with the same local entity ID keep independent selection, revision, history, dirty, and saved facts.
- Wrong kind, foreign document, stale revision, missing source, or unsupported stable ID rejects before document/history/workspace mutation.
- Prefab Save/Reopen uses the exact Prefab lane and canonical writer; a Scene receipt cannot commit a Prefab checkpoint and vice versa.
- A Save receipt for an older content state may mark that state saved while a later accepted edit remains dirty.
- One valid primary UI surface profile lowers exact authored extent/safe-area facts; a missing profile remains explicitly absent and an invalid profile rejects effective project admission without a product fallback.
- Existing Scene authoring, persistence, Play, Scene View, and Game View owner tests remain valid without compatibility wrappers.

**Verification:** Focused `nara_scene`, `nara_tooling`, and Editor persistence tests prove the owner
contracts. No egui, Canvas, or WGPU test is used to prove document correctness.

### EUA-U2. Toolkit-neutral UI authoring frame, typed fields, and gestures

**Goal:** Define the single immutable model and ordered action transaction that hierarchy,
Inspector, Canvas, and a future second toolkit can share.

**Requirements:** R6-R9, R13-R15; KD2-KD5; KTD4, KTD6-KTD7, KTD9.

**Dependencies:** EUA-U1 true Prefab document/revision/session authority; existing Nara UI codecs and
layout facts.

**Files:**

- `crates/nara_ui/src/lib.rs` plus a focused private/publicly narrow `authoring.rs` split
- `crates/nara_ui/src/{codec.rs,runtime.rs,runtime/projection.rs}` only where source mapping or descriptor ownership requires it
- `crates/nara_tooling/src/{frame.rs,workspace.rs}` plus a focused `gesture.rs` split
- Focused UI-authoring/model tests; egui rendering remains U4.

**Approach:**

1. Define a closed `nara_ui` authoring projection over one Prefab source revision without importing tooling identity. Project canonical roots/children and current persistent UI component fields once; derive stable field descriptors, declared layout ownership, Canvas projection outcome, read-only reasons, source map, and gizmo capabilities. In the root Host, pair that payload with the exact tooling document/session/revision/selection stamp as one private `EditorUiAuthoringFrame`.
2. Reuse existing component schema/codec identities and validation. Add only the UI-specific hints the generic schema cannot express: enum choices, dimensions, colors/brushes, units/ranges, discrete/continuous commit policy, and bounded composite field groups.
3. Keep `nara_tooling` UI-neutral. Add ordered actions for one-field or descriptor-declared composite discrete patches and one bounded continuous transaction carrying exact document/revision/target/field baseline and transient preview generation. Every composite action lowers to one atomically validated `ScenePatchDocument` and one Undo record.
4. Implement Begin/Preview/Commit/Cancel so Preview only affects the authoring frame input overlay. Commit lowers one final value set to one failure-atomic patch/history record. Cancel reasons include Escape, focus loss, source handoff, Reopen, target removal, stale revision, target retirement, and invalid preview.
5. Classify direct geometry capability from authored fields. Only absolute pixel position/size exposes move/resize. Other modes remain visible and editable through supported descriptors but have no direct handle.

**Patterns to follow:** Nara's immutable `EditorFrameModel`, `UiFrame`/layout facts,
`ScenePatchDocument`, exact base-revision dispatch, and adapter-local draft identity. Use Godot typed
Inspector/continuous-change concepts and Open GPUI immutable Canvas frames as reference pressure,
not as ownership models.

**Test scenarios:**

- Hierarchy, Inspector fields, Canvas layout facts, source map, and gizmo capability in one authoring frame share the exact document/revision/selection.
- A representative enum, dimension, color, text, and action field has a stable typed descriptor; dimension kind/value, brush mode/payload, and color channels commit atomically with no invalid intermediate document.
- An absolute pixel node admits a move/resize gesture; an auto/percent/flex-managed node reports the correct read-only direct-manipulation reason.
- Multiple Preview updates do not change document revision/history; Commit produces one revision/Undo record; Cancel produces none and restores the accepted authoring frame.
- Reopen, selection target deletion, or stale revision cancels the transaction and makes the old adapter draft unusable.

**Verification:** Focused authoring-frame and transaction tests. Do not enumerate every UI field or
layout combination; representative categories plus codec/schema coverage are sufficient.

### EUA-U3. Exact-generation private UI Canvas presentation and picking

**Goal:** Render one accepted unsaved UI Prefab through the real Nara UI pipeline into the existing
private Editor target and bind Canvas selection/handles to the exact visible generation.

**Requirements:** R10-R15, R21; KD2, KD4-KD5; KTD5-KTD7, KTD9-KTD10.

**Dependencies:** EUA-U2 authoring frame/source map/gesture; completed ADR 0111/0112 private Editor
execution and composite receipt contracts.

**Files:**

- `src/project_host/runtime/editor.rs` plus private `src/project_host/runtime/editor/ui_preview.rs`
- Root project-profile lowering plus the U1 effective-project surface fields
- `editor/src/platform/viewport.rs` and a focused `editor/src/platform/ui_canvas.rs` split if warranted
- `editor/src/platform.rs` only for fixed source handoff and receipt publication
- `crates/nara_ui/src/{runtime.rs,runtime/projection.rs}` and `crates/nara_ui_render` only for exact source/fact handoff admitted by U2
- Existing private `nara_render_wgpu` Editor execution files only where the additional Authoring producer requires a narrow extension
- `reference-game/{nara.toml,src/runtime_ui.rs}` to remove the duplicated logical-size constants and consume the same accepted primary UI profile
- Focused Host/render/Canvas tests.

**Approach:**

1. Deepen the existing private edit-preview owner so it materializes exactly one active Authoring document producer at a time. For a Prefab, apply the transient gesture overlay in a private preview World and lower the accepted project-owned primary UI surface profile into its logical Canvas environment. Missing/invalid profile produces `Unavailable` while Hierarchy/Inspector remain usable. The preview does not share the Play Runtime or write persistence state.
2. Compile with existing Runtime UI components, kernel, planner, fonts, and backend capability closure, but capture authoring-only resource-complete paint packets rather than Runtime backend presentation attempts. Carry the exact U2 source map and authoring identity beside candidate facts rather than modifying UI stable keys for Editor purposes.
3. Extend the Authoring producer sum to Scene/UI while preserving the existing top-level Authoring/Game handoff and one active target. Source or producer handoff retires the old target, texture registration, composite receipt, pick facts, and active gesture before admitting UI Canvas.
4. Submit the UI Canvas packets through the existing `WgpuEditorExecution`, register/sample the image in egui, and promote one Canvas presented lease only after final Editor-window presentation. Candidate or image-only success is not Canvas input authority, and no Runtime UI receipt/outcome is manufactured.
5. Bind UI layout/clip/order/source facts to the composite receipt. Resolve selection and handle hit testing from those presented facts and reject the entire identity tuple on mismatch.
6. Reuse existing zero-extent, resize, deferral, content rejection, target retirement, and whole-owner GPU replacement behavior. Last-good pixels may remain with a stale overlay but cannot accept selection or gesture input.

**Patterns to follow:** Scene View exact composite picking, Game View private logical-window-to-image
mapping, Runtime UI presented-session discipline, and existing one-device recovery. Do not add a
public image UI Surface or RenderGraph.

**Test scenarios:**

- The UI candidate can compile while no pickable Canvas exists; only the matching final composite receipt publishes layout/source facts for input.
- A visible current-generation click selects the topmost source node, a clipped/hidden/outside-content click does not, and hierarchy selection matches Canvas selection.
- Document revision, transient generation, resize, source switch, target retirement, or backend replacement invalidates old Canvas input and active gestures.
- A gesture Preview changes the visible candidate without advancing authoritative document revision; Commit presents the accepted successor after one patch.
- Zero extent, compile rejection, deferred submission, and device loss remain bounded and never fall back to current-document inference.
- Runtime and Canvas lower the same primary UI authored extent/safe-area facts; deleting or invalidating the profile makes Canvas unavailable rather than falling back to the project window or Reference Game constants.

**Verification:** Focused private Host/render tests plus one synthetic recovery sequence. No window,
resolution, theme, or GPU matrix is required.

### EUA-U4. egui UI hierarchy, typed Inspector, and Canvas workflow

**Goal:** Expose the established document/frame/action/Canvas contracts through the fixed egui Editor
shell without making egui memory durable authority.

**Requirements:** R1, R4-R15, R19; KD2-KD5; KTD4-KTD7.

**Dependencies:** EUA-U1 through EUA-U3.

**Files:**

- `crates/nara_tooling_egui/src/{editor.rs,inspector.rs,fields.rs,tests.rs}`
- A focused `crates/nara_tooling_egui/src/ui_authoring.rs` split
- `crates/nara_tooling_egui/Cargo.toml` only for the direct borrowed `nara_ui` authoring-view dependency
- `editor/src/{app.rs,platform.rs}` and private Canvas/platform modules admitted by U3
- `editor/tests/authoring_loop.rs` or the existing smallest product-test owner.

**Approach:**

1. Add `Open Prefab Source`, a source breadcrumb/back action, and a UI Hierarchy that renders the exact ordered rows from the borrowed tooling stamp and `nara_ui` projection paired by the Host in `EditorUiAuthoringFrame`. Reuse workspace selection; the Adapter never imports the root-private envelope or builds a second tree/selection set.
2. Render typed controls from the descriptor set: enum combos, bounded drag values, dimension kind/value pairs, color editors, validated strings, booleans, and read-only complex fields. Draft keys include document, Prefab session lineage/revision, entity, component, and field.
3. Translate discrete changes and continuous widget lifecycles into the U2 ordered actions. No egui widget receives mutable document values, and `egui::Memory`/IDs/Undoer remain surface state only.
4. Display the U3 Canvas with a deterministic fit-to-panel transform, selection outline, handles only where admitted, drag threshold, stale/unavailable overlays, and explicit gesture cancel. The adapter uses presented frame facts; it does not derive hit boxes from its own paint commands. User pan/zoom, marquee, guides, and snapping do not enter v1.
5. Preserve semantic keyboard/focus behavior for opening the source, hierarchy selection, Inspector navigation, Save/Undo/Redo, and Escape cancel. This is ordinary egui accessibility/focus evidence, not Nara UI Editor parity.
6. Keep Scene-only Play/Apply controls disabled with a clear explanation while the Prefab source is active. Back to Scene restores the existing Scene/Game workflow without implicit Save or Restart.

**Patterns to follow:** Existing ordered `EditorActionBatch`, revision-bound Inspector drafts,
Scene View texture/input integration, egui response lifecycle, and fixed-shell diagnostics.

**Test scenarios:**

- `Open Prefab Source`, hierarchy selection, Inspector selection, and Canvas selection emit one ordered trace with exact Prefab document/revision identity.
- Reopen or source switch invalidates every old field draft and active gesture; the new Prefab session cannot accept them.
- A discrete text/color edit commits once; a continuous numeric/Canvas gesture previews repeatedly and commits one Undo record.
- Unsupported list/asset/managed-layout values render read-only reasons and cannot emit a generic string mutation.
- Save/Undo/Redo and dirty indicators remain workspace/Host facts; the adapter cannot clear dirty on click.
- One native canary opens the real UI Prefab, shows a nonblank Canvas, selects a known node, and completes one supported gesture against an exact composite receipt.

**Verification:** Focused adapter/action tests and one native canary. Visual inspection covers the
bounded workflow and failure overlays only, not pixel parity across platforms.

### EUA-U5. Save, project-content refresh, Game Restart, and product evidence

**Goal:** Prove the complete source-authority loop from saved UI Prefab through successor project
content and explicit Game View Restart, then close the documentation/evidence boundary.

**Requirements:** R16-R20 and AE1-AE6; all KDs and KTDs.

**Dependencies:** EUA-U1 through EUA-U4.

**Files:**

- `src/project_host/runtime/editor/{persistence.rs,model.rs,play.rs,game_view.rs}`
- `src/project_content.rs` only for a bounded full-snapshot rebuild/publish seam owned by the Host
- `editor/tests/authoring_loop.rs` and/or one focused Reference Game Editor trajectory owner
- `reference-game` source/tests only for product carrier plumbing and structured observations; no shared API definitions
- `docs/architecture/adr/0113-editor-ui-prefab-authoring-and-canvas-authority.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/open-questions.md` only for evidence links and unchanged deferrals
- `docs/knowledge/engineering/verification/` completion record and engineering-memory registration.

**Approach:**

1. After an accepted Prefab Save receipt, rebuild the project content closure through ordinary capabilities/readers into a complete successor candidate. Bind the exact normalized Prefab path and published digest from the receipt into closure loading; external mismatch rejects as conflict. Validate startup Scene expansion and existing overrides before publishing a non-reused content generation.
2. Charge candidate retention through the current `ProjectContentBudgetHost`. Admit the candidate only when the Editor current snapshot, every Runtime lease, and the candidate contain at most two distinct generations. If an already-out-of-date Runtime would force a third generation, require Stop or Restart onto current content before refresh. Capacity failure names the required limit/state change and releases candidate charges.
3. Keep saved-source state and project-content freshness separate in the immutable Editor model. On refresh rejection retain the last-good snapshot/preview, expose exact diagnostics and only offer Retry when the named cause is retryable, and reject new Play/Restart against the stale generation.
4. On refresh success resynchronize the edit preview and mark an existing Play generation out of date. Never mutate the running Runtime. Explicit Restart retires it and starts from the successor content generation.
5. Run one serial Reference Game trajectory through real Editor controls and ordinary source/persistence paths. Compare structured Prefab source identity, selected entity, action/label/layout values, Canvas logical bounds, saved digest/generation, and restarted Runtime UI facts. Do not join unrelated telemetry or cite a component presenter as native evidence.
6. Retain one owner-level negative for invalid source refresh and extend the same bounded native canary from EUA-U4 with stale-input/recovery assertions. Do not add a second native launch or build a state×field×layout×platform matrix.
7. Remove abandoned adapters, synthetic Scene experiments, duplicate Scene/Prefab history code, and temporary product-specific shared seams before closure. Update the focused ADR, implementation ledger, verification record, and completed registration from the reviewed final commit.

**Patterns to follow:** Existing project-content candidate/publication, EGV source freshness and
Restart generation, RUP structured observation, EFI/ESV/EGV evidence classification, and repository
authority rules.

**Test scenarios:**

- Save receipt succeeds and full project refresh publishes a successor content generation; Scene preview and a fresh Restart observe the saved Prefab while the prior Runtime remains unchanged.
- External mutation of the saved Prefab between receipt and refresh fails the exact digest check; a third-generation or aggregate-capacity attempt fails without losing the current snapshot or leaking candidate charges.
- Refresh fails after Save: source remains honestly saved, last-good project content remains visible, Play/Restart rejects, and the typed remediation can later publish the successor without another source edit when the cause is retryable or has been corrected.
- The serial UI authoring trajectory proves hierarchy/Inspector/Canvas parity, one-record gesture, Undo/Redo, Save, successor content refresh, Reopen, and exact Game Restart consumption of the same source.
- Runtime UI action identity and visible label/geometry after Restart match the saved Prefab; no Rust source change or test-private presenter supplies the product facts.
- Final evidence clearly separates domain, adapter, native Canvas, and Runtime/Game observations.

**Verification:** One serial product trajectory, one bounded native canary, affected focused nextest
suites, Editor package check, and documentation consistency checks. No broad release or platform
matrix is required.

---

## Verification Contract

### Required gates

1. **Authority and documentation:** `git diff --check`; ADR 0113, implementation status, OQ-010 deferral, this plan, and the completion record agree on document/Canvas/toolkit ownership and current activity.
2. **Document owners:** focused `nara_scene`, `nara_tooling`, and Editor persistence suites prove direct Prefab patch/history, exact document/revision, kind-bound Save/Reopen, dirty checkpoints, and unchanged Scene behavior.
3. **Authoring model:** focused `nara_ui`/tooling tests prove one immutable UI authoring frame, representative typed fields, layout ownership, source mapping, and Begin/Preview/Commit/Cancel semantics without exhaustive field permutations.
4. **Canvas transaction:** focused Host/render tests prove one active Authoring producer or Game source, exact final composite receipt, current-generation pick/gesture, resize/retirement, and whole-owner GPU replacement.
5. **Editor package:** the private Editor package and affected requested-subset consumers compile with no public crate depending on `editor` and no new general Canvas/viewport abstraction.
6. **Product trajectory:** one serial real-project run proves open source → edit → Canvas → gesture → Undo/Redo → Save → successor project refresh → Reopen → explicit Game Restart over the same Prefab source.
7. **Native canary:** one local Editor launch proves a nonblank Canvas and one exact-generation selection/gesture. It is bounded native evidence, not a screenshot or platform matrix.
8. **Formatting:** inspect the final diff, then run `cargo fmt --all -- --check`; Cargo commands remain serial with `CARGO_BUILD_JOBS=1` for substantial gates.

### Evidence rules

- A Prefab instantiated as a Scene is preview data, never source-authoring or persistence evidence.
- A UI candidate or offscreen submission is not a visible Canvas. Selection/gesture evidence names the final composite receipt.
- A component/model test is not native Canvas or restarted Runtime evidence; records label each layer honestly.
- A Save receipt does not prove project-content refresh, and refresh does not prove a running Runtime changed. Each generation boundary is recorded separately.
- Reference Game is one heterogeneous consumer. No Reference Game-specific identity may become shared architecture.
- Do not manufacture confidence through opaque hashes, source scanners, or field/layout/platform Cartesian fixtures. Prefer exact structured identities and one representative failure per owner boundary.

---

## Definition of Done

### Global

- [ ] A path-bound UI Prefab opens as a true Prefab document with exact source, revision, history, selection, dirty state, and receipt-backed persistence.
- [ ] Existing Scene authoring, Scene View, Game View, Play isolation, and project workflow contracts remain valid without compatibility layers or a second authority.
- [ ] UI Hierarchy, typed Inspector, Canvas paint/pick, and overlays consume one root-private exact-revision `EditorUiAuthoringFrame`, one `nara_ui` projection payload, and one workspace selection owner without reversing crate dependencies.
- [ ] Nara UI, not egui layout, renders the accepted unsaved Prefab; Canvas input is enabled only by the matching final Editor composite receipt.
- [ ] Absolute pixel move/resize uses Begin/Preview/Commit/Cancel, creates one Undo record on Commit, zero on Cancel, and never rewrites flex/auto/percent resolved bounds.
- [ ] Save/Reopen uses the canonical Prefab writer and Host receipt; successful refresh publishes a successor immutable project-content generation, while failure retains last-good content and blocks dishonest Play/Restart.
- [ ] Runtime UI and Editor Canvas consume the same project-owned primary UI authored extent/safe-area profile; no Reference Game constant or project-window fallback supplies Canvas geometry.
- [ ] Successor refresh verifies the accepted Save digest, charges one aggregate budget host, and never retains more than two distinct project-content generations.
- [ ] Explicit Game Restart consumes the same saved Prefab source proven by the serial trajectory; no live edit or automatic runtime write-back was added.
- [ ] No Asset Browser, structural UI composition, instance override editor, Binding DSL, plugin registry, public Canvas/viewport framework, RenderGraph, docking, multi-window system, or toolkit convergence claim entered v1.
- [ ] ADR 0113, implementation status, OQ links, one completion record, and one completed engineering-memory registration agree on the final reviewed scope.

### Per-unit

- **EUA-U1:** true Prefab session/workspace/persistence owners and focused ADR are accepted; Scene regressions remain green.
- **EUA-U2:** immutable authoring frame, typed descriptors, layout ownership, source map, and continuous transaction are proven without egui/WGPU ownership.
- **EUA-U3:** private UI Canvas candidate, final composite receipt, exact pick/gesture facts, target retirement, and recovery are proven.
- **EUA-U4:** real egui source navigation, hierarchy, typed Inspector, Canvas interaction, draft invalidation, and native canary are proven.
- **EUA-U5:** Save → content refresh → explicit Restart consumes the same source; evidence and authority documents are reconciled.

---

## Sources and References

### Nara authority and landed patterns

- `STRATEGY.md`
- `docs/architecture/adr/0006-scene-and-prefab-data-model.md`
- `docs/architecture/adr/0015-editor-tooling-and-dogfooding-boundary.md`
- `docs/architecture/adr/0026-editor-command-patch-and-undo-model.md`
- `docs/architecture/adr/0047-editor-workspace-and-scene-document-state.md`
- `docs/architecture/adr/0101-ordered-persistent-placement-and-runtime-projection.md`
- `docs/architecture/adr/0104-host-owned-editor-workspace-installation.md`
- `docs/architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md`
- `docs/architecture/adr/0112-editor-game-view-runtime-presentation-and-input-authority.md`
- `docs/architecture/open-questions.md` (`OQ-010`)
- `docs/plans/2026-08-08-003-feat-editor-foundation-workflow-plan.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md`
- `docs/plans/2026-08-16-1440-feat-editor-scene-view-and-composition-plan.md`
- `docs/plans/2026-08-16-2157-feat-editor-game-view-v1-plan.md`
- `crates/nara_scene/src/{authoring.rs,patch.rs,prefab.rs,format.rs,provenance.rs}`
- `crates/nara_tooling/src/{workspace.rs,frame.rs,inspector.rs}`
- `crates/nara_ui/src/{codec.rs,runtime.rs,runtime/projection.rs}`
- `src/project_host/{persistence.rs,runtime/editor.rs}`
- `src/project_content.rs`
- `editor/src/platform/viewport.rs`

### Local comparative sources

- `repo-ref/godot/scene/gui/control.h`
- `repo-ref/godot/scene/gui/container.h`
- `repo-ref/godot/editor/scene/canvas_item_editor_plugin.h`
- `repo-ref/godot/editor/scene/gui/control_editor_plugin.cpp`
- `repo-ref/godot/editor/inspector/editor_inspector.h`
- `repo-ref/godot/editor/inspector/editor_properties.h`
- `repo-ref/godot/editor/editor_undo_redo_manager.h`
- `repo-ref/open-gpui/crates/canvas/src/{session.rs,store.rs,gesture.rs,transform.rs}`
- `repo-ref/open-gpui/crates/canvas/src/gpui/{frame.rs,view.rs,input.rs}`
- `repo-ref/open-gpui/crates/form/src/lens.rs`
- `repo-ref/open-gpui/crates/ui_components/src/form_adapter.rs`
- `repo-ref/egui/ARCHITECTURE.md`
- `repo-ref/egui/crates/egui/src/{response.rs,painter.rs}`
- `repo-ref/dear-imgui-rs/extensions/dear-imgui-reflect/src/{session.rs,response.rs}`
