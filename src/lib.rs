//! Public facade for the nara engine workspace.

#[cfg(feature = "runtime-core")]
pub use nara_app as app;
#[cfg(feature = "runtime-core")]
pub use nara_asset as asset;
#[cfg(feature = "asset-watch")]
pub use nara_asset::watch as asset_watch;
#[cfg(feature = "audio")]
pub use nara_audio as audio;
#[cfg(feature = "runtime-core")]
pub use nara_core as core;
#[cfg(feature = "runtime-core")]
pub mod diagnostic {
    //! Bounded reports and application diagnostics.

    pub use nara_app::diagnostics::{
        DIAGNOSTICS_PLUGIN_DECLARATION, DIAGNOSTICS_PLUGIN_ID, DiagnosticCleanupSet,
        DiagnosticDedupePolicy, DiagnosticsPlugin, PressureDraftError, PressureMeasurement,
        PressureMetricKind, PressurePublishOutcome, PressurePublishRejection,
        PressureSettingsError, PressureStats, PressureUnit, RuntimeDiagnosticDraft,
        RuntimeDiagnosticEntry, RuntimeDiagnosticFilter, RuntimeDiagnosticFilterError,
        RuntimeDiagnostics, RuntimeDiagnosticsSnapshot, RuntimeDiagnosticsStats,
        RuntimePressureRetention, RuntimePressureSettings, RuntimePressureSnapshot,
        RuntimePressureSnapshotDraft, RuntimePressureSnapshots, RuntimePressureSnapshotsSnapshot,
        RuntimePublishOutcome, RuntimePublishRejection, RuntimeTracingCursor,
        emit_diagnostic_to_tracing, emit_report_to_tracing, plugin,
    };
    pub use nara_core::diagnostic::{
        DiagnosticBuildError, DiagnosticCode, DiagnosticDomain, DiagnosticField,
        DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer, DiagnosticReport,
        DiagnosticReportMergeOutcome, DiagnosticReportOutcome, DiagnosticReportRejection,
        DiagnosticReportSettings, DiagnosticReportStats, DiagnosticSettingsError,
        DiagnosticSeverity, DiagnosticTruncation, DiagnosticValueRef, IdentityError,
        IdentityErrorReason, MAX_DIAGNOSTIC_DRAFT_FIELDS, MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES,
        MAX_RUNTIME_DIAGNOSTIC_BYTES, MAX_RUNTIME_DIAGNOSTIC_ENTRIES, MAX_SAFE_STATIC_TEXT_BYTES,
        PressureMetricId, PressureSourceId, PublicDiagnosticIdentifier, ReportDiagnostic,
        ReportDiagnostic as Diagnostic, RuntimeDiagnosticRetention, RuntimeDiagnosticsSettings,
        SafeDisplayText, SafeSummary, SafeTextError, SafeTextErrorReason,
    };

    pub mod prelude {
        pub use super::{
            Diagnostic, DiagnosticCode, DiagnosticDedupePolicy, DiagnosticDomain, DiagnosticField,
            DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer, DiagnosticReport,
            DiagnosticReportSettings, DiagnosticSeverity, DiagnosticsPlugin, PressureMeasurement,
            PressureMetricId, PressureMetricKind, PressureSourceId, PressureUnit,
            PublicDiagnosticIdentifier, ReportDiagnostic, RuntimeDiagnosticDraft,
            RuntimeDiagnosticEntry, RuntimeDiagnosticFilter, RuntimeDiagnosticRetention,
            RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureSettings,
            RuntimePressureSnapshotDraft, RuntimePressureSnapshots, SafeDisplayText, SafeSummary,
            plugin,
        };
    }
}
#[cfg(feature = "project-runtime")]
pub use nara_core::vfs::capabilities as fs;
#[cfg(feature = "runtime-core")]
pub use nara_ecs as ecs;
#[cfg(feature = "runtime-core")]
pub use nara_scene::hierarchy;
#[cfg(feature = "runtime-core")]
pub mod identity {
    //! Stable identity surface exposed by the Nara product facade.
    //!
    //! Scene lifetime transactions are available through `nara::ecs::identity::domain`.

    pub use nara_core::identity::{AssetId, DocumentId, EntityUuid, UuidIdError};
    pub use nara_ecs::identity::{
        AuthoredEntityReference, AuthoredReferenceTarget, EntityIdentityAxis, EntityLookup,
        EntityReference, EntityReferenceCost, EntityReferenceRemap, IdentityAllocationError,
        IdentityDomainError, IdentityDomainStats, IdentityRemapError, IdentityTombstone,
        IdentityTombstoneSubject, PersistentRetirementDisposition, PersistentRuntimeId,
        PersistentRuntimeIdError, PersistentRuntimeNamespaceId, PersistentRuntimeNamespaceIdError,
        PersistentRuntimeReference, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop,
        ReferenceInputId, ReferenceInputIdError, RuntimeEntityIncarnation,
        RuntimeEntityIncarnationError, RuntimeEntityReference, SceneEntityId, SceneEntityIdError,
        SceneIdentitySnapshot, SceneInstanceId, SpawnedSceneInstance, TombstoneCause,
        WorldEntityLocator, WorldEntityLocatorRemap, WorldEntityLocators, WorldEntityToken,
        WorldIdentityDomain, WorldIdentityDomainId, WorldIdentityDomainSettings,
        adopt_identity_entity, register_persistent_identity, register_scene_instance,
        resolve_in_world, retire_identity_entity, retire_identity_entity_with_disposition,
        spawn_identity_entity,
    };
}
#[cfg(feature = "runtime-core")]
pub use nara_app::tasks;
#[cfg(feature = "runtime-input")]
pub use nara_input as input;
#[cfg(feature = "physics-2d")]
pub use nara_physics2d as physics2d;
#[cfg(feature = "project-source")]
pub use nara_project as project;
#[cfg(feature = "runtime-core")]
pub use nara_reflect as reflect;
#[cfg(feature = "runtime-core")]
pub use nara_reflect::PersistentComponent;
#[cfg(feature = "runtime-core")]
pub use nara_reflect::persistent_components;
#[cfg(any(
    feature = "runtime-2d",
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "render-wgpu"
))]
pub use nara_render as render;
#[cfg(feature = "render-wgpu")]
pub use nara_render::gpu as render_wgpu;
#[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
pub use nara_render::image;
#[cfg(feature = "runtime-2d")]
pub use nara_render::material;
#[cfg(feature = "runtime-2d")]
pub use nara_render::sprite;
#[cfg(feature = "presentation-2d")]
pub use nara_render::sprite_render;
#[cfg(feature = "runtime-2d")]
pub use nara_render::tilemap;
#[cfg(feature = "runtime-core")]
pub use nara_scene as scene;
#[cfg(feature = "runtime-ui")]
pub use nara_ui as ui;
#[cfg(feature = "presentation-ui")]
pub mod ui_render {
    //! Runtime UI presentation product surface.

    pub use nara_render::hud::{
        UI_RENDER_PLUGIN_DECLARATION, UI_RENDER_PLUGIN_ID, UiRenderPlugin, plugin,
    };

    /// Explicit packet/compiler boundary for Host, backend, and product-verification integrations.
    pub mod advanced {
        #[cfg(feature = "render-wgpu")]
        pub use nara_render::hud::PresentedUiWgpuFacts;
        pub use nara_render::hud::advanced::*;
        pub use nara_render::hud::{PresentedUiObservation, latest_presented_ui_observation};
    }
}
#[cfg(feature = "runtime-window")]
pub use nara_window as window;
#[cfg(feature = "desktop-winit")]
pub use nara_window::winit;

#[cfg(feature = "project-runtime")]
pub mod project_host;

#[cfg(feature = "project-workflow")]
pub mod project_workflow;

#[cfg(feature = "project-workflow")]
pub use nara_asset::artifact as asset_build;

#[cfg(feature = "project-tooling")]
pub mod project_tooling;

#[cfg(all(test, feature = "project-runtime", not(feature = "project-tooling")))]
#[path = "project_tooling/host_fs.rs"]
mod test_host_fs;

#[cfg(all(test, feature = "runtime-core", feature = "project-tooling"))]
use project_tooling::host_fs as test_host_fs;

#[cfg(feature = "project-source")]
mod project_diagnostic_ids;

#[cfg(feature = "project-source")]
mod project_content;

#[cfg(all(feature = "project-runtime", feature = "serde"))]
mod project_assets;

#[cfg(all(feature = "project-runtime", feature = "serde"))]
pub mod runtime_content;

#[cfg(feature = "project-runtime")]
mod scene_retention;

#[cfg(feature = "project-runtime")]
mod startup_scene;

#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub mod __macro_support {
    pub use nara_reflect::__macro_support::*;
}

#[cfg(feature = "runtime-core")]
mod product;

#[cfg(feature = "desktop-winit")]
pub use product::DesktopWinitPlugins;
#[cfg(feature = "presentation-2d")]
pub use product::Presentation2dPlugins;
#[cfg(feature = "presentation-ui")]
pub use product::PresentationUiPlugins;
#[cfg(feature = "runtime-2d")]
pub use product::Runtime2dPlugins;
#[cfg(feature = "runtime-ui")]
pub use product::RuntimeUiPlugins;
#[cfg(feature = "render-wgpu")]
pub use product::WgpuBackendPlugins;
#[cfg(feature = "runtime-core")]
pub use product::{DefaultPlugins, HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};
#[cfg(feature = "project-runtime")]
pub use product::{
    ProductRecipe, ProductRecipeEntry, ProductRecipeEntryKind, ProductRecipeError,
    SchemaContribution,
};

#[cfg(feature = "runtime-core")]
pub mod prelude {
    #[cfg(feature = "presentation-2d")]
    pub use crate::Presentation2dPlugins;
    #[cfg(feature = "presentation-ui")]
    pub use crate::PresentationUiPlugins;
    #[cfg(feature = "runtime-2d")]
    pub use crate::Runtime2dPlugins;
    #[cfg(feature = "runtime-ui")]
    pub use crate::RuntimeUiPlugins;
    #[cfg(feature = "project-source")]
    pub use crate::project_host::ProjectSceneSourceUpdateStatus;
    #[cfg(all(feature = "project-runtime", feature = "serde"))]
    pub use crate::project_host::{
        PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectScenes,
    };
    #[cfg(all(feature = "project-source", feature = "asset-watch"))]
    pub use crate::project_host::{
        ProjectSceneWatchConfigurationError, ProjectSceneWatchPlugin, ProjectSceneWatchState,
        ProjectSceneWatchStatus,
    };
    pub use crate::{DefaultPlugins, HeadlessRuntimePlugins, MinimalPlugins, ServerPlugins};
    #[cfg(feature = "project-runtime")]
    pub use crate::{
        ProductRecipe, ProductRecipeEntry, ProductRecipeEntryKind, ProductRecipeError,
        SchemaContribution,
    };
    pub use nara_app::CoreStage::{
        Extract as RenderExtract, First, FixedUpdate, Last, PostUpdate, PreUpdate, Update,
    };
    pub use nara_app::StartupStage::Scene as Startup;
    pub use nara_app::{
        AddPluginsError, App, AppExit, AppExitRequests, AppRunError, CoreStage, FixedCatchUpPolicy,
        FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginGroup, PluginId, RealTime, RuntimeInstance, RuntimeStartFailure, RuntimeState,
        RuntimeTimeSettings, StartupStage, VirtualTime,
    };
    pub use nara_asset::{
        Asset, AssetId, AssetPath, AssetRef, AssetServer, Assets, Handle, StableAssetId,
    };
    #[cfg(feature = "audio")]
    pub use nara_audio::{
        AudioClip, AudioFinished, AudioListener, AudioMixer, AudioPlayback, AudioPlayer,
        SpatialAudio,
    };
    pub use nara_core::{Color, Vec2, Vec3};
    pub use nara_ecs::identity::{
        EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
        PersistentRuntimeReference, RuntimeEntityReference, SceneEntityId,
    };
    pub use nara_ecs::{
        Bundle, Commands, Component, Entity, Query, Res, ResMut, Resource, SystemParam, World,
    };
    #[cfg(feature = "runtime-input")]
    pub use nara_input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMaps, ActionOutcome,
        ActionOutcomes, ActionPhase, ActionState, ActionValue, AxisDirection, ButtonInput,
        ButtonTransition, ButtonTransitionPhase, GamepadAxis, GamepadInput, GamepadInputKind,
        ImeEvent, ImeEventKind, InputBinding, KeyCode, MouseButton, MouseWheel, MouseWheelUnit,
        PointerState, SemanticActionCause, SemanticActionOutcome, SemanticActions, TextInput,
        TouchInput, TouchPhase,
    };
    #[cfg(feature = "physics-2d")]
    pub use nara_physics2d::{
        Collider2d, CollisionEvent, Physics2dPlugin, Physics2dSet, PhysicsPose2d, PhysicsWorld2d,
        RigidBody2d, Sensor2d, Teleport2d, TopDownCharacter,
    };
    pub use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueShape, PersistentComponent, PersistentComponentProvider, RegisterSchema,
    };
    pub use nara_scene::fixed_spatial::{
        FixedInputSpatialSnapshot2d, FixedSpatialPlugin, FixedSpatialSnapshot2d,
        HistoricalSpatialSnapshot2d, PresentedSpatialSnapshot2d, SpatialSnapshot2d,
        SpatialSnapshotError, SpatialSnapshotPhase, SpatialSnapshotStamp,
        add_fixed_transform_input_systems, add_fixed_transform_projection_systems,
        add_presented_transform_projection_systems, fixed_input_spatial_snapshot,
        fixed_spatial_snapshot, historical_spatial_snapshot, presented_spatial_snapshot,
    };
    pub use nara_scene::hierarchy::{Children, HierarchyPlugin, Parent};
    pub use nara_scene::spatial::{
        ChildOf, GlobalTransform2d, InheritedVisibility, SpatialPlugin, SpatialSet, Transform2d,
        TransformComponentsPlugin, Visibility,
    };
    pub use nara_scene::{
        CompiledSceneCandidate, CompiledSceneComponent, CompiledSceneDocument, CompiledSceneEntity,
        ExpandedSceneDocument, ExpandedSceneEntity, Name, PrefabDocument, PrefabInstance,
        PrefabInstanceBinding, PreparedScene, SceneAuthoringSession, SceneComponentRecord,
        SceneDocument, SceneEntityPlacement, SceneEntityRecord, SceneOccurrenceKey,
        ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementParent,
        ScenePlacementSlot, SourceDocumentKey, SourceDocumentKind, export_live_scene,
        prepare_scene, prepare_scene_replacement, spawn_scene,
    };

    #[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
    pub use nara_render::image::{
        ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageExtent, ImageFormat,
        ImageImportSettings, ImagePlugin,
    };
    #[cfg(feature = "runtime-2d")]
    pub use nara_render::material::{
        AddressMode, AlphaMode2d, FilterMode, Material2dBinding, Material2dDeclaration,
        Material2dGeneration, Material2dLibrary, MaterialDeclarationError, MaterialParameterKind,
        MaterialParameterLayout, MaterialParameterValue, MaterialParameterValues,
        MaterialParameters, MaterialShaderArtifact, MaterialShaderError,
        MaterialTextureDeclaration, MaterialUniformField, MaterialUpdateError,
        PreparedMaterial2dUpdate, SamplerDescriptor,
    };
    #[cfg(feature = "runtime-2d")]
    pub use nara_render::sprite::{
        Sprite, SpriteAnchor, SpriteMaterial, SpritePlugin, TextureRegion,
    };
    #[cfg(feature = "runtime-2d")]
    pub use nara_render::tilemap::{
        TileCell, TileCoord, TileIndex, TileLayer, TileSet, Tilemap, TilemapPlugin,
    };
    #[cfg(any(
        feature = "runtime-2d",
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{Camera2d, RenderComponentsPlugin, RenderImage2d, RenderTarget};
    #[cfg(any(
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{ClearColor, RenderPlugin, WorldCanvas, WorldPointer};
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::{
        UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionId,
        UiActionJournal, UiActionSequence, UiAlign, UiBorder, UiBrush, UiButton, UiButtonVisuals,
        UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges, UiFlexDirection, UiImageId,
        UiJustify, UiLayer, UiNode, UiOverflow, UiPlugin, UiPositionMode, UiRoot, UiRuntimeKey,
        UiSemanticRole, UiSemantics, UiStyle, UiText, UiTextAlignment, UiTextOverflow, UiTheme,
        UiThemePalette, UiVisual,
    };
}

#[cfg(feature = "runtime-core")]
pub mod advanced_prelude {
    pub use crate::diagnostic::{
        Diagnostic, DiagnosticBuildError, DiagnosticCode, DiagnosticDedupePolicy, DiagnosticDomain,
        DiagnosticField, DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer,
        DiagnosticReport, DiagnosticReportSettings, DiagnosticSeverity, DiagnosticsPlugin,
        PressureMeasurement, PressureMetricId, PressureMetricKind, PressureSourceId, PressureUnit,
        PublicDiagnosticIdentifier, ReportDiagnostic, RuntimeDiagnosticDraft,
        RuntimeDiagnosticEntry, RuntimeDiagnosticFilter, RuntimeDiagnosticRetention,
        RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureSettings,
        RuntimePressureSnapshotDraft, RuntimePressureSnapshots, SafeDisplayText, SafeSummary,
    };
    pub use crate::prelude::*;
    #[cfg(all(feature = "project-runtime", feature = "serde"))]
    pub use crate::project_host::advanced::commit_prepared_project_scene;
    pub use nara_app::tasks::{
        OrderedTaskReadySnapshot, OrderedTaskResults, OrderedTaskTerminal, TaskCancellation,
        TaskCancellationReason, TaskCancellationToken, TaskCoalesceKey, TaskCompletionCutoff,
        TaskCompletionCutoffError, TaskConfigError, TaskDescriptor, TaskDomainKey, TaskFacilities,
        TaskFailure, TaskHandle, TaskId, TaskInlineRunReport, TaskKindConfig, TaskOrderKey,
        TaskOverloadPolicy, TaskPlugin, TaskPoolConfig, TaskPoolError, TaskPoolKind,
        TaskPoolShutdownReport, TaskPoolStats, TaskPools, TaskReadySnapshotError, TaskRejectReason,
        TaskRejection, TaskReturnObserver, TaskReturnState, TaskScope, TaskScopeCloseReport,
        TaskShutdownPolicy, TaskShutdownReport, TaskSpawnOutcome, TaskSpawnRequest, TaskStats,
        TaskTerminal, TaskTerminalState,
    };
    pub use nara_app::{
        AppExitRequests, AppFrameOutcome, EditedPluginGroup, FixedTimeError, PluginCategory,
        PluginCommitError, PluginConfigurationFingerprint, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginDefinitionKey, PluginDomainError, PluginDomainErrorError,
        PluginFailure, PluginFailureReport, PluginGroupBuilder, PluginGroupId, PluginHook,
        PluginHookMutation, PluginId, PluginInstantiationError, PluginLifecycleState, PluginPlan,
        PluginPlanEntry, PluginPlanError, PluginPlanFingerprint, PluginPrepareError,
        PluginProductCapability, PluginSchemaProviderId, PluginShutdownContext,
        PluginShutdownError, PluginShutdownObligationId, PluginSlot, PluginSlotId,
        PluginSlotPresence, RenderTime, ResolvedPluginGroup, RuntimeFrameStatus, SealedApp,
        TimeFrameError, TimeFrameResource, TimeSettingsError,
    };
    pub use nara_asset::{
        AssetDatabaseError, AssetDependencyGraph, AssetError, AssetEvent, AssetEventKind,
        AssetEvents, AssetLoadGeneration, AssetLoadGenerations, AssetMeta, AssetMetaIdentity,
        AssetMutationBaseline, AssetPathError, AssetPathScope, AssetRecord, AssetRefError,
        AssetRefExportPolicy, AssetReferenceResolver, AssetReloadDiagnostics, AssetReloadRequest,
        AssetReloadRequestId, AssetReloadRequestKind, AssetReloadRequests, AssetSourceChange,
        AssetSourceChangeKind, AssetSourceChanges, AssetSourceKind, AssetSourcePath,
        AssetSourceRoot, AssetState, AssetStateError, AssetStates, AssetTaskUpdateSet,
        AssetVersion, DigestParseError, ImportDependency, ImportDependencyDigest,
        ImportDependencyRole, ImportRecipe, ImportRecipeDigest, ImportSettingsHash, ImporterId,
        LoadState, ProjectAssetDatabase, SourceHash, StableAssetIdGenerationError,
    };
    #[cfg(feature = "project-runtime")]
    pub use nara_core::vfs::capabilities::{
        CapabilityGeneration, CapabilityReader, CapabilityRights, CapabilitySessionId,
        ConflictProtection, ContentDigest, DigestLimit, DirectoryCapability,
        DirectoryEntryObservation, DirectorySyncReceipt, DirectorySyncTier, DurabilityProgress,
        ExpectedTarget, FileCapability, FileIdentity, FileKind, FileLockError, FileSyncReceipt,
        FileSyncTier, FileUnlockError, FsError, FsOperation, HostCapabilityOptions, LockGuarantee,
        LockMode, LockScope, LockedFileCapability, ParentAuthorizationTier, PathValidationError,
        PlatformCapabilityMatrix, ProofStatus, PublicationAtomicity, PublicationIdentityEvidence,
        PublicationSourceBinding, RelativeComponent, RelativePath, ReplaceReceipt, ResolutionTier,
        StageStatus, TemporaryFile, TrustMode, platform_capability_matrix,
    };
    pub use nara_core::{ByteLimit, DepthLimit, ItemLimit, TimeLimit};
    pub use nara_ecs::identity::{
        EntityIdentityAxis, EntityLookup, IdentityDomainError, IdentityDomainStats,
        IdentityTombstone, IdentityTombstoneSubject, SceneIdentitySnapshot, SceneInstanceId,
        SpawnedSceneInstance, TombstoneCause, WorldEntityLocator, WorldEntityLocatorRemap,
        WorldEntityLocators, WorldIdentityDomain, WorldIdentityDomainId,
        WorldIdentityDomainSettings, adopt_identity_entity, register_persistent_identity,
        register_scene_instance, resolve_in_world, retire_identity_entity,
        retire_identity_entity_with_disposition,
    };
    pub use nara_reflect::{
        ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        ComponentFieldPathError, ComponentFieldPathSegment, ComponentFloat,
        ComponentMigrationError, ComponentRegistryError, ComponentValueError, ComponentValueKind,
        MigratedComponentValue, PreparedComponent, PreparedComponentCandidate,
    };
    pub use nara_scene::advanced::{
        SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
        replace_expanded_scene_with_product, replace_expanded_scene_with_product_and_reservations,
    };

    #[cfg(feature = "project-runtime")]
    pub use crate::startup_scene::{
        CurrentScene, STARTUP_SCENE_ACTIVATION_PLUGIN_ID, SceneSource, SceneSourceView,
        StartupSceneActivationPlugin, StartupSceneActivationSet, StartupSceneMaterializeError,
        add_startup_scene_activation_systems, materialize_startup_scene,
        replace_current_scene_with_product, restart_current_scene_with_product,
    };

    #[cfg(feature = "asset-watch")]
    pub use nara_asset::watch::{
        AssetWatchError, AssetWatchEvent, AssetWatchEventKind, AssetWatchEventQueue,
        AssetWatchEventSender, AssetWatchPlugin, AssetWatchQueueDrain, AssetWatchQueueLimits,
        AssetWatchQueueSendError, AssetWatchQueueStats, AssetWatchRuntimeState,
        AssetWatchRuntimeStatus, AssetWatchTranslator, AssetWatcher,
    };
    #[cfg(any(feature = "runtime-2d", feature = "presentation-ui"))]
    pub use nara_render::image::{
        AdmittedImageImport, IMAGE_IMPORT_MEMORY_PLAN_VERSION, ImageBytesImportRequest,
        ImageFileImportRequest, ImageImportBudgetError, ImageImportBudgetHost,
        ImageImportBudgetSnapshot, ImageImportError, ImageImportLimitKind, ImageImportLimits,
        ImageImportLimitsError, ImageImportMemoryPlan, ImageImportSettings, ImageImportStage,
        ImageImportedAsset, ImageImporter, ImageImporterCreateError, ImagePngFailureKind,
        ImagePublicationFailureKind, ImageReloadError, ImageReloadStats, ImageSourceDirectory,
        ImageSourceFailureKind, ImageSourceMetadata, ImageSourceMetadataCreateError,
        ImageUnsupportedFeature,
    };
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    pub use nara_render::image::{
        ImagePreparePlugin, ImagePrepareStats, PreparedImageResource, PreparedImageSnapshot,
        PreparedImages, image_content_hash, image_resource_key, prepare_images,
    };
    #[cfg(any(
        feature = "presentation-2d",
        feature = "presentation-ui",
        feature = "render-wgpu"
    ))]
    pub use nara_render::{
        Extent2d, ExtractedView, ExtractedViews, FrameStats, RenderBackendState,
        RenderBackendStatus, RenderFrame, RenderFrameSkip, RenderFrameSkipReason, RenderFrameState,
        RenderResourceContentHash, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
        ViewportRect,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::advanced::{
        UiAccessibilityInput, UiAccessibilityRequest, UiActivationSource, UiFrame, UiKey,
        UiNonActivation, UiSemanticSnapshot,
    };
    #[cfg(feature = "runtime-ui")]
    pub use nara_ui::{
        UiAdapterDiagnostic, UiAdapterDiagnostics, UiAdapterError, UiInputProjectionIssue,
        UiPresentationReceipt, UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog,
        UiRouteRejectionRecord, UiRouteStamp, UiSafeAreaInsets, UiSeatId, UiSize,
        UiSurfaceAvailability, UiSurfaceContentTransform, UiSurfaceContentTransformError,
        UiSurfaceEndpoint, UiSurfaceId, UiSurfaceRetirement, UiSurfaceSlot, UiSurfaceSlotError,
        UiSurfaceSlotSnapshot, UiSurfaceSuspension, UiSurfaceSuspensionReason,
        active_ui_surface_slots, dispatch_ui_accessibility_input, install_ui_surface,
        restore_ui_surface, retire_ui_surface, suspend_ui_surface, ui_candidate_frame,
        ui_semantic_snapshot, ui_surface_slot_snapshots, update_ui_surface_slot,
    };
}

#[cfg(feature = "runtime-core")]
pub mod backend_prelude {
    #[cfg(feature = "desktop-winit")]
    pub use crate::DesktopWinitPlugins;
    #[cfg(feature = "render-wgpu")]
    pub use crate::WgpuBackendPlugins;
    #[cfg(feature = "render-wgpu")]
    pub use nara_render::gpu::{
        SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus,
        WGPU_CAPTURE_PLUGIN_DECLARATION, WGPU_CAPTURE_PLUGIN_ID,
        WGPU_STOCK_CAPTURE_PLUGIN_DECLARATION, WGPU_STOCK_CAPTURE_PLUGIN_ID,
        WGPU_STOCK_POLICY_PLUGIN_DECLARATION, WGPU_STOCK_POLICY_PLUGIN_ID,
        WGPU_WINDOW_PRESENTATION_PLUGIN_DECLARATION, WGPU_WINDOW_PRESENTATION_PLUGIN_ID,
        WgpuBackendState, WgpuCapturePlugin, WgpuFrameTransactionStats, WgpuRenderError,
        WgpuRenderHost, WgpuRenderProducer, WgpuStockCapturePlugin, WgpuStockPolicyPlugin,
        WgpuWindowPresentationPlugin,
    };
    #[cfg(all(feature = "presentation-2d", feature = "render-wgpu"))]
    pub use nara_render::gpu::{
        WGPU_STOCK_SPRITE_PLUGIN_DECLARATION, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuStockSpritePlugin,
    };
    #[cfg(all(feature = "presentation-ui", feature = "render-wgpu"))]
    pub use nara_render::gpu::{
        WGPU_STOCK_UI_PLUGIN_DECLARATION, WGPU_STOCK_UI_PLUGIN_ID, WgpuStockUiPlugin,
    };
    #[cfg(feature = "desktop-winit")]
    pub use nara_window::winit::{WinitControlFlow, WinitRunner};
    #[cfg(feature = "runtime-window")]
    pub use nara_window::{
        PresentMode, PrimaryWindow, PrimaryWindowId, Window, WindowCloseRequest,
        WindowCloseRequests, WindowEvent, WindowEvents, WindowId, WindowMode, WindowPlugin,
        WindowResolution, apply_window_event, push_window_event,
    };
}

#[cfg(all(test, feature = "runtime-core"))]
mod tests {
    use super::*;
    use nara_app::{App, FixedCatchUpPolicy, PluginGroup, PluginGroupId};

    #[test]
    fn minimal_plugins_install_only_headless_core_resources() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).unwrap();

        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        #[cfg(feature = "runtime-input")]
        assert!(app.world().contains_resource::<nara_input::PointerState>());
        #[cfg(feature = "runtime-window")]
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
    }

    #[cfg(feature = "runtime-input")]
    #[test]
    fn headless_runtime_plugins_install_local_input() {
        let mut app = App::new();
        app.add_plugins(HeadlessRuntimePlugins).unwrap();
        assert!(app.world().contains_resource::<nara_input::ActionState>());
    }

    #[test]
    fn server_plugins_omit_desktop_and_raw_input() {
        let mut app = App::new();
        app.add_plugins(ServerPlugins).unwrap();

        assert!(
            app.world()
                .contains_resource::<crate::diagnostic::RuntimeDiagnostics>()
        );
        assert!(
            app.world()
                .contains_resource::<crate::diagnostic::RuntimePressureSnapshots>()
        );
        assert!(app.world().contains_resource::<nara_asset::AssetServer>());
        assert!(
            app.world()
                .resource::<nara_app::tasks::TaskPools>()
                .config()
                .kind(nara_app::tasks::TaskPoolKind::Io)
                .workers()
                .get()
                > 0
        );
        #[cfg(feature = "runtime-input")]
        assert_server_omits_input(app.world());
        #[cfg(any(
            feature = "runtime-2d",
            feature = "presentation-ui",
            feature = "render-wgpu"
        ))]
        assert!(!app.world().contains_resource::<nara_render::RenderFrame>());
        #[cfg(feature = "runtime-window")]
        assert!(!app.world().contains_resource::<nara_window::WindowEvents>());
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

    #[cfg(feature = "runtime-input")]
    fn assert_server_omits_input(world: &nara_ecs::World) {
        assert!(!world.contains_resource::<nara_input::PointerState>());
        assert!(!world.contains_resource::<nara_input::ButtonInput<nara_input::KeyCode>>());
        assert!(!world.contains_resource::<nara_input::ButtonInput<nara_input::MouseButton>>());
        assert!(!world.contains_resource::<nara_input::ActionMaps>());
        assert!(!world.contains_resource::<nara_input::ActionOutcomes>());
    }

    #[test]
    fn server_plugins_preserve_explicit_task_plugin_configuration() {
        let mut app = App::new();
        let one = nara_core::ItemLimit::new(1).unwrap();
        let config = nara_app::tasks::TaskPoolConfig::threaded(one, one, one).unwrap();
        app.add_plugins(
            ServerPlugins
                .edit()
                .configure(nara_app::tasks::plugin(config)),
        )
        .unwrap();

        assert_eq!(
            *app.world()
                .resource::<nara_app::tasks::TaskPools>()
                .config(),
            config
        );
    }

    #[test]
    fn server_tick_does_not_wait_for_a_running_worker() {
        let mut app = App::new();
        let one = nara_core::ItemLimit::new(1).unwrap();
        let config = nara_app::tasks::TaskPoolConfig::threaded(one, one, one).unwrap();
        app.add_plugins(
            ServerPlugins
                .edit()
                .configure(nara_app::tasks::plugin(config)),
        )
        .unwrap();

        let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(0);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let mut handle = app
            .world()
            .resource::<nara_app::tasks::TaskPools>()
            .spawn(
                nara_app::tasks::TaskPoolKind::Io,
                nara_app::tasks::TaskSpawnRequest::new(
                    0,
                    nara_app::tasks::TaskDomainKey::new(0x5345_5256_4552),
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

        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(std::time::Duration::ZERO).unwrap();

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
    fn runtime_2d_installs_content_without_presentation() {
        let mut app = App::new();
        app.add_plugins(Runtime2dPlugins).unwrap();

        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id() == PluginGroupId::new("nara.plugins.runtime-2d"))
            .unwrap();
        assert!(
            [
                nara_app::PluginId::new("nara.render"),
                nara_app::PluginId::new("nara.image.prepare"),
                nara_app::PluginId::new("nara.sprite-render"),
            ]
            .into_iter()
            .all(|plugin| !metadata.plugins().contains(&plugin))
        );
    }

    #[cfg(feature = "runtime-ui")]
    #[test]
    fn runtime_ui_installs_content_without_presentation() {
        let mut app = App::new();
        app.add_plugins(RuntimeUiPlugins).unwrap();

        let metadata = app
            .installed_plugin_groups()
            .find(|group| group.id() == PluginGroupId::new("nara.plugins.runtime-ui"))
            .unwrap();
        assert!(
            [
                nara_app::PluginId::new("nara.render"),
                nara_app::PluginId::new("nara.image.prepare"),
                nara_app::PluginId::new("nara.ui-render"),
            ]
            .into_iter()
            .all(|plugin| !metadata.plugins().contains(&plugin))
        );
        assert!(metadata.plugins().contains(&nara_ui::UI_PLUGIN_ID));
    }

    #[cfg(feature = "presentation-ui")]
    #[test]
    fn presentation_ui_installs_packet_production_explicitly() {
        let mut app = App::new();
        app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
            .unwrap();

        assert!(
            app.installed_plugins()
                .any(|plugin| plugin.id == nara_render::hud::UI_RENDER_PLUGIN_ID)
        );
        assert!(
            app.world()
                .contains_resource::<nara_render::hud::presentation::UiPresentationOutcomes>()
        );
    }

    #[cfg(feature = "presentation-2d")]
    #[test]
    fn presentation_2d_installs_packet_production_explicitly() {
        let mut app = App::new();
        app.add_plugins((Runtime2dPlugins, Presentation2dPlugins))
            .unwrap();

        assert!(
            app.installed_plugins()
                .any(|plugin| plugin.id == nara_render::sprite_render::SPRITE_RENDER_PLUGIN_ID)
        );
        assert!(
            app.world()
                .contains_resource::<nara_render::sprite_render::batching::SpriteBatches>()
        );
    }
}
