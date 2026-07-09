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

use nara_app::{App, Plugin, PluginError};

/// Minimal runtime defaults for headless examples, tests, and AI-generated scenes.
///
/// Platform windows and GPU backends are intentionally excluded. They should be
/// installed by dedicated backend plugins.
#[derive(Debug, Default, Clone, Copy)]
pub struct MinimalPlugins;

impl Plugin for MinimalPlugins {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        app.add_plugin_if_missing(nara_tasks::TaskPlugin::default())?;
        app.add_plugin_if_missing(nara_asset::AssetPlugin)?;
        app.add_plugin_if_missing(nara_transform::TransformPlugin)?;
        app.add_plugin_if_missing(nara_input::InputPlugin)?;
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

pub mod prelude {
    pub use crate::MinimalPlugins;
    pub use nara_app::{
        App, AppExit, AppRunError, CoreStage, FixedTime, Plugin, PluginError, StartupStage,
        TaskUpdateSet, Time,
    };
    pub use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, Asset, AssetDatabaseError, AssetDependencyGraph,
        AssetError, AssetEvent, AssetEventKind, AssetEvents, AssetId, AssetLoadGeneration,
        AssetLoadGenerations, AssetMeta, AssetPath, AssetPathError, AssetPlugin, AssetRecord,
        AssetRef, AssetRefError, AssetRefExportPolicy, AssetReloadRequest, AssetReloadRequestId,
        AssetReloadRequestKind, AssetReloadRequests, AssetServer, AssetSourceChange,
        AssetSourceChangeKind, AssetSourceChanges, AssetSourceKind, AssetSourceRoot, AssetState,
        AssetStateError, AssetStates, AssetVersion, Assets, DigestParseError, Handle,
        ImportArtifactDigest, ImportArtifactKey, ImportArtifactPath, ImportArtifactPathError,
        ImportArtifactRecord, ImportDependency, ImportDependencyDigest, ImportDependencyRole,
        ImportError, ImportJobInput, ImportLabelError, ImportLabelKind, ImportProfile,
        ImportRequest, ImportSettingsHash, ImportedAsset, ImportedAssetType, Importer,
        ImporterDescriptor, ImporterDescriptorError, ImporterId, ImporterRegistry,
        ImporterRegistryError, ImporterSelectionError, ImporterVersion, LoadState,
        MissingMetaPolicy, ProjectAssetDatabase, SourceChangeResolver, SourceExtension, SourceHash,
        StableAssetId, StableAssetIdError, TypedImporter, UnresolvedAssetSourceChange,
    };
    #[cfg(feature = "asset-watch")]
    pub use nara_asset_watch::{
        AssetWatchError, AssetWatchEvent, AssetWatchEventKind, AssetWatchEventQueue,
        AssetWatchPlugin, AssetWatchTranslator, AssetWatcher,
    };
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_diagnostic::{Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity};
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageImportError,
        ImageImportedAsset, ImageImporter, ImagePlugin, ImagePreparePlugin, ImagePrepareStats,
        ImageReloadError, ImageReloadStats, ImageSourceMetadata, PreparedImageResource,
        image_descriptor_hash, image_resource_key, prepare_images,
    };
    pub use nara_input::{ButtonInput, InputPlugin, KeyCode, MouseButton, PointerState};
    pub use nara_material::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDescriptor, Material2dKey,
        SamplerDescriptor, material2d_descriptor_key,
    };
    pub use nara_reflect::{
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
        ComponentFieldSchema, ComponentFloat, ComponentMigrationError, ComponentRegistry,
        ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, ComponentValueError, ComponentValueKind,
        MigratedComponentValue, PreparedComponent,
    };
    pub use nara_render::{
        Camera2d, ClearColor, Extent2d, ExtractedView, ExtractedViews, FrameStats,
        PreparedRenderResource, PreparedRenderResourceRecord, PreparedRenderResources,
        RenderBackendState, RenderBackendStatus, RenderFrame, RenderFrameSkip,
        RenderFrameSkipReason, RenderFrameState, RenderImage2d, RenderPhaseLabel, RenderPlugin,
        RenderPrepareApplyResult, RenderPrepareError, RenderPrepareInvalidation,
        RenderPrepareInvalidationReason, RenderPrepareInvalidations, RenderPrepareStatus,
        RenderResourceKey, RenderResourceKind, RenderResourceSnapshot, RenderTarget, ViewportRect,
    };
    #[cfg(feature = "wgpu")]
    pub use nara_render_wgpu::{
        SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuBackendState,
        WgpuRenderBackend, WgpuRenderError, WgpuRenderPlugin,
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
    pub use nara_sprite_render::{
        ColorKey, ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
        QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance,
        SpriteMaterialKey, SpriteRenderPlugin, SpriteRenderStats, TextureUvRect,
    };
    pub use nara_tasks::{
        TaskCancellationToken, TaskExecutionMode, TaskHandle, TaskId, TaskPlugin, TaskPoolConfig,
        TaskPoolKind, TaskPoolStats, TaskPools, TaskResult, TaskResultState, TaskStats,
    };
    pub use nara_tilemap::{
        DEFAULT_CHUNK_SIZE, DEFAULT_TILE_SIZE, DirtyTileChunk, TileAtlasLayout, TileAtlasRegion,
        TileCell, TileChunkCoord, TileCoord, TileIndex, TileLayer, TileSet, TileSetMaterial,
        Tilemap, TilemapPlugin,
    };
    pub use nara_tooling::{
        SceneApplyChangesReport, SceneEditorMode, SceneEditorModel, SceneEditorState,
        SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorComponentView,
        SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
        SceneInspectorFieldView, SceneInspectorModel, SceneInspectorState, ScenePlaySession,
        ScenePlayTransitionReport, ToolingPlugin, WorldSnapshot,
    };
    #[cfg(feature = "egui")]
    pub use nara_tooling_egui::{
        EguiSceneEditorAction, EguiSceneEditorPanel, EguiSceneEditorPanelResponse,
        EguiSceneInspectorPanel, EguiSceneInspectorPanelResponse,
    };
    pub use nara_transform::{GlobalTransform2d, Transform2d, TransformPlugin};
    pub use nara_ui::{
        ComputedUiLayout, ComputedUiLayouts, UiInteractionState, UiNode, UiPanel, UiPanelMaterial,
        UiPlugin, UiRect, UiRoot, UiStyle, UiVal,
    };
    pub use nara_ui_render::{
        ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems,
        UiBatch, UiBatches, UiClipRect, UiInstance, UiMaterialKey, UiRenderPlugin, UiRenderStats,
        UiTextureRect,
    };
    pub use nara_window::{
        PresentMode, PrimaryWindow, PrimaryWindowId, Window, WindowEvent, WindowEvents, WindowId,
        WindowMode, WindowPlugin, WindowResolution, apply_window_event, push_window_event,
    };
    #[cfg(feature = "winit")]
    pub use nara_winit::{WinitControlFlow, WinitPlugin, WinitRunner};
}
