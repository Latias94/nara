# Source-built Editor panels

Source-built panels can use the public, toolkit-independent `EditorPanelHost<P>`, `EditorPanel`,
`EditorPanelContext` and `EditorPanelSubmission` APIs. The Host provides semantic custody for
panel state, queued actions and exact dispatch observations around an existing `EditorApplication`.
The caller supplies drawing, platform integration and presentation. The Dear ImGui implementation
is documented separately below as the explicit reference frontend.

An authoring document owns accepted content, history and persistence. An editor panel presents or
edits that content: the Inspector, Project Browser and Scene View are panel kinds. A panel instance
is one open occurrence with its own target and presentation state; the workspace layout arranges
those instances across editor windows. Closing a panel does not close its document. Scene View
presents authored content, while Game View presents a running game's output.

Panels are trusted Rust code compiled into the consumer executable. Ordinary project Open does
not compile or load project code. These source APIs do not establish a stable dynamic plugin ABI.
For a domain-owned non-Scene document, see the [document extension guide](editor-documents.md).

## Toolkit-Independent Semantic Panels

The independently selected first-party source workspace is available as
`nara-editor-drawn`. Build/run it without the reference toolkit:

```text
cargo +1.95.0 run -p nara-editor --no-default-features --bin nara-editor-drawn -- <project-root> [--profile <profile>]
```

It consumes the same semantic/native panel interfaces described below. Its
[owning unit](../plans/2026-11-05-independent-drawn-source-workspace-plan.md) records actual
workflow qualification and remaining panels. Its initial workflow includes source opening,
document tabs, ordinary and specialized UI structure, Undo/Redo and Save/Reopen. Keep the
default reference for workflows not yet supplied by that composition; starting the successor
is not a default switch.
Its current engineering controls qualify functional workflows, not finished component-library
styling. The [visual prerequisite](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite)
separately covers typography, density, scoped styles, responsive sizing and visual/native checks.

The [styled workspace unit](../plans/2026-11-05-styled-source-workspace-plan.md) adds the original
complete property Inspector to that separate executable. Both frontends use the same retained
field drafts, component/reference/recursive controls and source-tool result handling. The drawn
Shell, Hierarchy and Inspector use the shared typography/appearance recipes documented in
[editable text](editable-text.md#scoped-appearance-and-measured-controls), with a regular primary
face, measured control groups and an anchored Document actions menu. Current source-built
composition inputs support density/text scale; this does not add a persisted user-preference UI.
The source workspace applies an Editor-local graphite/teal palette from
[Design Kit v0.4](../design/editor-reference/README.md). Encoded sRGB colors pass through the
existing linear-color conversion; generic widget defaults retain their separate appearance.
This palette adoption does not complete the design's compact workspace arrangement, typography,
icons, property catalogue or docking behavior.
The complete future component catalogue, Scene/Game and other panel breadth, docking, native
multi-window behavior and default switch retain their separate plans.

`EditorPanelHost<P>` retains typed per-instance state and one current submission candidate.
`EditorPanel` supplies the optional `action_finished` callback; it has no drawing method.
`host.prepare(&application, |panel, context| { ... })` gives the caller mutable access to its
concrete `P` and a scoped `EditorPanelContext<'_, '_>` for each visible registered instance, in
workspace order. The caller can update local controls, compose UI descriptions, draw through its
own toolkit, or perform a semantic operation there. Preparation does not require drawing.

The semantic Host owns no Application, Session, document, Undo history, window, GPU, input route
or drawing interface. The caller retains the original Application and drives it. Queries and
source preparation delegate to that Application and its existing Session/source owners; mutations
use the original action dispatcher. The reference frontend and semantic Host share the
candidate/submission queue, panel binding, original-slot filtering and exact result-resolution
implementation in [`editor/src/panels`](../../editor/src/panels). Each retains its own concrete
panel state and callback adapter.

### Select Features And Register Typed State

`nara-editor` currently enables `imgui-reference` by default. That feature selects the Dear
reference frontend, native composition/runner and Editor binary until a complete successor is
accepted. A custom Application/panel consumer disables defaults, as in the
[independent consumer manifest](../../tests/fixtures/editor-ui-host-consumer/Cargo.toml):

```toml
[dependencies]
nara = { path = "../../..", default-features = false, features = ["project-workflow", "serde", "tooling"] }
nara-editor = { path = "../../../editor", default-features = false }
```

These paths are relative to that fixture; adjust them for another workspace. This selection
keeps Application, workspace and semantic panel APIs available without enabling Dear packages or
requiring a Dear context. It does not supply a custom native shell. The consumer selects its UI
and rendering dependencies separately; another dependency enabling `imgui-reference` also enables
the reference frontend in the shared Cargo feature graph.

Register the descriptor and `Fn() -> P + 'static` factory together with
`host.register_panel(&mut application, descriptor, factory)`. Admission failure leaves the
workspace and factory registry unchanged. Empty or duplicate kinds, including built-in kinds,
are refused. Each visible instance gets a separate `P` on its first preparation. One Host stores
one concrete panel type; its registered kinds use factories returning that type. Neither factories
nor panel objects require `Send` or `Sync`.

The following example registers one Workspace-target instance and performs one explicitly
requested property replacement. The caller supplies a bounded `EditorPropertyRead` for an open
Scene/Prefab, the component's admitted version and the intended replacement. Keep the returned
Host for later cycles. The example uses `expect` for its stated preconditions; a UI should retain
read/preparation refusals and recoverable queue errors as local feedback.

```rust
use std::time::Duration;
use nara::{
    reflect::{ComponentSchemaVersion, ComponentValue},
    scene::AuthoringPropertyEdit,
    tooling::EditorPropertyRead,
};
use nara_editor::{
    EditorApplication, EditorPanel, EditorPanelHost, PanelActionDispatch, PanelSubmissionId,
    workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelTarget,
    },
};

#[derive(Default)]
struct PropertyPanel {
    pending: Option<PanelSubmissionId>,
    result: Option<PanelActionDispatch>,
}

impl EditorPanel for PropertyPanel {
    fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
        if self.pending == Some(id) {
            self.pending = None;
            self.result = Some(result);
        }
    }
}

fn install_and_apply(
    application: &mut EditorApplication,
    read: EditorPropertyRead,
    version: ComponentSchemaVersion,
    replacement: ComponentValue,
) -> EditorPanelHost<PropertyPanel> {
    let mut host = EditorPanelHost::new();
    host.register_panel(
        application,
        EditorPanelDescriptor::new(
            "consumer.property", "Property", EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Right, false,
        ),
        PropertyPanel::default,
    ).expect("unique panel kind");
    application.panel_workspace_mut()
        .open("consumer.property", EditorPanelTarget::Workspace)
        .expect("registered panel kind");

    let submission = host.prepare(application, |panel, context| {
        let observed = context.read_document_property(read.clone()).expect("current read basis");
        let target = observed.target;
        let edits = [AuthoringPropertyEdit {
            entity: target.entity,
            component: target.component,
            component_version: version,
            property: target.property,
            value: replacement.clone(),
        }];
        let prepared = context.prepare_authoring_property_edit(&target.basis, &edits)
            .expect("admitted source edit");
        panel.pending = Some(context.commit_authoring_edit(prepared)
            .expect("one action fits the cycle budget"));
    }).expect("published Application frame");
    host.submit_and_drive(application, submission, Duration::ZERO);
    host
}
```

The callback's stored `PanelActionDispatch` is the exact dispatch observation. For this edit, inspect
`Dispatched(EditorActionOutcome::Workspace(report))` and `report.applied`, including its rejection
details. Queue admission alone establishes no source change. This example performs no native
presentation and does not create another document or history owner.

### Query, Prepare, Queue And Dispatch

`context.model()` borrows the retained admitted frame and this instance's binding. It exposes
the instance, frame, resolved document and available selection/extension projections. A retained
frame does not freeze source state or turn an old observation into current mutation authority.
The finite context access delegates as follows:

| Context access | Original-owner operation |
|---|---|
| `document_query_basis`, `query_document_entities` | Capture an open document's exact basis and enumerate bounded direct source records. |
| `query_document_authoring`, `read_document_property` | Discover bounded component/root metadata and read a bounded selected property. |
| `observe_prefab_instance`, `prefab_composition_failure` | Observe stored Prefab intent or a verified composition failure at the exact basis. |
| `prepare_authoring_edit`, `prepare_authoring_property_edit`, `prepare_prefab_revert` | Prepare an unpublished source-owned edit and inverse. |
| `begin_authoring_construction`, `prepare_authoring_construction` | Build bounded construction intent and prepare its fixed local-to-source-ID mapping. |
| `source_tool_review` | Borrow the existing Session-owned review. |

Use the complete basis returned with the observation that informed the proposal. A named open
non-active document is accessible without changing tabs or selection. Direct reads/preparation
return synchronously and create no action callback. Retain a prepared edit/construction in `P`
for review, inspect its patch/inverse, or drop it to cancel before enqueue. Preparation, local
drafts and repeated UI composition publish no document, history or dirty change.

`context.submit(EditorAction)` queues an ordinary action; `submit_source_tool` queues the existing
finite source-tool action. `commit_authoring_edit` and `commit_authoring_construction` consume the
exact prepared candidate into the same ordered queue. Each admitted entry returns its own
`PanelSubmissionId`, an opaque observation identity scoped to that Host. Retain the returned ID
and match it in the original panel's callback; an ID alone cannot route work across Hosts or
successor instances. The shared per-cycle limit is 1,024 entries across panels. Immediate
`PanelSubmitError::Capacity` refusal creates no slot or completion callback; the consuming commit
methods return `(error, original_candidate)` so the caller can retain it for explicit retry.

`prepare` returns `None` when no Application frame is published. Otherwise its opaque, move-only
`EditorPanelSubmission` belongs to that exact Host and candidate. Pass it to the same Host's
`submit_and_drive(&mut application, submission, real_delta)`. That call reconciles live instances,
filters retired origins before dispatch, uses the existing Application dispatcher, drives one
owner pass, reconciles again, and then settles the original surviving callbacks. The returned
`EditorApplicationUpdate` describes drive/exit observations; action results arrive through
`EditorPanel::action_finished(id, dispatch)`.

`PanelActionDispatch::Dispatched` carries the complete `EditorActionOutcome`, including owner
rejections. `NotDispatched` explains why the action did not reach its owner, such as supersession,
closing filters or failed batch admission. Result slots retain their original identities when
an earlier action is removed: a surviving later panel cannot receive that earlier action's
outcome. A surviving original instance receives one result per admitted action when its candidate
is settled. The callback has no Context or drawing argument and cannot enqueue through that call.

Preparing another cycle supersedes the preceding candidate and settles its still-live original
entries with `NotDispatched(Superseded)`. Dropping a submission performs no Application work or
immediate callbacks; a later `prepare` can settle those pending observations as superseded.
Submitting a foreign or superseded candidate is idle: it neither drives the Application nor
settles the current candidate or another Host's callbacks. Direct Application submission/advance
also does not settle panel callbacks. A project generation change prevents the old candidate
from dispatching.

Ordered actions are not an atomic transaction. Two edits from one basis can accept the first
and reject the second as stale; use one source-owned atomic patch for a cohesive edit. Dispatch
acceptance of Save, Reopen, Play or a finite tool request still requires that owner's later
completion observation. Continue `application.advance(real_delta)` while work is pending, even
without another presentation; a submission already includes its own drive.

### Instance, Target And Presentation Lifetimes

Workspace owns panel kind/instance identity, targeting and layout preferences. The Host binds
each `P` to the exact instance lifetime, kind, target and resolved document:

- `ActiveDocument` follows the active document; a changed resolution retires its state.
- `Source { kind, locator }` resolves the named open source. Losing or changing that resolved
  document retires its state; a matching locator cannot revive the previous incarnation.
- `Workspace` has no resolved document. Use explicit query bases for tools working on named
  documents, including non-active ones, without tying their local state to active-tab changes.

Semantic target resolution is read-only. Submit an explicit navigation action when activation
is intended; reading or preparing an edit does not activate the target.

Movement, docking/placement, native-window relocation and render-view changes preserve compatible
panel state. For GPU reconstruction, keep the same semantic Host and its `P` objects while the
caller reconstructs graphics resources. Creating a new Host recreates its state. This continuity
does not certify that old input routes, presentation receipts or GPU resources remain valid.

Hide, close, an explicit target change or a changed resolved document retires the original
instance's local state. Layout reset/restore, including restoration of identical saved fields,
and project/workspace replacement retire the previous states; loss of a published frame clears
them as well. Reconciliation occurs at `prepare` and before/after current-candidate dispatch.
Reopening creates fresh state on its next preparation, even when the instance ID is reused.

Hide/reopen, close/reopen and target-away/target-back round trips between Host calls still retire
the original lifetime. Workspace records the transition independently of the final visible
fields. Matching IDs, labels, targets and visibility cannot revive the old `P` or queued action.
Retired origins are filtered before dispatch and receive no callback; replacement instances never
inherit their results. Retirement after source publication does not undo the accepted edit,
close the document or discard its history.

The caller separately owns input, focus/IME/clipboard/platform integration, popups, clipping,
refresh scheduling, rendering, exact presentation feedback and physical resource retirement.
Semantic lifetime reconciliation alone performs none of those native operations. A frontend
adapter must retire its old routes and pending presentation work and establish fresh admitted
presentation/input for a replacement. `EditorPanelHost` exposes no universal widget tree or
native attachment/service API.

### Independent Consumer And Verification Scope

The separate workspace at
[`tests/fixtures/editor-ui-host-consumer`](../../tests/fixtures/editor-ui-host-consumer/README.md)
disables `nara-editor` defaults and authors its own `DraftPanel` with reusable Nara controls,
local text history, focus and modal state. Its `tests/public_host.rs` contains semantic CPU
trajectories for bounded non-active reads, prepared edits, exact callbacks, Undo/Redo, Save/Reopen,
source preservation, relocation, retirement round trips, layout/project replacement,
foreign/superseded/dropped submissions, slot filtering and returned-candidate retry.

The optional `native` example `examples/panel_surface.rs` and `tests/native_panel.rs` exercise
that panel through public Winit/WGPU and UI presentation interfaces. The Windows/DX12 trajectory
passes modal/input behavior, source editing/persistence and retirement while presentation is
pending, including six actual presentations and five journal commits. The
[owning plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#toolkit-independent-panel-custody-prerequisite)
records results and limits. See the fixture README for the explicit build/run procedure.
CPU semantics or UI description compilation alone do not qualify native presentation or OS input.
This guide claims no full independent Editor, general toolkit compatibility, OS IME/accessibility
parity, extension hot reload or performance result from this prerequisite.

## Native Image Panel Attachment

`nara_editor::native::EditorPanelImageHost<S>` supplies a concrete single-window attachment for
frontends that publish complete retained images through the original `WgpuRenderHost`. `S` is
the caller's immutable captured presentation state. A Nara adapter may retain its `UiFrame`; a
user frontend may retain its own control geometry and text snapshot without constructing a
Nara widget tree, journal or interaction session. The semantic `EditorPanelHost<P>` remains the
only panel-state/action owner. The native Host creates no window, device, queue, renderer or
document owner.

Construct it with the caller's original `Arc<Window>`, logical `WindowId` and shared
`EditorClipboard`. The default clipboard uses the desktop text backend; `with_access` accepts
a Host-owned `EditorClipboardAccess` provider. Clones share one provider, preserve exact text
and errors, and refuse reentrant access. Tests may inject their own provider without replacing
the production system clipboard integration or writing to the user's clipboard.

During semantic preparation, capture `context.instance_token()`. It identifies that exact live
panel state, project, layout and binding. Relocation preserves it; hide/close/target changes,
layout/project replacement or releasing the original state invalidates it. Retaining a token or
image does not keep the original panel state alive. Attach it with the complete required
`EditorPanelCapability` set before staging an image. One native image Host binds one original
semantic Host: another Host's token or dispatch receiver refuses before routing or draining
pending cancellation. A composition may use its own concrete enum or trait-object panel type
for several provider implementations within that semantic owner.

Capture one `EditorPanelImage::new(token, display, Arc::new(snapshot), mapping, clip)` for each
visible native panel output. `DisplayMapping2d` describes the complete image in logical window
coordinates and supplies its inverse to target pixels. The explicit clip, actual window scale
and original image extent must agree. `with_window_modal(true)` makes that image a window-modal
provider. The Host admits at most 64 attachments/images, one complete image per semantic panel
instance in each candidate. Cropped UVs, direct rendering, out-of-panel overlays, native
detachment and accessibility are explicit unsupported capabilities of this first path.

Call `stage(&application, display_frame_index, images)` for one complete ordered window
candidate. Draw **its** exposed displays using **its** mappings and clips through the original
renderer, and record the original samples. Pass the opaque candidate and actual
`WgpuWindowFrameOutcome` to `presented`. Only a complete matching native outcome adopts new
input geometry. Submission, a copied image receipt, a foreign/superseded candidate, deferral or
an incomplete composite list cannot substitute successful presentation. The original renderer
retains all GPU submission and physical-retirement responsibilities. A Nara provider separately
returns and drains its original presenter outcome before using its journal for input.

Forward normalized native events with `EditorPanelPlatformEvent::from_window_event`, after
filtering the Winit window ID. `dispatch` receives the Application, original semantic Host,
optional event and a callback with `&mut P`, original `EditorPanelContext`, the accepted
`EditorPanelInput<S>` and scoped `EditorPanelServices`. Its `submission`, when present, returns
to that same semantic Host's `submit_and_drive`. Do not call another `prepare` from inside the
callback: cancellation and input already share one original ordered candidate. `dispatch(None)`
flushes cancellation caused by presentation, invalidation or retirement without another input.

The callback receives the **accepted** snapshot and exact endpoint. Repainting or mutating
desired state does not authorize its new hit geometry. Keep local focus/pressed/editing state
separate from captured geometry so an ordinary press and release can share a presentation.
Nara adapters reuse their content transform, field tokens, reducer and presenter. A custom
frontend owns its corresponding input validation rather than borrowing another control's token.

Pointer press establishes provider keyboard ownership; capture requires the matching pressed
button, and another button's release cannot end it. Captured motion may leave the clip/image
without clamping. Wheel values retain native Lines/Pixels and modifiers. `focus_panel` explicitly
targets an already presented keyboard provider, including one without Pointer capability. It
does not focus an OS window or select a local control. Its `KeyboardFocused` callback may choose
that control and request text input. A window-modal provider blocks another provider's focus.

Native text input uses `set_text_input(Some(scope))` with a retained
`EditorPanelTextInputScope` for each local text-focus lifetime. Keep the same scope while that
focus survives; use a fresh scope when the field/token changes, including within the same
panel. Delivered Text/IME carries its original scope. `None` disables input, and IME preedit and
commit wait for native readiness. Submit the caret in accepted image target pixels through
`set_ime_area`. The Host maps and clips a nonzero exclusion area to visible content; a fully
invisible area refuses. A zero-width caret must already be inside the visible interval and
is never moved to the clip edge. Scope replacement, native focus loss and retirement reset
native IME state.

Services also provide explicit clipboard errors, cursor selection, immediate redraw demand and
earliest per-panel deadlines. `next_deadline` and `poll_deadlines` let the caller's original
event loop coalesce work without polling static panels. Declared capabilities and current
keyboard/pointer ownership are checked for each request. Input locking, resize/scale, changed
placement, omitted output and retirement cancel before subsequent input. `invalidate` reports
backend/window invalidation; `close` withdraws admission and native services. Flush cancellation
before releasing frontend state, then physically retire graphics through the original GPU Host.

The [independent consumer](../../tests/fixtures/editor-ui-host-consumer/README.md) contains both
the Nara presenter adapter and a separately authored software-painted conformance frontend.
Their [owning unit](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#image-backed-native-panel-attachment)
records actual qualification. The software renderer is a finite extension consumer, with
explicit CPU raster/upload/copy work; it supplies no performance recommendation. This attachment
does not establish named-toolkit compatibility, OS IME/accessibility parity, multi-window
hosting, code reload or the complete self-drawn Editor.

### Observe And Operate Presented Controls

`EditorPanelImageHost::observe(&application)` returns opaque `EditorPanelImageObservation<S>`
values for its currently accepted images, in paint order. Each retains the original endpoint,
provider snapshot, mapping/clip and native geometry. The token exposes its captured `kind()` and
`document()` so equal-label controls can be distinguished by exact panel/document scope.
`window_bounds(target_pixel_bounds)` maps visible bounds into logical window coordinates; it
does not grant pointer access through an overlapping image or modal provider.

Use `EditorPanelHost::inspect(&application, token, callback)` to read the exact live panel and
current model without preparing actions. It refuses foreign or retired state and does not
instantiate a panel, reconcile it or supersede a pending submission. Keep observation separate
from `prepare`, which intentionally replaces the previous action candidate.

`dispatch_observed` routes a normalized platform event through the original native router after
checking the observed image and geometry. Pointer input retains actual occlusion and capture;
a modal's outside click still reaches that modal's dismissal path. Keyboard/text requires the
observed keyboard endpoint and exact current text scope. Observe again after focus changes.
An IME `Disabled` acknowledgment may finish a retired scope through an observation without a
current text scope; old preedit and commit still cannot enter a successor field. Window focus,
resize and other lifecycle events use ordinary `dispatch`.

A provider declaring `ObservedOperations` plus its required `Keyboard` capability can use
`invoke_observed` for a typed semantic request captured by its callback. This capability grants
neither native accessibility nor a common widget vocabulary. The callback may first receive
ordinary cancellation deliveries; execute the captured request only for `ObservedOperation`.
Resolve its original control, owned behavior action, enabled/inert state and current draft
through the provider's existing reducer. Report unsupported or stale requests explicitly.
Nara adapters use `UiSemanticSnapshot` and `UiAccessibilityInput`; other frontends retain their
own control/snapshot types.

Return `outcome.submission` to the same semantic Host. Its borrowing `submissions()` iterator
exposes original `(EditorPanelInstanceId, PanelSubmissionId)` pairs in ledger order, including
entries whose action slots may later be filtered. It transfers no completion ownership: the
panel's original `action_finished` callback still receives each result. Native admission,
provider acceptance, exact action result, persistence terminal and a later matching presentation
are distinct observations. Wait for the named result with a timeout rather than global idle.

An observation can survive an unrelated panel repaint when its own output remains accepted.
Replacing its output, snapshot, mapping, geometry or instance refuses instead of redirecting
input. The original attachment tracks one nominal lifetime for its continuously accepted exact
image. Restoring an earlier image or mapping after withdrawal starts a new lifetime; it cannot
revive an earlier observation, even when output receipts and snapshot allocations match again.
Retained observations do not keep semantic panel state alive. The
[operations unit](../plans/2026-11-05-presented-ui-operations-plan.md) records actual qualification
and remaining agent/platform scope.

## ImGui Reference Frontend

The following controls, `ImguiEditorPanel` examples and `EditorNativeComposition` embedding apply
to the explicitly selected `imgui-reference` frontend. Its current shell includes hosted
Nara-drawn panels. The
[independent frontend development route](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#independent-reference-and-self-drawn-editor-implementations)
builds the self-drawn Editor with its own shell beside this reference, then switches/removes the
reference after complete acceptance. The reference's current composition does not prescribe a
permanent mixed-toolkit architecture for the independent frontend. Optional product embedding
remains governed by its separate Host contract.

### Built-In Reference Panels

The built-in ordinary Scene View uses Nara-drawn Reset View, status and viewport controls.
Scroll over the visible Scene image to zoom around the pointer, drag with the middle mouse
button to pan, and use Reset View to restore the full image. Picking is available only after
the exact image and panel have been presented and the retained Scene is the active document.
Letterboxing, clipped regions and stale source or placement mappings do not authorize picks.
The authoring Canvas retains its separate controls and presentation priority for UI documents.

The existing native Host renders the Nara panel and directly samples the retained Scene image
in the workspace shell's drawing order. Scene pixels are not read back or uploaded into a CPU
UI image. The shell still owns docking and native-window placement; Nara owns the Scene panel's
layout and interactions. This does not qualify a complete Nara replacement for every Editor panel.

The ordinary Scene/Prefab Hierarchy uses Nara controls to list source declarations and their
authored placement intent. It does not infer an effective parent or sibling order. Use + Root
and + Child to create declarations, Parent and Order to choose explicit placement, and Remove
Entity followed by confirmation to remove a source-local branch. Unavailable deletion explains
its source/readiness restriction. These actions use the document's existing history and Save/Reopen.

Filter entities matches committed entity IDs and names. Composition remains local to the filter;
typing, clearing and scrolling do not select another entity or publish a document edit. Clear
filter restores the source list; Show selection reveals the Session's current entity. The panel
accepts up to 1,024 UTF-8 bytes of filter text; an oversized edit preserves the previous buffer.
Long source identifiers use shortened display labels while commands retain their complete IDs.
The panel builds controls for its visible source/choice rows and retains their source identities separately
from placement. Inspector drafts remain owned by their original fields when filtering hides the
selected row. Parent/order/removal choices retain their displayed source basis and cannot
retarget after another entity or document is selected.

For a UI Prefab, the Hierarchy uses the specialized UI projection's exact source topology.
Select a UI row, then use Add UI node, Move subtree or Delete subtree. Creation offers Container,
Text, Image and Button. Choose placement lists the owner's legal parent/sibling positions;
Image creation requires a current project image and Button creation requires a valid Action.
Text and label input remain local until Apply structure change publishes one document edit.
Cancel structure change discards the form; Escape closes an open chooser before cancelling its
form. Protected roots and subtrees with omitted source records explain their read-only status.
Source, selection, Schema, image-catalog or permission changes retire the original form. A refused
or undispatched operation reports its original result and permits explicit retry when its basis
is still current. Scrolling builds only visible tree and choice controls and does not alter
selection or document history. The specialized Inspector and Canvas keep their separate controls
and original continuous-gesture/geometric admission.

The Inspector's default `Document` tab uses Nara-drawn controls for ordinary Scene and Prefab
documents. Use Add Component to inspect the admitted creation choices, then Add with the
component's stable ID. Existing or unavailable
components have disabled choices and an explanation. Expand/Collapse preserves compatible
unapplied field drafts and local text Undo; hidden fields lose focus and composition. Remove
uses the same document transaction and Undo history as ordinary field edits. Dirty drafts from
a removed component remain available through retained-input recovery. Read-only or mismatched
component versions cannot be removed. Component actions retain the displayed document, Schema,
entity and revision; an intervening source change requires a new observation. Entity navigation
uses the Nara Hierarchy. A selected Prefab anchor exposes Open Prefab Source in the Nara
Inspector, bound to the displayed document, source revision and anchor. Authored placement intent
is shown above component controls. The Source asset and Prefab Intent tabs, specialized UI
authoring and source-built custom panels retain their separate frontends.

### Register A Reference Panel

With `imgui-reference` enabled, register a panel descriptor and its Dear ImGui state factory
together through `EditorNativeComposition`:

```rust,ignore
use nara_editor::{
    EditorLaunchOptions, EditorNativeComposition, EditorRunError,
    ImguiEditorPanel, ImguiEditorPanelContext,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};

#[derive(Default)]
struct NotesPanel {
    draft: String,
}

impl ImguiEditorPanel for NotesPanel {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        ui.input_text("Draft note", &mut self.draft).build();
        if let Some(document) = context.model().document() {
            ui.text(&document.title);
        }
    }
}

fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let mut composition = EditorNativeComposition::new();
    composition.register_panel(
        EditorPanelDescriptor::new(
            "my-game.notes", "Notes", EditorPanelMultiplicity::Multiple,
            EditorPanelPlacement::Bottom, true,
        ),
        NotesPanel::default,
    )?;
    composition.run(options)
}
```

The factory creates a separate object for each painted panel instance. Empty identities,
duplicate identities and attempts to replace built-in panel kinds are rejected. Embedded shells
consume `composition.build(options)` to receive the fully configured `(application, frontend)`;
pass both to `EditorNativeHost::from_application` for the complete hosted workspace. The native
runner uses that same owner. For panel-only attachment to an existing application,
use `composition.install_panels(&mut application)`. It rejects selected Host contributions,
which require complete construction, before changing the workspace. A panel admission failure
also preserves the complete workspace.

The UI thread owns factories and panel objects, so they do not require `Send` or `Sync`. A panel
receives read-only document/selection projections. Keeping a draft in a panel does not dirty a
document or publish an undo entry.

The document extension guide's independent dialogue consumer uses the same panel retirement and
action paths while the Host retains history/dirty projections, Save/Reopen and document close
decisions.

### Edit A Persistent Field

Use the reference frontend's field helper when the panel should edit a field from the selected
entity's published Inspector projection. This `edit_field` drawing helper belongs to
`ImguiEditorPanelContext`; it is not a method on the semantic `EditorPanelContext`:

```rust,ignore
use nara::reflect::{ComponentFieldId, ComponentTypeId};

let selected = context.model().selection().and_then(|selection| selection.top_entity()).cloned();
if let Some(entity) = selected {
    context.edit_field(
        ui,
        &entity,
        &ComponentTypeId::new("nara.scene.Name"),
        &ComponentFieldId::new("value"),
    );
}
```

The helper owns its draft, validation feedback, pending submission and exact owner result. Invalid
text remains editable. A changed document revision makes an existing draft conflict; review and
reapply explicitly against the current value. Unrelated accepted edits do not reset widget
identity or silently discard text. Incompatible source/schema changes keep recoverable text but
do not reinterpret it as the new field. A temporarily unselected field or collapsed panel retains
its draft. The private finite draft budget rejects new controls before admitting input rather than
evicting dirty drafts. The current helper does not query arbitrary unselected entities or add new
component constraints.

### Submit A Custom Action

For reference-panel tools, call `context.submit(EditorAction)` and retain the returned
`PanelSubmissionId`. Queue admission can fail with `PanelSubmitError::Capacity`; an admitted
action has not yet been accepted by its owner. Implement the optional callback to settle exactly
that request:

```rust,ignore
fn action_finished(&mut self, id: PanelSubmissionId, result: PanelActionDispatch) {
    if self.pending == Some(id) {
        self.pending = None;
        self.status = format!("Dispatch: {result:?}");
    }
}
```

`PanelActionDispatch::Dispatched` contains the existing typed `EditorActionOutcome`, including
owner rejections. `NotDispatched` explains supersession, closing/filtering, stale batch or missing
frame admission. A surviving original panel receives one result after application drive and
lifetime reconciliation. Retired instances are dropped; their successors never receive old
results. The default field helper consumes its own results privately and does not send them to
the panel's custom-action callback. Multiple actions can be submitted in one paint, but an ordered
batch is not an atomic transaction. Two writes with the same document basis can accept the first
and reject the second. Submit one existing atomic patch for a cohesive multi-field operation.

The callback has no UI/context argument and cannot submit another action. Dispatch acceptance of
Save does not mean disk publication completed; read the later owner projection for that state.
While `EditorPersistenceOperation::Saving` is visible, continue driving the application and
inspect its eventual `EditorPersistenceResult`. Later edits remain possible and stay dirty when
an older captured revision finishes saving. Keep Close/Exit and project replacement with the
application lifecycle owner; a panel must not infer completion from dispatch or remove the
document while its Save is pending.
Reopen also completes across frames through `EditorPersistenceOperation::Opening`. Drive and
observe its eventual Opened/rejection result. Opening disables ordinary authoring controls and
new Play requests. If another valid consumer edits the document before publication, the owner
rejects the stale read rather than discarding that later edit. A completed read does not grant a
panel permission to replace a document, and a task's cancellation is not an Opened result.

Project Browser Open uses a separate request-specific projection. Dispatching
`EditorProjectBrowserAction::Open` returns `Preparing { request }` while source reading and
validation run. Observe `frame.document_open()` across drives; preparation can complete before
the first subsequent frame. The terminal `EditorDocumentOpenResult` reports the exact request's
Opened, Rejected or Cancelled outcome. Submit `AcknowledgeOpen { request }` to release that result
before another Open. An already bound source returns Activated directly.

During preparation, `CancelOpen { request }` revokes that exact request's publication interest.
`OpenCancellationRequested` does not mean its filesystem call has physically returned; the
Cancelling projection keeps its work slot until return. Opening a distinct Scene/Prefab adds its
own document without closing an existing same-kind document. Use explicit Close for that
document's Save/Discard/Cancel. Refresh and selection can continue; changed target index/source
admission rejects a stale candidate, while editing an unrelated document does not.
Copy the opaque request ID from the public result/projection; its numeric sequence alone is not
request authority. Exact cancellation and acknowledgement remain valid after unrelated frame
advances and cannot affect another Editor session. Keep driving pending work even when a panel
does not request another presentation.

Contribution-only shells use `frontend.show_contributions(...)`, which draws the shell and
source-built panels without the hosted Hierarchy, Inspector or Scene panels. Game controls retain their
state and notices without an attached Runtime image. Pass its opaque move-only
candidate to that
same frontend's `submit_and_drive(...)`. It cannot be cloned or converted to a plain action batch.
Painting a newer candidate supersedes unfinished observations from the previous cycle; dropping
a candidate performs no application work. The ordinary application `EditorActionBatch` entry
remains available for callers that do not need panel result routing.

### Embed The Reference Workspace

For a complete embedded reference Editor, keep the original native owner instead of drawing a
workspace whose Nara image candidates have no renderer. The caller creates the Winit loop and
may supply its primary window before the first `resumed` callback:

```rust,ignore
let event_loop = winit::event_loop::EventLoop::<nara_editor::EditorNativeEvent>::with_user_event()
    .build()?;
let proxy = event_loop.create_proxy();
let (application, frontend) = composition.build(options)?;
let mut host = nara_editor::EditorNativeHost::from_application(
    application,
    frontend,
    None,
    move || proxy.send_event(nara_editor::EditorNativeEvent::SupervisedPlayWake).is_ok(),
);
let result = event_loop.run_app(&mut host);
host.finish(result)?;
```

`Some(Arc<Window>)` uses the caller's existing primary window. The same owner supplies native
image presentation, exact input admission, GPU recovery and physical retirement. A forwarding
application must deliver all Editor primary/secondary window events, wake events, `resumed` and
`about_to_wait`; the Editor owns loop scheduling and ordinary close/exit decisions. This is not
an independently scheduled child viewport inside another GUI runtime. Pass the loop result to
`finish` before propagating errors so terminal cleanup retains the original failure. Finish
does not replace dirty-document decisions that must settle while the loop is alive. The former
unattached `show` entry is removed; it cannot present a complete Nara workspace.

Direct application submissions return an owned `EditorApplicationSubmission`. Inspect `actions()`
for one dispatch observation per original action, including explicit Closing, WorkspaceUnavailable
or BatchRejected results for actions that never reached a session. An actual dispatched action
retains its full `EditorActionOutcome`, including domain rejection. `update()` separately reports
the subsequent drive's observation/exit flags. `actions_observed()` is not a success verdict.

```rust,ignore
let submission = application.submit_and_drive(batch, Duration::ZERO);
for action in submission.actions() {
    observe_dispatch(action);
}
if submission.update().exit_ready() {
    // The application has completed the requested process-exit boundary.
}
```

The returned observations stay associated with that call across later drives and do not settle
panel callbacks. Save/Play request admission still needs its owning asynchronous completion
observation; an Accepted request can later fail. Ordered actions are not an atomic transaction.

For ordinary progress without actions, call `application.advance(real_delta)`. Each call advances
one bounded owner pass through initial Open, a retained project replacement, the published
workspace or physical retirement. Zero delta still performs maintenance. A pass drives at most
one Session frame and does not drive a successor it just published. Action submissions already
include their own drive; use `advance` at the next maintenance boundary, not as an extra step
after each submission.

After requesting process Exit, keep calling `advance` until an update reports `exit_ready()`.
Resolve any ordinary dirty-document decision by submitting against the current frame revision.
Workspace retirement can remove `current_frame()` before the retained Open Host has physically
joined its facilities; `advance` continues that same request without a frame. Frame absence, an
accepted Exit action and a completed Session are not process-exit completion. A poll-only driver
must not issue another Exit to hide a rejected original request.

For Save/Reopen, retain the admitted `EditorPersistenceAttemptId` and match the later Pending or
Terminal observation to that attempt. Dismiss a terminal with the identity actually displayed,
not a newly sampled latest result. `Refused` admission publishes nothing; an immediate published
`Rejected` has its own attempt. See [persistence observation](editor-documents.md#save-reopen-and-close)
for the exact acknowledgment and retry rules shared with window-free consumers.

The reference frontend follows the same instance-retirement rules as the semantic Host above.
Panel state survives docking, movement, relocation and native GPU reconstruction. Closing or
hiding a panel retires that instance's drafts without closing its document. Reopening starts
fresh state. Changing the panel target or its resolved document also retires drafts, including a
source target that becomes unavailable. A fresh project/workspace and explicit layout
reset/restore retire all panel drafts. Hide/reopen or target round trips between calls also
retire the original state. Replacing a GPU owner moves the retained frontend and its state;
it does not rerun composition or recreate surviving panel objects.

The separately compiled consumer at
`tests/fixtures/module-consumer/editor-panel-consumer` contains an ordinary native runner and a
selection-summary panel with a local note draft. This is the ImGui reference-panel consumer;
the separate semantic Host consumer is described above. Neither establishes a dynamic plugin ABI.

## A Source-Local Snap Tool

The reference-panel consumer's `snap_local_position` module contributes a useful command without
changing engine APIs. Call `snap_local_position::install(&mut composition)` before building/running
the composition; its companion `run(options)` already includes this registration. Select one direct
Scene source entity with `nara.transform.Transform2d`, set the grid spacing, and choose **Preview**.
The panel shows the before/after coordinates and the Source owner's exact forward/inverse candidate.
**Apply** consumes that candidate; **Cancel** drops it without a source or history change.

This is a local-position tool, not a world-space transform operation. Its origin is local zero,
including for parented entities. The default grid is 16 local units; any positive finite `f64`
spacing is accepted. Rounding compares remainder distances, with exact binary ties away from zero.
Input and output must fit the Transform codec's finite `f32` range, but stored values remain `f64`:
there is no additional coordinate cap or hidden `f32` round trip. A no-op creates no candidate.
Both coordinates must be explicitly present F64 values; omitted/defaulted fields are refused.

`translation.x` and `translation.y` are dotted stable **root IDs**, not a `translation` root with
Member steps. Each read admits one scalar/eight value bytes and explicitly bounds selector scan
work to 64 entries/4096 logical bytes. A budget refusal is a tool error, not permission to copy
the complete Inspector or component. The pure planner checks the two observations' exact target
and basis; it neither checks current liveness nor mutates a document.

The panel and window-free Session use the same complete-observation preparation contract:
`prepare_authoring_property_edit(proposal.basis(), &proposal.property_edits())`,
then consume the prepared edit through `commit_authoring_edit`. Only changed axes enter one
Source-owned transaction and one Undo entry. No adapter constructs its own inverse or history.
An intervening edit rejects the stale candidate rather than silently rebasing it. The panel retains
the exact originating `PanelSubmissionId` and callback; a later unrelated drive cannot replace it.
Selection or grid changes require a new preview.

The independent `tests/snap_local_position.rs` exercises the real registered Dear ImGui panel and
a separate inactive-document Session path through Undo/Redo and exact Save/Reopen attempts. Its
panel test uses Dear ImGui input and Application routing without a native window/GPU; it is not
native rendering qualification. This source-built command does not establish inline Inspector
customization, child-process isolation, install/update/disable/remove, or a stable tool protocol.

### Explicit Finite Child Experiment

The independently compiled consumer also contains a default-off `snap-child-experiment` feature.
It selects a trusted fixture helper explicitly; opening project data and running the ordinary
companion do not launch it. Its window-free driver keeps target/basis and Source ownership in the
parent while the helper invokes the same pure numeric calculation in another process. The parent
does not compute the expected answer and ask the child to echo it.

`SnapInput::capture` validates coherent observations and finite tool input without checking current
source liveness. Its `proposal(after)` attaches a trusted placement's finite coordinates to the
retained target/basis; it does not certify that an arbitrary helper computed the right answer.
The ordinary `plan_snap_local_position` now composes capture, local calculation and attachment.
For the child placement, validate the original basis with the Session before preparing and previewing
the same Source-owned edit. Reopen or intervening edits do not silently acquire a new basis.

The private exchange admits one 40-byte request, one 32-byte response followed by EOF, and at most
4096 stderr bytes. Extra bytes, malformed/version-mismatched frames, wrong request, invalid values,
nonzero exit and I/O failure refuse the proposal. No raw stderr becomes a public diagnostic. A
successful frame alone is insufficient: the driver retains custody through exit/reap and joined
stdin/stdout/stderr workers. Cancellation revokes interest without overwriting an already observed
failure; it is not proof of physical completion. A joined worker panic remains a distinct failure.
One lane stays Busy until its physical result has returned and its terminal observation is consumed.

The helper is selected by Cargo's absolute executable path, without a shell or project command
discovery. It has a reviewed empty/allowlisted environment and an owned fixture CWD. This is trusted
direct-child execution: the helper does not create descendants. Portable blocking std I/O does not
guarantee bounded cleanup if that premise is violated. The fixture reports/retains incomplete cleanup;
it does not claim sandboxing, child memory/CPU limits, arbitrary process-tree retirement or a native
panel tool Host. A retired panel must not become the physical process owner by accident.

Run the focused public workflows explicitly, serially with the repository's pinned Rust toolchain:

```text
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/module-consumer/Cargo.toml --target-dir target -p nara-editor-panel-module-consumer --features snap-child-experiment --test snap_child_proposal --test snap_local_position --no-fail-fast --test-threads=1
```

The [experiment study](../research/2026-10-25-finite-snap-child-proposal.md) records alternatives and
remaining qualification limits. This finite consumer does not select R1b placement, local package
installation, an inline Inspector extension, or a reusable process/wire API.

## Prepare A Direct Document Edit

Direct Application consumers can preview an edit of an explicitly named open Scene or Prefab
without changing its tab or selection:

```rust,ignore
let basis = application.document_query_basis(document)?;
let prepared = application.prepare_authoring_edit(&basis, &patch)?;
preview_changes(prepared.patch(), prepared.inverse());
let submission = application.commit_authoring_edit(prepared, Duration::ZERO);
observe_dispatch(submission.actions());
```

`PreparedEditorDocumentEdit` is move-only. The document owner retains the exact candidate and
inverse inspected by the caller. Dropping the value publishes nothing. Commit rechecks the
original document/session, revision and Schema admission; an edit, Undo/Redo, Reopen or project
replacement cannot silently rebase it. Specialized UI Prefab structure still requires its domain
contract. The result uses the existing owned Application submission, not a result ticket.
Preparation itself validates that complete original observation, including exact Schema admission,
before interpreting intent. A freshly captured basis cannot replace the observation underlying an
older proposal. Tool publication and request identity remain the concrete tool owner's obligations.

Mixed construction admits one bounded ordered patch: create empty direct entities, add admitted
component templates and edit their fields/references. Final classification and forward/inverse
history admission precede publication. One accepted patch is one Undo/Redo entry. A valid forward
value is insufficient if its inverse cannot be replayed under the same field limits.
`MAX_AUTHORING_EDIT_OPERATIONS` bounds mixed operations; `MAX_AUTHORING_FIELD_WRITES` and the
existing field-value limits remain separate. This is not cross-document atomicity or atomic
multi-file Save.

Use `ScenePatchOperation::SetPrefabInstance { entity, expected, value }` to attach, replace or
detach a direct source entity's instance declaration through this same preparation path. Capture
the complete optional previous declaration in `expected`; an equal lookup with different overrides
does not match. `value: None` detaches only the declaration and keeps local components, children
and placement. It does not convert expanded objects to local records. The owner creates the exact
inverse and charges both snapshots to replay/history limits. Unavailable override records remain
protected by the ordinary preservation check. Preparing a declaration does not resolve its source,
qualify nested preview or activate Runtime data. Native snapshot loading requires current Schema
semantics without silently migrating either comparison side.

For removal of stored override intent, use the source-owned Revert port instead of constructing
replacement declarations in the tool:

```rust,ignore
let basis = application.document_query_basis(document)?;
let path = PrefabInstancePath::new(anchor);
let limits = PrefabDeclarationLimits::default();
let observation = application.observe_prefab_instance(&basis, &path, limits)?;
let prepared = application.prepare_prefab_revert(
    &basis,
    &observation.path,
    PrefabRevert::Intent(PrefabOverlaySlot::Component { index }),
    limits,
)?;
preview_changes(prepared.patch(), prepared.inverse());
let submission = application.commit_authoring_edit(prepared, Duration::ZERO);
```

The reader observes source declarations without requiring entity selection, inherited targets or
successful preview. Nested addition and instance-claim paths refer to indices in this exact
snapshot. The complete query basis fences source incarnation, revision and Schema admission;
requery after any change rather than transplanting old indices. Scene/Prefab sessions provide
the same bounded primitives, with their caller responsible for validating retained Schema basis.

Revert removes only the selected intent and preserves unknown siblings. Removing the last property
claim removes its empty Properties container but retains the overlay's base pin.
`PrefabRevert::Overlay` removes the overlay and its base pin while keeping the source link.
Removing an instance claim restores inheritance; a stored `None` claim instead explicitly detaches.
These operations do not convert effective entities to local records or implicitly remove dependent
placements/references. Composition can still reject after a valid source repair.

The built-in Inspector's `Prefab Intent` tab uses the same read, prepare, review, commit and result
ports. A verified `prefab_composition_failure(&basis)` reports the first actual failure, with a source
location and slot only where established. `None` is not proof that every declaration resolved.
A failure owned by another canonical Prefab requires that Prefab's own open-session basis.
Budget/loading/authority refusals cannot authorize a guessed repair target. Preserved source can
still Save/Reopen when composition rejects, and a rejected analysis supplies no native input stamp.
Compilation reports expose `SceneCompilationFailure`; its `reason()` returns
`SceneCompilationFailureKind`, covering source admission and work limits as well as invalid intent.
The optional location describes an established source declaration, not mutation authority.

### Convert A Complete Instance

The integrated Authoring Host accepts `EditorPrefabConversionAction::Prepare { basis, anchor }`
for a direct source anchor in an admitted Scene or Prefab. Use the complete basis from
`document_query_basis(document)`. The built-in `Prefab Intent` tab labels this operation
**Convert Entire Instance to Local**: nested instances and overlay additions in the selected
projection also become local. The anchor's own records and ordinary local children retain
their identities; other instances remain linked. Selected unavailable data refuses conversion.

Prepare returns dispatch acceptance with an operation ID. Observe the same ID through
`frame.prefab_conversion()` or `session.prefab_conversion_view()`. `Preparing` means actual
source acquisition and preparation are pending. `Review` exposes a bounded page of the fixed
origin-to-local-ID mapping, the original document basis and actual source counts.
The Inspector uses fixed-height local-ID rows; select one to inspect its complete source
breadcrumb in a separate scrollable detail. This changes panel focus only. The breadcrumb is
display text; the retained source token and successful receipt carry typed identity.
`ReviewPage { operation, offset }` reads that retained mapping without preparing again.
Navigation and paints do not retarget the operation.

`Commit { operation }` starts fresh file, metadata, Schema and stable-lookup validation.
During `Validating`, no conversion has been published. The Host also checks the current
root and each actually used file or working copy before committing the original source
token through Workspace. A changed participating source rejects that candidate; the caller
must explicitly prepare another operation. Unrelated document edits do not invalidate it.

Only `Committed { operation, receipt }` establishes source publication. The receipt contains
the actual accepted mapping and document report. Ordinary Undo/Redo and Save/Reopen continue
through that source document's existing owners; there is no conversion-specific history.
`Cancel { operation }` works during preparation, review or validation. Pending cancellation
waits for the worker's physical return before reporting `Cancelled`. After any terminal
result, send `Acknowledge { operation }` to release the single operation slot.
Dispatch refusal and terminal failure remain separate observations.

Raw mixed patches use explicit persistent `SceneEntityId` values and declared component templates;
use [retained construction](#construct-with-transaction-local-references) for source-owned
transaction-local allocation and references. Schema-relative nested replacements have their separate
[property-edit interface](persistent-components.md#stable-nested-properties). Panels can submit
ordinary atomic patches through the existing mutation action; direct Application clients use the retained
candidate interface above. Registered source-built panels can retain the same candidates through
the finite context methods below. No compatibility wrapper or second Undo authority is required.

## Prepare And Retain An Edit In A Panel

Both the semantic `EditorPanelContext` and reference `ImguiEditorPanelContext` expose these
finite preparation and consuming queue methods. Prepare from the complete basis returned with
a document query, not a newly sampled revision. The context borrows only finite read-only
preparation access; it exposes no mutable Application, Session, source document or persistence Host:

```rust,ignore
let prepared = context.prepare_authoring_property_edit(&observation.target.basis, &edits)?;
preview_changes(prepared.patch(), prepared.inverse());
self.prepared = Some(prepared);
```

`prepare_authoring_edit(&basis, &patch)` supports an existing atomic patch.
`begin_authoring_construction(&basis)` and
`prepare_authoring_construction(&basis, &draft)` reuse the existing source-owned draft and
fixed local-reference mapping. The source owners retain validation, inverse and history authority.
Preparing, inspecting or retaining a candidate changes no document, selection or dirty state.
Drop it to cancel before submission; changing tool inputs should replace the candidate rather
than edit it as a second document.

When the user accepts, consume that same candidate into the current Host/frontend submission:

```rust,ignore
if let Some(prepared) = self.prepared.take() {
    match context.commit_authoring_edit(prepared) {
        Ok(id) => self.pending = Some(id),
        Err((error, prepared)) => {
            self.prepared = Some(prepared);
            show_queue_refusal(error);
        }
    }
}
```

`commit_authoring_construction` similarly returns an unqueued candidate on refusal. A refused
entry has no completion callback. Successful enqueue transfers ownership to the existing ordered
action route; it does not apply during `prepare`/paint and is not a promise of cancellability
afterward. Use `action_finished` and the exact `PanelSubmissionId`. The successful construction
outcome is `EditorActionOutcome::AuthoringConstruction`, containing the actual owner report and mapping;
failed commit retains its ordinary owner rejection. Never infer success from selection or dirty.

Candidate-bearing `EditorAction` and `EditorActionBatch` values are move-only. Historical outcomes
remain owned observations. A commit-only batch uses source-candidate admission; mixing navigation
or other frame-sensitive work retains whole-batch frame admission. Two candidates prepared from
the same basis can accept then reject stale, and several actions are not one transaction.
Existing supersede, origin retirement and project-generation rules still apply. A panel that
retires after publication cannot roll the source back.

Use a Workspace-target panel to operate on a named non-active document. The reference frontend's
existing Source-target activation behavior is unchanged. The
[bounded panel design](../architecture/editor-prepared-panel-design.md) records alternatives and
qualification scope; this does not implement inline Inspector tools,
dynamic installation, asynchronous preparation or a second preview/history service.

## Create A Source Entity

Ordinary `ComposeScene` and `ComposePrefab` creation commands carry placement and the supported
recipe, not a predicted entity ID. The Scene/Prefab owner generates the ID, validates the complete
candidate and publishes one history entry. The Workspace selects the created entity only after
success; Prefab creation also returns `PrefabCompositionOutcome::Created { entity }`. Read those
actual results instead of constructing names such as `editor_entity_1`.

Direct native Scene/Prefab sessions use `create_entity_with_registry`, or
`create_entity_with_asset_resolution` with an `AssetReferenceResolver` from the actual catalogue
and admitted alias scope when their codecs/references require project asset admission.
They receive `CreatedSourceEntity` with the exact stored ID and ordinary patch report. Data-only
Published sessions use `create_authoring_entity` for empty creation; component templates retain
their existing admission policy. Raw native component records are not a Published escape hatch.

Generated identities are opaque source data, not display names. They contain fresh 128-bit entropy
in 22 URL-safe characters; existing authored IDs keep their spelling. Undo/Redo and saved Reopen
retain the exact ID. Entropy failure or bounded collision exhaustion refuses creation without
publishing document/history/selection changes. This is not an absolute non-reuse promise across
arbitrary discarded history. Ordinary creation does not expose an ID reservation or another Undo
authority. The retained construction experiment below extends that owner rather than the ordinal
or reservation mechanisms it replaced.

## Construct With Transaction-Local References

The direct Application experiment allows multiple new entities and references without assigning
persistent IDs or copying creation-template owner/version metadata into caller-built records:

```rust,ignore
let basis = application.document_query_basis(document)?;
let mut draft = application.begin_authoring_construction(&basis)?;
let first = draft.create_entity(ScenePlacement::last(ScenePlacementParent::Root))?;
let second = draft.create_entity(ConstructionPlacement::ChildOf((&first).into()))?;
for local in [&first, &second] {
    draft.add_component(local, ComponentTypeId::new("game.Link"))?;
}
let target = ComponentPropertyPath::new(ComponentFieldId::new("target"));
draft.set_entity_reference(&first, ComponentTypeId::new("game.Link"), target.clone(), &second)?;
draft.set_entity_reference(&second, ComponentTypeId::new("game.Link"), target, &first)?;
let prepared = application.prepare_authoring_construction(&basis, &draft)?;
preview_changes(prepared.patch(), prepared.inverse());
let result = application.commit_authoring_construction(prepared, Duration::ZERO);
observe_dispatch(result.submission().actions());
if let Some(created) = result.entity(&first) {
    observe_created_identity(created);
}
```

This example assumes the admitted `game.Link` template and editable EntityRef property exist.
Locals are opaque handles for one draft, not serialized IDs, allocation reservations or runtime
entities. Local parents and sibling anchors must already be declared; references may be mutual.
Each preparation allocates its own mapping. Dropping a prepared candidate publishes nothing;
commit never reallocates that candidate's IDs. Only successful commit returns a mapping. Its
historical receipt remains usable after Undo/Reopen, but does not certify that an entity still exists.

Appending validates bounded intent and local affinity, not the complete Schema or final document.
A refused append leaves earlier intent intact. After semantic preparation refusal, rebuild the
proposal from the tool's retained user inputs; appending a correction cannot erase an earlier
invalid write. Do not treat the draft as a second editable document or an input-buffer owner.
Declarations plus the exact placement replacement share the existing 256-operation ceiling;
raw property writes still share their independent eight-write and selector/value limits before coalescing.

The independent consumer covers non-active Scene/Prefab construction, exact successful IDs,
one Undo/Redo and persistence. The separately scoped qualification also checks affected native
structural/Canvas/preview regressions. Registered panels use the later
[finite preparation and consuming submission interface](#prepare-and-retain-an-edit-in-a-panel)
for construction as well as property edits. Neither interface adds a general construction widget,
unbounded field queries or installable tools.

## Query A Non-Active Document

`EditorAction::QueryDocumentEntities` enumerates direct source-record IDs from an explicitly
named open Scene or Prefab. Obtain the document revision and exact optional Schema basis from
its public tab facts, not from the active Inspector:

```rust,ignore
let query = EditorEntityQuery {
    position: EditorEntityQueryPosition::Start(EditorDocumentQueryBasis {
        document: tab.document,
        revision: tab.revision,
        schema: tab.authoring_schema_basis.clone(),
    }),
    limits: EditorEntityQueryLimits::new(64, 16)?,
};
let submission = application.submit_and_drive(
    EditorActionBatch::new(frame.revision(), [EditorAction::QueryDocumentEntities(query)]),
    Duration::ZERO,
);
```

The owned `DocumentEntities` outcome is available through direct submissions and the same panel
`action_finished` routing. Neither query changes active tabs, selection, history or dirty state.
The page's opaque continuation captures workspace identity, document kind, source incarnation,
revision and Schema admission. Reusing a current continuation repeats a read-only page; it does
not advance shared server state. Navigation, Save and edits of another document do not invalidate
it, while edits/Undo/Redo/Reopen of its target do. A query-only batch uses the document basis
rather than requiring a fresh geometry frame; mixed batches retain ordinary frame admission.

Scan-record and returned-row caps are independently checked. This concrete enumeration emits
one ID for each inspected record; it does not first construct a complete Inspector. These caps
do not bound ID bytes, total work across a batch, or the subsequent normal Application drive.
Expanded Prefab identities, field queries, unopened sources and project-wide indexing are not
part of entity enumeration; the separate bounded authoring-read experiment follows below.

The integrated Host retains a concrete file lane for every open Scene/Prefab document. Opening
another source of the same kind adds a slot; it does not trigger Close or discard dirty history.
The independent consumer keeps two Scenes and two Prefabs and queries, edits, saves, reopens and
explicitly closes non-active documents without retargeting the active source. This does not
introduce cross-document transactions, concurrent Save, or one preview Runtime per document.

## Discover Components And Read A Selected Property

The bounded authoring-read experiment uses the same `EditorDocumentQueryBasis`, direct submission
and action/outcome routing. Discover actual components with
`EditorAuthoringProjection::Components`, then discover an Available component's declared root
IDs with `EditorAuthoringProjection::Roots { component }`:

```rust,ignore
let discovery = EditorAuthoringQuery {
    position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
        basis: document_basis.clone(),
        entity: entity.clone(),
        projection: EditorAuthoringProjection::Components,
    }),
    limits: EditorAuthoringQueryLimits::new(64, 16, 4096)?,
};
let action = EditorAction::QueryDocumentAuthoring(discovery);
// Consume DocumentAuthoring through the ordinary owned dispatch result or panel callback.
```

This is actual presence, not the creation catalog: unavailable and zero-root components are not
lost merely because they have no editable roots. Root rows contain compact kind/required/Edit-
capability facts, not recursive shapes, aliases, defaults or payload trees. `edit_capability`
reports a declaration; ordinary source candidate admission still decides whether an edit is legal.
Continuation fixes the complete entity/component/projection and exact basis, not only its page
offset. A copied page remains historical after that basis changes.

Read the selected root or an existing descendant using the same stable selector as property
editing. Empty steps select the root; Member, keyed ListItem and literal MapEntry steps select
descendants. Root discovery does not enumerate recursive member IDs; a semantic tool uses its
declared Schema-relative selector rather than inferring identity from a display label or list index.

```rust,ignore
let read = EditorPropertyRead {
    target: EditorPropertyTarget {
        basis: document_basis,
        entity,
        component,
        property: ComponentPropertyPath::new(field),
    },
    limits: EditorPropertyReadLimits::new(4096, 256, 16 * 1024)?
        .with_scan_limits(512, 64 * 1024)?,
};
let action = EditorAction::ReadDocumentProperty(read);
// DocumentProperty returns the exact target and Present / Defaulted / Missing observation.
```

Explicit Null is `Present(ComponentValue::Null)`; a supplied Null default is `Defaulted`, not
Missing. Default origin propagates to a descendant without materializing the complete effective
component. Missing describes only an omitted declared root with no default and empty selector
steps. Descending from that absence, OptionalAbsent, missing keyed/map items and retired members
are typed source/selector errors, not Missing. A missing entity or
component, an unknown field, an unavailable record and a stale basis return distinct rejections.
An oversized selected value returns `ValueBudgetExceeded` without partial data. Metadata discovery
still advances independently of that value's size; it never copies the whole component first.

Discovery independently caps scans, rows and aggregate UTF-8 metadata, including duplicated page/
cursor/anchor IDs. Property reads independently cap UTF-8 target/selector metadata, selector scan
entries/logical bytes, and selected-value nodes/logical bytes before cloning. An exhausted scan
returns `Source(AuthoringReadError::Property(ComponentPropertyError::ReadBudgetExceeded))`; it does
not truncate or substitute a default. Fixed-size metadata is bounded by the row/type shape; these limits
are not wire size, RSS, allocator isolation, total batch work or the subsequent frame-drive budget.
The experiment caps discovery at 256 entries / 64 KiB metadata and selected values at 4,096 nodes /
256 KiB logical payload. Selector scans default to at most 65,536 logical probes / 4 MiB compared
logical bytes; callers choose smaller positive limits independently. These are finite ceilings,
not measured memory/latency guarantees. Borrowed Scene/Prefab views supply
source interpretation; they are not a second document or a promise of allocation-free path lookup.

Source-built panels using `EditorPanelContext` or `ImguiEditorPanelContext` can avoid a multi-frame
action/result exchange for a small read:

```rust,ignore
let observed = context.read_document_property(request)?;
// Compute from observed.value, then prepare against observed.target.basis.
// Preparation remains read-only; commit still consumes the candidate after paint.
```

An observation is historical, not an edit capability. Reading never materializes source. A prepared
nested property write can initialize only its omitted declared root from the default, modify it
in the private candidate, then commit through the ordinary root patch/history owner. Undo restores
authored absence rather than an explicit default-equal value. Empty selector steps assign the
supplied root directly, including an optional root with no default. Neither route creates missing
storage parents/list slots or merges an explicit Null/partial value with defaults. Such targets
remain explicit refusals; the same default can be readable without being writable in that layout.

The independent consumer discovers an editable integer in non-active Scene and Prefab documents,
reads its current value and computes the replacement before preparing one ordinary source edit,
Undo/Redo and Save/Reopen. Full query qualification is tracked separately from this guide; the
selector-directed consumer also computes a keyed/map scalar edit without copying the unrelated
root subtree. The experiment does not implement nested discovery, value pagination or unopened-source querying. Prepared panel
submission has its own contract and qualification scope rather than following from read-only
query outcomes.
