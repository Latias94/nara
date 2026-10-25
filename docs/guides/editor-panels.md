# Source-built Editor panels

An authoring document owns accepted content, history and persistence. An editor panel presents or
edits that content: the Inspector, Project Browser and Scene View are panel kinds. A panel instance
is one open occurrence with its own target and presentation state; the workspace layout arranges
those instances across editor windows. Closing a panel does not close its document. Scene View
presents authored content, while Game View presents a running game's output.

Native panel extensions are trusted Rust code compiled into an Editor executable. Opening a
project does not compile or load its code. Register a panel descriptor and its Dear ImGui state
factory together through `EditorNativeComposition`:

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
the native runner uses this construction. For panel-only attachment to an existing application,
use `composition.install_panels(&mut application)`. It rejects selected Host contributions,
which require complete construction, before changing the workspace. A panel admission failure
also preserves the complete workspace.

The UI thread owns factories and panel objects, so they do not require `Send` or `Sync`. A panel
receives read-only document/selection projections. Keeping a draft in a panel does not dirty a
document or publish an undo entry.

For a domain-owned non-Scene document, use the [document extension guide](editor-documents.md).
Its independent dialogue consumer uses the same panel retirement and action paths while the
Host retains history/dirty projections, Save/Reopen and document close decisions.

## Edit A Persistent Field

Use the default field editor when the panel should edit a field from the selected entity's
published Inspector projection:

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

## Submit A Custom Action

For custom tools, call `context.submit(EditorAction)` and retain the returned `PanelSubmissionId`.
Queue admission can fail with `PanelSubmitError::Capacity`; an admitted action has not yet been
accepted by its owner. Implement the optional callback to settle exactly that request:

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

Embedded shells pass the opaque move-only candidate returned by `frontend.show(...)` to that
same frontend's `submit_and_drive(...)`. It cannot be cloned or converted to a plain action batch.
Painting a newer candidate supersedes unfinished observations from the previous cycle; dropping
a candidate performs no application work. The ordinary application `EditorActionBatch` entry
remains available for callers that do not need panel result routing.

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

For Save/Reopen, retain the admitted `EditorPersistenceAttemptId` and match the later Pending or
Terminal observation to that attempt. Dismiss a terminal with the identity actually displayed,
not a newly sampled latest result. `Refused` admission publishes nothing; an immediate published
`Rejected` has its own attempt. See [persistence observation](editor-documents.md#save-reopen-and-close)
for the exact acknowledgment and retry rules shared with window-free consumers.

Panel state survives docking, movement, relocation and native GPU reconstruction. Closing or
hiding a panel retires that instance's drafts without closing its document. Reopening starts
fresh state. Changing the panel target or its resolved document also retires drafts, including a
source target that becomes unavailable. A fresh project/workspace and explicit layout
reset/restore retire all panel drafts. Replacing a GPU owner moves the retained frontend and its
state; it does not rerun composition or recreate surviving panel objects.

The separately compiled consumer at
`tests/fixtures/module-consumer/editor-panel-consumer` contains an ordinary native runner and a
selection-summary panel with a local note draft. This is a source extension interface, not a
stable dynamic plugin ABI or a toolkit-independent widget framework.

## A Source-Local Snap Tool

The same consumer's `snap_local_position` module contributes a useful command without changing
engine APIs. Call `snap_local_position::install(&mut composition)` before building/running the
composition; its companion `run(options)` already includes this registration. Select one direct
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

The panel uses context preparation to validate that basis. A window-free Session consumer must
explicitly call `validate_authoring_query_basis(proposal.basis())` before
`prepare_authoring_property_edit(document, proposal.basis().revision, &proposal.property_edits())`,
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
let tab = frame.workspace().document(document).unwrap();
let prepared = application.prepare_authoring_edit(document, tab.revision, &patch)?;
preview_changes(prepared.patch(), prepared.inverse());
let submission = application.commit_authoring_edit(prepared, Duration::ZERO);
observe_dispatch(submission.actions());
```

`PreparedEditorDocumentEdit` is move-only. The document owner retains the exact candidate and
inverse inspected by the caller. Dropping the value publishes nothing. Commit rechecks the
original document/session, revision and Schema admission; an edit, Undo/Redo, Reopen or project
replacement cannot silently rebase it. Specialized UI Prefab structure still requires its domain
contract. The result uses the existing owned Application submission, not a result ticket.

Mixed construction admits one bounded ordered patch: create empty direct entities, add admitted
component templates and edit their fields/references. Final classification and forward/inverse
history admission precede publication. One accepted patch is one Undo/Redo entry. A valid forward
value is insufficient if its inverse cannot be replayed under the same field limits.
`MAX_AUTHORING_EDIT_OPERATIONS` bounds mixed operations; `MAX_AUTHORING_FIELD_WRITES` and the
existing field-value limits remain separate. This is not cross-document atomicity or atomic
multi-file Save.

Raw mixed patches use explicit persistent `SceneEntityId` values and declared component templates;
use [retained construction](#construct-with-transaction-local-references) for source-owned
transaction-local allocation and references. Schema-relative nested replacements have their separate
[property-edit interface](persistent-components.md#stable-nested-properties). Panels can submit
ordinary atomic patches through the existing mutation action; direct Application clients use the retained
candidate interface above. Registered source-built panels can retain the same candidates through
the finite context methods below. No compatibility wrapper or second Undo authority is required.

## Prepare And Retain An Edit In A Panel

Prepare from the complete basis returned with a document query, not a newly sampled revision.
The context borrows only finite read-only preparation access; it exposes no mutable Application,
Session, source document or persistence Host:

```rust,ignore
let prepared = context.prepare_authoring_property_edit(&observation.target.basis, &edits)?;
preview_changes(prepared.patch(), prepared.inverse());
self.prepared = Some(prepared);
```

`prepare_authoring_edit(&basis, &patch)` supports an existing atomic patch.
`begin_authoring_construction(&basis)` and
`prepare_authoring_construction(document, &draft)` reuse the existing source-owned draft and
fixed local-reference mapping. The source owners retain validation, inverse and history authority.
Preparing, inspecting or retaining a candidate changes no document, selection or dirty state.
Drop it to cancel before submission; changing tool inputs should replace the candidate rather
than edit it as a second document.

When the user accepts, consume that same candidate through the current frontend submission:

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
action route; it does not apply during paint and is not a promise of cancellability afterward.
Use `action_finished` and the exact `PanelSubmissionId`. The successful construction outcome is
`EditorActionOutcome::AuthoringConstruction`, containing the actual owner report and mapping;
failed commit retains its ordinary owner rejection. Never infer success from selection or dirty.

Candidate-bearing `EditorAction` and `EditorActionBatch` values are move-only. Historical outcomes
remain owned observations. A commit-only batch uses source-candidate admission; mixing navigation
or other frame-sensitive work retains whole-batch frame admission. Two candidates prepared from
the same basis can accept then reject stale, and several actions are not one transaction.
Existing supersede, origin retirement and project-generation rules still apply. A panel that
retires after publication cannot roll the source back.

Use a Workspace-target panel to operate on a named non-active document. Existing Source-target
activation behavior is unchanged. The [bounded panel design](../architecture/editor-prepared-panel-design.md)
records alternatives and qualification scope; this does not implement inline Inspector tools,
dynamic installation, asynchronous preparation or a second preview/history service.

## Create A Source Entity

Ordinary `ComposeScene` and `ComposePrefab` creation commands carry placement and the supported
recipe, not a predicted entity ID. The Scene/Prefab owner generates the ID, validates the complete
candidate and publishes one history entry. The Workspace selects the created entity only after
success; Prefab creation also returns `PrefabCompositionOutcome::Created { entity }`. Read those
actual results instead of constructing names such as `editor_entity_1`.

Direct native Scene/Prefab sessions use `create_entity_with_registry`, or
`create_entity_with_asset_database` when their codecs/references require project asset admission.
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
let mut draft = application.begin_authoring_construction(document, tab.revision)?;
let first = draft.create_entity(ScenePlacement::last(ScenePlacementOwner::Root))?;
let second = draft.create_entity(ConstructionPlacement::ChildOf((&first).into()))?;
for local in [&first, &second] {
    draft.add_component(local, ComponentTypeId::new("game.Link"))?;
}
let target = ComponentPropertyPath::new(ComponentFieldId::new("target"));
draft.set_entity_reference(&first, ComponentTypeId::new("game.Link"), target.clone(), &second)?;
draft.set_entity_reference(&second, ComponentTypeId::new("game.Link"), target, &first)?;
let prepared = application.prepare_authoring_construction(document, &draft)?;
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
Declarations plus coalesced roots share the existing 256-operation ceiling; raw property writes
still share their independent eight-write and selector/value limits before coalescing.

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

Source-built panels can avoid a multi-frame action/result exchange for a small read:

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
