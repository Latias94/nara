//! A trusted source-built Editor contribution, independently compiled from the product module.

use nara_editor::{
    EditorLaunchOptions, EditorNativeComposition, EditorRunError, ImguiEditorPanel,
    ImguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelModel, EditorPanelMultiplicity, EditorPanelPlacement,
        EditorPanelRegistrationError,
    },
};
use nara_tooling::{EditorAction, EditorNavigationAction};

pub const PANEL_KIND: &str = "sample.selection-summary";

pub fn install(
    composition: &mut EditorNativeComposition,
) -> Result<(), EditorPanelRegistrationError> {
    composition.register_panel(
        EditorPanelDescriptor::new(
            PANEL_KIND,
            "Selection summary",
            EditorPanelMultiplicity::Multiple,
            EditorPanelPlacement::Bottom,
            true,
        ),
        SelectionSummary::default,
    )
}

/// A normal native runner entry point, compiled outside the Editor crate.
pub fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let mut composition = EditorNativeComposition::new();
    install(&mut composition)?;
    composition.run(options)
}

#[derive(Default)]
struct SelectionSummary {
    note: String,
}

impl ImguiEditorPanel for SelectionSummary {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        ui.input_text("Draft note", &mut self.note).build();
        ui.text(format!(
            "Selected entities: {}",
            selected_count(context.model())
        ));
        let action = clear_selection(context.model());
        let _disabled = ui.begin_disabled_with_cond(action.is_none());
        if ui.button("Clear selection")
            && let Some(action) = action
        {
            context.emit(action);
        }
    }
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
