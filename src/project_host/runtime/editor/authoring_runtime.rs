#[cfg(feature = "runtime-ui")]
use std::collections::BTreeSet;
use std::time::Duration;

use nara_app::{
    __RuntimeDriverPort, App, RuntimeClosePolicy, RuntimeControl, RuntimeInstance,
    RuntimeObligationLedger, RuntimeState, RuntimeWorldAccessError,
};
#[cfg(feature = "runtime-ui")]
use nara_asset::AssetId;
use nara_asset::{
    AssetEvents, AssetRecord, AssetServer, AssetSourceKind, AssetStates, AssetVersion, Assets,
    Handle,
};
use nara_core::ItemLimit;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Mut, Resource, World};
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, RelativePath};
use nara_image::{
    ImageAsset, ImageFileImportRequest, ImageImportError, ImageImportSettings, ImageImportedAsset,
    ImageImporter,
};
use nara_reflect::{ComponentRegistryPlugin, ComponentRegistrySnapshot};
#[cfg(feature = "presentation-2d")]
use nara_scene::SceneAuthoringRevision;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskShutdownReport, TaskSpawnRequest, TaskTerminal,
};
use nara_tilemap::TileSetAssetPlugin;

#[cfg(test)]
mod construction_tests;
mod retirement;
mod tilesets;
#[cfg(feature = "presentation-2d")]
use nara_tooling::{EditorDocumentId, WorldIdentitySnapshot};
pub(super) use retirement::{EditorAuthoringRetirement, EditorAuthoringRetirements};
pub(super) use tilesets::EditorTileSetSourceUpdate;

#[cfg(test)]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};

use crate::project_host::{ProjectContentSnapshot, authoring::AuthoringProjectSessionSnapshot};

use super::super::{publish_snapshot_assets, single_error};
#[cfg(feature = "runtime-ui")]
use super::authoring_images::{
    EditorAuthoringImageCatalogCandidate, EditorAuthoringImageCatalogError,
};
#[cfg(feature = "presentation-2d")]
use super::preview::{EditorPreviewCandidate, EditorPreviewSession, EditorPreviewSource};

const EDITOR_IMAGE_IMPORT_TASK_DOMAIN: TaskDomainKey = TaskDomainKey::new(0x6e61_7261_696d_6701);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorImageRuntimeFailure {
    RuntimeUnavailable,
    AssetIdentity,
    IdentityCapacity,
    ImportAdmission,
    TaskAdmission,
    SourceChanged,
    Import,
    TaskCancelled,
    TaskFailed,
    Publication,
}

enum EditorImageTaskOutput {
    Imported {
        imported: ImageImportedAsset,
        source: EditorImageSourceCompletion,
    },
    Rejected(EditorImageRuntimeFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditorImageSourceCompletion {
    identity: FileIdentity,
    digest: ContentDigest,
}

impl EditorImageSourceCompletion {
    #[must_use]
    pub(super) const fn identity(self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub(super) const fn digest(self) -> ContentDigest {
        self.digest
    }
}

pub(super) struct EditorImageRuntimeTask {
    handle: Handle<ImageAsset>,
    task: TaskHandle<EditorImageTaskOutput>,
}

#[cfg(test)]
struct EditorImageImportTestGate {
    started: SyncSender<()>,
    release: Receiver<()>,
    cancellation: EditorImageImportTestCancellation,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorImageImportTestCancellation {
    Cooperative,
    ManualRelease,
}

#[cfg(test)]
pub(crate) struct EditorImageImportTestControl {
    started: Receiver<()>,
    release: Option<Sender<()>>,
}

#[cfg(test)]
impl EditorImageImportTestControl {
    pub(crate) fn wait_until_started(&self) -> bool {
        self.started.recv_timeout(Duration::from_secs(1)).is_ok()
    }

    pub(crate) fn release(mut self) -> bool {
        self.release
            .take()
            .is_some_and(|release| release.send(()).is_ok())
    }
}

#[cfg(test)]
impl Drop for EditorImageImportTestControl {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl std::fmt::Debug for EditorImageRuntimeTask {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorImageRuntimeTask")
            .field("handle", &self.handle)
            .field("task", &self.task)
            .finish()
    }
}

impl Drop for EditorImageRuntimeTask {
    fn drop(&mut self) {
        let _ = self.task.cancel();
        let _ = self.task.try_take();
    }
}

pub(super) enum EditorImageRuntimeTerminal {
    Imported {
        handle: Handle<ImageAsset>,
        imported: ImageImportedAsset,
        source: EditorImageSourceCompletion,
    },
    Rejected(EditorImageRuntimeFailure),
}

pub(super) enum EditorAuthoringRuntimeCloseProgress {
    Incomplete(TaskShutdownReport),
    Complete(TaskShutdownReport),
}

#[derive(Debug)]
pub(super) struct EditorAuthoringConstructionFailure {
    pub(super) diagnostics: DiagnosticReport,
    pub(super) retirement: EditorAuthoringRetirement,
}

/// Owns the Editor's single task and image-asset world.
///
/// Scene preview is an optional private subsystem of this runtime. Other authoring workflows must
/// reuse this owner instead of creating another `App`, task pool, or image store.
pub(super) struct EditorAuthoringRuntime {
    runtime: RuntimeInstance,
    tileset_records: Vec<AssetRecord>,
    image_revocation_failure: Option<EditorImageRuntimeFailure>,
    #[cfg(feature = "presentation-2d")]
    preview: EditorPreviewSession,
    #[cfg(feature = "presentation-2d")]
    image_publication_revision: u64,
    #[cfg(test)]
    next_image_import_gate: Option<EditorImageImportTestGate>,
    #[cfg(test)]
    fault_after_next_image_publication: bool,
}

impl std::fmt::Debug for EditorAuthoringRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut state = formatter.debug_struct("EditorAuthoringRuntime");
        #[cfg(feature = "presentation-2d")]
        state.field("preview", &self.preview);
        state.finish_non_exhaustive()
    }
}

impl EditorAuthoringRuntime {
    pub(super) fn new(
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, EditorAuthoringConstructionFailure> {
        #[cfg(not(feature = "presentation-2d"))]
        let _ = registry;
        let mut app = App::new();
        let preparation = (|| {
            configure_authoring_app(&mut app)?;
            #[cfg(feature = "presentation-2d")]
            let preview =
                EditorPreviewSession::install_trusted(&mut app, project.clone(), registry)?;
            publish_snapshot_assets(
                app.world_mut()
                    .map_err(|_| runtime_error("project.editor.preview-world-unavailable"))?,
                &project,
            )
            .map_err(|error| error.diagnostics)?;
            #[cfg(feature = "presentation-2d")]
            return Ok(preview);
            #[cfg(not(feature = "presentation-2d"))]
            Ok::<(), DiagnosticReport>(())
        })();
        let _preview = match preparation {
            Ok(preview) => preview,
            Err(diagnostics) => {
                return Err(EditorAuthoringConstructionFailure {
                    diagnostics,
                    retirement: EditorAuthoringRetirement::Configuration(
                        app.begin_retirement(RuntimeClosePolicy::default()),
                    ),
                });
            }
        };
        let runtime = start_authoring_app(app)?;
        Ok(Self {
            runtime,
            tileset_records: Vec::new(),
            image_revocation_failure: None,
            #[cfg(feature = "presentation-2d")]
            preview: _preview,
            #[cfg(feature = "presentation-2d")]
            image_publication_revision: 0,
            #[cfg(test)]
            next_image_import_gate: None,
            #[cfg(test)]
            fault_after_next_image_publication: false,
        })
    }

    pub(super) fn new_authoring(
        project: &AuthoringProjectSessionSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, EditorAuthoringConstructionFailure> {
        #[cfg(not(feature = "presentation-2d"))]
        let _ = registry;
        let mut app = App::new();
        let preparation = (|| {
            configure_authoring_app(&mut app)?;
            #[cfg(feature = "presentation-2d")]
            let preview = EditorPreviewSession::install_authoring(
                &mut app,
                registry,
                project.asset_database(),
            )?;
            publish_authoring_image_records(
                app.world_mut()
                    .map_err(|_| runtime_error("project.editor.preview-world-unavailable"))?,
                project,
            )?;
            #[cfg(feature = "presentation-2d")]
            return Ok(preview);
            #[cfg(not(feature = "presentation-2d"))]
            Ok::<(), DiagnosticReport>(())
        })();
        let _preview = match preparation {
            Ok(preview) => preview,
            Err(diagnostics) => {
                return Err(EditorAuthoringConstructionFailure {
                    diagnostics,
                    retirement: EditorAuthoringRetirement::Configuration(
                        app.begin_retirement(RuntimeClosePolicy::default()),
                    ),
                });
            }
        };
        let runtime = start_authoring_app(app)?;
        Ok(Self {
            runtime,
            tileset_records: project
                .asset_records()
                .filter(|record| {
                    record.source_kind() == &AssetSourceKind::Other(String::from("tileset"))
                })
                .cloned()
                .collect(),
            image_revocation_failure: None,
            #[cfg(feature = "presentation-2d")]
            preview: _preview,
            #[cfg(feature = "presentation-2d")]
            image_publication_revision: 0,
            #[cfg(test)]
            next_image_import_gate: None,
            #[cfg(test)]
            fault_after_next_image_publication: false,
        })
    }

    pub(super) fn rebuild_for_project(
        &self,
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, EditorAuthoringConstructionFailure> {
        let replacement = Self::new(project, registry)?;
        #[cfg(feature = "presentation-2d")]
        let replacement = {
            let mut replacement = replacement;
            replacement.preview.set_extent(self.preview.extent());
            replacement
        };
        Ok(replacement)
    }

    #[cfg(feature = "presentation-2d")]
    pub(super) fn set_preview_extent(&mut self, extent: Option<nara_render::Extent2d>) {
        self.preview.set_extent(extent);
    }

    #[cfg(feature = "presentation-2d")]
    pub(super) fn preview_requires_source_projection(
        &self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
    ) -> bool {
        self.preview
            .requires_source_projection(self.runtime.world(), document, revision)
    }

    #[cfg(feature = "presentation-2d")]
    pub(super) fn drive_preview(
        &mut self,
        source: Option<EditorPreviewSource>,
    ) -> Option<DiagnosticReport> {
        if self.image_revocation_failure.is_some() {
            return None;
        }
        self.preview
            .drive(&mut self.runtime, source, self.image_publication_revision)
    }

    #[cfg(feature = "presentation-2d")]
    pub(super) fn take_preview_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.preview.take_candidate()
    }

    #[cfg(feature = "presentation-2d")]
    pub(super) fn preview_world_snapshot(&self) -> Option<&WorldIdentitySnapshot> {
        self.preview.world_snapshot()
    }

    pub(super) fn spawn_image_import(
        &mut self,
        attempt_generation: u64,
        record: AssetRecord,
        source: FileCapability,
        source_identity: FileIdentity,
        project_root: DirectoryCapability,
        project_relative_path: RelativePath,
        settings: ImageImportSettings,
    ) -> Result<EditorImageRuntimeTask, EditorImageRuntimeFailure> {
        if let Some(failure) = self.image_revocation_failure {
            return Err(failure);
        }
        let request = EditorImageImportAdmission {
            attempt_generation,
            record,
            source,
            source_identity,
            project_root,
            project_relative_path,
            settings,
            #[cfg(test)]
            test_gate: self.next_image_import_gate.take(),
        };
        self.runtime
            .with_driver_scope(|scope| scope.__apply_port::<EditorImageAdmissionPort>(request))
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?
            // A task may already own the source and captures. Return its owner even if the Runtime
            // faulted after admission; the Host observes the sticky fault through its normal drive.
            .output
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?
    }

    pub(super) fn poll_image_import(
        task: &mut EditorImageRuntimeTask,
    ) -> Option<EditorImageRuntimeTerminal> {
        match task.task.try_take()? {
            TaskTerminal::Completed(EditorImageTaskOutput::Imported { imported, source }) => {
                Some(EditorImageRuntimeTerminal::Imported {
                    handle: task.handle,
                    imported,
                    source,
                })
            }
            TaskTerminal::Completed(EditorImageTaskOutput::Rejected(reason)) => {
                Some(EditorImageRuntimeTerminal::Rejected(reason))
            }
            TaskTerminal::Cancelled(_) => Some(EditorImageRuntimeTerminal::Rejected(
                EditorImageRuntimeFailure::TaskCancelled,
            )),
            TaskTerminal::Failed(_) => Some(EditorImageRuntimeTerminal::Rejected(
                EditorImageRuntimeFailure::TaskFailed,
            )),
        }
    }

    pub(super) fn commit_image_import(
        &mut self,
        imported: ImageImportedAsset,
    ) -> Result<AssetVersion, EditorImageRuntimeFailure> {
        if let Some(failure) = self.image_revocation_failure {
            return Err(failure);
        }
        #[cfg(feature = "presentation-2d")]
        let successor = self
            .image_publication_revision
            .checked_add(1)
            .ok_or(EditorImageRuntimeFailure::Publication)?;
        #[cfg(test)]
        let inject_fault = std::mem::take(&mut self.fault_after_next_image_publication);
        #[cfg(test)]
        let fault_reporter = self.runtime.fault_reporter();
        let outcome = self
            .runtime
            .with_driver_scope(|scope| {
                let output = scope.__apply_port::<EditorImagePublicationPort>(imported);
                #[cfg(test)]
                if inject_fault && matches!(&output, Ok(Ok(_))) {
                    fault_reporter.report(nara_app::RuntimeFault::engine(
                        nara_app::RuntimeFaultKind::RequiredService,
                        "nara.test.editor-image-publication-post-operation",
                    ));
                }
                output
            })
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?;
        let version = outcome
            .output
            .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)??;
        #[cfg(feature = "presentation-2d")]
        {
            self.image_publication_revision = successor;
        }
        // Publication is already committed. Its caller must register this version as last-good
        // before the normal authoring drive follows the Runtime's sticky fault policy.
        Ok(version)
    }

    pub(super) fn revoke_images(
        &mut self,
        handles: Vec<Handle<ImageAsset>>,
    ) -> Result<(), EditorImageRuntimeFailure> {
        if handles.is_empty() {
            return Ok(());
        }
        let result = (|| {
            if let Some(failure) = self.image_revocation_failure {
                return Err(failure);
            }
            #[cfg(feature = "presentation-2d")]
            let successor = self
                .image_publication_revision
                .checked_add(1)
                .ok_or(EditorImageRuntimeFailure::Publication)?;
            let outcome = self
                .runtime
                .with_driver_scope(|scope| scope.__apply_port::<EditorImageRevocationPort>(handles))
                .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)?;
            outcome
                .output
                .map_err(|_| EditorImageRuntimeFailure::RuntimeUnavailable)??;
            #[cfg(feature = "presentation-2d")]
            {
                self.image_publication_revision = successor;
            }
            Ok(())
        })();
        if let Err(failure) = result {
            self.image_revocation_failure = Some(failure);
        }
        #[cfg(feature = "presentation-2d")]
        self.preview.retire_candidate();
        result
    }

    #[must_use]
    pub(super) fn image_import_asset(
        &self,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
    ) -> Option<&ImageAsset> {
        if self.image_revocation_failure.is_some() {
            return None;
        }
        let world = self.runtime.world();
        if world
            .get_resource::<AssetStates>()?
            .state(handle.id())?
            .usable_version()
            != Some(expected_version)
        {
            return None;
        }
        world.get_resource::<Assets<ImageAsset>>()?.get(handle)
    }

    #[cfg(feature = "runtime-ui")]
    pub(super) fn capture_authoring_image_catalog_candidate(
        &self,
    ) -> Result<EditorAuthoringImageCatalogCandidate, EditorAuthoringImageCatalogError> {
        if self.image_revocation_failure.is_some() {
            return Err(EditorAuthoringImageCatalogError::RuntimeUnavailable);
        }
        let world = self.runtime.world();
        let server = world
            .get_resource::<AssetServer>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        let images = world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        let states = world
            .get_resource::<AssetStates>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        EditorAuthoringImageCatalogCandidate::capture(server, images, states)
    }

    #[cfg(feature = "runtime-ui")]
    pub(super) fn capture_selected_authoring_image_catalog_candidate(
        &self,
        selected: &BTreeSet<AssetId>,
    ) -> Result<EditorAuthoringImageCatalogCandidate, EditorAuthoringImageCatalogError> {
        let world = self.runtime.world();
        let server = world
            .get_resource::<AssetServer>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        let images = world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        let states = world
            .get_resource::<AssetStates>()
            .ok_or(EditorAuthoringImageCatalogError::RuntimeUnavailable)?;
        EditorAuthoringImageCatalogCandidate::capture_selected(server, images, states, selected)
    }

    pub(super) fn persistence_tasks(&self) -> &TaskPools {
        self.runtime.world().resource::<TaskPools>()
    }

    pub(super) fn has_close_failure(&self) -> bool {
        self.runtime.close_evidence().has_terminal_failure()
    }

    pub(super) fn host_custody(&self) -> &super::persistence::HostCustody {
        self.runtime
            .world()
            .resource::<super::persistence::HostCustody>()
    }

    pub(super) fn shutdown_tasks(&mut self) -> EditorAuthoringRuntimeCloseProgress {
        match self.runtime.state() {
            RuntimeState::CloseIncomplete => {
                let _ = self.runtime.request_control(RuntimeControl::RetryClose);
            }
            RuntimeState::Running | RuntimeState::Paused | RuntimeState::Faulted => {
                let _ = self.runtime.request_control(RuntimeControl::Stop);
            }
            RuntimeState::Stepping | RuntimeState::Stopping | RuntimeState::Stopped => {}
        }
        let _ = self.runtime.drive(Duration::ZERO);
        let report = self
            .runtime
            .world()
            .get_resource::<TaskShutdownReport>()
            .cloned()
            .unwrap_or_default();
        if self.runtime.state() == RuntimeState::Stopped && report.is_complete() {
            EditorAuthoringRuntimeCloseProgress::Complete(report)
        } else {
            EditorAuthoringRuntimeCloseProgress::Incomplete(report)
        }
    }

    #[cfg(test)]
    #[cfg(feature = "presentation-2d")]
    pub(super) fn test_exhaust_image_publication_revision(&mut self) {
        self.image_publication_revision = u64::MAX;
    }

    #[cfg(test)]
    pub(super) fn test_fault_after_next_image_publication(
        &mut self,
    ) -> nara_app::RuntimeFaultReporter {
        self.fault_after_next_image_publication = true;
        self.runtime.fault_reporter()
    }

    #[cfg(test)]
    pub(super) fn hold_next_image_import(&mut self) -> EditorImageImportTestControl {
        self.hold_next_image_import_with_cancellation(
            EditorImageImportTestCancellation::Cooperative,
        )
    }

    #[cfg(test)]
    pub(super) fn hold_next_image_import_through_shutdown(
        &mut self,
    ) -> EditorImageImportTestControl {
        self.hold_next_image_import_with_cancellation(
            EditorImageImportTestCancellation::ManualRelease,
        )
    }

    #[cfg(test)]
    fn hold_next_image_import_with_cancellation(
        &mut self,
        cancellation: EditorImageImportTestCancellation,
    ) -> EditorImageImportTestControl {
        assert!(
            self.next_image_import_gate.is_none(),
            "only one image import test gate may be armed"
        );
        let (started_sender, started) = mpsc::sync_channel(1);
        let (release, release_receiver) = mpsc::channel();
        self.next_image_import_gate = Some(EditorImageImportTestGate {
            started: started_sender,
            release: release_receiver,
            cancellation,
        });
        EditorImageImportTestControl {
            started,
            release: Some(release),
        }
    }

    #[cfg(test)]
    pub(super) fn image_import_budget_host(&self) -> nara_image::ImageImportBudgetHost {
        self.runtime
            .world()
            .resource::<ImageImporter>()
            .budget_host()
    }
}

fn map_image_import_error(error: &ImageImportError) -> EditorImageRuntimeFailure {
    match error {
        ImageImportError::Cancelled { .. } => EditorImageRuntimeFailure::TaskCancelled,
        _ => EditorImageRuntimeFailure::Import,
    }
}

fn start_authoring_app(app: App) -> Result<RuntimeInstance, EditorAuthoringConstructionFailure> {
    let candidate = RuntimeInstance::prepare(
        app,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::default(),
    )
    .map_err(|owner| EditorAuthoringConstructionFailure {
        diagnostics: runtime_error("project.editor.preview-runtime-start-failed"),
        retirement: EditorAuthoringRetirement::Admission(owner),
    })?;
    candidate
        .complete_startup()
        .map(|ready| ready.promote())
        .map_err(|owner| EditorAuthoringConstructionFailure {
            diagnostics: runtime_error("project.editor.preview-runtime-start-failed"),
            retirement: EditorAuthoringRetirement::Startup(owner),
        })
}

#[cfg(test)]
fn build_authoring_app() -> Result<App, DiagnosticReport> {
    let mut app = App::new();
    configure_authoring_app(&mut app)?;
    Ok(app)
}

fn configure_authoring_app(app: &mut App) -> Result<(), DiagnosticReport> {
    let task_config = authoring_task_pool_config()
        .ok_or_else(|| runtime_error("project.editor.authoring-task-config-invalid"))?;
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_scene::SceneComponentsPlugin,
        nara_diagnostic::DiagnosticsPlugin::default(),
        // The fixed authoring composition currently submits only I/O-class work. The task API
        // requires each class to retain at least one worker; reassess this allocation when adding
        // a non-I/O authoring consumer.
        nara_tasks::TaskPlugin::new(task_config),
        nara_asset::AssetPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
    ))
    .and_then(|app| app.add_plugins(super::persistence::HostCustodyPlugin))
    .and_then(|app| app.add_plugins(nara_image::ImagePlugin::default()))
    .and_then(|app| app.add_plugins(TileSetAssetPlugin))
    .map_err(|_| runtime_error("project.editor.preview-plugin-install-failed"))?;
    tilesets::install(app)?;
    app.insert_resource(EditorImageAdmissionPort)
        .and_then(|app| app.insert_resource(EditorImagePublicationPort))
        .and_then(|app| app.insert_resource(EditorImageRevocationPort))
        .and_then(|app| {
            app.__register_driver_port::<EditorImageAdmissionPort>(
                EditorImageAdmissionPort::apply_driver_input_in_world,
            )
        })
        .and_then(|app| {
            app.__register_driver_port::<EditorImagePublicationPort>(
                EditorImagePublicationPort::apply_driver_input_in_world,
            )
        })
        .and_then(|app| {
            app.__register_driver_port::<EditorImageRevocationPort>(
                EditorImageRevocationPort::apply_driver_input_in_world,
            )
        })
        .map_err(|_| runtime_error("project.editor.preview-state-install-failed"))?;
    #[cfg(feature = "presentation-2d")]
    app.add_plugins((
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .map_err(|_| runtime_error("project.editor.preview-plugin-install-failed"))?;
    Ok(())
}

fn authoring_task_pool_config() -> Option<TaskPoolConfig> {
    let defaults = TaskPoolConfig::default();
    let io = TaskKindConfig::new(
        ItemLimit::new(2)?,
        defaults.kind(TaskPoolKind::Io).pending(),
    );
    let compute = TaskKindConfig::new(
        ItemLimit::ONE,
        defaults.kind(TaskPoolKind::Compute).pending(),
    );
    let async_compute = TaskKindConfig::new(
        ItemLimit::ONE,
        defaults.kind(TaskPoolKind::AsyncCompute).pending(),
    );
    TaskPoolConfig::new(io, compute, async_compute, defaults.shutdown_policy()).ok()
}

#[cfg(test)]
mod task_config_tests {
    use super::*;

    #[test]
    fn authoring_task_pool_config_is_bounded_and_preserves_default_queue_policy() {
        let defaults = TaskPoolConfig::default();
        let config = authoring_task_pool_config().expect("the fixed authoring config is valid");

        assert_eq!(
            config.kind(TaskPoolKind::Io).workers(),
            ItemLimit::new(2).expect("two is a non-zero worker count")
        );
        assert_eq!(config.kind(TaskPoolKind::Compute).workers(), ItemLimit::ONE);
        assert_eq!(
            config.kind(TaskPoolKind::AsyncCompute).workers(),
            ItemLimit::ONE
        );
        for kind in TaskPoolKind::ALL {
            assert_eq!(config.kind(kind).pending(), defaults.kind(kind).pending());
        }
        assert_eq!(config.shutdown_policy(), defaults.shutdown_policy());
    }

    #[test]
    fn authoring_app_installs_its_bounded_task_pool_config() {
        let app = build_authoring_app().expect("the authoring app composition is valid");
        let expected = authoring_task_pool_config().expect("the fixed authoring config is valid");
        let installed = app
            .world()
            .get_resource::<TaskPools>()
            .expect("the authoring app installs its task pools");
        assert_eq!(installed.config(), &expected);

        let mut runtime = RuntimeInstance::start(app).expect("the authoring runtime starts");
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            nara_app::RuntimeControlRequestResult::Accepted(_)
        ));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while runtime.state() != RuntimeState::Stopped {
            assert!(
                std::time::Instant::now() < deadline,
                "the authoring runtime must stop"
            );
            runtime
                .drive(Duration::ZERO)
                .expect("the authoring runtime remains healthy while stopping");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

struct EditorImageImportAdmission {
    attempt_generation: u64,
    record: AssetRecord,
    source: FileCapability,
    source_identity: FileIdentity,
    project_root: DirectoryCapability,
    project_relative_path: RelativePath,
    settings: ImageImportSettings,
    #[cfg(test)]
    test_gate: Option<EditorImageImportTestGate>,
}

#[derive(Resource)]
struct EditorImageAdmissionPort;

impl __RuntimeDriverPort for EditorImageAdmissionPort {
    type Input = EditorImageImportAdmission;
    type Output = Result<EditorImageRuntimeTask, EditorImageRuntimeFailure>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(EditorImageRuntimeFailure::RuntimeUnavailable)
    }
}

impl EditorImageAdmissionPort {
    fn apply_driver_input_in_world(
        world: &mut World,
        input: EditorImageImportAdmission,
    ) -> Result<
        Result<EditorImageRuntimeTask, EditorImageRuntimeFailure>,
        (RuntimeWorldAccessError, EditorImageImportAdmission),
    > {
        Ok(spawn_image_import_in_world(world, input))
    }
}

fn spawn_image_import_in_world(
    world: &mut World,
    request: EditorImageImportAdmission,
) -> Result<EditorImageRuntimeTask, EditorImageRuntimeFailure> {
    let EditorImageImportAdmission {
        attempt_generation,
        record,
        source,
        source_identity,
        project_root,
        project_relative_path,
        settings,
        #[cfg(test)]
        test_gate,
    } = request;
    let handle = world
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .map_err(|_| EditorImageRuntimeFailure::AssetIdentity)?;
    let expected_version = world
        .resource::<AssetStates>()
        .version(handle.id())
        .unwrap_or(AssetVersion::ZERO);
    let importer = world.resource::<ImageImporter>().clone();
    let encoded_limit =
        u64::try_from(importer.limits().max_encoded_bytes().get()).unwrap_or(u64::MAX);
    let admitted = importer
        .admit_file(
            ImageFileImportRequest::new(record, source, settings),
            handle,
            expected_version,
            &world.resource::<AssetServer>(),
            &world.resource::<Assets<ImageAsset>>(),
            &world.resource::<AssetStates>(),
        )
        .map_err(|_| EditorImageRuntimeFailure::ImportAdmission)?;
    let task = world
        .resource::<TaskPools>()
        .spawn(
            TaskPoolKind::Io,
            TaskSpawnRequest::new(attempt_generation, EDITOR_IMAGE_IMPORT_TASK_DOMAIN),
            move |cancellation| {
                let imported = match admitted.import(cancellation.clone()) {
                    Ok(imported) => imported,
                    Err(error) => {
                        return EditorImageTaskOutput::Rejected(map_image_import_error(&error));
                    }
                };
                #[cfg(test)]
                if let Some(gate) = test_gate {
                    let _ = gate.started.send(());
                    loop {
                        if gate.cancellation == EditorImageImportTestCancellation::Cooperative
                            && cancellation.is_cancelled()
                        {
                            return EditorImageTaskOutput::Rejected(
                                EditorImageRuntimeFailure::TaskCancelled,
                            );
                        }
                        match gate.release.recv_timeout(Duration::from_millis(2)) {
                            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                            Err(RecvTimeoutError::Timeout) => {}
                        }
                    }
                }
                if cancellation.is_cancelled() {
                    return EditorImageTaskOutput::Rejected(
                        EditorImageRuntimeFailure::TaskCancelled,
                    );
                }
                let source = match project_root.open_file(&project_relative_path) {
                    Ok(source) if source.identity() == source_identity => source,
                    _ => {
                        return EditorImageTaskOutput::Rejected(
                            EditorImageRuntimeFailure::SourceChanged,
                        );
                    }
                };
                if source
                    .verify_digest(imported.source_digest(), encoded_limit)
                    .is_err()
                {
                    return EditorImageTaskOutput::Rejected(
                        EditorImageRuntimeFailure::SourceChanged,
                    );
                }
                EditorImageTaskOutput::Imported {
                    source: EditorImageSourceCompletion {
                        identity: source.identity(),
                        digest: imported.source_digest(),
                    },
                    imported,
                }
            },
        )
        .into_handle()
        .map_err(|_| EditorImageRuntimeFailure::TaskAdmission)?;
    Ok(EditorImageRuntimeTask { handle, task })
}

#[derive(Resource)]
struct EditorImagePublicationPort;

impl __RuntimeDriverPort for EditorImagePublicationPort {
    type Input = ImageImportedAsset;
    type Output = Result<AssetVersion, EditorImageRuntimeFailure>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(EditorImageRuntimeFailure::RuntimeUnavailable)
    }
}

impl EditorImagePublicationPort {
    fn apply_driver_input_in_world(
        world: &mut World,
        imported: ImageImportedAsset,
    ) -> Result<
        Result<AssetVersion, EditorImageRuntimeFailure>,
        (RuntimeWorldAccessError, ImageImportedAsset),
    > {
        let record = {
            let source = imported.image().source();
            let record = AssetRecord::new(
                source.stable_id(),
                source.path().clone(),
                AssetSourceKind::Image,
            );
            #[cfg(feature = "presentation-2d")]
            if let Err(reason) = super::preview::validate_authoring_image_identity(world, &record) {
                return Ok(Err(reason));
            }
            let database = world.resource::<nara_asset::ProjectAssetDatabase>();
            if database.record_for_path(record.path()) != Some(&record)
                && database.validate_insert(&record).is_err()
            {
                return Ok(Err(EditorImageRuntimeFailure::AssetIdentity));
            }
            record
        };
        let server = world.resource::<AssetServer>().clone();
        let result = world.resource_scope(
            |world, mut images: Mut<'_, Assets<ImageAsset>>| -> Result<_, _> {
                world.resource_scope(|world, mut states: Mut<'_, AssetStates>| -> Result<_, _> {
                    let mut events = world.resource_mut::<AssetEvents>();
                    imported.commit_retained(&server, &mut images, &mut states, &mut events)
                })
            },
        );
        if result.is_ok() {
            {
                let mut database = world.resource_mut::<nara_asset::ProjectAssetDatabase>();
                if database.record_for_path(record.path()) != Some(&record) {
                    database
                        .insert(record.clone())
                        .expect("exclusive image identity admission was preflighted");
                }
            }
            #[cfg(feature = "presentation-2d")]
            super::preview::publish_authoring_image_identity(world, record);
        }
        Ok(result.map_err(|_| EditorImageRuntimeFailure::Publication))
    }
}

#[derive(Resource)]
struct EditorImageRevocationPort;

impl __RuntimeDriverPort for EditorImageRevocationPort {
    type Input = Vec<Handle<ImageAsset>>;
    type Output = Result<(), EditorImageRuntimeFailure>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(EditorImageRuntimeFailure::RuntimeUnavailable)
    }
}

impl EditorImageRevocationPort {
    fn apply_driver_input_in_world(
        world: &mut World,
        handles: Vec<Handle<ImageAsset>>,
    ) -> Result<
        Result<(), EditorImageRuntimeFailure>,
        (RuntimeWorldAccessError, Vec<Handle<ImageAsset>>),
    > {
        let server = world.resource::<AssetServer>();
        let states = world.resource::<AssetStates>();
        for handle in &handles {
            if server.get_handle::<ImageAsset>(handle.id()) != Some(*handle) {
                return Ok(Err(EditorImageRuntimeFailure::AssetIdentity));
            }
            if states.next_version(handle.id()).is_err() {
                return Ok(Err(EditorImageRuntimeFailure::Publication));
            }
        }
        world.resource_scope(|world, mut images: Mut<'_, Assets<ImageAsset>>| {
            world.resource_scope(|world, mut states: Mut<'_, AssetStates>| {
                let mut events = world.resource_mut::<AssetEvents>();
                for handle in &handles {
                    images
                        .remove_with_state(*handle, &mut states, &mut events)
                        .expect("exclusive Image revocation preflighted every next version");
                }
            });
        });
        #[cfg(feature = "presentation-2d")]
        if let Some(mut prepared) = world.get_resource_mut::<nara_image::PreparedImages>() {
            for handle in handles {
                prepared.remove(nara_image::image_resource_key(handle));
            }
        }
        Ok(Ok(()))
    }
}

fn publish_authoring_image_records(
    world: &mut World,
    snapshot: &AuthoringProjectSessionSnapshot,
) -> Result<(), DiagnosticReport> {
    world.insert_resource((*snapshot.asset_database()).clone());
    {
        let mut server = world.resource_mut::<AssetServer>();
        for record in snapshot
            .asset_records()
            .filter(|record| record.source_kind() == &AssetSourceKind::Image)
        {
            server
                .reserve_record::<ImageAsset>(record)
                .map_err(|_| runtime_error("project.editor.preview-image-identity-failed"))?;
        }
    }
    tilesets::publish_initial(world, snapshot)
}

fn runtime_error(code: &'static str) -> DiagnosticReport {
    single_error(code, "Editor Scene preview could not publish a candidate")
}

#[cfg(all(test, feature = "presentation-2d"))]
mod preview_content_tests {
    mod epoch;
    mod lifetime;

    use std::sync::Arc;

    use nara_asset::{AssetPath, ProjectAssetDatabase, StableAssetId};
    use nara_image::ImageBytesImportRequest;
    use nara_render::Extent2d;
    use nara_scene::{SceneAuthoringSession, SceneDocument};

    use super::*;
    use crate::project_host::runtime::editor::{
        authoring_preview::EditorAuthoringPreviewOmissions, preview::EditorPreviewProjection,
    };

    fn preview_source_key() -> nara_scene::SourceDocumentKey {
        nara_scene::SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "authoring-preview-fixture",
            nara_scene::SourceDocumentKind::Scene,
        )
        .unwrap()
    }

    #[test]
    fn preview_switches_the_containing_source_without_changing_its_logical_slot() {
        let (mut runtime, _, _) = authoring_runtime();
        runtime.set_preview_extent(Extent2d::new(320, 180));
        let source = SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
            nara_scene::SceneEntityRecord::new(
                nara_scene::SceneEntityId::new("shared-local-id").unwrap(),
            ),
        ]));
        let first_key = preview_source_key();
        let second_key = nara_scene::SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "second-authoring-preview-fixture",
            nara_scene::SourceDocumentKind::Scene,
        )
        .unwrap();
        for (document, key) in [
            (EditorDocumentId::from_raw(1), &first_key),
            (EditorDocumentId::from_raw(2), &second_key),
            (EditorDocumentId::from_raw(1), &first_key),
        ] {
            let revision = source.revision();
            assert!(
                runtime
                    .drive_preview(Some(EditorPreviewSource {
                        document,
                        revision,
                        projection: Some(EditorPreviewProjection::new(
                            key.clone(),
                            source.document().clone(),
                            EditorAuthoringPreviewOmissions::empty(document, revision),
                        )),
                    }))
                    .is_none()
            );
            assert_eq!(
                runtime.take_preview_candidate().unwrap().document(),
                document
            );
            let world = runtime.runtime.world();
            let mut materialized_count = 0;
            for entity in world.iter_entities() {
                if let Some(materialized_source) = entity.get::<nara_scene::SceneEntitySource>() {
                    materialized_count += 1;
                    assert_eq!(materialized_source.containing_source(), key);
                    assert_eq!(
                        materialized_source.occurrence(),
                        &nara_scene::SceneOccurrenceKey::new("nara.editor", "scene-preview")
                            .unwrap(),
                    );
                }
            }
            assert_eq!(materialized_count, 1);
        }
        retire(&mut runtime);
    }

    fn authoring_runtime() -> (EditorAuthoringRuntime, AssetRecord, Handle<ImageAsset>) {
        authoring_runtime_with_claim_limit(None)
    }

    fn authoring_runtime_with_claim_limit(
        lifetime_claims: Option<usize>,
    ) -> (EditorAuthoringRuntime, AssetRecord, Handle<ImageAsset>) {
        let registry = crate::project_host::authoring::first_party_authoring_preview_registry(
            crate::project_host::composition::built_in_schema_providers(),
        )
        .unwrap();
        let record = AssetRecord::new(
            StableAssetId::parse_str("0438d605-f4a0-4b18-a30a-6587413dd63e").unwrap(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );
        let mut database = ProjectAssetDatabase::default();
        database.insert(record.clone()).unwrap();
        let mut app = build_authoring_app().unwrap();
        if let Some(lifetime_claims) = lifetime_claims {
            let world = app.world_mut().unwrap();
            let domain = nara_identity::WorldIdentityDomain::new(
                world,
                nara_identity::WorldIdentityDomainSettings::new(
                    ItemLimit::new(lifetime_claims).unwrap(),
                    ItemLimit::new(1_024).unwrap(),
                    ItemLimit::new(64).unwrap(),
                ),
            )
            .unwrap();
            world.insert_resource(domain);
        }
        let preview =
            EditorPreviewSession::install_authoring(&mut app, &registry, Arc::new(database))
                .unwrap();
        let handle = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetServer>()
            .reserve_record::<ImageAsset>(&record)
            .unwrap();
        (
            EditorAuthoringRuntime {
                runtime: RuntimeInstance::start(app).unwrap(),
                tileset_records: Vec::new(),
                image_revocation_failure: None,
                preview,
                image_publication_revision: 0,
                next_image_import_gate: None,
                fault_after_next_image_publication: false,
            },
            record,
            handle,
        )
    }

    fn imported_image(
        runtime: &EditorAuthoringRuntime,
        record: &AssetRecord,
        handle: Handle<ImageAsset>,
    ) -> ImageImportedAsset {
        let world = runtime.runtime.world();
        let states = world.resource::<AssetStates>();
        world
            .resource::<ImageImporter>()
            .import_image(
                ImageBytesImportRequest::new(
                    record.clone(),
                    include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png").to_vec(),
                    ImageImportSettings::default(),
                ),
                handle,
                states.version(handle.id()).unwrap_or(AssetVersion::ZERO),
                world.resource::<AssetServer>(),
                world.resource::<Assets<ImageAsset>>(),
                states,
            )
            .unwrap()
    }

    fn preview(
        runtime: &mut EditorAuthoringRuntime,
        source: &SceneAuthoringSession,
    ) -> EditorPreviewCandidate {
        let document = EditorDocumentId::from_raw(1);
        let revision = source.revision();
        let projection = runtime
            .preview_requires_source_projection(document, revision)
            .then(|| {
                EditorPreviewProjection::new(
                    preview_source_key(),
                    source.document().clone(),
                    EditorAuthoringPreviewOmissions::empty(document, revision),
                )
            });
        assert!(
            runtime
                .drive_preview(Some(EditorPreviewSource {
                    document,
                    revision,
                    projection,
                }))
                .is_none()
        );
        runtime.take_preview_candidate().unwrap()
    }

    fn retire(runtime: &mut EditorAuthoringRuntime) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !matches!(
            runtime.shutdown_tasks(),
            EditorAuthoringRuntimeCloseProgress::Complete(_)
        ) {
            assert!(
                std::time::Instant::now() < deadline,
                "the authoring Runtime must retire"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn preview_content_reuses_unchanged_drives_and_tracks_real_image_publications() {
        let (mut runtime, record, handle) = authoring_runtime();
        runtime.set_preview_extent(Extent2d::new(320, 240));
        let source = SceneAuthoringSession::new(SceneDocument::default());
        let before = preview(&mut runtime, &source);
        let unchanged = preview(&mut runtime, &source);
        let imported = imported_image(&runtime, &record, handle);
        let version = runtime.commit_image_import(imported).unwrap();
        let published = preview(&mut runtime, &source);
        let repainted = preview(&mut runtime, &source);
        let actual_pixels = runtime
            .image_import_asset(handle, version)
            .unwrap()
            .pixels()
            .to_vec();
        let stale = imported_image(&runtime, &record, handle);
        let accepted = imported_image(&runtime, &record, handle);
        runtime.commit_image_import(accepted).unwrap();
        let successor = preview(&mut runtime, &source);
        let rejected = runtime.commit_image_import(stale);
        let after_rejection = preview(&mut runtime, &source);
        retire(&mut runtime);

        assert_eq!(before.content_stamp(), unchanged.content_stamp());
        assert!(unchanged.geometry_revision() > before.geometry_revision());
        assert_ne!(published.content_stamp(), unchanged.content_stamp());
        assert_eq!(published.content_stamp(), repainted.content_stamp());
        assert_eq!(actual_pixels, [24, 120, 220, 255]);
        assert!(rejected.is_err());
        assert_eq!(successor.content_stamp(), after_rejection.content_stamp());
        assert_eq!(before.revision(), after_rejection.revision());
    }

    #[test]
    fn replacement_preview_runtime_cannot_reuse_a_predecessors_content_stamp() {
        let source = SceneAuthoringSession::new(SceneDocument::default());
        let (mut predecessor, _, _) = authoring_runtime();
        predecessor.set_preview_extent(Extent2d::new(320, 240));
        let before = preview(&mut predecessor, &source);
        retire(&mut predecessor);
        let (mut replacement, _, _) = authoring_runtime();
        replacement.set_preview_extent(Extent2d::new(320, 240));
        let after = preview(&mut replacement, &source);
        retire(&mut replacement);
        assert_eq!(before.document(), after.document());
        assert_eq!(before.revision(), after.revision());
        assert_ne!(before.content_stamp(), after.content_stamp());
    }

    #[test]
    fn revoked_image_rejects_admitted_results_and_can_be_imported_again_under_the_same_handle() {
        let (mut runtime, record, handle) = authoring_runtime();
        let initial = imported_image(&runtime, &record, handle);
        let initial_version = runtime.commit_image_import(initial).unwrap();
        let old_candidate = imported_image(&runtime, &record, handle);
        runtime.revoke_images(vec![handle]).unwrap();
        let after_revoke = runtime
            .image_import_asset(handle, initial_version)
            .is_none();
        let old_rejected = runtime.commit_image_import(old_candidate).is_err();
        let replacement = imported_image(&runtime, &record, handle);
        let replacement_version = runtime.commit_image_import(replacement).unwrap();
        let restored = runtime
            .image_import_asset(handle, replacement_version)
            .is_some();
        retire(&mut runtime);
        assert!(after_revoke);
        assert!(old_rejected);
        assert!(restored);
        assert!(replacement_version > initial_version);
    }

    #[test]
    fn failed_image_revocation_retires_preview_and_rejects_further_publication() {
        let (mut runtime, record, handle) = authoring_runtime();
        let initial = imported_image(&runtime, &record, handle);
        let version = runtime.commit_image_import(initial).unwrap();
        let admitted = imported_image(&runtime, &record, handle);
        runtime.set_preview_extent(Extent2d::new(320, 240));
        let source = SceneAuthoringSession::new(SceneDocument::default());
        let _ = preview(&mut runtime, &source);
        let retained_drive = runtime.drive_preview(Some(EditorPreviewSource {
            document: EditorDocumentId::from_raw(1),
            revision: source.revision(),
            projection: None,
        }));
        runtime.test_exhaust_image_publication_revision();
        let revoked = runtime.revoke_images(vec![handle]);
        let retained_candidate = runtime.take_preview_candidate().is_some();
        let subsequent_report = runtime.drive_preview(None);
        let new_candidate = runtime.take_preview_candidate().is_some();
        let available = runtime.image_import_asset(handle, version).is_some();
        let publication = runtime.commit_image_import(admitted);
        retire(&mut runtime);
        assert!(retained_drive.is_none());
        assert_eq!(revoked, Err(EditorImageRuntimeFailure::Publication));
        assert!(!retained_candidate);
        assert!(subsequent_report.is_none());
        assert!(!new_candidate);
        assert!(!available);
        assert_eq!(publication, Err(EditorImageRuntimeFailure::Publication));
    }

    #[test]
    fn exhausted_image_publication_identity_rejects_before_mutating_the_asset_store() {
        let (mut runtime, record, handle) = authoring_runtime();
        let imported = imported_image(&runtime, &record, handle);
        runtime.image_publication_revision = u64::MAX;
        let result = runtime.commit_image_import(imported);
        let world = runtime.runtime.world();
        let image_was_written = world.resource::<Assets<ImageAsset>>().get(handle).is_some();
        let version = world.resource::<AssetStates>().version(handle.id());
        retire(&mut runtime);
        assert_eq!(result, Err(EditorImageRuntimeFailure::Publication));
        assert!(!image_was_written);
        assert!(version.is_none_or(|version| version == AssetVersion::ZERO));
    }
}
