use super::*;
use crate::{
    EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext,
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};
use dear_imgui_rs::Ui;
use nara::reflect::{ComponentFieldId, ComponentTypeId};

const PANEL: &str = "trial.native.custom-fields";

struct NativeCustomFields {
    component: ComponentTypeId,
    fields: Vec<ComponentFieldId>,
}

impl ImguiEditorPanel for NativeCustomFields {
    fn show(&mut self, ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        let origin = ui.main_viewport().pos();
        let scale = ui.current_font_size() / 16.0;
        ui.set_window_pos([origin[0] + 800.0 * scale, origin[1] + 220.0 * scale]);
        let entity = context
            .model()
            .frame()
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned();
        if let Some(entity) = entity {
            for field in &self.fields {
                context.edit_field(ui, &entity, &self.component, field);
            }
        }
    }
}

impl AuthoringWorkflow<'_> {
    pub(super) fn install_native_custom_fields(
        &mut self,
        component: &str,
        fields: &[&str],
    ) -> Result<EditorPanelInstanceId, String> {
        let component = ComponentTypeId::new(component);
        let fields = fields
            .iter()
            .map(|field| ComponentFieldId::new(*field))
            .collect::<Vec<_>>();
        let mut composition = EditorNativeComposition::new();
        composition
            .register_panel(
                EditorPanelDescriptor::new(
                    PANEL,
                    "Native custom fields",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Floating,
                    false,
                ),
                move || NativeCustomFields {
                    component: component.clone(),
                    fields: fields.clone(),
                },
            )
            .map_err(|error| error.to_string())?;
        self.native.frontend = composition
            .install_panels(self.application)
            .map_err(|error| error.to_string())?;
        self.application
            .panel_workspace_mut()
            .open(PANEL, EditorPanelTarget::ActiveDocument)
            .map_err(|error| error.to_string())
    }
}
