use std::{path::Path, sync::mpsc::TryRecvError, time::Duration};

use anyhow::{Context as _, Result};
use nara_core::{DiagnosticSeverity, DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{
    AssetBrowserModel, AssetKind, ConsoleModel, EditorCommand, EditorDocument, EditorGesture,
    EditorProject, EditorRegistry, EditorSession,
    play::{PlayControl, PlayState},
};
use nara_project::{
    EditorPlayMode,
    workflow::{ProjectTrust, read_schema_cache},
};
use nara_reflect::{ComponentSchemaCatalog, ComponentTypeId, PersistentComponentProvider};
use nara_scene::{
    authoring::{SceneDocument, SceneEntity, SpriteSource, builtin_component_schemas},
    spatial::{Transform2d, Visibility},
};
use open_gpui::Context;

use crate::jobs::{Job, Message};

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

pub(crate) struct Workspace {
    pub(crate) session: EditorSession,
    pub(crate) revision: u64,
    asset_scan_revision: u64,
    pub project: EditorProject,
    pub assets: AssetBrowserModel,
    pub selected_asset: Option<AssetPath>,
    pub console: ConsoleModel,
    pending: Option<PendingPlayControl>,
    pub(crate) steps: u64,
    pub(crate) status: JobStatus,
    trust: ProjectTrust,
    job: Option<Job>,
}

impl Workspace {
    pub fn open(directory: &Path, trust: ProjectTrust) -> Result<Self> {
        let project = EditorProject::open(directory)?;
        let assets = project.assets()?;
        let initial_scene = project.entry_scene().or_else(|| {
            assets
                .entries()
                .iter()
                .find(|entry| entry.kind == AssetKind::Scene)
                .map(|entry| &entry.path)
        });
        let document = match initial_scene {
            Some(path) => project.open_document(path)?,
            None => EditorDocument::new(SceneDocument::new(DocumentId::try_from_uuid(
                uuid::Uuid::new_v4(),
            )?))?,
        };
        let selected_asset = initial_scene.cloned();
        let mut console = ConsoleModel::default();
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
        console.line(
            DiagnosticSeverity::Info,
            format!("Opened {}", project.manifest().project.name),
        );
        Ok(Self {
            session: EditorSession::new(document, registry),
            revision: 1,
            asset_scan_revision: 0,
            project,
            assets,
            selected_asset,
            console,
            status: JobStatus::Idle,
            pending: None,
            steps: 0,
            trust,
            job: None,
        })
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
            cx.notify();
        }
    }

    pub(crate) fn cancel_gesture(&mut self, gesture: &EditorGesture, cx: &mut Context<Self>) {
        self.edit(|session| Ok(session.cancel_gesture(gesture)), cx);
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
        let result = (|| -> Result<_> {
            let id = EntityUuid::try_from_uuid(uuid::Uuid::new_v4())?;
            let name = image.as_ref().map_or("New Entity", |path| {
                path.as_str().rsplit('/').next().unwrap_or("Sprite")
            });
            let mut entity = SceneEntity::new(id, name);
            entity.parent = parent;
            entity.set(&Transform2d::from_translation(center))?;
            if let Some(path) = image {
                entity.set(&SpriteSource {
                    size: Vec2::splat(32.0),
                    image: Some(path.to_string()),
                    ..Default::default()
                })?;
            }
            Ok(entity)
        })();
        match result {
            Ok(entity) => {
                let id = entity.id;
                if self.apply(EditorCommand::Create(entity), cx) {
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
        let value = visibility.encode_persistent_component();
        match value {
            Ok(value) => self.apply(
                EditorCommand::SetComponent {
                    entities: self.session.selection().entities().to_vec(),
                    component: id,
                    value: nara_scene::authoring::SceneComponent {
                        version: Visibility::persistent_component_schema().version(),
                        value,
                    },
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
        self.selected_asset = Some(path.clone());
        if matches!(AssetKind::of(path), AssetKind::Scene | AssetKind::Prefab) {
            let document = self.session.document();
            if self.session.is_dirty()
                && (document.path().is_some() || !document.scene().entities.is_empty())
            {
                self.warn("Save the current document before opening another one", cx);
                return;
            }
            let result = self
                .project
                .open_document(path)
                .map_err(anyhow::Error::from)
                .and_then(|document| {
                    self.session
                        .open_document(document)
                        .map_err(anyhow::Error::from)
                });
            if let Err(error) = result {
                self.warn(error.to_string(), cx);
                return;
            }
            self.revision = self.revision.wrapping_add(1);
        }
        cx.notify();
    }

    pub(crate) fn preview_revision(&self) -> (u64, u64) {
        (self.revision, self.asset_scan_revision)
    }

    pub(crate) fn refresh_assets(&mut self, cx: &mut Context<Self>) {
        match self.project.assets() {
            Ok(assets) => {
                self.assets = assets;
                self.asset_scan_revision = self.asset_scan_revision.wrapping_add(1);
                cx.notify();
            }
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
            let result = before_write(self, cx).and_then(|()| Ok(self.session.save()?));
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
        let picker = cx.prompt_for_new_path(&root, Some("untitled.nara.scene.ron"));
        cx.spawn(async move |workspace, cx| {
            let result = picker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let saved = match result {
                    Ok(Ok(Some(path))) => {
                        let result = (|| -> Result<()> {
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
                                name.ends_with(".nara.scene.ron"),
                                "Include .nara.scene.ron in the filename and save again"
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
                            Ok(())
                        })();
                        let saved = workspace.saved(result, cx);
                        if saved {
                            workspace.refresh_assets(cx);
                        }
                        saved
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
        self.steps = 0;
        self.start_job(
            JobStatus::Building,
            |project, trust| Job::play(project.to_path_buf(), scene, trust),
            cx,
        );
    }

    fn start_job(
        &mut self,
        status: JobStatus,
        start: impl FnOnce(&Path, ProjectTrust) -> std::io::Result<Job>,
        cx: &mut Context<Self>,
    ) {
        if self.job.is_some() {
            return;
        }
        if self.trust != ProjectTrust::Trusted {
            self.warn(
                "Trust this project before building or running its Rust code",
                cx,
            );
            return;
        }
        match start(self.project.directory(), self.trust) {
            Ok(job) => {
                self.job = Some(job);
                self.status = status;
                self.pending = None;
                cx.notify();
                cx.spawn(async move |workspace, cx| {
                    let timer = cx.background_executor().clone();
                    while workspace
                        .update(cx, |workspace, cx| workspace.poll_job(cx))
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
            match job.control(control) {
                Ok(()) => {
                    self.pending = Some(PendingPlayControl::Queued(control));
                    cx.notify();
                }
                Err(error) => self.warn(format!("Could not send Play control: {error}"), cx),
            }
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = &self.job {
            job.stop();
            self.status = JobStatus::Stopping;
            self.pending = None;
            cx.notify();
        }
    }

    fn poll_job(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        for _ in 0..256 {
            let Some(job) = &self.job else { break };
            let message = match job.messages.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.finish_job(Err("The project worker exited unexpectedly".into()));
                    changed = true;
                    break;
                }
            };
            changed = true;
            self.receive_job_message(message);
        }
        if changed {
            cx.notify();
        }
        self.job.is_some()
    }

    fn receive_job_message(&mut self, message: Message) {
        match message {
            Message::Log(diagnostic) => self.console.push(diagnostic),
            Message::Schema(catalog) => {
                let result = (|| -> Result<()> {
                    let registry = EditorRegistry::new(builtin_component_schemas()?, catalog)?;
                    self.session.set_registry(registry)?;
                    Ok(())
                })();
                match result {
                    Ok(()) => self
                        .console
                        .line(DiagnosticSeverity::Info, "Schema refreshed"),
                    Err(error) => self
                        .console
                        .line(DiagnosticSeverity::Warning, error.to_string()),
                }
            }
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

    fn finish_job(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            self.console.line(DiagnosticSeverity::Error, error);
        } else if self.status == JobStatus::Stopping {
            self.console
                .line(DiagnosticSeverity::Info, "Stopped the project process");
        }
        self.job = None;
        self.status = JobStatus::Idle;
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_controls_wait_for_matching_delivery_and_acknowledgment() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join("assets")).unwrap();
        std::fs::write(
            project.path().join("nara.toml"),
            "[project]\nname = 'Test game'\nversion = '0.1.0'\n",
        )
        .unwrap();
        let mut workspace = Workspace::open(project.path(), ProjectTrust::Untrusted).unwrap();
        let state = |state, pending, steps| Message::Play {
            state,
            pending,
            steps,
            mode: EditorPlayMode::Snapshot,
        };
        let delivery = |control, result| Message::PlayControlResult { control, result };

        workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Pause));
        workspace.receive_job_message(state(PlayState::Running, None, 0));
        workspace.receive_job_message(delivery(PlayControl::Resume, Ok(())));
        assert!(workspace.control_pending());
        workspace.receive_job_message(delivery(PlayControl::Pause, Ok(())));
        workspace.receive_job_message(state(PlayState::Running, Some(PlayControl::Pause), 0));
        assert!(workspace.control_pending());
        workspace.receive_job_message(state(PlayState::Paused, None, 0));
        assert!(!workspace.control_pending());
        assert!(workspace.status == JobStatus::Paused);

        workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Step));
        workspace.receive_job_message(delivery(
            PlayControl::Step,
            Err("Control delivery failed".into()),
        ));
        assert!(!workspace.control_pending());
        assert!(workspace.console.entries().iter().any(|entry| {
            entry.severity == DiagnosticSeverity::Warning
                && entry.message == "Control delivery failed"
        }));

        workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Step));
        workspace.receive_job_message(delivery(PlayControl::Step, Ok(())));
        workspace.receive_job_message(state(PlayState::Paused, None, 1));
        assert!(!workspace.control_pending());
        assert_eq!(workspace.steps, 1);
        workspace.pending = Some(PendingPlayControl::Queued(PlayControl::Resume));
        workspace.receive_job_message(Message::Finished(Ok(())));
        assert!(!workspace.control_pending());
        assert!(workspace.status == JobStatus::Idle);
    }
}
