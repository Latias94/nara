use std::{path::Path, time::Duration};

use anyhow::{Context as _, Result};
use nara_core::{DiagnosticSeverity, DocumentId, EntityUuid, Vec2, vfs::AssetPath};
use nara_editor_core::{
    AssetBrowserModel, AssetKind, ConsoleModel, EditorCommand, EditorDocument, EditorProject,
    EditorRegistry, EditorSession,
    play::{PlayControl, PlayState},
};
use nara_project::workflow::{ProjectTrust, read_schema_cache};
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
            Self::Paused => "Paused",
            Self::Stopping => "Stopping…",
        }
    }
}

pub(crate) struct Workspace {
    pub(crate) session: EditorSession,
    pub(crate) revision: u64,
    pub project: EditorProject,
    pub assets: AssetBrowserModel,
    pub selected_asset: Option<AssetPath>,
    pub console: ConsoleModel,
    pub(crate) pending: bool,
    pub(crate) steps: u64,
    pub(crate) status: JobStatus,
    pub(crate) viewport_stats: String,
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
            project,
            assets,
            selected_asset,
            console,
            status: JobStatus::Idle,
            pending: false,
            steps: 0,
            viewport_stats: String::new(),
            trust,
            job: None,
        })
    }

    pub(crate) fn apply(&mut self, command: EditorCommand, cx: &mut Context<Self>) -> bool {
        self.edit(|session| session.apply(command), cx)
    }

    pub(crate) fn begin_gesture(&mut self, cx: &mut Context<Self>) -> bool {
        self.edit(|session| session.begin_gesture().map(|()| true), cx)
    }

    pub(crate) fn end_gesture(&mut self, cx: &mut Context<Self>) -> bool {
        self.edit(|session| Ok(session.end_gesture()), cx)
    }

    pub(crate) fn cancel_gesture(&mut self, cx: &mut Context<Self>) -> bool {
        self.edit(|session| Ok(session.cancel_gesture()), cx)
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

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.session.document().path().is_some() {
            let result = self.session.save().map_err(anyhow::Error::from);
            self.saved(result, cx);
            return;
        }
        let Ok(source) = self.project.asset_source() else {
            self.warn("The project asset directory is unavailable", cx);
            return;
        };
        let root = source.root().to_path_buf();
        let picker = cx.prompt_for_new_path(&root, Some("untitled.nara.scene.ron"));
        cx.spawn(async move |workspace, cx| {
            let Ok(result) = picker.await else { return };
            let _ = workspace.update(cx, |workspace, cx| match result {
                Ok(Some(path)) => {
                    let result = (|| -> Result<()> {
                        let parent = path
                            .parent()
                            .context("The selected path has no parent")?
                            .canonicalize()?;
                        anyhow::ensure!(
                            parent.starts_with(&root),
                            "Save the scene inside the project's asset directory"
                        );
                        let mut name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .context("Use a portable scene filename")?
                            .to_owned();
                        if !name.ends_with(".nara.scene.ron") {
                            name.push_str(".nara.scene.ron");
                        }
                        workspace.session.save_as(parent.join(name))?;
                        workspace.assets = workspace.project.assets()?;
                        Ok(())
                    })();
                    workspace.saved(result, cx);
                }
                Ok(None) => {}
                Err(error) => workspace.warn(error.to_string(), cx),
            });
        })
        .detach();
    }

    fn saved(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.console.line(DiagnosticSeverity::Info, "Scene saved"),
            Err(error) => self.warn(error.to_string(), cx),
        }
        cx.notify();
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
                self.pending = false;
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

    pub fn control(&mut self, control: PlayControl, cx: &mut Context<Self>) {
        if !self.pending && self.job.as_ref().is_some_and(|job| job.control(control)) {
            self.pending = true;
            cx.notify();
        }
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = &self.job {
            job.stop();
            self.status = JobStatus::Stopping;
            cx.notify();
        }
    }

    fn poll_job(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let incoming: Vec<_> = job.messages.try_iter().take(256).collect();
        let changed = !incoming.is_empty();
        for message in incoming {
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
                    pending,
                    steps,
                } => {
                    if self.status != JobStatus::Stopping {
                        self.status = match state {
                            PlayState::Starting => JobStatus::Starting,
                            PlayState::Running => JobStatus::Running,
                            PlayState::Paused => JobStatus::Paused,
                            PlayState::Exited(_) => self.status,
                        };
                    }
                    self.steps = steps;
                    self.pending = pending.is_some();
                }
                Message::Finished(result) => {
                    if let Err(error) = result {
                        self.console.line(DiagnosticSeverity::Error, error);
                    } else if self.status == JobStatus::Stopping {
                        self.console
                            .line(DiagnosticSeverity::Info, "Stopped the project process");
                    }
                    self.job = None;
                    self.status = JobStatus::Idle;
                    self.pending = false;
                }
            }
        }
        if changed {
            cx.notify();
        }
        self.job.is_some()
    }
}
