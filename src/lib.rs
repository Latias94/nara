//! Public facade for the nara engine workspace.

pub use nara_app as app;
pub use nara_asset as asset;
#[cfg(feature = "asset-watch")]
pub use nara_asset_watch as asset_watch;
pub use nara_audio as audio;
pub use nara_core as core;
pub use nara_diagnostic as diagnostic;
pub use nara_ecs as ecs;
pub use nara_image as image;
pub use nara_input as input;
pub use nara_material as material;
pub use nara_reflect as reflect;
pub use nara_render as render;
#[cfg(feature = "wgpu")]
pub use nara_render_wgpu as render_wgpu;
pub use nara_scene as scene;
pub use nara_sprite as sprite;
pub use nara_sprite_render as sprite_render;
pub use nara_tasks as tasks;
pub use nara_tilemap as tilemap;
pub use nara_tooling as tooling;
#[cfg(feature = "egui")]
pub use nara_tooling_egui as tooling_egui;
pub use nara_transform as transform;
pub use nara_ui as ui;
pub use nara_ui_render as ui_render;
pub use nara_window as window;
#[cfg(feature = "winit")]
pub use nara_winit as winit;

use nara_app::{App, PluginError, PluginGroup, PluginGroupId, PluginGroupMetadata, PluginId};

const HIERARCHY_PLUGIN_ID: PluginId = PluginId::new("nara.scene.hierarchy");
const TASK_PLUGIN_ID: PluginId = PluginId::new("nara.tasks");
const ASSET_PLUGIN_ID: PluginId = PluginId::new("nara.asset");
const TRANSFORM_PLUGIN_ID: PluginId = PluginId::new("nara.transform");
const INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.input");
const SPRITE_PLUGIN_ID: PluginId = PluginId::new("nara.sprite");
const TILEMAP_PLUGIN_ID: PluginId = PluginId::new("nara.tilemap");
const RENDER_PLUGIN_ID: PluginId = PluginId::new("nara.render");
const IMAGE_PLUGIN_ID: PluginId = PluginId::new("nara.image");
const SPRITE_RENDER_PLUGIN_ID: PluginId = PluginId::new("nara.sprite-render");
const UI_PLUGIN_ID: PluginId = PluginId::new("nara.ui");
const UI_RENDER_PLUGIN_ID: PluginId = PluginId::new("nara.ui-render");
const WINDOW_PLUGIN_ID: PluginId = PluginId::new("nara.window");
#[cfg(feature = "winit")]
const WINIT_PLUGIN_ID: PluginId = PluginId::new("nara.winit");
#[cfg(feature = "wgpu")]
const WGPU_RENDER_PLUGIN_ID: PluginId = PluginId::new("nara.render-wgpu");
const TOOLING_PLUGIN_ID: PluginId = PluginId::new("nara.tooling");

const MINIMAL_PLUGIN_IDS: &[PluginId] = &[
    HIERARCHY_PLUGIN_ID,
    TASK_PLUGIN_ID,
    ASSET_PLUGIN_ID,
    TRANSFORM_PLUGIN_ID,
    INPUT_PLUGIN_ID,
];

const RUNTIME_2D_PLUGIN_IDS: &[PluginId] = &[
    HIERARCHY_PLUGIN_ID,
    TASK_PLUGIN_ID,
    ASSET_PLUGIN_ID,
    TRANSFORM_PLUGIN_ID,
    INPUT_PLUGIN_ID,
    SPRITE_PLUGIN_ID,
    TILEMAP_PLUGIN_ID,
    RENDER_PLUGIN_ID,
    IMAGE_PLUGIN_ID,
    SPRITE_RENDER_PLUGIN_ID,
    UI_PLUGIN_ID,
    UI_RENDER_PLUGIN_ID,
];

#[cfg(not(feature = "winit"))]
const DESKTOP_WINDOW_PLUGIN_IDS: &[PluginId] = &[WINDOW_PLUGIN_ID];
#[cfg(feature = "winit")]
const DESKTOP_WINDOW_PLUGIN_IDS: &[PluginId] = &[WINDOW_PLUGIN_ID, WINIT_PLUGIN_ID];

#[cfg(all(feature = "wgpu", not(feature = "winit")))]
const DESKTOP_WGPU_PLUGIN_IDS: &[PluginId] = &[
    WINDOW_PLUGIN_ID,
    WGPU_RENDER_PLUGIN_ID,
    SPRITE_RENDER_PLUGIN_ID,
    UI_RENDER_PLUGIN_ID,
];
#[cfg(all(feature = "wgpu", feature = "winit"))]
const DESKTOP_WGPU_PLUGIN_IDS: &[PluginId] = &[
    WINDOW_PLUGIN_ID,
    WINIT_PLUGIN_ID,
    WGPU_RENDER_PLUGIN_ID,
    SPRITE_RENDER_PLUGIN_ID,
    UI_RENDER_PLUGIN_ID,
];

const TOOLING_PLUGIN_IDS: &[PluginId] = &[TOOLING_PLUGIN_ID];

/// Minimal runtime defaults for headless examples, tests, and AI-generated scenes.
///
/// Platform windows and GPU backends are intentionally excluded. They should be
/// installed by dedicated backend plugins.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl PluginGroup for MinimalPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.minimal"),
            MINIMAL_PLUGIN_IDS,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        app.add_plugin_if_missing(nara_tasks::TaskPlugin::default())?;
        app.add_plugin_if_missing(nara_asset::AssetPlugin)?;
        app.add_plugin_if_missing(nara_transform::TransformPlugin)?;
        app.add_plugin_if_missing(nara_input::InputPlugin)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Runtime2dPlugins;

impl PluginGroup for Runtime2dPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.runtime-2d"),
            RUNTIME_2D_PLUGIN_IDS,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugins(MinimalPlugins)?;
        app.add_plugin_if_missing(nara_sprite::SpritePlugin)?;
        app.add_plugin_if_missing(nara_tilemap::TilemapPlugin)?;
        app.add_plugin_if_missing(nara_render::RenderPlugin)?;
        app.add_plugin_if_missing(nara_image::ImagePlugin)?;
        app.add_plugin_if_missing(nara_sprite_render::SpriteRenderPlugin)?;
        app.add_plugin_if_missing(nara_ui::UiPlugin)?;
        app.add_plugin_if_missing(nara_ui_render::UiRenderPlugin)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopWindowPlugins;

impl PluginGroup for DesktopWindowPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.desktop-window"),
            DESKTOP_WINDOW_PLUGIN_IDS,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(nara_window::WindowPlugin::default())?;
        #[cfg(feature = "winit")]
        app.add_plugin_if_missing(nara_winit::WinitPlugin::default())?;
        Ok(())
    }
}

#[cfg(feature = "wgpu")]
#[derive(Debug, Default, Clone, Copy)]
pub struct DesktopWgpuPlugins;

#[cfg(feature = "wgpu")]
impl PluginGroup for DesktopWgpuPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.desktop-wgpu"),
            DESKTOP_WGPU_PLUGIN_IDS,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugins(Runtime2dPlugins)?;
        app.add_plugins(DesktopWindowPlugins)?;
        app.add_plugin_if_missing(nara_render_wgpu::WgpuRenderPlugin)?;
        app.add_plugin_if_missing(nara_sprite_render::SpriteRenderPlugin)?;
        app.add_plugin_if_missing(nara_ui_render::UiRenderPlugin)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ToolingPlugins;

impl PluginGroup for ToolingPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.tooling"),
            TOOLING_PLUGIN_IDS,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(nara_tooling::ToolingPlugin)?;
        Ok(())
    }
}

pub mod prelude {
    #[cfg(feature = "wgpu")]
    pub use crate::DesktopWgpuPlugins;
    pub use crate::{DesktopWindowPlugins, MinimalPlugins, Runtime2dPlugins};
    pub use nara_app::{
        App, AppExit, AppExitRequests, AppFrameOutcome, AppRunError, CoreStage, FixedTime, Plugin,
        PluginError, PluginGroup, RealTime, RenderTime, RuntimeFrameStatus, RuntimeTimeSettings,
        StartupStage, VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetPathError, AssetPlugin, AssetRef, AssetRefError,
        AssetRefExportPolicy, AssetServer, Assets, Handle, StableAssetId, StableAssetIdError,
    };
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity};
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImagePlugin, ImageSourceMetadata,
    };
    pub use nara_input::{ButtonInput, InputPlugin, KeyCode, MouseButton, PointerState};
    pub use nara_material::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDescriptor, Material2dKey,
        SamplerDescriptor, material2d_descriptor_key,
    };
    pub use nara_reflect::{
        ComponentCapability, ComponentCodec, ComponentCodecError, ComponentDecodeContext,
        ComponentEncodeContext, ComponentFieldPath, ComponentFieldPathError,
        ComponentFieldPathSegment, ComponentFieldSchema, ComponentFloat, ComponentMigrationError,
        ComponentRegistry, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueError,
        ComponentValueKind, MigratedComponentValue, PreparedComponent,
    };
    pub use nara_render::{
        Camera2d, ClearColor, Extent2d, RenderImage2d, RenderPlugin, RenderTarget, ViewportRect,
    };
    pub use nara_scene::{
        Children, HierarchyPlugin, InMemoryPrefabSourceResolver, Name, Parent, PrefabDocument,
        PrefabExpansionOptions, PrefabExpansionReport, PrefabInstance, PrefabInstantiationReport,
        PrefabSourceResolver, SceneAuthoringHistoryStatus, SceneAuthoringRevision,
        SceneAuthoringSession, SceneAuthoringSourceId, SceneAuthoringSyncReport,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityIdError, SceneEntityMap,
        SceneEntityRecord, SceneEntitySource, SceneExportOptions, SceneExportReport,
        SceneFormatError, SceneInstanceId, ScenePatchDocument, ScenePatchOperation,
        ScenePatchReport, SceneSpawnReport, SceneSpawner, Visibility, export_scene,
        export_scene_with_options, spawn_child, spawn_prefab, spawn_prefab_with_asset_database,
        spawn_prefab_with_patch, spawn_prefab_with_patch_and_asset_database, spawn_scene,
        spawn_scene_with_asset_database, spawn_scene_with_prefab_resolver,
        spawn_scene_with_prefab_resolver_and_asset_database, sync_children,
    };
    pub use nara_sprite::{Sprite, SpriteAnchor, SpriteMaterial, SpritePlugin, TextureRegion};
    pub use nara_sprite_render::SpriteRenderPlugin;
    pub use nara_tasks::{TaskPlugin, TaskPoolConfig};
    pub use nara_tilemap::{
        DEFAULT_CHUNK_SIZE, DEFAULT_TILE_SIZE, DirtyTileChunk, TileAtlasLayout, TileAtlasRegion,
        TileCell, TileChunkCoord, TileCoord, TileIndex, TileLayer, TileSet, TileSetMaterial,
        Tilemap, TilemapPlugin,
    };
    pub use nara_transform::{GlobalTransform2d, Transform2d, TransformPlugin};
    pub use nara_ui::{
        ComputedUiLayout, ComputedUiLayouts, UiInteractionState, UiNode, UiPanel, UiPanelMaterial,
        UiPlugin, UiRect, UiRoot, UiStyle, UiVal,
    };
    pub use nara_ui_render::UiRenderPlugin;
}

pub mod advanced_prelude {
    pub use crate::prelude::*;
    pub use nara_app::{
        PluginCapability, PluginCategory, PluginGroupId, PluginGroupMetadata, PluginId,
        PluginMetadata, TaskUpdateSet,
    };
    pub use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetDatabaseError, AssetDependencyGraph, AssetError,
        AssetEvent, AssetEventKind, AssetEvents, AssetLoadGeneration, AssetLoadGenerations,
        AssetMeta, AssetRecord, AssetReloadDiagnostics, AssetReloadRequest, AssetReloadRequestId,
        AssetReloadRequestKind, AssetReloadRequests, AssetSourceChange, AssetSourceChangeKind,
        AssetSourceChanges, AssetSourceKind, AssetSourceRoot, AssetState, AssetStateError,
        AssetStates, AssetVersion, DigestParseError, ImportArtifactDigest, ImportArtifactKey,
        ImportArtifactPath, ImportArtifactPathError, ImportArtifactRecord, ImportDependency,
        ImportDependencyDigest, ImportDependencyRole, ImportError, ImportJobInput,
        ImportLabelError, ImportLabelKind, ImportProfile, ImportRequest, ImportSettingsHash,
        ImportedAsset, ImportedAssetType, Importer, ImporterDescriptor, ImporterDescriptorError,
        ImporterId, ImporterRegistry, ImporterRegistryError, ImporterSelectionError,
        ImporterVersion, LoadState, MissingMetaPolicy, ProjectAssetDatabase, SourceChangeResolver,
        SourceExtension, SourceHash, TypedImporter, UnresolvedAssetSourceChange,
    };
    #[cfg(feature = "asset-watch")]
    pub use nara_asset_watch::{
        AssetWatchDiagnostic, AssetWatchDiagnosticKind, AssetWatchDiagnostics, AssetWatchError,
        AssetWatchEvent, AssetWatchEventKind, AssetWatchEventQueue, AssetWatchPlugin,
        AssetWatchQueueItem, AssetWatchTranslator, AssetWatcher,
    };
    pub use nara_image::{
        ImageImportError, ImageImportedAsset, ImageImporter, ImagePreparePlugin, ImagePrepareStats,
        ImageReloadError, ImageReloadStats, PreparedImageResource, image_descriptor_hash,
        image_resource_key, prepare_images,
    };
    pub use nara_render::{
        ExtractedView, ExtractedViews, FrameStats, PreparedRenderResource,
        PreparedRenderResourceRecord, PreparedRenderResources, RenderBackendState,
        RenderBackendStatus, RenderFrame, RenderFrameSkip, RenderFrameSkipReason, RenderFrameState,
        RenderPhaseLabel, RenderPrepareApplyResult, RenderPrepareError, RenderPrepareInvalidation,
        RenderPrepareInvalidationReason, RenderPrepareInvalidations, RenderPrepareStatus,
        RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
    };
    pub use nara_sprite_render::{
        ColorKey, ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
        QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance,
        SpriteMaterialKey, SpriteRenderStats, TextureUvRect,
    };
    pub use nara_tasks::{
        TaskCancellationToken, TaskExecutionMode, TaskHandle, TaskId, TaskPoolKind, TaskPoolStats,
        TaskPools, TaskResult, TaskResultState, TaskStats,
    };
    pub use nara_ui_render::{
        ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems,
        UiBatch, UiBatches, UiClipRect, UiInstance, UiMaterialKey, UiRenderStats, UiTextureRect,
    };
}

pub mod tooling_prelude {
    pub use nara_tooling::{
        SceneApplyChangesComponentReport, SceneApplyChangesComponentStatus,
        SceneApplyChangesReport, SceneApplyChangesRequest, SceneEditorMode, SceneEditorModel,
        SceneEditorState, SceneInspectorCommand, SceneInspectorCommandReport,
        SceneInspectorComponentView, SceneInspectorEntityRow, SceneInspectorEntityView,
        SceneInspectorFieldState, SceneInspectorFieldView, SceneInspectorModel,
        SceneInspectorState, ScenePlaySession, ScenePlayTransitionReport, ToolingPlugin,
        WorldSnapshot,
    };
    #[cfg(feature = "egui")]
    pub use nara_tooling_egui::{
        EguiSceneEditorAction, EguiSceneEditorPanel, EguiSceneEditorPanelResponse,
        EguiSceneInspectorPanel, EguiSceneInspectorPanelResponse,
    };
}

pub mod backend_prelude {
    #[cfg(feature = "wgpu")]
    pub use nara_render_wgpu::{
        SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuBackendState,
        WgpuRenderBackend, WgpuRenderError, WgpuRenderPlugin,
    };
    pub use nara_window::{
        PresentMode, PrimaryWindow, PrimaryWindowId, Window, WindowCloseRequest,
        WindowCloseRequests, WindowEvent, WindowEvents, WindowId, WindowMode, WindowPlugin,
        WindowResolution, apply_window_event, push_window_event,
    };
    #[cfg(feature = "winit")]
    pub use nara_winit::{WinitControlFlow, WinitPlugin, WinitRunner};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_plugins_install_only_headless_core_resources() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(
            !app.world()
                .contains_resource::<nara_sprite_render::SpriteBatches>()
        );
        assert!(!app.world().contains_resource::<nara_ui_render::UiBatches>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
    }

    #[test]
    fn runtime_2d_plugins_install_render_and_submitter_resources() {
        let mut app = App::new();

        app.add_plugins(Runtime2dPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(
            app.world()
                .contains_resource::<nara_sprite_render::SpriteBatches>()
        );
        assert!(app.world().contains_resource::<nara_ui_render::UiBatches>());
    }
}
