//! Public facade for the nara engine workspace.

#[cfg(feature = "runtime-core")]
pub use nara_app as app;
#[cfg(feature = "runtime-core")]
pub use nara_asset as asset;
#[cfg(feature = "asset-watch")]
pub use nara_asset_watch as asset_watch;
#[cfg(feature = "runtime-core")]
pub use nara_core as core;
#[cfg(feature = "runtime-core")]
pub use nara_diagnostic as diagnostic;
#[cfg(feature = "runtime-core")]
pub use nara_ecs as ecs;
#[cfg(feature = "runtime-core")]
pub use nara_fs as fs;
#[cfg(feature = "runtime-core")]
pub use nara_gameplay as gameplay;
#[cfg(feature = "runtime-core")]
pub use nara_identity as identity;
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
pub use nara_image as image;
#[cfg(feature = "runtime-core")]
pub use nara_input as input;
#[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
pub use nara_material as material;
#[cfg(feature = "runtime-core")]
pub use nara_project as project;
#[cfg(feature = "runtime-core")]
pub use nara_reflect as reflect;
#[cfg(feature = "runtime-core")]
pub use nara_reflect::PersistentComponent;
#[cfg(any(
    feature = "runtime-2d",
    feature = "runtime-ui",
    feature = "render-wgpu"
))]
pub use nara_render as render;
#[cfg(feature = "render-wgpu")]
pub use nara_render_wgpu as render_wgpu;
#[cfg(feature = "runtime-core")]
pub use nara_scene as scene;
#[cfg(feature = "runtime-2d")]
pub use nara_sprite as sprite;
#[cfg(feature = "runtime-2d")]
pub use nara_sprite_render as sprite_render;
#[cfg(feature = "runtime-core")]
pub use nara_tasks as tasks;
#[cfg(feature = "runtime-2d")]
pub use nara_tilemap as tilemap;
#[cfg(feature = "tooling")]
pub use nara_tooling as tooling;
#[cfg(feature = "tooling-egui")]
pub use nara_tooling_egui as tooling_egui;
#[cfg(feature = "runtime-core")]
pub use nara_transform as transform;
#[cfg(feature = "runtime-ui")]
pub use nara_ui as ui;
#[cfg(feature = "runtime-ui")]
pub use nara_ui_render as ui_render;
#[cfg(feature = "runtime-core")]
pub use nara_window as window;
#[cfg(feature = "desktop-winit")]
pub use nara_winit as winit;

#[cfg(feature = "runtime-core")]
pub mod project_host;

#[cfg(feature = "runtime-core")]
mod project_diagnostic_ids;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
mod project_content;

#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub mod __macro_support {
    pub use nara_reflect::__macro_support::*;
}

#[cfg(feature = "runtime-core")]
mod product;

#[cfg(feature = "desktop-winit")]
pub use product::DesktopWinitPlugins;
#[cfg(feature = "runtime-2d")]
pub use product::Runtime2dPlugins;
#[cfg(feature = "runtime-ui")]
pub use product::RuntimeUiPlugins;
#[cfg(feature = "tooling")]
pub use product::ToolingPlugins;
#[cfg(feature = "render-wgpu")]
pub use product::WgpuBackendPlugins;
#[cfg(feature = "runtime-core")]
pub use product::{HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};

#[cfg(feature = "runtime-core")]
pub mod prelude {
    #[cfg(feature = "runtime-2d")]
    pub use crate::Runtime2dPlugins;
    #[cfg(feature = "runtime-ui")]
    pub use crate::RuntimeUiPlugins;
    pub use crate::{HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};
    pub use nara_app::{
        AddPluginsError, App, AppExit, AppRunError, CoreStage, FixedCatchUpPolicy, FixedTime,
        FixedUpdateSet, Plugin, PluginError, PluginGroup, RealTime, RuntimeTimeSettings,
        StartupStage, VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetRef, AssetServer, Assets, Handle, StableAssetId,
    };
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_ecs::{Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, World};
    pub use nara_gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandKey, GameplayCommandPayload, GameplayCommandSource, GameplayCommandSourceId,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTarget,
        GameplayCommandTargetId, GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
    };
    pub use nara_identity::{
        EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
        PersistentRuntimeReference, RuntimeEntityReference, SceneEntityId,
    };
    pub use nara_input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionOutcome, ActionOutcomes,
        ActionPhase, ActionValue, ButtonInput, ButtonInputError, ButtonTransition,
        ButtonTransitionPhase, InputBinding, KeyCode, MAX_BUTTON_TRANSITIONS_PER_FRAME,
        MouseButton, PointerState,
    };
    pub use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentRegistry, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, PersistentComponent, PersistentComponentProvider,
    };
    pub use nara_scene::{
        Children, HierarchyPlugin, Name, Parent, PrefabDocument, SceneAuthoringSession,
        SceneComponentRecord, SceneDocument, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation, SceneSpawner, Visibility, export_scene, spawn_prefab, spawn_scene,
    };
    pub use nara_transform::{GlobalTransform2d, Transform2d, TransformPlugin};

    #[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
    pub use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImagePlugin};
    #[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
    pub use nara_material::{
        AddressMode, AlphaMode2d, FilterMode, Material2dDescriptor, SamplerDescriptor,
    };
    #[cfg(any(
        feature = "runtime-2d",
        feature = "runtime-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{Camera2d, ClearColor, RenderImage2d, RenderPlugin, RenderTarget};
    #[cfg(feature = "runtime-2d")]
    pub use nara_sprite::{Sprite, SpriteAnchor, SpriteMaterial, SpritePlugin, TextureRegion};
    #[cfg(feature = "runtime-2d")]
    pub use nara_tilemap::{
        TileCell, TileCoord, TileIndex, TileLayer, TileSet, Tilemap, TilemapPlugin,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::{UiNode, UiPanel, UiPanelMaterial, UiPlugin, UiRect, UiRoot, UiStyle, UiVal};
}

#[cfg(feature = "runtime-core")]
pub mod advanced_prelude {
    pub use crate::prelude::*;
    pub use nara_app::{
        AppExitRequests, AppFrameOutcome, AppScheduleRunError, EditedPluginGroup, FixedTimeError,
        PluginCapability, PluginCategory, PluginConfigurationFingerprint, PluginDeclaration,
        PluginDefinition, PluginDefinitionId, PluginDefinitionKey, PluginFailure,
        PluginFailureReport, PluginGroupBuilder, PluginGroupId, PluginHook, PluginHookMutation,
        PluginId, PluginInstantiationError, PluginLifecycleState, PluginPlan, PluginPlanEntry,
        PluginPlanError, PluginPlanFingerprint, PluginPrepareError, PluginPrepareFailure,
        PluginProductCapability, PluginSchemaProviderId, PluginServiceId, PluginShutdownContext,
        PluginShutdownError, PluginShutdownObligationId, PluginSlot, PluginSlotId,
        PluginSlotPresence, RenderTime, ResolvedPluginGroup, RuntimeFrameStatus, SealedApp,
        TimeFrameError, TimeFrameResource, TimeSettingsError,
    };
    pub use nara_asset::{
        ArtifactFormatVersion, ArtifactLabel, AssetDatabaseError, AssetDependencyGraph, AssetError,
        AssetEvent, AssetEventKind, AssetEvents, AssetLoadGeneration, AssetLoadGenerations,
        AssetMeta, AssetPathError, AssetRecord, AssetRefError, AssetRefExportPolicy,
        AssetReloadDiagnostics, AssetReloadRequest, AssetReloadRequestId, AssetReloadRequestKind,
        AssetReloadRequests, AssetSourceChange, AssetSourceChangeKind, AssetSourceChanges,
        AssetSourceKind, AssetSourceRoot, AssetState, AssetStateError, AssetStates,
        AssetTaskUpdateSet, AssetVersion, DigestParseError, ImportArtifactDigest,
        ImportArtifactKey, ImportArtifactPath, ImportArtifactRecord, ImportDependency,
        ImportDependencyDigest, ImportDependencyRole, ImportError, ImportJobInput, ImportProfile,
        ImportRequest, ImportSettingsHash, ImportedAsset, Importer, ImporterDescriptor, ImporterId,
        ImporterRegistry, LoadState, MissingMetaPolicy, ProjectAssetDatabase, SourceChangeResolver,
        SourceExtension, SourceHash, TypedImporter,
    };
    pub use nara_core::{ByteLimit, DepthLimit, ItemLimit, TimeLimit};
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
    pub use nara_gameplay::{
        GameplayCommandBatch, GameplayCommandEnvelope, GameplayCommandIdError,
        GameplayCommandLifecycleError, GameplayCommandLimitKind, GameplayCommandPayloadError,
        GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandQueueSettings,
        GameplayCommandQueueStats, GameplayCommandRejection, GameplayCommandSet,
        GameplayCommandSettingsError,
    };
    pub use nara_identity::{
        EntityIdentityAxis, EntityLookup, IdentityDomainError, IdentityDomainStats,
        IdentityTombstone, IdentityTombstoneSubject, SceneIdentitySnapshot, SceneInstanceId,
        SpawnedSceneInstance, TombstoneCause, WorldEntityLocator, WorldEntityLocatorRemap,
        WorldEntityLocators, WorldIdentityDomain, WorldIdentityDomainId,
        WorldIdentityDomainSettings, resolve_in_world,
    };
    pub use nara_reflect::{
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        ComponentFieldPathError, ComponentFieldPathSegment, ComponentFloat,
        ComponentMigrationError, ComponentRegistryError, ComponentValueError, ComponentValueKind,
        MigratedComponentValue, PreparedComponent, PreparedComponentCandidate,
    };
    pub use nara_tasks::{
        OrderedTaskReadySnapshot, OrderedTaskResults, OrderedTaskTerminal, TaskCancellation,
        TaskCancellationReason, TaskCancellationToken, TaskCoalesceKey, TaskCompletionCutoff,
        TaskCompletionCutoffError, TaskConfigError, TaskDescriptor, TaskDomainKey, TaskFailure,
        TaskHandle, TaskId, TaskInlineRunReport, TaskKindConfig, TaskOrderKey, TaskOverloadPolicy,
        TaskPlugin, TaskPoolConfig, TaskPoolError, TaskPoolKind, TaskPoolShutdownReport,
        TaskPoolStats, TaskPools, TaskReadySnapshotError, TaskRejectReason, TaskRejection,
        TaskShutdownPolicy, TaskShutdownReport, TaskSpawnOutcome, TaskSpawnRequest, TaskStats,
        TaskTerminal, TaskTerminalState,
    };

    #[cfg(feature = "asset-watch")]
    pub use nara_asset_watch::{
        AssetWatchError, AssetWatchEvent, AssetWatchEventKind, AssetWatchEventQueue,
        AssetWatchEventSender, AssetWatchPlugin, AssetWatchQueueDrain, AssetWatchQueueLimits,
        AssetWatchQueueSendError, AssetWatchQueueStats, AssetWatchRuntimeState,
        AssetWatchRuntimeStatus, AssetWatchTranslator, AssetWatcher,
    };
    #[cfg(any(feature = "runtime-2d", feature = "runtime-ui"))]
    pub use nara_image::{
        AdmittedImageImport, IMAGE_IMPORT_MEMORY_PLAN_VERSION, ImageBytesImportRequest,
        ImageFileImportRequest, ImageImportBudgetError, ImageImportBudgetHost,
        ImageImportBudgetSnapshot, ImageImportError, ImageImportLimitKind, ImageImportLimits,
        ImageImportLimitsError, ImageImportMemoryPlan, ImageImportStage, ImageImportedAsset,
        ImageImporter, ImageImporterCreateError, ImagePngFailureKind, ImagePreparePlugin,
        ImagePrepareStats, ImagePublicationFailureKind, ImageReloadError, ImageReloadStats,
        ImageSourceDirectory, ImageSourceFailureKind, ImageSourceMetadata, ImageUnsupportedFeature,
        PreparedImageResource, image_descriptor_hash, image_resource_key, prepare_images,
    };
    #[cfg(any(
        feature = "runtime-2d",
        feature = "runtime-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{
        Extent2d, ExtractedView, ExtractedViews, FrameStats, PreparedRenderResource,
        PreparedRenderResourceRecord, PreparedRenderResources, RenderBackendState,
        RenderBackendStatus, RenderFrame, RenderFrameSkip, RenderFrameSkipReason, RenderFrameState,
        RenderPhaseLabel, RenderPrepareApplyResult, RenderPrepareError, RenderPrepareInvalidation,
        RenderPrepareInvalidationReason, RenderPrepareInvalidations, RenderPrepareStatus,
        RenderResourceKey, RenderResourceKind, RenderResourceSnapshot, ViewportRect,
    };
    #[cfg(feature = "runtime-2d")]
    pub use nara_sprite_render::{
        ColorKey, ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
        QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches, SpriteInstance,
        SpriteMaterialKey, SpriteRenderStats, TextureUvRect,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::{
        ComputedUiLayout, ComputedUiLayouts, UiInteractionState, UiInteractionTarget,
        UiPointerRoute,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui_render::{
        ExtractedUiItem, ExtractedUiItems, ExtractedUiMaterial, QueuedUiItem, QueuedUiItems,
        UiBatch, UiBatches, UiClipRect, UiColorKey, UiInstance, UiMaterialKey, UiRenderStats,
        UiTextureRect,
    };
}

#[cfg(feature = "tooling")]
pub mod tooling_prelude {
    pub use crate::ToolingPlugins;
    pub use nara_tooling::{
        EditorDocumentId, EditorExternalReloadState, EditorSceneModel, EditorSceneSlot,
        EditorSceneTabModel, EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand,
        EditorWorkspaceCommandReport, EditorWorkspaceModel, SceneApplyChangesComponentReport,
        SceneApplyChangesComponentStatus, SceneApplyChangesReport, SceneApplyChangesRequest,
        SceneEditorMode, SceneEditorModel, SceneEditorState, SceneInspectorCommand,
        SceneInspectorCommandReport, SceneInspectorComponentView, SceneInspectorEntityRow,
        SceneInspectorEntityView, SceneInspectorFieldState, SceneInspectorFieldView,
        SceneInspectorModel, SceneInspectorState, ScenePlaySession, ScenePlayTransitionReport,
        ToolingPlugin, WorldIdentitySnapshot,
    };
    #[cfg(feature = "tooling-egui")]
    pub use nara_tooling_egui::{
        EguiSceneEditorPanel, EguiSceneEditorPanelResponse, EguiSceneInspectorPanel,
        EguiSceneInspectorPanelResponse,
    };
}

#[cfg(feature = "runtime-core")]
pub mod backend_prelude {
    #[cfg(feature = "desktop-winit")]
    pub use crate::DesktopWinitPlugins;
    #[cfg(feature = "render-wgpu")]
    pub use crate::WgpuBackendPlugins;
    #[cfg(feature = "render-wgpu")]
    pub use nara_render_wgpu::{
        SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuBackendState,
        WgpuFrameTransactionStats, WgpuRenderBackend, WgpuRenderError, WgpuRenderPlugin,
    };
    pub use nara_window::{
        PresentMode, PrimaryWindow, PrimaryWindowId, Window, WindowCloseRequest,
        WindowCloseRequests, WindowEvent, WindowEvents, WindowId, WindowMode, WindowPlugin,
        WindowResolution, apply_window_event, push_window_event,
    };
    #[cfg(feature = "desktop-winit")]
    pub use nara_winit::{WinitControlFlow, WinitRunner};
}

#[cfg(all(test, feature = "runtime-core"))]
mod tests {
    use super::*;
    use nara_app::{App, FixedCatchUpPolicy, PluginGroup, PluginGroupId};
    use nara_ecs::schedule::IntoScheduleConfigs;

    #[test]
    fn minimal_plugins_install_only_headless_core_resources() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
    }

    #[test]
    fn headless_runtime_plugins_install_semantic_command_resources() {
        let mut app = App::new();
        app.add_plugins(HeadlessRuntimePlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_gameplay::GameplayCommandQueue>()
        );
        assert!(app.world().contains_resource::<nara_input::PointerState>());
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
        #[cfg(any(
            feature = "runtime-2d",
            feature = "runtime-ui",
            feature = "render-wgpu"
        ))]
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
        #[cfg(feature = "tooling")]
        assert!(
            !app.world()
                .contains_resource::<nara_tooling::EditorWorkspace>()
        );
        assert_eq!(
            app.world()
                .resource::<nara_app::FixedTime>()
                .catch_up_policy(),
            FixedCatchUpPolicy::PreserveDebt
        );
        assert!(
            app.installed_plugin_groups()
                .any(|group| group.id() == PluginGroupId::new("nara.plugins.server"))
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
                .map(|command| { (command.source().clone(), command.source_sequence().get()) })
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
        app.add_plugins(ServerPlugins.edit().configure(nara_tasks::plugin(config)))
            .unwrap();

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
        app.add_plugins(ServerPlugins.edit().configure(nara_tasks::plugin(config)))
            .unwrap();

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

    #[cfg(feature = "runtime-2d")]
    #[test]
    fn runtime_2d_installs_sprite_submission_without_runtime_ui() {
        let mut app = App::new();
        app.add_plugins(Runtime2dPlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<nara_sprite_render::SpriteBatches>()
        );
        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id() == PluginGroupId::new("nara.plugins.runtime-2d"))
            .unwrap();
        assert!(
            !metadata
                .plugins()
                .contains(&nara_app::PluginId::new("nara.ui"))
        );
    }

    #[cfg(feature = "runtime-ui")]
    #[test]
    fn runtime_ui_installs_ui_submission_as_an_independent_product() {
        let mut app = App::new();
        app.add_plugins(RuntimeUiPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_ui_render::UiBatches>());
        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id() == PluginGroupId::new("nara.plugins.runtime-ui"))
            .unwrap();
        assert!(
            !metadata
                .plugins()
                .contains(&nara_app::PluginId::new("nara.sprite"))
        );
    }
}
