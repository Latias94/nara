//! Public facade for the nara engine workspace.

pub use nara_app as app;
pub use nara_asset as asset;
#[cfg(feature = "asset-watch")]
pub use nara_asset_watch as asset_watch;
pub use nara_audio as audio;
pub use nara_core as core;
pub use nara_diagnostic as diagnostic;
pub use nara_ecs as ecs;
pub use nara_fs as fs;
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
    App, Plugin, PluginError, PluginGroup, PluginGroupBuilder, PluginGroupId, PluginGroupMetadata,
    PluginId,
};

const HIERARCHY_PLUGIN_ID: PluginId = PluginId::new("nara.scene.hierarchy");
const DIAGNOSTIC_PLUGIN_ID: PluginId = PluginId::new("nara.diagnostic");
const TASK_PLUGIN_ID: PluginId = PluginId::new("nara.tasks");
const ASSET_PLUGIN_ID: PluginId = PluginId::new("nara.asset");
const TRANSFORM_PLUGIN_ID: PluginId = PluginId::new("nara.transform");
const INPUT_PLUGIN_ID: PluginId = PluginId::new("nara.input");
const GAMEPLAY_COMMAND_PLUGIN_ID: PluginId = PluginId::new("nara.gameplay.commands");
const SERVER_TIME_POLICY_PLUGIN_ID: PluginId = PluginId::new("nara.server-time-policy");
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
    SERVER_TIME_POLICY_PLUGIN_ID,
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
        group.add_plugin_if_missing(nara_gameplay::GameplayCommandPlugin::default())?;
        Ok(())
    }
}

/// Dedicated-server-ready defaults without desktop, render, editor, audio, or raw input plugins.
///
/// Networking is intentionally not included. Server producers should write
/// semantic commands into `GameplayCommandQueue`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ServerPlugins;

#[derive(Debug, Default, Clone, Copy)]
struct ServerTimePolicyPlugin;

impl Plugin for ServerTimePolicyPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(SERVER_TIME_POLICY_PLUGIN_ID, nara_app::PluginCategory::Core)
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let Some(mut fixed_time) = app.world_mut()?.get_resource_mut::<nara_app::FixedTime>()
        else {
            return Err(PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: "server time policy requires FixedTime".to_owned(),
            });
        };
        fixed_time
            .set_catch_up_policy(nara_app::FixedCatchUpPolicy::PreserveDebt)
            .map_err(|error| PluginError::SetupFailed {
                plugin: SERVER_TIME_POLICY_PLUGIN_ID,
                message: error.to_string(),
            })?;
        Ok(())
    }
}

impl PluginGroup for ServerPlugins {
    fn metadata(&self) -> PluginGroupMetadata {
        PluginGroupMetadata::new(PluginGroupId::new("nara.plugins.server"), SERVER_PLUGIN_IDS)
    }

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
        group.add_plugin_if_missing(ServerTimePolicyPlugin)?;
        group.add_plugin_if_missing(nara_scene::HierarchyPlugin)?;
        group.add_plugin_if_missing(nara_diagnostic::DiagnosticsPlugin::default())?;
        group.add_plugin_if_missing(nara_tasks::TaskPlugin::default())?;
        group.add_plugin_if_missing(nara_asset::AssetPlugin)?;
        group.add_plugin_if_missing(nara_transform::TransformPlugin)?;
        group.add_plugin_if_missing(nara_gameplay::GameplayCommandPlugin::default())?;
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

/// Additive desktop window adapters for an app that already has its runtime core.
///
/// This group intentionally reports and installs only window/platform plugins. Use
/// [`add_project_plugin_plan`] for the complete `desktop-window` product plan.
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

/// Additive editor tooling adapters for an app that already has its runtime core.
///
/// This group intentionally reports and installs only tooling plugins. Use
/// [`add_project_plugin_plan`] for the complete `tooling` product plan.
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

/// Installs a complete product plan, composing the core runtime with any additive adapters.
pub fn add_project_plugin_plan(
    app: &mut App,
    plan: nara_project::ProjectPluginPlan,
) -> Result<&mut App, PluginError> {
    match plan {
        nara_project::ProjectPluginPlan::Minimal => app.add_plugins(MinimalPlugins),
        nara_project::ProjectPluginPlan::HeadlessRuntime => app.add_plugins(HeadlessRuntimePlugins),
        nara_project::ProjectPluginPlan::Server => app.add_plugins(ServerPlugins),
        nara_project::ProjectPluginPlan::Runtime2d => app.add_plugins(Runtime2dPlugins),
        nara_project::ProjectPluginPlan::DesktopWindow => {
            app.add_plugins(MinimalPlugins)?;
            app.add_plugins(DesktopWindowPlugins)
        }
        nara_project::ProjectPluginPlan::DesktopWgpu => add_desktop_wgpu_plugin_plan(app),
        nara_project::ProjectPluginPlan::Tooling => {
            app.add_plugins(MinimalPlugins)?;
            app.add_plugins(ToolingPlugins)
        }
    }
}

/// Applies validated project settings before installing the selected product bundle.
pub fn apply_project_settings(
    app: &mut App,
    settings: nara_project::EffectiveProjectSettings,
) -> Result<&mut App, PluginError> {
    let plan = settings.plugin_plan;
    let runtime_time = settings.runtime.runtime_time_settings();
    let fixed_time = settings.runtime.fixed_time();
    let task_config = settings.tasks.pool_config;
    let diagnostics = settings.diagnostics.runtime;

    app.insert_resource(settings)?;
    app.insert_resource(runtime_time)?;
    app.insert_resource(fixed_time)?;
    app.add_plugin(nara_diagnostic::DiagnosticsPlugin::new(
        diagnostics,
        nara_diagnostic::RuntimePressureSettings::default(),
    ))?;
    app.add_plugin(nara_tasks::TaskPlugin::new(task_config))?;
    add_project_plugin_plan(app, plan)
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
        ServerPlugins, add_project_plugin_plan, apply_project_settings,
    };
    pub use nara_app::{
        App, AppExit, AppExitRequests, AppFrameOutcome, AppRunError, CoreStage, FixedCatchUpPolicy,
        FixedTime, FixedTimeError, FixedUpdateSet, Plugin, PluginCleanupContext, PluginError,
        PluginGroup, PluginGroupBuilder, RealTime, RenderTime, RuntimeFrameStatus,
        RuntimeTimeSettings, StartupStage, TimeFrameError, TimeFrameResource, TimeSettingsError,
        VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetPathError, AssetPlugin, AssetRef, AssetRefError,
        AssetRefExportPolicy, AssetServer, Assets, Handle, StableAssetId, StableAssetIdError,
    };
    pub use nara_audio::{AudioClip, AudioCommand, AudioSink};
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_diagnostic::{
        Diagnostic, DiagnosticBuildError, DiagnosticCode, DiagnosticDedupePolicy, DiagnosticDomain,
        DiagnosticField, DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer,
        DiagnosticReport, DiagnosticReportSettings, DiagnosticSeverity, DiagnosticsPlugin,
        PressureMeasurement, PressureMetricId, PressureMetricKind, PressureSourceId, PressureUnit,
        PublicDiagnosticIdentifier, RuntimeDiagnosticDraft, RuntimeDiagnosticEntry,
        RuntimeDiagnosticFilter, RuntimeDiagnosticRetention, RuntimeDiagnostics,
        RuntimeDiagnosticsSettings, RuntimePressureSettings, RuntimePressureSnapshotDraft,
        RuntimePressureSnapshots, SafeDisplayText, SafeSummary,
    };
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_gameplay::{
        ActionCommandBinding, ActionCommandMap, ActionCommandMapError, GameplayCommandBatch,
        GameplayCommandDraft, GameplayCommandEnvelope, GameplayCommandIdError,
        GameplayCommandIngressSource, GameplayCommandKey, GameplayCommandLifecycleError,
        GameplayCommandLimitKind, GameplayCommandPayload, GameplayCommandPayloadError,
        GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandQueueSettings,
        GameplayCommandQueueStats, GameplayCommandRejection, GameplayCommandSet,
        GameplayCommandSettingsError, GameplayCommandSource, GameplayCommandSourceId,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTarget,
        GameplayCommandTargetId, GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
        MAX_ACTION_COMMAND_BINDINGS, PersistentRuntimeId, SceneStableId,
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
        ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy, ProjectInfo, ProjectInputManifest,
        ProjectManifest, ProjectManifestLoad, ProjectPath, ProjectPathError, ProjectPathsManifest,
        ProjectPluginPlan, ProjectProfileError, ProjectProfileKind, ProjectProfileOverlay,
        ProjectRuntimeManifest, ProjectStartupManifest, ProjectTaskPoolManifest,
        ProjectTaskShutdownManifest, ProjectTasksManifest, ProjectWindowManifest,
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
    pub use nara_core::{ByteLimit, DepthLimit, ItemLimit, TimeLimit};
    pub use nara_fs::{
        CapabilityGeneration, CapabilityReader, CapabilityRights, CapabilitySessionId,
        ConflictProtection, ContentDigest, DigestLimit, DirectoryCapability,
        DirectoryEntryObservation, DirectorySyncReceipt, DirectorySyncTier, DurabilityProgress,
        ExpectedTarget, FileCapability, FileIdentity, FileKind, FileLock, FileSyncReceipt,
        FileSyncTier, FsError, FsOperation, HostCapabilityOptions, LockGuarantee, LockMode,
        LockScope, ParentAuthorizationTier, PathValidationError, PlatformCapabilityMatrix,
        ProofStatus, PublicationAtomicity, PublicationIdentityEvidence, RelativeComponent,
        RelativePath, ReplaceReceipt, ReplaceSourceBinding, ResolutionTier, StageStatus,
        TemporaryFile, TrustMode, platform_capability_matrix,
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
        OrderedTaskResults, OrderedTaskTerminal, TaskCancellation, TaskCancellationReason,
        TaskCancellationToken, TaskCoalesceKey, TaskConfigError, TaskDescriptor, TaskDomainKey,
        TaskFailure, TaskHandle, TaskId, TaskInlineRunReport, TaskKindConfig, TaskOrderKey,
        TaskOverloadPolicy, TaskPoolError, TaskPoolKind, TaskPoolShutdownReport, TaskPoolStats,
        TaskPools, TaskRejectReason, TaskRejection, TaskShutdownPolicy, TaskShutdownReport,
        TaskSpawnOutcome, TaskSpawnRequest, TaskStats, TaskTerminal, TaskTerminalState,
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
    use nara_ecs::schedule::IntoScheduleConfigs;

    #[test]
    fn minimal_plugins_install_only_headless_core_resources() {
        let mut app = App::new();

        app.add_plugins(MinimalPlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimePressureSnapshots>()
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
    fn desktop_window_plugins_remain_an_additive_adapter_group() {
        let mut app = App::new();

        app.add_plugins(DesktopWindowPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_window::WindowEvents>());
        assert!(
            !app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(!app.world().contains_resource::<nara_tasks::TaskPools>());
        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id == PluginGroupId::new("nara.plugins.desktop-window"))
            .unwrap();
        assert_eq!(metadata.plugins, DESKTOP_WINDOW_PLUGIN_IDS);
        assert!(!metadata.plugins.contains(&DIAGNOSTIC_PLUGIN_ID));
    }

    #[test]
    fn tooling_plugins_remain_an_additive_adapter_group() {
        let mut app = App::new();

        app.add_plugins(ToolingPlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_tooling::EditorWorkspace>()
        );
        assert!(
            !app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(!app.world().contains_resource::<nara_tasks::TaskPools>());
        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id == PluginGroupId::new("nara.plugins.tooling"))
            .unwrap();
        assert_eq!(metadata.plugins, TOOLING_PLUGIN_IDS);
        assert!(!metadata.plugins.contains(&DIAGNOSTIC_PLUGIN_ID));
    }

    #[test]
    fn desktop_window_project_plan_composes_core_with_additive_window_adapters() {
        let mut app = App::new();

        add_project_plugin_plan(&mut app, nara_project::ProjectPluginPlan::DesktopWindow).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimePressureSnapshots>()
        );
        assert!(app.world().contains_resource::<nara_tasks::TaskPools>());
        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        assert!(app.world().contains_resource::<nara_window::WindowEvents>());
    }

    #[test]
    fn tooling_project_plan_composes_core_with_additive_tooling_adapters() {
        let mut app = App::new();

        add_project_plugin_plan(&mut app, nara_project::ProjectPluginPlan::Tooling).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_diagnostic::RuntimePressureSnapshots>()
        );
        assert!(app.world().contains_resource::<nara_tasks::TaskPools>());
        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        assert!(
            app.world()
                .contains_resource::<nara_tooling::EditorWorkspace>()
        );
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
                .contains_resource::<nara_diagnostic::RuntimePressureSnapshots>()
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
                .contains_resource::<nara_diagnostic::RuntimePressureSnapshots>()
        );
        assert!(
            app.world()
                .contains_resource::<nara_gameplay::GameplayCommandQueue>()
        );
        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(
            app.world()
                .resource::<nara_tasks::TaskPools>()
                .config()
                .kind(nara_tasks::TaskPoolKind::Io)
                .workers()
                .get()
                > 0
        );
        assert_eq!(
            app.world()
                .resource::<nara_app::FixedTime>()
                .catch_up_policy(),
            nara_app::FixedCatchUpPolicy::PreserveDebt
        );
        assert!(!app.world().contains_resource::<nara_input::PointerState>());
        assert!(
            !app.world()
                .contains_resource::<nara_input::ButtonInput<nara_input::KeyCode>>()
        );
        assert!(
            !app.world()
                .contains_resource::<nara_input::ButtonInput<nara_input::MouseButton>>()
        );
        assert!(!app.world().contains_resource::<nara_input::ActionMap>());
        assert!(
            !app.world()
                .contains_resource::<nara_input::ActionOutcomes>()
        );
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
        assert!(
            !app.world()
                .contains_resource::<nara_tooling::EditorWorkspace>()
        );
    }

    #[derive(Debug, Default, nara_ecs::Resource)]
    struct ObservedServerCommands(Vec<(u64, Vec<nara_gameplay::GameplayCommandEnvelope>)>);

    fn observe_server_commands(
        batch: nara_ecs::Res<nara_gameplay::GameplayCommandBatch>,
        fixed_time: nara_ecs::Res<nara_app::FixedTime>,
        mut observed: nara_ecs::ResMut<ObservedServerCommands>,
    ) {
        observed
            .0
            .push((fixed_time.tick(), batch.commands().to_vec()));
    }

    #[test]
    fn server_plugins_retain_manual_commands_until_their_authoritative_tick() {
        let mut app = App::new();
        app.add_plugins(ServerPlugins).unwrap();
        app.insert_resource(ObservedServerCommands::default())
            .expect("server observation resource should install")
            .add_systems(
                nara_app::CoreStage::FixedUpdate,
                observe_server_commands.in_set(nara_gameplay::GameplayCommandSet::Consume),
            )
            .expect("server observation system should install");
        app.world_mut()
            .expect("a configured app should allow world mutation")
            .resource_mut::<nara_gameplay::GameplayCommandQueue>()
            .submit(nara_gameplay::GameplayCommandSubmission::new(
                nara_gameplay::GameplayCommandTick::new(1).unwrap(),
                nara_gameplay::GameplayCommandIngressSource::external("test-server").unwrap(),
                nara_gameplay::GameplayCommandSourceSequence::new(1).unwrap(),
                nara_gameplay::GameplayCommandDraft::new(
                    nara_gameplay::GameplayCommandTypeId::new("server.tick").unwrap(),
                ),
            ))
            .unwrap();

        app.run_once(std::time::Duration::ZERO).unwrap();

        assert!(
            !app.world()
                .contains_resource::<nara_input::ActionOutcomes>()
        );
        assert!(
            app.world()
                .resource::<ObservedServerCommands>()
                .0
                .is_empty()
        );
        assert_eq!(
            app.world()
                .resource::<nara_gameplay::GameplayCommandQueue>()
                .stats()
                .pending_commands,
            1
        );

        app.run_once(nara_app::FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<ObservedServerCommands>().0[0].1[0]
                .command_type()
                .as_str(),
            "server.tick"
        );
        assert!(
            app.world()
                .resource::<nara_gameplay::GameplayCommandQueue>()
                .is_idle()
        );
    }

    fn run_server_command_stream(
        reverse_arrival: bool,
    ) -> Vec<(u64, Vec<nara_gameplay::GameplayCommandEnvelope>)> {
        let mut app = App::new();
        app.add_plugins(ServerPlugins).unwrap();
        app.insert_resource(ObservedServerCommands::default())
            .unwrap()
            .add_systems(
                nara_app::CoreStage::FixedUpdate,
                observe_server_commands.in_set(nara_gameplay::GameplayCommandSet::Consume),
            )
            .unwrap();

        let mut submissions = vec![
            nara_gameplay::GameplayCommandSubmission::new(
                nara_gameplay::GameplayCommandTick::new(1).unwrap(),
                nara_gameplay::GameplayCommandIngressSource::external("peer-b").unwrap(),
                nara_gameplay::GameplayCommandSourceSequence::new(2).unwrap(),
                nara_gameplay::GameplayCommandDraft::new(
                    nara_gameplay::GameplayCommandTypeId::new("move.b").unwrap(),
                ),
            ),
            nara_gameplay::GameplayCommandSubmission::new(
                nara_gameplay::GameplayCommandTick::new(1).unwrap(),
                nara_gameplay::GameplayCommandIngressSource::external("peer-a").unwrap(),
                nara_gameplay::GameplayCommandSourceSequence::new(3).unwrap(),
                nara_gameplay::GameplayCommandDraft::new(
                    nara_gameplay::GameplayCommandTypeId::new("move.a").unwrap(),
                ),
            ),
            nara_gameplay::GameplayCommandSubmission::new(
                nara_gameplay::GameplayCommandTick::new(2).unwrap(),
                nara_gameplay::GameplayCommandIngressSource::external("peer-a").unwrap(),
                nara_gameplay::GameplayCommandSourceSequence::new(1).unwrap(),
                nara_gameplay::GameplayCommandDraft::new(
                    nara_gameplay::GameplayCommandTypeId::new("move.next").unwrap(),
                ),
            ),
        ];
        if reverse_arrival {
            submissions.reverse();
        }
        {
            let mut queue = app
                .world_mut()
                .unwrap()
                .resource_mut::<nara_gameplay::GameplayCommandQueue>();
            for submission in submissions {
                queue.submit(submission).unwrap();
            }
        }

        app.run_once(nara_app::FixedTime::DEFAULT_TIMESTEP * 2)
            .unwrap();
        assert!(!app.world().contains_resource::<nara_input::PointerState>());
        assert!(
            !app.world()
                .contains_resource::<nara_input::ButtonInput<nara_input::KeyCode>>()
        );
        assert!(
            !app.world()
                .contains_resource::<nara_input::ButtonInput<nara_input::MouseButton>>()
        );
        assert!(!app.world().contains_resource::<nara_input::ActionMap>());
        assert!(
            !app.world()
                .contains_resource::<nara_input::ActionOutcomes>()
        );
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
        app.world().resource::<ObservedServerCommands>().0.clone()
    }

    #[test]
    fn server_plugins_admit_the_same_command_stream_in_canonical_order() {
        let forward = run_server_command_stream(false);
        let reverse = run_server_command_stream(true);
        assert_eq!(forward, reverse);
        assert_eq!(forward.len(), 2);
        assert_eq!(forward[0].0, 1);
        assert_eq!(forward[1].0, 2);
        assert_eq!(forward[0].1.len(), 2);
        assert_eq!(forward[1].1.len(), 1);
        assert_eq!(
            forward[0]
                .1
                .iter()
                .map(|command| { (command.source().clone(), command.source_sequence().get(),) })
                .collect::<Vec<_>>(),
            [
                (
                    nara_gameplay::GameplayCommandSource::external("peer-a").unwrap(),
                    3,
                ),
                (
                    nara_gameplay::GameplayCommandSource::external("peer-b").unwrap(),
                    2,
                ),
            ]
        );
        assert_eq!(
            (
                forward[1].1[0].source().clone(),
                forward[1].1[0].source_sequence().get(),
            ),
            (
                nara_gameplay::GameplayCommandSource::external("peer-a").unwrap(),
                1,
            )
        );
        assert_eq!(
            forward[0]
                .1
                .iter()
                .map(|command| command.command_type().as_str())
                .collect::<Vec<_>>(),
            ["move.a", "move.b"]
        );
        assert_eq!(forward[1].1[0].command_type().as_str(), "move.next");
    }

    #[test]
    fn server_plugins_preserve_explicit_task_plugin_configuration() {
        let mut app = App::new();
        let one = nara_core::ItemLimit::new(1).unwrap();
        let config = nara_tasks::TaskPoolConfig::threaded(one, one, one).unwrap();
        app.add_plugin(nara_tasks::TaskPlugin::new(config)).unwrap();

        app.add_plugins(ServerPlugins).unwrap();

        assert_eq!(
            *app.world().resource::<nara_tasks::TaskPools>().config(),
            config
        );
    }

    #[test]
    fn server_tick_does_not_wait_for_a_running_worker() {
        let mut app = App::new();
        let one = nara_core::ItemLimit::new(1).unwrap();
        let config = nara_tasks::TaskPoolConfig::threaded(one, one, one).unwrap();
        app.add_plugin(nara_tasks::TaskPlugin::new(config)).unwrap();
        app.add_plugins(ServerPlugins).unwrap();

        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(0);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let mut handle = app
            .world()
            .resource::<nara_tasks::TaskPools>()
            .spawn(
                nara_tasks::TaskPoolKind::Io,
                nara_tasks::TaskSpawnRequest::new(
                    0,
                    nara_tasks::TaskDomainKey::new(0x5345_5256_4552),
                ),
                move |_| {
                    started_sender.send(()).unwrap();
                    release_receiver.recv().unwrap();
                },
            )
            .into_handle()
            .unwrap();
        started_receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the server worker should start the blocking task");

        let (watchdog_cancel_sender, watchdog_cancel_receiver) = std::sync::mpsc::channel();
        let watchdog_release_sender = release_sender.clone();
        let watchdog_fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watchdog_fired_in_thread = watchdog_fired.clone();
        let watchdog = std::thread::spawn(move || {
            if watchdog_cancel_receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_err()
            {
                watchdog_fired_in_thread.store(true, std::sync::atomic::Ordering::Release);
                let _ = watchdog_release_sender.send(());
            }
        });

        app.run_once(std::time::Duration::ZERO).unwrap();

        watchdog_cancel_sender.send(()).unwrap();
        release_sender.send(()).unwrap();
        watchdog.join().unwrap();
        assert!(
            !watchdog_fired.load(std::sync::atomic::Ordering::Acquire),
            "the main server tick waited for a running background task"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while handle.try_take().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "the released server task should reach a terminal state"
            );
            std::thread::yield_now();
        }
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
        assert_eq!(
            app.world()
                .resource::<nara_app::FixedTime>()
                .catch_up_policy(),
            nara_app::FixedCatchUpPolicy::PreserveDebt
        );
    }

    #[test]
    fn project_settings_configure_time_and_task_plugins_before_the_bundle() {
        let load = nara_project::ProjectManifest::parse_toml_str(
            r#"
schema_version = 1

[project]
name = "Configured Server"

[runtime]
plugin_plan = "server"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 9

[tasks.io]
workers = 1
pending_capacity = 3

[tasks.compute]
workers = 1
pending_capacity = 4

[tasks.async_compute]
workers = 1
pending_capacity = 5

[tasks.shutdown]
drain_timeout_ms = 20
cancel_timeout_ms = 20
join_timeout_ms = 20
"#,
        );
        assert!(!load.has_errors(), "{:?}", load.diagnostics);
        let settings = load.manifest.unwrap().resolve_profile(None).unwrap();
        let expected_tasks = settings.tasks.pool_config;

        let mut app = App::new();
        apply_project_settings(&mut app, settings).unwrap();

        assert_eq!(
            *app.world().resource::<nara_tasks::TaskPools>().config(),
            expected_tasks
        );
        assert_eq!(
            app.world()
                .resource::<nara_app::FixedTime>()
                .catch_up_policy(),
            nara_app::FixedCatchUpPolicy::PreserveDebt
        );
        assert!(
            app.world()
                .contains_resource::<nara_project::EffectiveProjectSettings>()
        );
        assert!(!app.world().contains_resource::<nara_input::PointerState>());
    }
}
