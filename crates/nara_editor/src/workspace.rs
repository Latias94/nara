use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::TryRecvError,
    time::Duration,
};

use anyhow::{Context as _, Result};
use nara_core::{AssetId, DiagnosticSeverity, DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{
    AssetBrowserModel, AssetKind, ConsoleModel, EditorCommand, EditorDocument, EditorGesture,
    EditorProject, EditorRegistry, EditorSession, PrefabFieldOverride,
    play::{PlayControl, PlayState},
};
use nara_project::{
    EditorPlayMode,
    workflow::{ProjectTrust, read_schema_cache},
};
use nara_reflect::{ComponentSchemaCatalog, ComponentTypeId, Schema};
use nara_scene::{
    authoring::{
        SceneAsset, SceneComponent, SceneDocument, SceneEntity, SpriteSource,
        builtin_component_schemas,
    },
    spatial::{Transform2d, Visibility},
};
use open_gpui::{Context, WeakEntity};

use crate::{
    inspector::InspectorPanel,
    jobs::{Job, Message},
};

mod hot_reload;
#[cfg(test)]
mod tests;
use hot_reload::HotReload;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Idle,
    Schema,
    Building,
    Starting,
    Running,
    RunningExternal,
    Paused,
    Stopping,
}

impl JobStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Edit mode",
            Self::Schema => "Refreshing Schema…",
            Self::Building => "Building game…",
            Self::Starting => "Starting game…",
            Self::Running => "Playing",
            Self::RunningExternal => "Playing saved project",
            Self::Paused => "Paused",
            Self::Stopping => "Stopping…",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingPlayControl {
    Queued(PlayControl),
    Delivered,
}

struct JobOrigin {
    directory: PathBuf,
}

struct ProjectJob {
    origin: Rc<JobOrigin>,
    worker: Job,
}

struct DeferredSchema {
    origin: Rc<JobOrigin>,
    catalog: ComponentSchemaCatalog,
}

pub(crate) struct Workspace {
    pub(crate) session: EditorSession,
    pub(crate) revision: u64,
    asset_scan_revision: u64,
    preview_assets_revision: u64,
    pub project: EditorProject,
    pub assets: AssetBrowserModel,
    pub selected_asset: Option<AssetPath>,
    opened_asset: Option<AssetId>,
    pub console: ConsoleModel,
    pending: Option<PendingPlayControl>,
    pub(crate) steps: u64,
    pub(crate) status: JobStatus,
    trust: ProjectTrust,
    job: Option<ProjectJob>,
    deferred_schema: Option<DeferredSchema>,
    inspector: Option<WeakEntity<InspectorPanel>>,
    hot_reload: HotReload,
}

impl Workspace {
    pub fn open(directory: &Path, trust: ProjectTrust) -> Result<Self> {
        let project = EditorProject::open(directory)?;
        let assets = project.assets();
        let saved_entry = project.entry_scene();
        let initial_scene = saved_entry
            .filter(|path| assets.entries().iter().any(|entry| entry.path == **path))
            .or_else(|| {
                assets
                    .entries()
                    .iter()
                    .find(|entry| entry.kind == AssetKind::Scene)
                    .map(|entry| &entry.path)
            });
        let selected_asset = initial_scene.cloned();
        let opened_asset = initial_scene
            .and_then(|path| project.asset_reference(path))
            .map(|asset| asset.id);
        let mut console = ConsoleModel::default();
        if let Some(path) = saved_entry.filter(|path| Some(*path) != initial_scene) {
            console.line(
                DiagnosticSeverity::Warning,
                format!("Saved entry scene '{path}' is unavailable"),
            );
        }
        let project_catalog = match read_schema_cache(project.directory()) {
            Ok(Some(catalog)) => {
                console.line(
                    DiagnosticSeverity::Info,
                    "Loaded the project's Schema cache",
                );
                catalog
            }
            Ok(None) => ComponentSchemaCatalog::default(),
            Err(error) => {
                console.line(DiagnosticSeverity::Warning, error.to_string());
                ComponentSchemaCatalog::default()
            }
        };
        let registry = EditorRegistry::new(builtin_component_schemas()?, project_catalog)?;
        let schemas = registry.catalog().components();
        let document = match initial_scene {
            Some(path) => project.open_document_with(path, schemas)?,
            None => EditorDocument::new(SceneDocument::new(DocumentId::try_from_uuid(
                uuid::Uuid::new_v4(),
            )?))?,
        };
        let prefabs = project.load_prefabs(document.scene(), schemas)?;
        let mut session = EditorSession::new(document, registry);
        session.set_prefabs(prefabs)?;
        console.line(
            DiagnosticSeverity::Info,
            format!("Opened {}", project.manifest().project.name),
        );
        Ok(Self {
            session,
            revision: 1,
            asset_scan_revision: 0,
            preview_assets_revision: 0,
            project,
            assets,
            selected_asset,
            opened_asset,
            console,
            status: JobStatus::Idle,
            pending: None,
            steps: 0,
            trust,
            job: None,
            deferred_schema: None,
            inspector: None,
            hot_reload: HotReload::default(),
        })
    }

    pub(crate) fn attach_inspector(&mut self, inspector: WeakEntity<InspectorPanel>) {
        self.inspector = Some(inspector);
    }

    pub(crate) fn apply(&mut self, command: EditorCommand, cx: &mut Context<Self>) -> bool {
        self.edit(|session| session.apply(command), cx)
    }

    pub(crate) fn begin_gesture(&mut self, cx: &mut Context<Self>) -> Option<EditorGesture> {
        match self.session.begin_gesture() {
            Ok(gesture) => {
                cx.notify();
                Some(gesture)
            }
            Err(error) => {
                self.warn(error.to_string(), cx);
                None
            }
        }
    }

    pub(crate) fn apply_gesture(
        &mut self,
        gesture: &EditorGesture,
        command: EditorCommand,
        cx: &mut Context<Self>,
    ) -> bool {
        self.edit(|session| session.apply_gesture(gesture, command), cx)
    }

    pub(crate) fn end_gesture(&mut self, gesture: &EditorGesture, cx: &mut Context<Self>) {
        if self.session.owns_gesture(gesture) {
            self.session.end_gesture(gesture);
            self.publish_deferred_schema();
            cx.notify();
        }
    }

    pub(crate) fn cancel_gesture(&mut self, gesture: &EditorGesture, cx: &mut Context<Self>) {
        if self.session.cancel_gesture(gesture) {
            self.revision = self.revision.wrapping_add(1);
            self.publish_deferred_schema();
            cx.notify();
        }
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        self.edit(EditorSession::undo, cx);
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        self.edit(EditorSession::redo, cx);
    }

    fn edit(
        &mut self,
        operation: impl FnOnce(&mut EditorSession) -> Result<bool, nara_editor_core::EditorError>,
        cx: &mut Context<Self>,
    ) -> bool {
        match operation(&mut self.session) {
            Ok(true) => {
                self.revision = self.revision.wrapping_add(1);
                cx.notify();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.warn(error.to_string(), cx);
                false
            }
        }
    }

    pub(crate) fn warn(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.console.line(DiagnosticSeverity::Warning, message);
        cx.notify();
    }

    pub fn select(
        &mut self,
        entities: impl IntoIterator<Item = EntityUuid>,
        cx: &mut Context<Self>,
    ) {
        if self.session.gesture_active() {
            return;
        }
        self.session.selection_mut().replace(entities);
        cx.notify();
    }

    pub fn create(
        &mut self,
        center: Vec2,
        image: Option<AssetPath>,
        parent: Option<EntityUuid>,
        cx: &mut Context<Self>,
    ) {
        let image = if let Some(path) = image {
            let Some(asset) = self.project.asset_reference(&path) else {
                self.warn(
                    format!("Image '{path}' is missing from the asset index"),
                    cx,
                );
                return;
            };
            Some(asset)
        } else {
            None
        };
        self.create_entity(center, image, parent, cx);
    }

    pub(crate) fn create_image(&mut self, center: Vec2, id: AssetId, cx: &mut Context<Self>) {
        let Some(hint) = self.project.asset_hint(id) else {
            self.warn(format!("Image {id} is missing from the asset index"), cx);
            return;
        };
        self.create_entity(center, Some(SceneAsset { id, hint }), None, cx);
    }

    fn create_entity(
        &mut self,
        center: Vec2,
        image: Option<SceneAsset>,
        parent: Option<EntityUuid>,
        cx: &mut Context<Self>,
    ) {
        let result = (|| -> Result<_> {
            let id = EntityUuid::try_from_uuid(uuid::Uuid::new_v4())?;
            let name = image.as_ref().map_or("New Entity", |asset| {
                asset.hint.as_str().rsplit('/').next().unwrap_or("Sprite")
            });
            let mut entity = SceneEntity::new(id, name);
            entity.parent = parent;
            entity.set(&Transform2d::from_translation(center))?;
            let mut commands = Vec::new();
            if let Some(asset) = image {
                entity.set(&SpriteSource {
                    size: Vec2::splat(32.0),
                    image: Some(asset.id),
                    ..Default::default()
                })?;
                commands.push(EditorCommand::DeclareAsset(asset));
            }
            commands.push(EditorCommand::Create(entity));
            Ok((id, EditorCommand::Batch(commands)))
        })();
        match result {
            Ok((id, command)) => {
                if self.apply(command, cx) {
                    self.select([id], cx);
                }
            }
            Err(error) => self.warn(error.to_string(), cx),
        }
    }

    pub fn delete_selection(&mut self, cx: &mut Context<Self>) {
        self.apply(
            EditorCommand::Delete(self.session.selection().entities().to_vec()),
            cx,
        );
    }

    pub fn set_visibility(&mut self, visibility: Visibility, cx: &mut Context<Self>) {
        let id = ComponentTypeId::new(Visibility::COMPONENT_ID);
        let value = Visibility::type_schema().and_then(|schema| {
            Ok(SceneComponent::known(
                schema.version(),
                visibility.encode_component()?,
            ))
        });
        match value {
            Ok(value) => self.apply(
                EditorCommand::SetComponent {
                    entities: self.session.selection().entities().to_vec(),
                    component: id,
                    value,
                },
                cx,
            ),
            Err(error) => {
                self.warn(error.to_string(), cx);
                false
            }
        };
    }

    pub fn open_asset(&mut self, path: &AssetPath, cx: &mut Context<Self>) {
        if self.session.gesture_active() {
            return;
        }
        if matches!(AssetKind::of(path), AssetKind::Scene | AssetKind::Prefab) {
            let document = self.session.document();
            if self.session.is_dirty()
                && (document.path().is_some()
                    || !document.scene().entities.is_empty()
                    || !document.scene().assets.is_empty()
                    || self.session.can_undo())
            {
                self.warn("Save the current document before opening another one", cx);
                return;
            }
            if self.inspector.as_ref().is_some_and(|inspector| {
                inspector
                    .read_with(cx, |panel, cx| !panel.drafts(cx).is_empty())
                    .unwrap_or(false)
            }) {
                self.warn(
                    "Apply or cancel Inspector drafts before opening another document",
                    cx,
                );
                return;
            }
            let result = (|| -> Result<()> {
                if self.project.asset_reference(path).is_none() {
                    self.project.refresh_assets()?;
                    self.assets = self.project.assets();
                }
                let schemas = self.session.registry().catalog().components();
                let document = self.project.open_document_with(path, schemas)?;
                let prefabs = self.project.load_prefabs(document.scene(), schemas)?;
                self.session.open_document_with_prefabs(document, prefabs)?;
                Ok(())
            })();
            if let Err(error) = result {
                self.warn(error.to_string(), cx);
                return;
            }
            self.opened_asset = self.project.asset_reference(path).map(|asset| asset.id);
            self.revision = self.revision.wrapping_add(1);
        }
        self.selected_asset = Some(path.clone());
        cx.notify();
    }

    pub(crate) fn preview_revision(&self) -> (u64, u64) {
        (self.revision, self.preview_assets_revision)
    }

    pub(crate) fn refresh_assets(&mut self, cx: &mut Context<Self>) {
        if self.hot_reload.request_discovery() {
            return;
        }
        if !self.discover_assets(cx) {
            return;
        }
        let refreshed = (|| -> Result<()> {
            let prefabs = self.project.load_prefabs(
                self.session.document().scene(),
                self.session.registry().catalog().components(),
            )?;
            self.session.set_prefabs(prefabs)?;
            Ok(())
        })();
        match refreshed {
            Ok(()) => self.revision = self.revision.wrapping_add(1),
            Err(error) => self.warn(error.to_string(), cx),
        }
        cx.notify();
    }

    fn discover_assets(&mut self, cx: &mut Context<Self>) -> bool {
        let selected = self.selected_asset_id();
        if let Err(error) = self.project.refresh_assets() {
            self.warn(error.to_string(), cx);
            return false;
        }
        self.update_asset_browser(selected);
        cx.notify();
        true
    }

    fn selected_asset_id(&self) -> Option<AssetId> {
        self.selected_asset
            .as_ref()
            .and_then(|path| self.project.asset_reference(path))
            .map(|asset| asset.id)
    }

    fn update_asset_browser(&mut self, selected: Option<AssetId>) {
        self.assets = self.project.assets();
        self.selected_asset = selected.and_then(|id| self.project.asset_hint(id));
        self.asset_scan_revision = self.asset_scan_revision.wrapping_add(1);
    }

    pub(crate) fn apply_to_prefab(&mut self, edit: &PrefabFieldOverride, cx: &mut Context<Self>) {
        let catalog = self.session.registry().catalog().clone();
        let expected = self.session.prefab_sources().clone();
        let project = &self.project;
        match self.session.apply_to_prefab(edit, |id, _, after| {
            project.write_prefab(id, after, &expected, &catalog)
        }) {
            Ok(true) => {
                self.revision = self.revision.wrapping_add(1);
                self.console.line(
                    DiagnosticSeverity::Info,
                    "Prefab saved. Save the scene to keep the cleared override.",
                );
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => self.warn(error.to_string(), cx),
        }
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        self.save_with_completion(cx, |_, _| Ok(()), |_, _| {});
    }

    pub(crate) fn save_with_completion(
        &mut self,
        cx: &mut Context<Self>,
        before_write: impl FnOnce(&Workspace, &open_gpui::App) -> Result<()> + 'static,
        complete: impl FnOnce(bool, &mut Context<Self>) + 'static,
    ) {
        if self.session.gesture_active() {
            self.warn("Finish the current gesture before saving", cx);
            complete(false, cx);
            return;
        }
        if self.session.document().path().is_some() {
            let result = before_write(self, cx).and_then(|()| {
                let asset = self
                    .opened_asset
                    .context("Reopen this scene from Assets to restore its file identity")?;
                let selected = self.selected_asset_id();
                let path = self
                    .project
                    .document_save_path(asset, self.session.document().scene().id)?;
                self.update_asset_browser(selected);
                self.session
                    .refresh_asset_hints(|id| self.project.asset_hint(id));
                Ok(self.session.save_as(path)?)
            });
            complete(self.saved(result, cx), cx);
            return;
        }
        let Ok(source) = self.project.asset_source() else {
            self.warn("The project asset directory is unavailable", cx);
            complete(false, cx);
            return;
        };
        let document_id = self.session.document().scene().id;
        let root = source.root().to_path_buf();
        let picker = cx.prompt_for_new_path(&root, Some("untitled.scene.ron"));
        cx.spawn(async move |workspace, cx| {
            let result = picker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let saved = match result {
                    Ok(Ok(Some(path))) => {
                        let result = (|| -> Result<AssetPath> {
                            let document = workspace.session.document();
                            anyhow::ensure!(
                                document.scene().id == document_id && document.path().is_none(),
                                "The active untitled scene changed while the save dialog was open"
                            );
                            before_write(workspace, cx)?;
                            let name = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .filter(|name| {
                                    path.as_os_str()
                                        .as_encoded_bytes()
                                        .ends_with(name.as_bytes())
                                })
                                .context("Use a portable scene filename")?;
                            AssetPath::new(name).context("Use a portable scene filename")?;
                            anyhow::ensure!(
                                name.ends_with(".scene.ron"),
                                "Include .scene.ron in the filename and save again"
                            );
                            let parent = path
                                .parent()
                                .context("The selected path has no parent")?
                                .canonicalize()?;
                            anyhow::ensure!(
                                parent.starts_with(&root),
                                "Save the scene inside the project's asset directory"
                            );
                            let destination = parent.join(name);
                            let path = workspace.project.relative_document_path(&destination)?;
                            workspace
                                .session
                                .refresh_asset_hints(|id| workspace.project.asset_hint(id));
                            match workspace.session.save_new(&destination) {
                                Err(nara_editor_core::EditorError::Scene(
                                    nara_scene::authoring::SceneDocumentError::Io {
                                        source, ..
                                    },
                                )) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                                    anyhow::bail!(
                                        "A file already exists at {}. Choose a new filename.",
                                        destination.display()
                                    );
                                }
                                result => result?,
                            }
                            Ok(path)
                        })();
                        match result {
                            Ok(path) => {
                                workspace.discover_assets(cx);
                                workspace.opened_asset = workspace
                                    .project
                                    .asset_reference(&path)
                                    .map(|asset| asset.id);
                                workspace.saved(Ok(()), cx)
                            }
                            Err(error) => workspace.saved(Err(error), cx),
                        }
                    }
                    Ok(Err(error)) => {
                        workspace.warn(error.to_string(), cx);
                        false
                    }
                    Ok(Ok(None)) | Err(_) => false,
                };
                complete(saved, cx);
            });
        })
        .detach();
    }

    fn saved(&mut self, result: Result<()>, cx: &mut Context<Self>) -> bool {
        match result {
            Ok(()) => {
                self.console.line(DiagnosticSeverity::Info, "Scene saved");
                cx.notify();
                true
            }
            Err(error) => {
                self.warn(format!("{error:#}"), cx);
                false
            }
        }
    }

    pub fn trust(&self) -> ProjectTrust {
        self.trust
    }

    pub fn trust_project(&mut self, cx: &mut Context<Self>) {
        self.trust = ProjectTrust::Trusted;
        self.console.line(
            DiagnosticSeverity::Info,
            "Rust builds and Play enabled for this session",
        );
        cx.notify();
    }

    pub fn refresh_schema(&mut self, cx: &mut Context<Self>) {
        self.start_job(
            JobStatus::Schema,
            |project, trust| Job::refresh(project.to_path_buf(), trust),
            cx,
        );
    }

    pub fn play(&mut self, cx: &mut Context<Self>) {
        if self.session.gesture_active() {
            return;
        }
        let scene = self.session.document().scene().clone();
        let prefabs = self.session.prefab_sources().clone();
        let schemas = self.session.registry().catalog().clone();
        self.steps = 0;
        self.start_job(
            JobStatus::Building,
            |project, trust| Job::play(project.to_path_buf(), scene, prefabs, schemas, trust),
            cx,
        );
    }

    fn start_job(
        &mut self,
        status: JobStatus,
        start: impl FnOnce(&Path, ProjectTrust) -> std::io::Result<Job>,
        cx: &mut Context<Self>,
    ) {
        if self.status != JobStatus::Idle {
            return;
        }
        if self.trust != ProjectTrust::Trusted {
            self.warn(
                "Trust this project before building or running its Rust code",
                cx,
            );
            return;
        }
        let directory = self.project.directory();
        match start(directory, self.trust) {
            Ok(worker) => {
                let origin = Rc::new(JobOrigin {
                    directory: directory.to_path_buf(),
                });
                self.job = Some(ProjectJob {
                    origin: origin.clone(),
                    worker,
                });
                self.deferred_schema = None;
                self.status = status;
                self.pending = None;
                cx.notify();
                cx.spawn(async move |workspace, cx| {
                    let timer = cx.background_executor().clone();
                    while workspace
                        .update(cx, |workspace, cx| workspace.poll_job(&origin, cx))
                        .unwrap_or(false)
                    {
                        timer.timer(Duration::from_millis(25)).await;
                    }
                })
                .detach();
            }
            Err(error) => self.warn(error.to_string(), cx),
        }
    }

    pub fn control_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn control(&mut self, control: PlayControl, cx: &mut Context<Self>) {
        if self.control_pending() || !matches!(self.status, JobStatus::Running | JobStatus::Paused)
        {
            return;
        }
        if let Some(job) = &self.job {
            match job.worker.control(control) {
                Ok(()) => {
                    self.pending = Some(PendingPlayControl::Queued(control));
                    cx.notify();
                }
                Err(error) => self.warn(format!("Could not send Play control: {error}"), cx),
            }
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let schema_waiting = self.deferred_schema.take().is_some();
        if let Some(job) = &self.job {
            job.worker.stop();
            self.status = JobStatus::Stopping;
            self.pending = None;
            cx.notify();
        } else if schema_waiting && self.status == JobStatus::Schema {
            self.status = JobStatus::Idle;
            cx.notify();
        }
    }

    pub(crate) fn take_job(&mut self, cx: &mut Context<Self>) -> Option<Job> {
        self.stop(cx);
        self.job.take().map(|job| job.worker)
    }

    fn poll_job(&mut self, origin: &Rc<JobOrigin>, cx: &mut Context<Self>) -> bool {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| Rc::ptr_eq(origin, &job.origin))
        {
            return false;
        }
        if origin.directory != self.project.directory() {
            if let Some(worker) = self.take_job(cx) {
                cx.background_executor()
                    .spawn(async move { drop(worker) })
                    .detach();
            }
            self.status = JobStatus::Idle;
            return false;
        }
        let mut changed = false;
        for _ in 0..256 {
            let Some(job) = &self.job else { break };
            let message = match job.worker.messages.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.finish_job(Err("The project worker exited unexpectedly".into()));
                    changed = true;
                    break;
                }
            };
            changed = true;
            self.receive_job_message(message, origin);
        }
        if changed {
            cx.notify();
        }
        self.job.is_some()
    }

    fn receive_job_message(&mut self, message: Message, origin: &Rc<JobOrigin>) {
        match message {
            Message::Log(diagnostic) => self.console.push(diagnostic),
            Message::Schema(catalog) if self.status != JobStatus::Stopping => {
                self.deferred_schema = Some(DeferredSchema {
                    origin: origin.clone(),
                    catalog,
                });
                self.publish_deferred_schema();
            }
            Message::Schema(_) => {}
            Message::Play {
                state,
                mode,
                pending,
                steps,
            } => {
                if self.status != JobStatus::Stopping {
                    self.status = match state {
                        PlayState::Starting => JobStatus::Starting,
                        PlayState::Running if mode == EditorPlayMode::External => {
                            JobStatus::RunningExternal
                        }
                        PlayState::Running => JobStatus::Running,
                        PlayState::Paused => JobStatus::Paused,
                        PlayState::Exited(_) => self.status,
                    };
                }
                self.steps = steps;
                if self.pending == Some(PendingPlayControl::Delivered) && pending.is_none() {
                    self.pending = None;
                }
            }
            Message::PlayControlResult { control, result }
                if self.pending == Some(PendingPlayControl::Queued(control)) =>
            {
                match result {
                    Ok(()) => self.pending = Some(PendingPlayControl::Delivered),
                    Err(error) => {
                        self.pending = None;
                        self.console.line(DiagnosticSeverity::Warning, error);
                    }
                }
            }
            Message::PlayControlResult { .. } => {}
            Message::Finished(result) => self.finish_job(result),
        }
    }

    fn publish_deferred_schema(&mut self) {
        if self.session.gesture_active() {
            return;
        }
        let Some(schema) = self.deferred_schema.take() else {
            return;
        };
        if self.status == JobStatus::Schema && self.job.is_none() {
            self.status = JobStatus::Idle;
        }
        if schema.origin.directory != self.project.directory()
            || self
                .job
                .as_ref()
                .is_some_and(|job| !Rc::ptr_eq(&schema.origin, &job.origin))
        {
            return;
        }
        let result = (|| -> Result<_> {
            let registry = EditorRegistry::new(builtin_component_schemas()?, schema.catalog)?;
            Ok(self.session.set_registry(registry)?)
        })();
        match result {
            Ok(diagnostics) => {
                self.revision = self.revision.wrapping_add(1);
                self.console
                    .line(DiagnosticSeverity::Info, "Schema refreshed");
                for error in diagnostics {
                    self.console
                        .line(DiagnosticSeverity::Warning, error.to_string());
                }
            }
            Err(error) => self
                .console
                .line(DiagnosticSeverity::Warning, error.to_string()),
        }
    }

    fn finish_job(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.deferred_schema = None;
            self.console.line(DiagnosticSeverity::Error, error);
        } else if self.status == JobStatus::Stopping {
            self.console
                .line(DiagnosticSeverity::Info, "Stopped the project process");
        }
        self.job = None;
        if self.status != JobStatus::Schema || self.deferred_schema.is_none() {
            self.status = JobStatus::Idle;
        }
        self.pending = None;
    }
}
