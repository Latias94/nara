use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginId, PluginProductCapability,
};
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_ui::{
    UiPrimarySurfaceMetrics, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, install_ui_surface,
};

pub(crate) const PROJECT_PRIMARY_UI_SURFACE_ID: u64 = 1;
pub(crate) const PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID: PluginId =
    PluginId::new("nara.project-primary-ui-surface");
const PROJECT_PRIMARY_UI_SURFACE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.project-primary-ui-surface.configured", 1);
const PROJECT_PRIMARY_UI_SURFACE_REQUIREMENTS: &[PluginId] = &[nara_ui::UI_PLUGIN_ID];
const PROJECT_PRIMARY_UI_SURFACE_PRODUCT_REQUIREMENTS: &[PluginProductCapability] =
    &[PluginProductCapability::new("runtime-ui")];
const PROJECT_PRIMARY_UI_SURFACE_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
    PluginCategory::Runtime,
)
.requires_plugins(PROJECT_PRIMARY_UI_SURFACE_REQUIREMENTS)
.requires_product_capabilities(PROJECT_PRIMARY_UI_SURFACE_PRODUCT_REQUIREMENTS);

#[derive(Debug, Clone, Copy)]
struct ProjectPrimaryUiSurfacePlugin {
    profile: UiPrimarySurfaceMetrics,
}

impl Plugin for ProjectPrimaryUiSurfacePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PROJECT_PRIMARY_UI_SURFACE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let transform = self.profile.direct_transform().ok_or_else(setup_error)?;
        let surface_id = self.profile.surface_id();
        let endpoint = UiSurfaceEndpoint::new(
            InputWindowId::new(nara_window::WindowId::PRIMARY.get()),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
        );
        let slot =
            UiSurfaceSlot::new(surface_id, endpoint, transform).map_err(|_| setup_error())?;
        app.insert_resource(self.profile)?;
        {
            let mut input = app.world_mut()?.resource_mut::<nara_input::InputTimeline>();
            input.connect_endpoint(endpoint.window(), endpoint.seat())
                .and_then(|()| input.connect_pointer(endpoint.window(), endpoint.seat(), endpoint.pointer()))
                .map_err(|error| PluginError::SetupFailed {
                    plugin: PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
                    message: format!("the primary UI surface could not connect its logical Input endpoint: {error}"),
                })?;
        }
        install_ui_surface(app.world_mut()?, slot).map_err(|_| setup_error())
    }
}

pub(crate) fn project_primary_ui_surface_definition(
    logical_width: u32,
    logical_height: u32,
    safe_area: [u32; 4],
) -> PluginDefinition {
    let surface_id = UiSurfaceId::new(PROJECT_PRIMARY_UI_SURFACE_ID)
        .expect("the project primary UI surface identity is non-zero");
    let profile =
        UiPrimarySurfaceMetrics::new(surface_id, logical_width, logical_height, safe_area);
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(b"nara.project-primary-ui-surface.v1\0");
    for value in [logical_width, logical_height].into_iter().chain(safe_area) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    PluginDefinition::infallible::<ProjectPrimaryUiSurfacePlugin, _>(
        PROJECT_PRIMARY_UI_SURFACE_DEFINITION_ID,
        bytes,
        move || ProjectPrimaryUiSurfacePlugin { profile },
    )
}

fn setup_error() -> PluginError {
    PluginError::SetupFailed {
        plugin: PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
        message: "the accepted primary UI surface profile could not be lowered".to_owned(),
    }
}
