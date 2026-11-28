use nara_app::{
    App, CoreStage, FrameCleanup, FrameIntent, Plugin, PluginDefinition, PluginDefinitionId,
    PluginError,
};
use nara_asset::{AssetAppExt, AssetLoadContext, AssetLoader, AssetServer, NpakEntryKind};
use nara_ecs::{ResMut, bevy_ecs::message::Messages, schedule::IntoScheduleConfigs};
use nara_input::InputSet;

use crate::{
    FontFace, FontFaceError, UiActionEvent, UiAdapterDiagnostics, interaction, runtime, text,
};

/// Installs code-first UI layout and routes input against accepted layouts.
#[derive(Debug, Clone, Default)]
pub struct UiPlugin {
    font: Option<FontFace>,
}

impl UiPlugin {
    #[must_use]
    pub fn with_font(font: FontFace) -> Self {
        Self { font: Some(font) }
    }
}

const UI_PLUGIN_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.ui.configured", 1);

/// Creates a repeatable UI plugin definition using the selected font.
#[must_use]
pub fn plugin(font: FontFace) -> PluginDefinition {
    let mut configuration = b"nara.ui.font.v1\0".to_vec();
    configuration.extend_from_slice(font.id().as_bytes());
    PluginDefinition::infallible::<UiPlugin, _>(UI_PLUGIN_DEFINITION_ID, configuration, move || {
        UiPlugin::with_font(font.clone())
    })
}

pub const UI_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui");
pub const UI_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_PLUGIN_ID, nara_app::PluginCategory::Runtime)
        .requires_plugins(&[nara_input::INPUT_PLUGIN_ID]);

impl Plugin for UiPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        register_font_loader(app)?;
        let font = match &self.font {
            Some(font) => font.clone(),
            None => text::bundled_font_face().map_err(|error| PluginError::SetupFailed {
                plugin: UI_PLUGIN_ID,
                message: error.to_string(),
            })?,
        };
        app.insert_resource(runtime::UiRuntimeState::new(font))?
            .init_resource::<UiAdapterDiagnostics>()?
            .init_resource::<Messages<UiActionEvent>>()?
            .add_systems(
                CoreStage::First,
                interaction::route_input.in_set(InputSet::RouteUi),
            )?
            .add_systems(
                FrameIntent,
                interaction::route_input.in_set(InputSet::RouteUi),
            )?
            .add_systems(CoreStage::Extract, runtime::compile_ui_frames)?
            .add_systems(FrameCleanup, advance_actions)?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        register_font_loader(app)
    }
}

fn register_font_loader(app: &mut App) -> Result<(), PluginError> {
    if let Some(server) = app.world().get_resource::<AssetServer>()
        && !server.loader_registry().contains::<FontFace>()
    {
        app.register_asset_loader(FontFaceLoader)?;
    }
    Ok(())
}

/// Loads the first face of a game font in either a project directory or an NPAK.
#[derive(Clone, Copy, Debug, Default)]
pub struct FontFaceLoader;

impl AssetLoader for FontFaceLoader {
    type Asset = FontFace;
    type Error = FontFaceError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Font)
    }

    fn load(&self, bytes: &[u8], _: &mut AssetLoadContext<'_>) -> Result<FontFace, FontFaceError> {
        FontFace::from_bytes(bytes.to_vec(), 0)
    }
}

fn advance_actions(mut actions: ResMut<Messages<UiActionEvent>>) {
    actions.update();
}
