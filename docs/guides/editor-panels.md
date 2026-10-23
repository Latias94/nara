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
Cancelling projection keeps its work slot until return. Once preparation enters AwaitingClose,
use the existing dirty-close prompt's Save/Discard/Cancel. Refresh and selection can continue,
but changed index or predecessor facts reject a stale candidate before destructive retirement.
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
