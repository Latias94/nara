use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_app::{App, CoreStage, RuntimeInstance};
use nara_asset::{AssetPathScope, AssetRecord, Handle, ProjectAssetDatabase};
use nara_core::Vec2;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Entity, Resource, With, World};
use nara_reflect::{ComponentRegistry, ComponentRegistrySnapshot};
use nara_render::{
    __private::{
        EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
    },
    Camera2d, Color, Extent2d, ExtractedViews, RenderFrame, RenderFramePacket,
    RenderFrameSourceGeneration, RenderImage2d, RenderTarget, ViewportRect,
};
use nara_scene::{
    __private::AuthoringPreviewOmission, ExpandedSceneCandidate, SceneAuthoringRevision,
    SceneDocument, SceneEntityId, SceneEntitySource, SceneProjection, SourceDocumentKey,
};
use nara_sprite_render::__private::{QueuedSpriteItems, SpriteBatches, SpriteInstance};
use nara_tooling::{EditorDocumentId, WorldIdentitySnapshot};
use nara_transform::Transform2d;

use crate::project_host::ProjectContentSnapshot;

use super::super::single_error;
use super::authoring_preview::EditorAuthoringPreviewOmissions;
use super::authoring_runtime::EditorImageRuntimeFailure;
use super::image_import::{MAX_LAST_GOOD_IMAGES, MAX_RETAINED_PATH_BYTES};

#[derive(Debug, Clone, PartialEq)]
pub struct EditorPreviewHit {
    entity: SceneEntityId,
    draw_ordinal: u64,
    instance: SpriteInstance,
}

impl EditorPreviewHit {
    /// Returns the selectable entity in the containing Scene.
    ///
    /// Prefab-derived geometry selects its instance anchor; materialized locators are not
    /// Workspace selection targets.
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

/// Opaque content identity for reusable Scene preview pixels, independent from frame geometry.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPreviewContentStamp {
    source: RenderFrameSourceGeneration,
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    dependencies: EditorPreviewDependencies,
    extent: Extent2d,
    image_publication_revision: u64,
}

impl EditorPreviewContentStamp {
    pub(super) const fn image_publication_revision(&self) -> u64 {
        self.image_publication_revision
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
    content_stamp: EditorPreviewContentStamp,
    packet: RenderFramePacket,
    sprite_batches: SpriteBatches,
    hits: Vec<EditorPreviewHit>,
    omissions: EditorAuthoringPreviewOmissions,
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

    /// Compares retained pixels without conflating their content with per-drive geometry identity.
    #[must_use]
    pub fn content_stamp(&self) -> EditorPreviewContentStamp {
        self.content_stamp.clone()
    }

    #[must_use]
    pub fn hits(&self) -> &[EditorPreviewHit] {
        &self.hits
    }

    /// Returns exact source facts omitted from this fail-closed authoring projection.
    #[doc(hidden)]
    #[must_use]
    pub fn preview_omissions(&self) -> &[AuthoringPreviewOmission] {
        self.omissions.facts()
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

enum EditorPreviewRequest {
    Publish(EditorPreviewPublicationRequest),
    Clear,
}

struct EditorPreviewPublicationRequest {
    generation: EditorRenderGeneration,
    stamp: EditorPreviewDocumentStamp,
    projection: Option<EditorPreviewProjection>,
    extent: Extent2d,
    target: Handle<RenderImage2d>,
}

pub(super) enum EditorPreviewProjection {
    Authoring {
        candidate: ExpandedSceneCandidate,
        omissions: EditorAuthoringPreviewOmissions,
        database: Arc<ProjectAssetDatabase>,
        asset_scope: Arc<AssetPathScope>,
    },
    TrustedCompiled {
        source_key: SourceDocumentKey,
        scene: SceneDocument,
        omissions: EditorAuthoringPreviewOmissions,
    },
    Rejected(DiagnosticReport),
}

impl EditorPreviewProjection {
    pub(super) fn authoring(
        candidate: ExpandedSceneCandidate,
        omissions: EditorAuthoringPreviewOmissions,
        database: Arc<ProjectAssetDatabase>,
        asset_scope: Arc<AssetPathScope>,
    ) -> Self {
        Self::Authoring {
            candidate,
            omissions,
            database,
            asset_scope,
        }
    }

    pub(super) fn trusted_compiled(
        source_key: SourceDocumentKey,
        scene: SceneDocument,
        omissions: EditorAuthoringPreviewOmissions,
    ) -> Self {
        Self::TrustedCompiled {
            source_key,
            scene,
            omissions,
        }
    }

    /// Retains a capture failure before any native materialization candidate exists.
    pub(super) fn rejected(diagnostics: DiagnosticReport) -> Self {
        Self::Rejected(diagnostics)
    }

    fn omissions(&self) -> Option<&EditorAuthoringPreviewOmissions> {
        match self {
            Self::Authoring { omissions, .. } | Self::TrustedCompiled { omissions, .. } => {
                Some(omissions)
            }
            Self::Rejected(_) => None,
        }
    }
}

#[derive(Clone, Resource)]
struct EditorPreviewInput(Arc<Mutex<Option<EditorPreviewRequest>>>);

#[derive(Resource)]
struct EditorPreviewRegistry(ComponentRegistry);

#[derive(Resource)]
enum EditorPreviewProject {
    Authoring(AuthoringPreviewAssets),
    TrustedCompiled(ProjectContentSnapshot),
}

struct AuthoringPreviewAssets {
    database: Arc<ProjectAssetDatabase>,
    admitted_count: usize,
    additional_path_bytes: usize,
}

#[cfg(test)]
pub(super) fn test_authoring_asset_database(world: &World) -> Arc<ProjectAssetDatabase> {
    let EditorPreviewProject::Authoring(assets) = world.resource::<EditorPreviewProject>() else {
        panic!("the authoring fixture requires its admitted asset database");
    };
    Arc::clone(&assets.database)
}

pub(super) fn validate_authoring_image_identity(
    world: &World,
    record: &AssetRecord,
) -> Result<(), EditorImageRuntimeFailure> {
    let Some(EditorPreviewProject::Authoring(assets)) =
        world.get_resource::<EditorPreviewProject>()
    else {
        return Ok(());
    };
    if assets.database.record_for_path(record.path()) == Some(record) {
        return Ok(());
    }
    if assets.database.len().saturating_sub(assets.admitted_count) >= MAX_LAST_GOOD_IMAGES
        || assets
            .additional_path_bytes
            .checked_add(record.path().as_str().len())
            .is_none_or(|bytes| bytes > MAX_RETAINED_PATH_BYTES)
    {
        return Err(EditorImageRuntimeFailure::IdentityCapacity);
    }
    assets
        .database
        .validate_insert(record)
        .map_err(|_| EditorImageRuntimeFailure::AssetIdentity)
}

pub(super) fn publish_authoring_image_identity(world: &mut World, record: AssetRecord) {
    let Some(mut project) = world.get_resource_mut::<EditorPreviewProject>() else {
        return;
    };
    let EditorPreviewProject::Authoring(assets) = &mut *project else {
        return;
    };
    if assets.database.record_for_path(record.path()) == Some(&record) {
        return;
    }
    let path_bytes = record.path().as_str().len();
    Arc::make_mut(&mut assets.database)
        .insert(record)
        .expect("the same exclusive image publication preflighted this identity");
    assets.additional_path_bytes += path_bytes;
}

/// One open source incarnation that participated in the captured preview closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorPreviewParticipant {
    pub source_key: SourceDocumentKey,
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
}

/// Exact capture identity, independent from rendering and projection epochs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorPreviewDependencies {
    generation: u64,
    participants: Arc<[EditorPreviewParticipant]>,
}

impl EditorPreviewDependencies {
    /// Retains exact participants in canonical source/document order, without deduplicating facts.
    pub(super) fn captured(
        generation: u64,
        mut participants: Arc<[EditorPreviewParticipant]>,
    ) -> Self {
        let compare = |left: &EditorPreviewParticipant, right: &EditorPreviewParticipant| {
            (&left.source_key, left.document).cmp(&(&right.source_key, right.document))
        };
        if !participants.is_sorted_by(|left, right| compare(left, right).is_le()) {
            Arc::make_mut(&mut participants).sort_by(compare);
        }
        Self {
            generation,
            participants,
        }
    }

    pub(super) fn empty() -> Self {
        Self::captured(0, Arc::from([]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorPreviewDocumentStamp {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    dependencies: EditorPreviewDependencies,
}

#[derive(Resource)]
struct EditorPreviewRuntimeState {
    materializer: SceneProjection,
    requested: Option<EditorPreviewDocumentStamp>,
    requested_projection: Option<EditorPreviewProjection>,
    accepted: Option<AcceptedEditorPreview>,
    camera: Option<Entity>,
    diagnostics: DiagnosticReport,
}

impl EditorPreviewRuntimeState {
    fn new() -> Self {
        Self {
            materializer: SceneProjection::new(
                nara_scene::SceneOccurrenceKey::new("nara.editor", "scene-preview")
                    .expect("the Editor declares a valid preview slot"),
            ),
            requested: None,
            requested_projection: None,
            accepted: None,
            camera: None,
            diagnostics: DiagnosticReport::default(),
        }
    }
}

struct AcceptedEditorPreview {
    stamp: EditorPreviewDocumentStamp,
    generation: EditorRenderGeneration,
    omissions: EditorAuthoringPreviewOmissions,
}

pub(super) struct EditorPreviewSource {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    pub dependencies: EditorPreviewDependencies,
    pub projection: Option<EditorPreviewProjection>,
}

pub(super) struct EditorPreviewSession {
    input: Arc<Mutex<Option<EditorPreviewRequest>>>,
    generation: EditorRenderGeneration,
    target: Handle<RenderImage2d>,
    extent: Option<Extent2d>,
    geometry_revision: u64,
    candidate: Option<EditorPreviewCandidate>,
    valid_content_stamp: Option<EditorPreviewContentStamp>,
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
            .field("content_valid", &self.valid_content_stamp.is_some())
            .finish_non_exhaustive()
    }
}

impl EditorPreviewSession {
    pub(super) fn install_trusted(
        app: &mut App,
        project: ProjectContentSnapshot,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
        Self::install(
            app,
            EditorPreviewProject::TrustedCompiled(project),
            registry,
        )
    }

    pub(super) fn install_authoring(
        app: &mut App,
        registry: &ComponentRegistrySnapshot,
        database: Arc<ProjectAssetDatabase>,
    ) -> Result<Self, DiagnosticReport> {
        let admitted_count = database.len();
        Self::install(
            app,
            EditorPreviewProject::Authoring(AuthoringPreviewAssets {
                database,
                admitted_count,
                additional_path_bytes: 0,
            }),
            registry,
        )
    }

    fn install(
        app: &mut App,
        project: EditorPreviewProject,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, DiagnosticReport> {
        let generation = EditorRenderGeneration::try_acquire()
            .ok_or_else(|| preview_error("project.editor.preview-generation-exhausted"))?;
        let input = Arc::new(Mutex::new(None));
        app.insert_resource(EditorPreviewInput(Arc::clone(&input)))
            .and_then(|app| {
                app.insert_resource(EditorPreviewRegistry(ComponentRegistry::from_snapshot(
                    registry.clone(),
                )))
            })
            .and_then(|app| app.insert_resource(project))
            .and_then(|app| app.insert_resource(EditorPreviewRuntimeState::new()))
            .and_then(|app| app.add_systems(CoreStage::First, apply_editor_preview_request))
            .map_err(|_| preview_error("project.editor.preview-state-install-failed"))?;
        Ok(Self {
            input,
            generation,
            target: RenderImage2d::allocate_handle()
                .map_err(|_| preview_error("project.editor.preview-target-id-exhausted"))?,
            extent: None,
            geometry_revision: 0,
            candidate: None,
            valid_content_stamp: None,
            world_snapshot: None,
            last_failure: None,
        })
    }

    pub(super) fn set_extent(&mut self, extent: Option<Extent2d>) {
        if self.extent != extent {
            self.invalidate_content();
        }
        self.extent = extent;
        if extent.is_none() {
            self.candidate = None;
            self.world_snapshot = None;
        }
    }

    pub(super) const fn extent(&self) -> Option<Extent2d> {
        self.extent
    }

    pub(super) fn current_content_stamp(&self) -> Option<EditorPreviewContentStamp> {
        self.valid_content_stamp.clone()
    }

    /// Revokes queued pixels without clearing the last-good world or its source request.
    pub(super) fn invalidate_content(&mut self) {
        self.candidate = None;
        self.valid_content_stamp = None;
    }

    pub(super) fn requires_source_projection(
        &self,
        world: &World,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        dependencies: &EditorPreviewDependencies,
    ) -> bool {
        if self.extent.is_none() {
            return false;
        }
        world
            .resource::<EditorPreviewRuntimeState>()
            .requested
            .as_ref()
            .is_none_or(|requested| {
                requested.document != document
                    || requested.revision != revision
                    || &requested.dependencies != dependencies
            })
    }

    pub(super) fn drive(
        &mut self,
        runtime: &mut RuntimeInstance,
        source: Option<EditorPreviewSource>,
        image_publication_revision: u64,
    ) -> Option<DiagnosticReport> {
        match self.try_drive(runtime, source, image_publication_revision) {
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

    fn try_drive(
        &mut self,
        runtime: &mut RuntimeInstance,
        source: Option<EditorPreviewSource>,
        image_publication_revision: u64,
    ) -> Result<(), DiagnosticReport> {
        self.invalidate_content();
        let Some(source) = source else {
            self.world_snapshot = None;
            if runtime
                .world()
                .resource::<EditorPreviewRuntimeState>()
                .requested
                .is_some()
            {
                *self
                    .input
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                    Some(EditorPreviewRequest::Clear);
                runtime
                    .drive(Duration::ZERO)
                    .map_err(|_| preview_error("project.editor.preview-frame-failed"))?;
                let state = runtime.world().resource::<EditorPreviewRuntimeState>();
                if state.requested.is_some() {
                    return Err(state.diagnostics.clone());
                }
            }
            return Ok(());
        };
        let Some(extent) = self.extent else {
            return Ok(());
        };
        let stamp = EditorPreviewDocumentStamp {
            document: source.document,
            revision: source.revision,
            dependencies: source.dependencies,
        };
        let request = EditorPreviewRequest::Publish(EditorPreviewPublicationRequest {
            generation: self.generation,
            stamp: stamp.clone(),
            projection: source.projection,
            extent,
            target: self.target,
        });
        *self
            .input
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request);
        let outcome = runtime
            .drive(Duration::ZERO)
            .map_err(|_| preview_error("project.editor.preview-frame-failed"))?;
        let runtime_state = runtime.world().resource::<EditorPreviewRuntimeState>();
        let Some(accepted) = runtime_state
            .accepted
            .as_ref()
            .filter(|accepted| accepted.stamp == stamp)
        else {
            return Err(runtime_state.diagnostics.clone());
        };
        if outcome.frame().is_none() {
            return Err(preview_error("project.editor.preview-frame-unavailable"));
        }
        let views = runtime.world().resource::<ExtractedViews>();
        let target_descriptor = editor_viewport_image_target(self.target, extent)
            .map_err(|_| preview_error("project.editor.preview-target-rejected"))?;
        let packet = build_editor_render_frame_packet(
            accepted.generation,
            runtime.world().resource::<RenderFrame>().index,
            views,
            target_descriptor,
        )
        .map_err(|_| preview_error("project.editor.preview-packet-rejected"))?
        .ok_or_else(|| preview_error("project.editor.preview-view-missing"))?;
        let [view] = packet.views() else {
            return Err(preview_error("project.editor.preview-view-count-mismatch"));
        };
        let target = view.target;
        let queued = runtime.world().resource::<QueuedSpriteItems>();
        let mut hits = Vec::with_capacity(queued.len());
        for (ordinal, item) in queued.as_slice().iter().enumerate() {
            if item.view_index != 0 || item.target != target {
                return Err(preview_error(
                    "project.editor.preview-queue-target-mismatch",
                ));
            }
            let source = runtime
                .world()
                .get::<SceneEntitySource>(item.entity)
                .ok_or_else(|| preview_error("project.editor.preview-source-identity-missing"))?;
            let provenance = source.provenance();
            let entity = provenance
                .projection_path()
                .map_or(provenance.local_entity_id(), |path| path.root_anchor());
            hits.push(EditorPreviewHit {
                entity: entity.clone(),
                draw_ordinal: u64::try_from(ordinal)
                    .map_err(|_| preview_error("project.editor.preview-hit-count-exhausted"))?,
                instance: item.instance,
            });
        }
        let geometry_revision = self
            .geometry_revision
            .checked_add(1)
            .ok_or_else(|| preview_error("project.editor.preview-geometry-exhausted"))?;
        let world_snapshot = WorldIdentitySnapshot::capture_default(runtime.world())
            .map_err(|_| preview_error("project.editor.preview-identity-snapshot-failed"))?;
        let content_stamp = EditorPreviewContentStamp {
            source: packet.source_generation(),
            document: source.document,
            revision: source.revision,
            dependencies: stamp.dependencies,
            extent,
            image_publication_revision,
        };
        self.candidate = Some(EditorPreviewCandidate {
            document: source.document,
            revision: source.revision,
            geometry_revision,
            content_stamp: content_stamp.clone(),
            packet,
            sprite_batches: runtime.world().resource::<SpriteBatches>().clone(),
            hits,
            omissions: accepted.omissions.clone(),
        });
        self.geometry_revision = geometry_revision;
        self.valid_content_stamp = Some(content_stamp);
        self.world_snapshot = Some(world_snapshot);
        Ok(())
    }

    pub(super) fn take_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.candidate.take()
    }

    pub(super) fn retire_candidate(&mut self) {
        self.invalidate_content();
        self.world_snapshot = None;
    }

    pub(super) fn world_snapshot(&self) -> Option<&WorldIdentitySnapshot> {
        self.world_snapshot.as_ref()
    }

    #[cfg(test)]
    pub(super) fn test_accepted_authoring_candidate<'a>(
        &self,
        world: &'a World,
    ) -> Option<&'a ExpandedSceneCandidate> {
        let state = world.get_resource::<EditorPreviewRuntimeState>()?;
        let requested = state.requested.as_ref()?;
        if &state.accepted.as_ref()?.stamp != requested {
            return None;
        }
        match state.requested_projection.as_ref()? {
            EditorPreviewProjection::Authoring { candidate, .. } => Some(candidate),
            EditorPreviewProjection::TrustedCompiled { .. }
            | EditorPreviewProjection::Rejected(_) => None,
        }
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

    let EditorPreviewRequest::Publish(request) = request else {
        match state.materializer.clear(world) {
            Ok(_) => {
                state.requested = None;
                state.requested_projection = None;
                state.accepted = None;
                state.diagnostics = DiagnosticReport::default();
            }
            Err(diagnostics) => state.diagnostics = diagnostics,
        }
        world.insert_resource(state);
        world.insert_resource(registry);
        world.insert_resource(project);
        return;
    };

    let stamp = request.stamp;
    if state.requested.as_ref() != Some(&stamp) {
        let projection = request
            .projection
            .expect("a changed Editor preview source carries its projection or capture rejection");
        if let Some(omissions) = projection.omissions() {
            debug_assert_eq!(omissions.document(), stamp.document);
            debug_assert_eq!(omissions.revision(), stamp.revision);
        }
        state.requested = Some(stamp.clone());
        state.requested_projection = Some(projection);
    }
    if state.accepted.as_ref().map(|accepted| &accepted.stamp) != Some(&stamp) {
        let projection = state
            .requested_projection
            .as_ref()
            .expect("a requested preview retains its exact projection");
        let report = match (&project, projection) {
            (_, EditorPreviewProjection::Rejected(diagnostics)) => {
                let mut report = nara_scene::SceneSpawnReport::default();
                report.diagnostics = diagnostics.clone();
                report
            }
            (
                EditorPreviewProject::Authoring(_),
                EditorPreviewProjection::Authoring {
                    candidate,
                    database,
                    asset_scope,
                    ..
                },
            ) => state.materializer.publish(
                world,
                &registry.0,
                candidate,
                database.resolver(asset_scope),
            ),
            (
                EditorPreviewProject::TrustedCompiled(project),
                EditorPreviewProjection::TrustedCompiled {
                    source_key, scene, ..
                },
            ) => project.sync_editor_preview_scene(
                &mut state.materializer,
                source_key,
                scene,
                world,
                &registry.0,
            ),
            _ => {
                let mut report = nara_scene::SceneSpawnReport::default();
                report.diagnostics =
                    preview_error("project.editor.preview-projection-lane-mismatch");
                report
            }
        };
        state.diagnostics = report.diagnostics;
        if let (Some(_), Some(omissions)) = (&report.instance, projection.omissions()) {
            remove_authored_preview_cameras(world);
            state.accepted = Some(AcceptedEditorPreview {
                stamp,
                generation: request.generation,
                omissions: omissions.clone(),
            });
        }
    }
    if !matches!(
        state.requested_projection.as_ref(),
        Some(EditorPreviewProjection::Rejected(_))
    ) {
        ensure_preview_camera(world, &mut state, request.target, request.extent);
    }

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

#[cfg(test)]
mod identity_tests {
    use nara_asset::{AssetSourceKind, AssetSourcePath, StableAssetId};

    use super::*;

    #[test]
    fn accepted_image_identities_copy_once_and_remain_bounded_with_idempotent_reimport() {
        let admitted = Arc::new(ProjectAssetDatabase::default());
        let mut world = World::new();
        world.insert_resource(EditorPreviewProject::Authoring(AuthoringPreviewAssets {
            database: Arc::clone(&admitted),
            admitted_count: 0,
            additional_path_bytes: 0,
        }));
        for index in 0..MAX_LAST_GOOD_IMAGES {
            let record = image_record(index, format!("images/{index}.png"));
            assert_eq!(validate_authoring_image_identity(&world, &record), Ok(()));
            publish_authoring_image_identity(&mut world, record);
        }
        assert!(
            admitted.is_empty(),
            "runtime import must not mutate the admitted snapshot"
        );
        assert_eq!(
            validate_authoring_image_identity(
                &world,
                &image_record(MAX_LAST_GOOD_IMAGES, "images/new.png".to_owned())
            ),
            Err(EditorImageRuntimeFailure::IdentityCapacity)
        );
        let reimport = image_record(0, "images/0.png".to_owned());
        assert_eq!(validate_authoring_image_identity(&world, &reimport), Ok(()));
        publish_authoring_image_identity(&mut world, reimport);
        let EditorPreviewProject::Authoring(assets) = world.resource::<EditorPreviewProject>()
        else {
            unreachable!()
        };
        assert_eq!(assets.database.len(), MAX_LAST_GOOD_IMAGES);
    }

    #[test]
    fn accepted_image_identity_paths_have_an_aggregate_byte_ceiling() {
        let mut world = World::new();
        world.insert_resource(EditorPreviewProject::Authoring(AuthoringPreviewAssets {
            database: Arc::new(ProjectAssetDatabase::default()),
            admitted_count: 0,
            additional_path_bytes: 0,
        }));
        let part = format!("{}/", "p".repeat(200));
        let long_path = format!("{}image.png", part.repeat(32));
        let mut capacity_rejected = false;
        for index in 0..MAX_LAST_GOOD_IMAGES {
            let record = image_record(index, format!("{index}/{long_path}"));
            match validate_authoring_image_identity(&world, &record) {
                Ok(()) => publish_authoring_image_identity(&mut world, record),
                Err(EditorImageRuntimeFailure::IdentityCapacity) => {
                    capacity_rejected = true;
                    break;
                }
                other => panic!("unexpected identity admission: {other:?}"),
            }
        }
        assert!(capacity_rejected);
        let EditorPreviewProject::Authoring(assets) = world.resource::<EditorPreviewProject>()
        else {
            unreachable!()
        };
        assert!(assets.database.len() < MAX_LAST_GOOD_IMAGES);
        assert!(assets.additional_path_bytes <= MAX_RETAINED_PATH_BYTES);
    }

    fn image_record(index: usize, path: String) -> AssetRecord {
        AssetRecord::new(
            StableAssetId::parse_str(format!("00000000-0000-0000-0000-{index:012x}")).unwrap(),
            AssetSourcePath::new(path).unwrap(),
            AssetSourceKind::Image,
        )
    }
}
