use std::{collections::BTreeSet, time::Duration};

use nara_app::{
    __RuntimeDriverPort, App, RuntimeControl, RuntimeInstance, RuntimeState,
    RuntimeWorldAccessError,
};
use nara_asset::{
    AssetEvents, AssetId, AssetRecord, AssetServer, AssetSourceKind, AssetStates, AssetVersion,
    Assets, Handle,
};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Mut, Resource, World};
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, FileIdentity, RelativePath};
use nara_image::{
    ImageAsset, ImageFileImportRequest, ImageImportError, ImageImportSettings, ImageImportedAsset,
    ImageImporter, ImagePublicationFailureKind,
};
use nara_reflect::{ComponentRegistryPlugin, ComponentRegistrySnapshot};
#[cfg(feature = "presentation-2d")]
use nara_scene::SceneAuthoringRevision;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskShutdownReport, TaskSpawnRequest,
    TaskTerminal,
};
#[cfg(feature = "presentation-2d")]
use nara_tooling::{EditorDocumentId, WorldIdentitySnapshot};

#[cfg(test)]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};

use crate::project_host::{ProjectContentSnapshot, authoring::AuthoringProjectSessionSnapshot};

use super::super::{publish_snapshot_images, single_error};
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

/// Owns the Editor's single task and image-asset world.
///
/// Scene preview is an optional private subsystem of this runtime. Other authoring workflows must
/// reuse this owner instead of creating another `App`, task pool, or image store.
pub(super) struct EditorAuthoringRuntime {
    runtime: RuntimeInstance,
    #[cfg(feature = "presentation-2d")]
    preview: EditorPreviewSession,
    #[cfg(test)]
    next_image_import_gate: Option<EditorImageImportTestGate>,
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
    ) -> Result<Self, DiagnosticReport> {
        #[cfg(not(feature = "presentation-2d"))]
        let _ = registry;
        let mut app = build_authoring_app()?;
        #[cfg(feature = "presentation-2d")]
        let preview = EditorPreviewSession::install_trusted(&mut app, project.clone(), registry)?;
        publish_snapshot_images(
            app.world_mut()
                .map_err(|_| runtime_error("project.editor.preview-world-unavailable"))?,
            &project,
        )
        .map_err(|error| error.diagnostics)?;
        let runtime = RuntimeInstance::start(app)
            .map_err(|_| runtime_error("project.editor.preview-runtime-start-failed"))?;
        Ok(Self {
            runtime,
            #[cfg(feature = "presentation-2d")]
            preview,
            #[cfg(test)]
            next_image_import_gate: None,
        })
    }

    pub(super) fn new_authoring(
        project: &AuthoringProjectSessionSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
        #[cfg(not(feature = "presentation-2d"))]
        let _ = registry;
        let mut app = build_authoring_app()?;
        #[cfg(feature = "presentation-2d")]
        let preview = EditorPreviewSession::install_authoring(&mut app, registry)?;
        publish_authoring_image_records(
            app.world_mut()
                .map_err(|_| runtime_error("project.editor.preview-world-unavailable"))?,
            project,
        )?;
        let runtime = RuntimeInstance::start(app)
            .map_err(|_| runtime_error("project.editor.preview-runtime-start-failed"))?;
        Ok(Self {
            runtime,
            #[cfg(feature = "presentation-2d")]
            preview,
            #[cfg(test)]
            next_image_import_gate: None,
        })
    }

    pub(super) fn rebuild_for_project(
        &self,
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
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
        self.preview.drive(&mut self.runtime, source)
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
    ) -> Result<AssetVersion, ImageImportError> {
        self.runtime
            .with_driver_scope(|scope| scope.__apply_port::<EditorImagePublicationPort>(imported))
            .map_err(|_| ImageImportError::Publication(ImagePublicationFailureKind::Stale))?
            .map_err(|_| ImageImportError::Publication(ImagePublicationFailureKind::Stale))?
    }

    #[must_use]
    pub(super) fn image_import_asset(
        &self,
        handle: Handle<ImageAsset>,
        expected_version: AssetVersion,
    ) -> Option<&ImageAsset> {
        let world = self.runtime.world();
        if world.get_resource::<AssetStates>()?.version(handle.id()) != Some(expected_version) {
            return None;
        }
        world.get_resource::<Assets<ImageAsset>>()?.get(handle)
    }

    #[cfg(feature = "runtime-ui")]
    pub(super) fn capture_authoring_image_catalog_candidate(
        &self,
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

fn build_authoring_app() -> Result<App, DiagnosticReport> {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_scene::SceneComponentsPlugin,
        nara_diagnostic::DiagnosticsPlugin::default(),
        nara_tasks::TaskPlugin::default(),
        nara_asset::AssetPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
    ))
    .and_then(|app| app.add_plugins(nara_image::ImagePlugin::default()))
    .map_err(|_| runtime_error("project.editor.preview-plugin-install-failed"))?;
    app.insert_resource(EditorImageAdmissionPort)
        .and_then(|app| app.insert_resource(EditorImagePublicationPort))
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
        .map_err(|_| runtime_error("project.editor.preview-state-install-failed"))?;
    #[cfg(feature = "presentation-2d")]
    app.add_plugins((
        nara_render::RenderPlugin,
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .map_err(|_| runtime_error("project.editor.preview-plugin-install-failed"))?;
    Ok(app)
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
    type Output = Result<AssetVersion, ImageImportError>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(ImageImportError::Publication(
            ImagePublicationFailureKind::Stale,
        ))
    }
}

impl EditorImagePublicationPort {
    fn apply_driver_input_in_world(
        world: &mut World,
        imported: ImageImportedAsset,
    ) -> Result<Result<AssetVersion, ImageImportError>, (RuntimeWorldAccessError, ImageImportedAsset)>
    {
        let server = world.resource::<AssetServer>().clone();
        Ok(world.resource_scope(
            |world, mut images: Mut<'_, Assets<ImageAsset>>| -> Result<_, _> {
                world.resource_scope(|world, mut states: Mut<'_, AssetStates>| -> Result<_, _> {
                    let mut events = world.resource_mut::<AssetEvents>();
                    imported.commit_retained(&server, &mut images, &mut states, &mut events)
                })
            },
        ))
    }
}

fn publish_authoring_image_records(
    world: &mut World,
    snapshot: &AuthoringProjectSessionSnapshot,
) -> Result<(), DiagnosticReport> {
    let mut server = world.resource_mut::<AssetServer>();
    for record in snapshot
        .asset_records()
        .filter(|record| record.source_kind() == &AssetSourceKind::Image)
    {
        server
            .reserve_record::<ImageAsset>(record)
            .map_err(|_| runtime_error("project.editor.preview-image-identity-failed"))?;
    }
    Ok(())
}

fn runtime_error(code: &'static str) -> DiagnosticReport {
    single_error(code, "Editor Scene preview could not publish a candidate")
}
