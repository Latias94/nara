mod manager;

use std::sync::Arc;

use bevy_ecs::{
    prelude::{Messages, ResMut, Resource},
    schedule::IntoScheduleConfigs,
};
use nara_app::{
    App, CoreStage, FrameCleanup, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
};
use nara_asset::{
    ASSET_PLUGIN_ID, AssetAppExt, AssetLoadContext, AssetLoadSet, AssetLoader, NpakEntryKind,
    RuntimeAssetMode,
};
use nara_core::AssetId;
use nara_reflect::{COMPONENT_REGISTRY_PLUGIN_ID, RegisterSchema, Schema};
use thiserror::Error;

use crate::cooked::{CookedRegistry, CookedScene, CookedSceneError};

pub use manager::{
    SceneError, SceneEvent, SceneInstanceId, SceneLoadState, SceneManager, SceneMember,
    switch_scene_asset_sync, switch_scene_sync,
};

/// Validated NSCN bytes with the stable UUIDs required by their component values.
#[derive(Clone, Debug)]
pub struct SceneAsset {
    bytes: Arc<[u8]>,
    dependencies: Arc<[AssetId]>,
}

impl SceneAsset {
    pub fn from_cooked(bytes: impl Into<Arc<[u8]>>) -> Result<Self, CookedSceneError> {
        let bytes = bytes.into();
        let dependencies = Arc::from(CookedScene::parse(&bytes)?.asset_dependencies());
        Ok(Self {
            bytes,
            dependencies,
        })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn asset_dependencies(&self) -> &[AssetId] {
        &self.dependencies
    }
}

/// Reads an NSCN or compiles an authored scene and its prefabs with the running game's Schema.
#[derive(Clone, Default)]
pub struct SceneAssetLoader {
    #[cfg(feature = "cook")]
    schemas: Arc<std::sync::RwLock<nara_reflect::ComponentSchemaCatalog>>,
}

impl AssetLoader for SceneAssetLoader {
    type Asset = SceneAsset;
    type Error = SceneAssetError;

    fn cooked_kind(&self) -> Option<NpakEntryKind> {
        Some(NpakEntryKind::Scene)
    }

    fn load(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<SceneAsset, SceneAssetError> {
        match context.mode() {
            RuntimeAssetMode::Cooked => Ok(SceneAsset::from_cooked(bytes)?),
            #[cfg(feature = "cook")]
            RuntimeAssetMode::Source => self.cook(bytes, context),
            #[cfg(not(feature = "cook"))]
            RuntimeAssetMode::Source => Err(SceneAssetError::SourceDisabled),
        }
    }
}

#[cfg(feature = "cook")]
impl SceneAssetLoader {
    fn cook(
        &self,
        bytes: &[u8],
        context: &mut AssetLoadContext<'_>,
    ) -> Result<SceneAsset, SceneAssetError> {
        use crate::authoring::SceneDocument;

        fn parse(
            bytes: &[u8],
            schemas: &nara_reflect::ComponentSchemaCatalog,
        ) -> Result<SceneDocument, SceneAssetError> {
            if bytes.len() > 64 * 1024 * 1024 {
                return Err(SceneAssetError::SourceTooLarge);
            }
            Ok(SceneDocument::parse_with(
                std::str::from_utf8(bytes)?,
                schemas.components(),
            )?)
        }

        let schemas = self
            .schemas
            .read()
            .map_err(|_| SceneAssetError::SchemaUnavailable)?
            .clone();
        if context
            .meta()
            .is_some_and(|meta| meta.importer() != "nara.scene")
        {
            return Err(SceneAssetError::WrongImporter {
                id: context.id(),
                expected: "nara.scene",
            });
        }
        let scene = parse(bytes, &schemas)?;
        let cooked = crate::cook::cook_scene(&scene, &schemas, |id| {
            if context
                .meta_for(id)
                .is_some_and(|meta| meta.importer() != "nara.prefab")
            {
                return Err(SceneAssetError::WrongImporter {
                    id,
                    expected: "nara.prefab",
                });
            }
            parse(&context.read_dependency(id, None)?, &schemas)
        })?;
        Ok(SceneAsset::from_cooked(cooked.bytes)?)
    }
}

#[derive(Debug, Error)]
pub enum SceneAssetError {
    #[error(transparent)]
    Cooked(#[from] CookedSceneError),
    #[cfg(not(feature = "cook"))]
    #[error("source scene loading requires the 'cook' feature")]
    SourceDisabled,
    #[cfg(feature = "cook")]
    #[error("the scene Schema catalog is unavailable")]
    SchemaUnavailable,
    #[cfg(feature = "cook")]
    #[error("authored scene or prefab exceeds 64 MiB")]
    SourceTooLarge,
    #[cfg(feature = "cook")]
    #[error("asset '{id}' does not use the '{expected}' importer")]
    WrongImporter { id: AssetId, expected: &'static str },
    #[cfg(feature = "cook")]
    #[error("scene source is not UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[cfg(feature = "cook")]
    #[error(transparent)]
    Document(#[from] crate::authoring::SceneDocumentError),
    #[cfg(feature = "cook")]
    #[error(transparent)]
    Source(#[from] nara_asset::AssetLoadError),
    #[cfg(feature = "cook")]
    #[error(transparent)]
    Cook(#[from] crate::cook::CookSceneError),
}

/// Native component codecs used by `SceneManager`.
#[derive(Resource)]
pub struct SceneRegistry {
    native: CookedRegistry,
    #[cfg(feature = "cook")]
    loader: SceneAssetLoader,
}

pub trait RegisterSceneComponent {
    /// Registers both the native scene codec and its discoverable Schema.
    fn register_scene_component<T: Schema>(&mut self) -> Result<&mut Self, SceneError>;
}

impl RegisterSceneComponent for App {
    fn register_scene_component<T: Schema>(&mut self) -> Result<&mut Self, SceneError> {
        let registry = self
            .world()
            .get_resource::<SceneRegistry>()
            .ok_or(SceneError::MissingPlugin)?;
        let mut native = registry.native.clone();
        native.register::<T>()?;
        #[cfg(feature = "cook")]
        let shared = registry.loader.schemas.clone();
        #[cfg(feature = "cook")]
        let mut schemas = shared
            .write()
            .map_err(|_| SceneAssetError::SchemaUnavailable)?;
        self.register_schema::<T>()?;
        #[cfg(feature = "cook")]
        {
            *schemas = nara_reflect::component_registry(self.world())
                .ok_or(nara_reflect::SchemaRegistrationError::MissingRegistry)?
                .clone();
        }
        self.world_mut()?.resource_mut::<SceneRegistry>().native = native;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScenePlugin;

pub const SCENE_PLUGIN_ID: PluginId = PluginId::new("nara.scene");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SCENE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[ASSET_PLUGIN_ID, COMPONENT_REGISTRY_PLUGIN_ID]);

impl Plugin for ScenePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let mut native = CookedRegistry::new();
        native
            .register_builtins()
            .map_err(|error| PluginError::SetupFailed {
                plugin: SCENE_PLUGIN_ID,
                message: error.to_string(),
            })?;
        let loader = SceneAssetLoader::default();
        app.register_asset_loader(loader.clone())?
            .insert_resource(SceneRegistry {
                native,
                #[cfg(feature = "cook")]
                loader,
            })?
            .init_resource::<SceneManager>()?
            .init_resource::<Messages<SceneEvent>>()?
            .add_systems(
                CoreStage::TaskUpdate,
                manager::update_scenes.after(AssetLoadSet::Publish),
            )?
            .add_systems(
                FrameCleanup,
                |mut messages: ResMut<Messages<SceneEvent>>| messages.update(),
            )?;
        Ok(())
    }
}
