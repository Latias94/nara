//! Public facade for the nara engine workspace.

pub use nara_app as app;
pub use nara_asset as asset;
#[cfg(feature = "asset-watch")]
pub use nara_asset_watch as asset_watch;
pub use nara_audio as audio;
pub use nara_core as core;
pub use nara_diagnostic as diagnostic;
pub use nara_ecs as ecs;
pub use nara_gameplay as gameplay;
pub use nara_image as image;
pub use nara_input as input;
pub use nara_material as material;
pub use nara_project as project;
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

use nara_app::{
    App, PluginError, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginGroupMetadata, PluginId,
};

const HIERARCHY_PLUGIN_ID: PluginId = PluginId::new("nara.scene.hierarchy");
const DIAGNOSTIC_PLUGIN_ID: PluginId = PluginId::new("nara.diagnostic");
const TASK_PLUGIN_ID: PluginId = PluginId::new("nara.tasks");
const ASSET_PLUGIN_ID: PluginId = PluginId::new("nara.asset");
const TRANSFORM_PLUGIN_ID: PluginId = PluginId::new("nara.transform");
const INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.input");
const GAMEPLAY_COMMAND_PLUGIN_ID: PluginId = PluginId::new("nara.gameplay.commands");
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
    DIAGNOSTIC_PLUGIN_ID,
    TASK_PLUGIN_ID,
    ASSET_PLUGIN_ID,
    TRANSFORM_PLUGIN_ID,
    INPUT_PLUGIN_ID,
];

const HEADLESS_RUNTIME_PLUGIN_IDS: &[PluginId] = &[
    HIERARCHY_PLUGIN_ID,
    DIAGNOSTIC_PLUGIN_ID,
    TASK_PLUGIN_ID,
    ASSET_PLUGIN_ID,
    TRANSFORM_PLUGIN_ID,
    INPUT_PLUGIN_ID,
    GAMEPLAY_COMMAND_PLUGIN_ID,
];

const SERVER_PLUGIN_IDS: &[PluginId] = &[
    HIERARCHY_PLUGIN_ID,
    DIAGNOSTIC_PLUGIN_ID,
    TASK_PLUGIN_ID,
    ASSET_PLUGIN_ID,
    TRANSFORM_PLUGIN_ID,
    GAMEPLAY_COMMAND_PLUGIN_ID,
];

const RUNTIME_2D_PLUGIN_IDS: &[PluginId] = &[
    HIERARCHY_PLUGIN_ID,
    DIAGNOSTIC_PLUGIN_ID,
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

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        group.add_plugin_if_missing(nara_diagnostic::DiagnosticsPlugin::default())?;
        group.add_plugin_if_missing(nara_tasks::TaskPlugin::default())?;
        group.add_plugin_if_missing(nara_asset::AssetPlugin)?;
        group.add_plugin_if_missing(nara_transform::TransformPlugin)?;
        group.add_plugin_if_missing(nara_input::InputPlugin)?;
        Ok(())
    }
}

/// Headless runtime defaults for tests, AI drivers, and non-windowed game logic.
///
/// This group keeps low-level input observations available for local drivers,
/// but adds semantic gameplay command resources so gameplay systems can consume
/// commands instead of raw keyboard or pointer state.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeadlessRuntimePlugins;

impl PluginGroup for HeadlessRuntimePlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(
            PluginGroupId::new("nara.plugins.headless-runtime"),
            HEADLESS_RUNTIME_PLUGIN_IDS,
        )
    }

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugins(MinimalPlugins)?;
        group.add_plugin_if_missing(nara_gameplay::GameplayCommandPlugin)?;
        Ok(())
    }
}

/// Dedicated-server-ready defaults without desktop, render, editor, audio, or raw input plugins.
///
/// Networking is intentionally not included. Server producers should write
/// semantic commands into `GameplayCommandQueue`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServerPlugins;

impl PluginGroup for ServerPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(PluginGroupId::new("nara.plugins.server"), SERVER_PLUGIN_IDS)
    }

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        group.add_plugin_if_missing(nara_diagnostic::DiagnosticsPlugin::default())?;
        group.add_plugin_if_missing(nara_tasks::TaskPlugin::deterministic())?;
        group.add_plugin_if_missing(nara_asset::AssetPlugin)?;
        group.add_plugin_if_missing(nara_transform::TransformPlugin)?;
        group.add_plugin_if_missing(nara_gameplay::GameplayCommandPlugin)?;
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

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugins(MinimalPlugins)?;
        group.add_plugin_if_missing(nara_sprite::SpritePlugin)?;
        group.add_plugin_if_missing(nara_tilemap::TilemapPlugin)?;
        group.add_plugin_if_missing(nara_render::RenderPlugin)?;
        group.add_plugin_if_missing(nara_image::ImagePlugin)?;
        group.add_plugin_if_missing(nara_sprite_render::SpriteRenderPlugin)?;
        group.add_plugin_if_missing(nara_ui::UiPlugin)?;
        group.add_plugin_if_missing(nara_ui_render::UiRenderPlugin)?;
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

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugin_if_missing(nara_window::WindowPlugin::default())?;
        #[cfg(feature = "winit")]
        group.add_plugin_if_missing(nara_winit::WinitPlugin::default())?;
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

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugins(Runtime2dPlugins)?;
        group.add_plugins(DesktopWindowPlugins)?;
        group.add_plugin_if_missing(nara_render_wgpu::WgpuRenderPlugin)?;
        group.add_plugin_if_missing(nara_sprite_render::SpriteRenderPlugin)?;
        group.add_plugin_if_missing(nara_ui_render::UiRenderPlugin)?;
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

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugin_if_missing(nara_tooling::ToolingPlugin)?;
        Ok(())
    }
}

pub fn add_project_plugin_plan(
    app: &mut App,
    plan: nara_project::ProjectPluginPlan,
) -> Result<&mut App, PluginError> {
    match plan {
        nara_project::ProjectPluginPlan::Minimal => app.add_plugins(MinimalPlugins),
        nara_project::ProjectPluginPlan::HeadlessRuntime => app.add_plugins(HeadlessRuntimePlugins),
        nara_project::ProjectPluginPlan::Server => app.add_plugins(ServerPlugins),
        nara_project::ProjectPluginPlan::Runtime2d => app.add_plugins(Runtime2dPlugins),
        nara_project::ProjectPluginPlan::DesktopWindow => app.add_plugins(DesktopWindowPlugins),
        nara_project::ProjectPluginPlan::DesktopWgpu => add_desktop_wgpu_plugin_plan(app),
        nara_project::ProjectPluginPlan::Tooling => app.add_plugins(ToolingPlugins),
    }
}

#[cfg(feature = "wgpu")]
fn add_desktop_wgpu_plugin_plan(app: &mut App) -> Result<&mut App, PluginError> {
    app.add_plugins(DesktopWgpuPlugins)
}

#[cfg(not(feature = "wgpu"))]
fn add_desktop_wgpu_plugin_plan(_app: &mut App) -> Result<&mut App, PluginError> {
    Err(PluginError::SetupFailed {
        plugin: PluginId::new("nara.project.plugin-plan"),
        message: "desktop-wgpu project plugin plan requires the 'wgpu' feature".to_owned(),
    })
}

pub mod prelude {
    #[cfg(feature = "wgpu")]
    pub use crate::DesktopWgpuPlugins;
    pub use crate::{
        DesktopWindowPlugins, HeadlessRuntimePlugins, MinimalPlugins, Runtime2dPlugins,
        ServerPlugins, add_project_plugin_plan,
    };
    pub use nara_app::{
        App, AppExit, AppExitRequests, AppFrameOutcome, AppRunError, CoreStage, FixedTime, Plugin,
        PluginCleanupContext, PluginError, PluginGroup, PluginGroupBuilder, RealTime, RenderTime,
        RuntimeFrameStatus, RuntimeTimeSettings, StartupStage, VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetPathError, AssetPlugin, AssetRef, AssetRefError,
        AssetRefExportPolicy, AssetServer, Assets, Handle, StableAssetId, StableAssetIdError,
    };
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_diagnostic::{
        Diagnostic, DiagnosticCode, DiagnosticReport, DiagnosticSeverity, DiagnosticsPlugin,
        RuntimeDiagnosticContext, RuntimeDiagnosticDomain, RuntimeDiagnosticEntry,
        RuntimeDiagnosticFilter, RuntimeDiagnostics, RuntimeDiagnosticsSettings,
    };
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandEnvelope, GameplayCommandIdError,
        GameplayCommandPayload, GameplayCommandPayloadError, GameplayCommandPlugin,
        GameplayCommandQueue, GameplayCommandSet, GameplayCommandSource, GameplayCommandTarget,
        GameplayCommandTime, GameplayCommandTypeId, GameplayCommandValue, PersistentRuntimeId,
        SceneStableId,
    };
    pub use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImagePlugin, ImageSourceMetadata,
    };
    pub use nara_input::{
        ActionBinding, ActionContext, ActionId, ActionIdError, ActionMap, ActionOutcome,
        ActionOutcomes, ActionPhase, ActionValue, ButtonInput, InputBinding, InputPlugin, KeyCode,
        MouseButton, PointerState,
    };
    pub use nara_material::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDescriptor, Material2dKey,
        SamplerDescriptor, material2d_descriptor_key,
    };
    pub use nara_project::{
        EffectiveDiagnosticsSettings, EffectiveInputSettings, EffectiveProjectInfo,
        EffectiveProjectPaths, EffectiveProjectSettings, EffectiveRuntimeSettings,
        EffectiveStartupSettings, EffectiveTaskSettings, EffectiveWindowSettings,
        ProjectDiagnosticsManifest, ProjectInfo, ProjectInputManifest, ProjectManifest,
        ProjectManifestLoad, ProjectPath, ProjectPathError, ProjectPathsManifest,
        ProjectPluginPlan, ProjectProfileError, ProjectProfileKind, ProjectProfileOverlay,
        ProjectRuntimeManifest, ProjectStartupManifest, ProjectTaskExecutionMode,
        ProjectTasksManifest, ProjectWindowManifest,
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
        ComputedUiLayout, ComputedUiLayouts, UiInteractionState, UiInteractionTarget, UiNode,
        UiPanel, UiPanelMaterial, UiPlugin, UiPointerRoute, UiRect, UiRoot, UiStyle, UiVal,
    };
    pub use nara_ui_render::UiRenderPlugin;
}

pub mod advanced_prelude {
    pub use crate::prelude::*;
    pub use nara_app::{
        PluginCapability, PluginCategory, PluginCleanupError, PluginFailure, PluginFailureReport,
        PluginFailureSubject, PluginGroupId, PluginGroupMetadata, PluginHook, PluginId,
        PluginLifecycleState, PluginMetadata, TaskUpdateSet,
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
        UiBatch, UiBatches, UiClipRect, UiColorKey, UiInstance, UiMaterialKey, UiRenderStats,
        UiTextureRect,
    };
}

pub mod tooling_prelude {
    pub use nara_tooling::{
        EditorDocumentId, EditorExternalReloadState, EditorSceneModel, EditorSceneSlot,
        EditorSceneTabModel, EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand,
        EditorWorkspaceCommandReport, EditorWorkspaceModel, SceneApplyChangesComponentReport,
        SceneApplyChangesComponentStatus, SceneApplyChangesReport, SceneApplyChangesRequest,
        SceneEditorMode, SceneEditorModel, SceneEditorState, SceneInspectorCommand,
        SceneInspectorCommandReport, SceneInspectorComponentView, SceneInspectorEntityRow,
        SceneInspectorEntityView, SceneInspectorFieldState, SceneInspectorFieldView,
        SceneInspectorModel, SceneInspectorState, ScenePlaySession, ScenePlayTransitionReport,
        ToolingPlugin, WorldSnapshot,
    };
    #[cfg(feature = "egui")]
    pub use nara_tooling_egui::{
        EguiSceneEditorPanel, EguiSceneEditorPanelResponse, EguiSceneInspectorPanel,
        EguiSceneInspectorPanelResponse,
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

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
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
        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id == PluginGroupId::new("nara.plugins.runtime-2d"))
            .unwrap();
        assert!(metadata.plugins.contains(&DIAGNOSTIC_PLUGIN_ID));
    }

    #[test]
    fn headless_runtime_plugins_install_command_resources() {
        let mut app = App::new();

        app.add_plugins(HeadlessRuntimePlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_gameplay::GameplayCommandQueue>()
        );
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
    }

    #[test]
    fn server_plugins_install_command_runtime_without_desktop_or_raw_input() {
        let mut app = App::new();

        app.add_plugins(ServerPlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_gameplay::GameplayCommandQueue>()
        );
        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert_eq!(
            app.world()
                .resource::<nara_tasks::TaskPools>()
                .config()
                .execution_mode(),
            nara_tasks::TaskExecutionMode::Deterministic
        );
        assert!(!app.world().contains_resource::<nara_input::PointerState>());
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
        assert!(
            !app.world()
                .contains_resource::<nara_tooling::EditorWorkspace>()
        );
    }

    #[derive(Debug, Default, nara_ecs::Resource)]
    struct ObservedServerCommands(Vec<nara_gameplay::GameplayCommandEnvelope>);

    fn observe_server_commands(
        queue: nara_ecs::Res<nara_gameplay::GameplayCommandQueue>,
        mut observed: nara_ecs::ResMut<ObservedServerCommands>,
    ) {
        observed.0 = queue.as_slice().to_vec();
    }

    #[test]
    fn server_plugins_run_without_action_outcomes_and_keep_manual_commands_observable() {
        let mut app = App::new();
        app.add_plugins(ServerPlugins).unwrap();
        app.insert_resource(ObservedServerCommands::default())
            .expect("server observation resource should install")
            .add_systems(nara_app::CoreStage::Update, observe_server_commands)
            .expect("server observation system should install");
        app.world_mut()
            .expect("a configured app should allow world mutation")
            .resource_mut::<nara_gameplay::GameplayCommandQueue>()
            .push(nara_gameplay::GameplayCommandEnvelope::new(
                nara_gameplay::GameplayCommandTypeId::new("server.tick").unwrap(),
                nara_gameplay::GameplayCommandSource::External {
                    producer: "test-server".to_owned(),
                },
                nara_gameplay::GameplayCommandTime::default(),
            ));

        app.run_once(std::time::Duration::ZERO).unwrap();

        assert!(
            !app.world()
                .contains_resource::<nara_input::ActionOutcomes>()
        );
        assert_eq!(
            app.world().resource::<ObservedServerCommands>().0[0]
                .command_type
                .as_str(),
            "server.tick"
        );
        assert!(
            app.world()
                .resource::<nara_gameplay::GameplayCommandQueue>()
                .is_empty()
        );
    }

    #[test]
    fn server_plugins_preserve_explicit_task_plugin_configuration() {
        let mut app = App::new();
        app.add_plugin(nara_tasks::TaskPlugin::new(
            nara_tasks::TaskPoolConfig::threaded(1, 1, 1),
        ))
        .unwrap();

        app.add_plugins(ServerPlugins).unwrap();

        assert_eq!(
            app.world()
                .resource::<nara_tasks::TaskPools>()
                .config()
                .execution_mode(),
            nara_tasks::TaskExecutionMode::Threaded
        );
    }

    #[test]
    fn project_plugin_plan_server_maps_to_server_plugins() {
        let mut app = App::new();

        add_project_plugin_plan(&mut app, nara_project::ProjectPluginPlan::Server).unwrap();

        assert!(
            app.installed_plugin_groups()
                .any(|group| group.id == PluginGroupId::new("nara.plugins.server"))
        );
        assert!(
            app.world()
                .contains_resource::<nara_gameplay::GameplayCommandQueue>()
        );
    }
}
