use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_app::{App, CoreStage};
use nara_asset::{AssetId, Handle};
use nara_core::Vec2;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Entity, Resource, With, World};
use nara_reflect::{ComponentRegistry, ComponentRegistrySnapshot};
use nara_render::{
    __private::{EditorRenderGeneration, build_editor_render_frame_packet},
    Camera2d, Color, Extent2d, ExtractedViews, RenderFrame, RenderFramePacket, RenderImage2d,
    RenderTarget, ViewportRect,
};
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntitySource,
};
use nara_sprite_render::{QueuedSpriteItems, SpriteBatches, SpriteInstance};
use nara_tooling::{EditorDocumentId, WorldIdentitySnapshot};
use nara_transform::Transform2d;

use crate::project_host::ProjectContentSnapshot;

use super::super::{publish_snapshot_images, single_error};

#[derive(Debug, Clone, PartialEq)]
pub struct EditorPreviewHit {
    entity: SceneEntityId,
    draw_ordinal: u64,
    instance: SpriteInstance,
}

impl EditorPreviewHit {
    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn draw_ordinal(&self) -> u64 {
        self.draw_ordinal
    }

    #[must_use]
    pub const fn instance(&self) -> SpriteInstance {
        self.instance
    }

    /// Tests one clip-space point against the exact projected quad used by this hit fact.
    #[must_use]
    pub fn contains_clip_point(&self, point: [f32; 2]) -> bool {
        let delta_x = point[0] - self.instance.center.x;
        let delta_y = point[1] - self.instance.center.y;
        let delta = Vec2::new(delta_x, delta_y);
        let determinant = self.instance.x_axis.perp_dot(self.instance.y_axis);
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return false;
        }
        let local_x = delta.perp_dot(self.instance.y_axis) / determinant;
        let local_y = self.instance.x_axis.perp_dot(delta) / determinant;
        local_x.is_finite() && local_y.is_finite() && local_x.abs() <= 1.0 && local_y.abs() <= 1.0
    }
}

/// One exact renderer-neutral Scene preview captured from the accepted Editor document.
///
/// The private Editor process may submit this candidate once. It does not become input authority
/// until the platform owner joins its image submission, toolkit sample, and window presentation.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorPreviewCandidate {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    packet: RenderFramePacket,
    sprite_batches: SpriteBatches,
    hits: Vec<EditorPreviewHit>,
}

impl EditorPreviewCandidate {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }

    #[must_use]
    pub fn hits(&self) -> &[EditorPreviewHit] {
        &self.hits
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        EditorDocumentId,
        SceneAuthoringRevision,
        u64,
        RenderFramePacket,
        SpriteBatches,
        Vec<EditorPreviewHit>,
    ) {
        (
            self.document,
            self.revision,
            self.geometry_revision,
            self.packet,
            self.sprite_batches,
            self.hits,
        )
    }
}

struct EditorPreviewRequest {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    scene: SceneDocument,
    extent: Extent2d,
    target: Handle<RenderImage2d>,
}

#[derive(Clone, Resource)]
struct EditorPreviewInput(Arc<Mutex<Option<EditorPreviewRequest>>>);

#[derive(Resource)]
struct EditorPreviewRegistry(ComponentRegistry);

#[derive(Resource)]
struct EditorPreviewProject(ProjectContentSnapshot);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditorPreviewDocumentStamp {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
}

#[derive(Resource)]
struct EditorPreviewRuntimeState {
    materializer: Option<SceneAuthoringSession>,
    requested: Option<EditorPreviewDocumentStamp>,
    accepted: Option<EditorPreviewDocumentStamp>,
    camera: Option<Entity>,
    diagnostics: DiagnosticReport,
}

impl Default for EditorPreviewRuntimeState {
    fn default() -> Self {
        Self {
            materializer: None,
            requested: None,
            accepted: None,
            camera: None,
            diagnostics: DiagnosticReport::default(),
        }
    }
}

pub(super) struct EditorPreviewSource {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    pub scene: SceneDocument,
}

pub(super) struct EditorPreviewSession {
    app: App,
    input: Arc<Mutex<Option<EditorPreviewRequest>>>,
    generation: EditorRenderGeneration,
    target: Handle<RenderImage2d>,
    extent: Option<Extent2d>,
    geometry_revision: u64,
    candidate: Option<EditorPreviewCandidate>,
    world_snapshot: Option<WorldIdentitySnapshot>,
    last_failure: Option<DiagnosticReport>,
}

impl std::fmt::Debug for EditorPreviewSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorPreviewSession")
            .field("extent", &self.extent)
            .field("geometry_revision", &self.geometry_revision)
            .field("candidate_ready", &self.candidate.is_some())
            .finish_non_exhaustive()
    }
}

impl EditorPreviewSession {
    pub(super) fn new(
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_scene::SceneComponentsPlugin,
            nara_diagnostic::DiagnosticsPlugin::default(),
            nara_tasks::TaskPlugin::default(),
            nara_asset::AssetPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderComponentsPlugin,
        ))
        .and_then(|app| {
            app.add_plugins((
                nara_image::ImagePlugin::default(),
                nara_render::RenderPlugin,
                nara_image::ImagePreparePlugin,
                nara_sprite_render::SpriteRenderPlugin,
            ))
        })
        .map_err(|_| preview_error("project.editor.preview-plugin-install-failed"))?;
        let input = Arc::new(Mutex::new(None));
        app.insert_resource(EditorPreviewInput(Arc::clone(&input)))
            .and_then(|app| {
                app.insert_resource(EditorPreviewRegistry(ComponentRegistry::from_snapshot(
                    registry.clone(),
                )))
            })
            .and_then(|app| app.insert_resource(EditorPreviewProject(project.clone())))
            .and_then(|app| app.insert_resource(EditorPreviewRuntimeState::default()))
            .and_then(|app| app.add_systems(CoreStage::First, apply_editor_preview_request))
            .map_err(|_| preview_error("project.editor.preview-state-install-failed"))?;
        publish_snapshot_images(
            app.world_mut()
                .map_err(|_| preview_error("project.editor.preview-world-unavailable"))?,
            &project,
        )
        .map_err(|error| error.diagnostics)?;
        let generation = EditorRenderGeneration::try_acquire()
            .ok_or_else(|| preview_error("project.editor.preview-generation-exhausted"))?;
        Ok(Self {
            app,
            input,
            generation,
            target: Handle::new(AssetId::from_raw(1)),
            extent: None,
            geometry_revision: 0,
            candidate: None,
            world_snapshot: None,
            last_failure: None,
        })
    }

    pub(super) fn set_extent(&mut self, extent: Option<Extent2d>) {
        self.extent = extent;
        if extent.is_none() {
            self.candidate = None;
            self.world_snapshot = None;
        }
    }

    pub(super) fn rebuild_for_project(
        &self,
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
        let mut replacement = Self::new(project, registry)?;
        replacement.set_extent(self.extent);
        Ok(replacement)
    }

    pub(super) fn drive(
        &mut self,
        source: Option<EditorPreviewSource>,
    ) -> Option<DiagnosticReport> {
        match self.try_drive(source) {
            Ok(()) => {
                self.last_failure = None;
                None
            }
            Err(report) if self.last_failure.as_ref() == Some(&report) => None,
            Err(report) => {
                self.last_failure = Some(report.clone());
                Some(report)
            }
        }
    }

    fn try_drive(&mut self, source: Option<EditorPreviewSource>) -> Result<(), DiagnosticReport> {
        self.candidate = None;
        let Some(extent) = self.extent else {
            return Ok(());
        };
        let Some(source) = source else {
            self.world_snapshot = None;
            return Ok(());
        };
        let request = EditorPreviewRequest {
            document: source.document,
            revision: source.revision,
            scene: source.scene,
            extent,
            target: self.target,
        };
        *self
            .input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request);
        self.app
            .run_once(Duration::ZERO)
            .map_err(|_| preview_error("project.editor.preview-frame-failed"))?;

        let stamp = EditorPreviewDocumentStamp {
            document: source.document,
            revision: source.revision,
        };
        let runtime_state = self.app.world().resource::<EditorPreviewRuntimeState>();
        if runtime_state.accepted != Some(stamp) {
            return Err(runtime_state.diagnostics.clone());
        }
        let views = self.app.world().resource::<ExtractedViews>();
        let packet = build_editor_render_frame_packet(
            self.generation,
            self.app.world().resource::<RenderFrame>().index,
            &views,
        )
        .map_err(|_| preview_error("project.editor.preview-packet-rejected"))?
        .ok_or_else(|| preview_error("project.editor.preview-view-missing"))?;
        let target = packet.view().target;
        let queued = self.app.world().resource::<QueuedSpriteItems>();
        let mut hits = Vec::with_capacity(queued.len());
        for (ordinal, item) in queued.as_slice().iter().enumerate() {
            if item.view_index != 0 || item.target != target {
                return Err(preview_error(
                    "project.editor.preview-queue-target-mismatch",
                ));
            }
            let source = self
                .app
                .world()
                .get::<SceneEntitySource>(item.entity)
                .ok_or_else(|| preview_error("project.editor.preview-source-identity-missing"))?;
            hits.push(EditorPreviewHit {
                entity: source.entity_id.clone(),
                draw_ordinal: u64::try_from(ordinal)
                    .map_err(|_| preview_error("project.editor.preview-hit-count-exhausted"))?,
                instance: item.instance,
            });
        }
        let geometry_revision = self
            .geometry_revision
            .checked_add(1)
            .ok_or_else(|| preview_error("project.editor.preview-geometry-exhausted"))?;
        let world_snapshot = WorldIdentitySnapshot::capture_default(self.app.world())
            .map_err(|_| preview_error("project.editor.preview-identity-snapshot-failed"))?;
        self.candidate = Some(EditorPreviewCandidate {
            document: source.document,
            revision: source.revision,
            geometry_revision,
            packet,
            sprite_batches: self.app.world().resource::<SpriteBatches>().clone(),
            hits,
        });
        self.geometry_revision = geometry_revision;
        self.world_snapshot = Some(world_snapshot);
        Ok(())
    }

    pub(super) fn take_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.candidate.take()
    }

    pub(super) fn world_snapshot(&self) -> Option<&WorldIdentitySnapshot> {
        self.world_snapshot.as_ref()
    }
}

fn apply_editor_preview_request(world: &mut World) {
    let input = Arc::clone(&world.resource::<EditorPreviewInput>().0);
    let request = input
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    let Some(request) = request else {
        return;
    };
    let project = world
        .remove_resource::<EditorPreviewProject>()
        .expect("the Editor preview project owner is installed");
    let registry = world
        .remove_resource::<EditorPreviewRegistry>()
        .expect("the Editor preview registry owner is installed");
    let mut state = world
        .remove_resource::<EditorPreviewRuntimeState>()
        .expect("the Editor preview runtime state is installed");

    let stamp = EditorPreviewDocumentStamp {
        document: request.document,
        revision: request.revision,
    };
    if state.requested != Some(stamp) {
        match state.materializer.as_mut() {
            Some(materializer) => materializer
                .replace_document(request.scene)
                .expect("Editor preview materializers always retain native authority"),
            None => state.materializer = Some(SceneAuthoringSession::new(request.scene)),
        }
        state.requested = Some(stamp);
    }
    if state.accepted != Some(stamp) {
        let report = project.0.sync_editor_preview_scene(
            state
                .materializer
                .as_mut()
                .expect("a requested preview owns one materializer"),
            world,
            &registry.0,
        );
        state.diagnostics = report.diagnostics.clone();
        if report.synced {
            remove_authored_preview_cameras(world);
            state.accepted = Some(stamp);
        }
    }
    ensure_preview_camera(world, &mut state, request.target, request.extent);

    world.insert_resource(state);
    world.insert_resource(registry);
    world.insert_resource(project);
}

fn remove_authored_preview_cameras(world: &mut World) {
    let cameras = {
        let mut query = world.query_filtered::<Entity, (With<SceneEntitySource>, With<Camera2d>)>();
        query.iter(world).collect::<Vec<_>>()
    };
    for entity in cameras {
        world.entity_mut(entity).remove::<Camera2d>();
    }
}

fn ensure_preview_camera(
    world: &mut World,
    state: &mut EditorPreviewRuntimeState,
    target: Handle<RenderImage2d>,
    extent: Extent2d,
) {
    let camera = Camera2d {
        target: RenderTarget::Image(target),
        viewport: ViewportRect::from_extent(extent),
        clear_color: Some(Color::rgba(0.075, 0.085, 0.11, 1.0)),
        viewport_height: 720.0,
        order: 0,
    };
    match state
        .camera
        .filter(|entity| world.get_entity(*entity).is_ok())
    {
        Some(entity) => {
            world.entity_mut(entity).insert(camera);
        }
        None => {
            state.camera = Some(world.spawn((camera, Transform2d::default())).id());
        }
    }
}

fn preview_error(code: &'static str) -> DiagnosticReport {
    single_error(code, "Editor Scene preview could not publish a candidate")
}
