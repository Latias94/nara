use nara::{
    project_host::EditorUiAuthoringFrame,
    scene::{SceneAuthoringRevision, SceneEntityId},
    tooling::{
        EditorAction, EditorDocumentId, EditorGestureAction, EditorGestureStamp,
        EditorNavigationAction,
    },
    ui::__private::UiAuthoringDirectEdit,
};

use super::{
    EditorRunError,
    viewport::{
        EditorUiCanvasPickRequest, EditorUiCanvasPresentedObservation, EditorUiCanvasPresenter,
    },
};

/// Product facts required by one bounded UI Prefab authoring native canary.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct EditorUiAuthoringCanaryRequest {
    scene_anchor: SceneEntityId,
    canvas_entity: SceneEntityId,
    move_delta: [f64; 2],
}

impl EditorUiAuthoringCanaryRequest {
    #[must_use]
    pub fn new(
        scene_anchor: SceneEntityId,
        canvas_entity: SceneEntityId,
        move_delta: [f64; 2],
    ) -> Self {
        Self {
            scene_anchor,
            canvas_entity,
            move_delta,
        }
    }
}

/// Exact visible Canvas facts retained at one authoring milestone.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorUiAuthoringCanaryFrame {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    gesture: Option<EditorGestureStamp>,
    editor_frame_index: u64,
    presentation_generation: u64,
    extent: [u32; 2],
    fact_count: usize,
    node_origin: [f32; 2],
}

impl EditorUiAuthoringCanaryFrame {
    fn from_observation(
        observation: &EditorUiCanvasPresentedObservation<'_>,
        entity: &SceneEntityId,
    ) -> Result<Self, EditorRunError> {
        let stamp = observation.stamp();
        let receipt = stamp.receipt();
        let extent = receipt.image().target().extent();
        let node_origin = observation.entity_origin(entity).ok_or_else(|| {
            EditorRunError::Platform(format!(
                "the UI authoring canary could not observe visible Canvas node {}",
                entity.as_str()
            ))
        })?;
        Ok(Self {
            document: stamp.document(),
            revision: stamp.revision(),
            gesture: stamp.gesture(),
            editor_frame_index: receipt.window().editor_frame_index(),
            presentation_generation: receipt.presentation_generation(),
            extent: [extent.width, extent.height],
            fact_count: observation.fact_count(),
            node_origin,
        })
    }

    #[must_use]
    pub const fn document(self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn gesture(self) -> Option<EditorGestureStamp> {
        self.gesture
    }

    #[must_use]
    pub const fn editor_frame_index(self) -> u64 {
        self.editor_frame_index
    }

    #[must_use]
    pub const fn presentation_generation(self) -> u64 {
        self.presentation_generation
    }

    #[must_use]
    pub const fn extent(self) -> [u32; 2] {
        self.extent
    }

    #[must_use]
    pub const fn fact_count(self) -> usize {
        self.fact_count
    }

    #[must_use]
    pub const fn node_origin(self) -> [f32; 2] {
        self.node_origin
    }
}

/// Result of the UI-authoring prefix executed in the same native Editor launch as Game View.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct EditorUiAuthoringCanaryEvidence {
    selected: EditorUiAuthoringCanaryFrame,
    previewed: EditorUiAuthoringCanaryFrame,
    committed: EditorUiAuthoringCanaryFrame,
    undo_depth: usize,
    stale_pick_rejected: bool,
}

impl EditorUiAuthoringCanaryEvidence {
    #[must_use]
    pub const fn selected(&self) -> EditorUiAuthoringCanaryFrame {
        self.selected
    }

    #[must_use]
    pub const fn previewed(&self) -> EditorUiAuthoringCanaryFrame {
        self.previewed
    }

    #[must_use]
    pub const fn committed(&self) -> EditorUiAuthoringCanaryFrame {
        self.committed
    }

    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo_depth
    }

    #[must_use]
    pub const fn stale_pick_rejected(&self) -> bool {
        self.stale_pick_rejected
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorUiAuthoringCanaryStage {
    OpenSource,
    AwaitCanvas,
    AwaitSelection,
    AwaitGestureBegin,
    AwaitPreview,
    AwaitCommit,
    RestoreScene,
    Complete,
}

pub(super) struct EditorUiAuthoringCanaryHost {
    request: EditorUiAuthoringCanaryRequest,
    stage: EditorUiAuthoringCanaryStage,
    scene_document: Option<EditorDocumentId>,
    selected: Option<EditorUiAuthoringCanaryFrame>,
    previewed: Option<EditorUiAuthoringCanaryFrame>,
    committed: Option<EditorUiAuthoringCanaryFrame>,
    undo_depth: Option<usize>,
    initial_pick: Option<EditorUiCanvasPickRequest>,
    stale_pick_rejected: bool,
}

impl EditorUiAuthoringCanaryHost {
    pub(super) fn new(request: EditorUiAuthoringCanaryRequest) -> Self {
        Self {
            request,
            stage: EditorUiAuthoringCanaryStage::OpenSource,
            scene_document: None,
            selected: None,
            previewed: None,
            committed: None,
            undo_depth: None,
            initial_pick: None,
            stale_pick_rejected: false,
        }
    }

    pub(super) fn advance(
        &mut self,
        frame: &nara::tooling::EditorFrameModel,
        authoring: Option<&EditorUiAuthoringFrame>,
        observation: Option<&EditorUiCanvasPresentedObservation<'_>>,
        presenter: &EditorUiCanvasPresenter,
    ) -> Result<Vec<EditorAction>, EditorRunError> {
        let mut actions = Vec::new();
        match self.stage {
            EditorUiAuthoringCanaryStage::OpenSource => {
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    EditorRunError::Platform(
                        "the UI authoring canary requires one active startup Scene".to_owned(),
                    )
                })?;
                self.scene_document = Some(scene.document);
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: scene.document,
                        entity: Some(self.request.scene_anchor.clone()),
                    },
                ));
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::OpenPrefabSource {
                        document: scene.document,
                        base_revision: scene.revision,
                        entity: self.request.scene_anchor.clone(),
                    },
                ));
                self.stage = EditorUiAuthoringCanaryStage::AwaitCanvas;
            }
            EditorUiAuthoringCanaryStage::AwaitCanvas => {
                let Some((authoring, observation)) = exact_authoring_canvas(authoring, observation)
                else {
                    return Ok(actions);
                };
                if observation.fact_count() == 0 {
                    return Err(EditorRunError::Platform(
                        "the UI authoring canary presented an empty Canvas".to_owned(),
                    ));
                }
                let uv = observation
                    .entity_center_uv(&self.request.canvas_entity)
                    .ok_or_else(|| {
                        EditorRunError::Platform(format!(
                            "the UI authoring canary could not map {} into the exact Canvas",
                            self.request.canvas_entity.as_str()
                        ))
                    })?;
                let request = EditorUiCanvasPickRequest {
                    stamp: observation.stamp(),
                    uv,
                };
                let selected = presenter.resolve_pick(request).map_err(|()| {
                    EditorRunError::Platform(
                        "the UI authoring canary lost its exact Canvas receipt".to_owned(),
                    )
                })?;
                if selected.as_ref() != Some(&self.request.canvas_entity) {
                    return Err(EditorRunError::Platform(format!(
                        "the UI authoring canary picked {selected:?} instead of {}",
                        self.request.canvas_entity.as_str()
                    )));
                }
                self.initial_pick = Some(request);
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: authoring.document(),
                        entity: selected,
                    },
                ));
                self.stage = EditorUiAuthoringCanaryStage::AwaitSelection;
            }
            EditorUiAuthoringCanaryStage::AwaitSelection => {
                let Some((authoring, observation)) = exact_authoring_canvas(authoring, observation)
                else {
                    return Ok(actions);
                };
                if authoring.selection().top_entity() != Some(&self.request.canvas_entity) {
                    return Ok(actions);
                }
                self.selected = Some(EditorUiAuthoringCanaryFrame::from_observation(
                    observation,
                    &self.request.canvas_entity,
                )?);
                let baseline = authoring
                    .lower_direct_edit(&UiAuthoringDirectEdit::MoveBy {
                        entity: self.request.canvas_entity.clone(),
                        delta: [0.0, 0.0],
                    })
                    .map_err(|error| EditorRunError::Platform(error.to_string()))?;
                actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                    baseline,
                }));
                self.stage = EditorUiAuthoringCanaryStage::AwaitGestureBegin;
            }
            EditorUiAuthoringCanaryStage::AwaitGestureBegin => {
                let Some((authoring, observation)) = exact_authoring_canvas(authoring, observation)
                else {
                    return Ok(actions);
                };
                let Some(gesture) = authoring.gesture() else {
                    return Ok(actions);
                };
                if gesture.stamp().preview_generation() != 0
                    || observation.stamp().gesture() != Some(gesture.stamp())
                {
                    return Ok(actions);
                }
                let edit = authoring
                    .lower_direct_edit(&UiAuthoringDirectEdit::MoveBy {
                        entity: self.request.canvas_entity.clone(),
                        delta: self.request.move_delta,
                    })
                    .map_err(|error| EditorRunError::Platform(error.to_string()))?;
                actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                    gesture: gesture.stamp().id(),
                    edit,
                }));
                self.stage = EditorUiAuthoringCanaryStage::AwaitPreview;
            }
            EditorUiAuthoringCanaryStage::AwaitPreview => {
                let Some((authoring, observation)) = exact_authoring_canvas(authoring, observation)
                else {
                    return Ok(actions);
                };
                let Some(gesture) = authoring.gesture() else {
                    return Ok(actions);
                };
                if gesture.stamp().preview_generation() == 0
                    || observation.stamp().gesture() != Some(gesture.stamp())
                {
                    return Ok(actions);
                }
                let previewed = EditorUiAuthoringCanaryFrame::from_observation(
                    observation,
                    &self.request.canvas_entity,
                )?;
                let selected = self.selected.ok_or_else(|| {
                    EditorRunError::Platform(
                        "the UI authoring canary lost its selected Canvas frame".to_owned(),
                    )
                })?;
                require_delta(
                    selected.node_origin(),
                    previewed.node_origin(),
                    self.request.move_delta,
                )?;
                self.previewed = Some(previewed);
                let edit = authoring
                    .lower_direct_edit(&UiAuthoringDirectEdit::MoveBy {
                        entity: self.request.canvas_entity.clone(),
                        delta: [0.0, 0.0],
                    })
                    .map_err(|error| EditorRunError::Platform(error.to_string()))?;
                actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                    gesture: gesture.stamp().id(),
                    edit,
                }));
                self.stage = EditorUiAuthoringCanaryStage::AwaitCommit;
            }
            EditorUiAuthoringCanaryStage::AwaitCommit => {
                let Some((authoring, observation)) = exact_authoring_canvas(authoring, observation)
                else {
                    return Ok(actions);
                };
                let selected = self.selected.ok_or_else(|| {
                    EditorRunError::Platform(
                        "the UI authoring canary lost its selected Canvas frame".to_owned(),
                    )
                })?;
                if authoring.gesture().is_some() || authoring.revision() == selected.revision() {
                    return Ok(actions);
                }
                let committed = EditorUiAuthoringCanaryFrame::from_observation(
                    observation,
                    &self.request.canvas_entity,
                )?;
                let initial_pick = self.initial_pick.ok_or_else(|| {
                    EditorRunError::Platform(
                        "the UI authoring canary lost its initial Canvas pick".to_owned(),
                    )
                })?;
                if presenter.resolve_pick(initial_pick).is_ok() {
                    return Err(EditorRunError::Platform(
                        "the committed Canvas accepted a stale pick receipt".to_owned(),
                    ));
                }
                let current_uv = observation
                    .entity_center_uv(&self.request.canvas_entity)
                    .ok_or_else(|| {
                        EditorRunError::Platform(
                            "the committed Canvas lost its selected node mapping".to_owned(),
                        )
                    })?;
                let recovered = presenter
                    .resolve_pick(EditorUiCanvasPickRequest {
                        stamp: observation.stamp(),
                        uv: current_uv,
                    })
                    .map_err(|()| {
                        EditorRunError::Platform(
                            "the committed Canvas did not recover exact pick authority".to_owned(),
                        )
                    })?;
                if recovered.as_ref() != Some(&self.request.canvas_entity) {
                    return Err(EditorRunError::Platform(format!(
                        "the committed Canvas recovered the wrong node {recovered:?}"
                    )));
                }
                self.stale_pick_rejected = true;
                let previewed = self.previewed.ok_or_else(|| {
                    EditorRunError::Platform(
                        "the UI authoring canary lost its previewed Canvas frame".to_owned(),
                    )
                })?;
                require_delta(previewed.node_origin(), committed.node_origin(), [0.0, 0.0])?;
                let document = frame
                    .workspace()
                    .document(authoring.document())
                    .ok_or_else(|| {
                        EditorRunError::Platform(
                            "the committed Prefab disappeared from the workspace".to_owned(),
                        )
                    })?;
                if document.history.undo_depth != 1 || document.history.redo_depth != 0 {
                    return Err(EditorRunError::Platform(format!(
                        "the UI authoring gesture produced history {:?}",
                        document.history
                    )));
                }
                self.undo_depth = Some(document.history.undo_depth);
                self.committed = Some(committed);
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SetActiveDocument {
                        document: self.scene_document.ok_or_else(|| {
                            EditorRunError::Platform(
                                "the UI authoring canary lost its startup Scene".to_owned(),
                            )
                        })?,
                    },
                ));
                self.stage = EditorUiAuthoringCanaryStage::RestoreScene;
            }
            EditorUiAuthoringCanaryStage::RestoreScene => {
                if frame
                    .workspace()
                    .active_scene
                    .as_ref()
                    .map(|scene| scene.document)
                    == self.scene_document
                {
                    self.stage = EditorUiAuthoringCanaryStage::Complete;
                }
            }
            EditorUiAuthoringCanaryStage::Complete => {}
        }
        Ok(actions)
    }

    pub(super) fn is_complete(&self) -> bool {
        self.stage == EditorUiAuthoringCanaryStage::Complete
    }

    pub(super) fn stage(&self) -> impl std::fmt::Debug + '_ {
        self.stage
    }

    pub(super) fn finish(self) -> Result<EditorUiAuthoringCanaryEvidence, EditorRunError> {
        if !self.is_complete() {
            return Err(EditorRunError::Platform(format!(
                "the native Editor exited before UI authoring completed ({:?})",
                self.stage
            )));
        }
        Ok(EditorUiAuthoringCanaryEvidence {
            selected: self
                .selected
                .expect("completed UI canary retains selection"),
            previewed: self.previewed.expect("completed UI canary retains preview"),
            committed: self.committed.expect("completed UI canary retains commit"),
            undo_depth: self
                .undo_depth
                .expect("completed UI canary retains history"),
            stale_pick_rejected: self.stale_pick_rejected,
        })
    }
}

fn exact_authoring_canvas<'a>(
    authoring: Option<&'a EditorUiAuthoringFrame>,
    observation: Option<&'a EditorUiCanvasPresentedObservation<'a>>,
) -> Option<(
    &'a EditorUiAuthoringFrame,
    &'a EditorUiCanvasPresentedObservation<'a>,
)> {
    let authoring = authoring?;
    let observation = observation?;
    let stamp = observation.stamp();
    (stamp.document() == authoring.document()
        && stamp.revision() == authoring.revision()
        && stamp.gesture() == authoring.gesture().map(|gesture| gesture.stamp()))
    .then_some((authoring, observation))
}

fn require_delta(
    before: [f32; 2],
    after: [f32; 2],
    expected: [f64; 2],
) -> Result<(), EditorRunError> {
    let observed = [after[0] - before[0], after[1] - before[1]];
    if observed
        .into_iter()
        .zip(expected)
        .all(|(observed, expected)| (f64::from(observed) - expected).abs() <= 0.01)
    {
        Ok(())
    } else {
        Err(EditorRunError::Platform(format!(
            "the UI authoring Canvas moved by {observed:?}, expected {expected:?}"
        )))
    }
}
