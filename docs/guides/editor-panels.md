# Source-built Editor panels

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
can call `composition.install(&mut application)` to receive the same configured frontend; the
native runner uses this admission path. A failed admission preserves the workspace.

The UI thread owns factories and panel objects, so they do not require `Send` or `Sync`. A panel
receives read-only document/selection projections. To change application state, emit an existing
`EditorAction` through the context; normal frame, document and revision admission still applies.
Keeping a draft in a panel does not dirty a document or publish an undo entry.

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
