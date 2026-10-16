//! A trusted source-built Editor contribution, independently compiled from the product module.

use nara_editor::{
    EguiEditorFrontend, EguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelModel, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelRegistrationError, EditorPanelWorkspace,
    },
};
use nara_tooling::{EditorAction, EditorNavigationAction};

pub const PANEL_KIND: &str = "sample.selection-summary";

pub fn install(workspace: &mut EditorPanelWorkspace) -> Result<(), EditorPanelRegistrationError> {
    workspace.register(EditorPanelDescriptor::new(
        PANEL_KIND,
        "Selection summary",
        EditorPanelMultiplicity::Multiple,
        EditorPanelPlacement::Bottom,
        true,
    ))
}

pub fn install_egui(frontend: &mut EguiEditorFrontend) {
    frontend.register_panel_painter(PANEL_KIND, paint);
}

/// Uses the ordinary navigation vocabulary; the contribution never mutates a session.
#[must_use]
pub fn clear_selection(model: EditorPanelModel<'_>) -> Option<EditorAction> {
    let document = model.document()?.document;
    Some(EditorAction::Navigate(
        EditorNavigationAction::SelectEntity {
            document,
            entity: None,
        },
    ))
}

#[must_use]
pub fn selected_count(model: EditorPanelModel<'_>) -> usize {
    model.selection().map_or(0, |selection| selection.len())
}

fn paint(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    ui.label(format!(
        "Selected entities: {}",
        selected_count(context.model())
    ));
    let action = clear_selection(context.model());
    if ui
        .add_enabled(action.is_some(), egui::Button::new("Clear selection"))
        .clicked()
        && let Some(action) = action
    {
        context.emit(action);
    }
}
