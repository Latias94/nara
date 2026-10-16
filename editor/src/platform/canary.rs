use std::time::{Duration, Instant};

use nara::{
    input::KeyCode,
    project_host::{EditorGameStartRequest, EditorUiAuthoringFrame},
    render::Extent2d,
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuSubmittedImageReceipt,
    },
    tooling::{
        EditorAction, EditorFrameModel, EditorPlayAction, EditorPlayOperation,
        EditorPlayOperationResult, EditorPlayState,
    },
};
use winit::window::Window;

use super::{
    EditorRunError,
    game_input::EditorGameInputRouter,
    ui_canary::{
        EditorUiAuthoringCanaryEvidence, EditorUiAuthoringCanaryHost,
        EditorUiAuthoringCanaryRequest,
    },
    viewport::{
        EditorGamePresentedObservation, EditorUiCanvasPresentedObservation, EditorUiCanvasPresenter,
    },
};

const CANARY_DEADLINE: Duration = Duration::from_secs(35);

pub(super) struct EditorGameViewCanaryFrameInput<'a> {
    pub(super) frame: &'a EditorFrameModel,
    pub(super) observation: Option<&'a EditorGamePresentedObservation<'a>>,
    pub(super) active_start: Option<EditorGameStartRequest>,
    pub(super) ui_authoring_frame: Option<&'a EditorUiAuthoringFrame>,
    pub(super) ui_observation: Option<&'a EditorUiCanvasPresentedObservation<'a>>,
    pub(super) ui_presenter: &'a EditorUiCanvasPresenter,
    pub(super) scene_receipt: Option<WgpuEditorCompositeReceipt>,
    pub(super) workspace: &'a mut crate::workspace::EditorPanelWorkspace,
    pub(super) gpu: &'a WgpuEditorExecution,
    pub(super) game_texture_registered: bool,
}

/// Product-provided facts for one bounded first-party Game View native canary.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct EditorGameViewCanaryRequest {
    tracked_sprite_sort_key: i32,
    input_key: KeyCode,
    ui_authoring: Option<EditorUiAuthoringCanaryRequest>,
}

impl EditorGameViewCanaryRequest {
    #[must_use]
    pub const fn new(tracked_sprite_sort_key: i32, input_key: KeyCode) -> Self {
        Self {
            tracked_sprite_sort_key,
            input_key,
            ui_authoring: None,
        }
    }

    #[must_use]
    pub fn with_ui_authoring(mut self, request: EditorUiAuthoringCanaryRequest) -> Self {
        self.ui_authoring = Some(request);
        self
    }
}

/// Exact composite facts retained from one canary milestone.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorGameViewCanaryFrame {
    runtime_generation: u64,
    runtime_frame_index: u64,
    geometry_generation: u64,
    target_generation: u64,
    presentation_generation: u64,
    width: u32,
    height: u32,
    sprite_instances: usize,
    runtime_ui_present: bool,
}

impl EditorGameViewCanaryFrame {
    fn from_observation(observation: &EditorGamePresentedObservation) -> Self {
        let stamp = observation.stamp();
        let receipt = observation.receipt();
        let image = receipt.image();
        let extent = image.target().extent();
        Self {
            runtime_generation: stamp.runtime_generation(),
            runtime_frame_index: observation.runtime_frame_index(),
            geometry_generation: stamp.geometry_generation(),
            target_generation: image.target_generation(),
            presentation_generation: receipt.presentation_generation(),
            width: extent.width,
            height: extent.height,
            sprite_instances: observation.total_sprite_instances(),
            runtime_ui_present: observation.runtime_ui_present(),
        }
    }

    #[must_use]
    pub const fn runtime_generation(self) -> u64 {
        self.runtime_generation
    }

    #[must_use]
    pub const fn runtime_frame_index(self) -> u64 {
        self.runtime_frame_index
    }

    #[must_use]
    pub const fn geometry_generation(self) -> u64 {
        self.geometry_generation
    }

    #[must_use]
    pub const fn target_generation(self) -> u64 {
        self.target_generation
    }

    #[must_use]
    pub const fn presentation_generation(self) -> u64 {
        self.presentation_generation
    }

    #[must_use]
    pub const fn extent(self) -> [u32; 2] {
        [self.width, self.height]
    }

    #[must_use]
    pub const fn sprite_instances(self) -> usize {
        self.sprite_instances
    }

    #[must_use]
    pub const fn runtime_ui_present(self) -> bool {
        self.runtime_ui_present
    }
}

/// Typed result of one exact native Reference Game Game View trajectory.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct EditorGameViewCanaryEvidence {
    ui_authoring: Option<EditorUiAuthoringCanaryEvidence>,
    initial: EditorGameViewCanaryFrame,
    input_response: EditorGameViewCanaryFrame,
    paused: EditorGameViewCanaryFrame,
    stepped: EditorGameViewCanaryFrame,
    resized: EditorGameViewCanaryFrame,
    input_delta_x: f32,
    step_delta_x: f32,
    concurrent_view_frames: u64,
    scene_view_id: u64,
    closed_game_view_id: u64,
    reopened_game_view_id: u64,
}

impl EditorGameViewCanaryEvidence {
    #[must_use]
    pub const fn concurrent_view_frames(&self) -> u64 {
        self.concurrent_view_frames
    }

    #[must_use]
    pub const fn scene_view_id(&self) -> u64 {
        self.scene_view_id
    }

    #[must_use]
    pub const fn closed_game_view_id(&self) -> u64 {
        self.closed_game_view_id
    }

    #[must_use]
    pub const fn reopened_game_view_id(&self) -> u64 {
        self.reopened_game_view_id
    }

    #[must_use]
    pub const fn ui_authoring(&self) -> Option<&EditorUiAuthoringCanaryEvidence> {
        self.ui_authoring.as_ref()
    }

    #[must_use]
    pub const fn initial(&self) -> EditorGameViewCanaryFrame {
        self.initial
    }

    #[must_use]
    pub const fn input_response(&self) -> EditorGameViewCanaryFrame {
        self.input_response
    }

    #[must_use]
    pub const fn paused(&self) -> EditorGameViewCanaryFrame {
        self.paused
    }

    #[must_use]
    pub const fn stepped(&self) -> EditorGameViewCanaryFrame {
        self.stepped
    }

    #[must_use]
    pub const fn resized(&self) -> EditorGameViewCanaryFrame {
        self.resized
    }

    #[must_use]
    pub const fn input_delta_x(&self) -> f32 {
        self.input_delta_x
    }

    #[must_use]
    pub const fn step_delta_x(&self) -> f32 {
        self.step_delta_x
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditorGameViewCanaryStage {
    Start,
    AwaitingInitial,
    AwaitingInputResponse,
    AwaitingHidden { redraws: u8 },
    AwaitingReopened,
    AwaitingReopenedInputResponse { baseline_center_x: f32 },
    AwaitingPause,
    AwaitingPausedRefocus,
    AwaitingStep,
    AwaitingResize { geometry_generation: u64 },
    AwaitingStopRequest,
    AwaitingStop,
    Complete,
}

pub(super) struct EditorGameViewCanaryHost {
    request: EditorGameViewCanaryRequest,
    ui_authoring: Option<EditorUiAuthoringCanaryHost>,
    stage: EditorGameViewCanaryStage,
    deadline: Instant,
    initial: Option<EditorGameViewCanaryFrame>,
    input_response: Option<EditorGameViewCanaryFrame>,
    paused: Option<EditorGameViewCanaryFrame>,
    stepped: Option<EditorGameViewCanaryFrame>,
    resized: Option<EditorGameViewCanaryFrame>,
    viewport_extent_override: Option<Extent2d>,
    input_baseline_center_x: Option<f32>,
    step_baseline_center_x: Option<f32>,
    input_delta_x: Option<f32>,
    step_delta_x: Option<f32>,
    concurrent_view_frames: u64,
    scene_view_id: Option<u64>,
    closed_game_image: Option<WgpuSubmittedImageReceipt>,
    reopened_game_view_id: Option<u64>,
    scene_before_game_resize: Option<WgpuSubmittedImageReceipt>,
}

pub(super) struct EditorGameViewCanaryAdvance {
    pub(super) actions: Vec<EditorAction>,
    pub(super) exit: bool,
}

impl EditorGameViewCanaryHost {
    pub(super) fn new(request: EditorGameViewCanaryRequest) -> Self {
        let ui_authoring = request
            .ui_authoring
            .clone()
            .map(EditorUiAuthoringCanaryHost::new);
        Self {
            request,
            ui_authoring,
            stage: EditorGameViewCanaryStage::Start,
            deadline: Instant::now() + CANARY_DEADLINE,
            initial: None,
            input_response: None,
            paused: None,
            stepped: None,
            resized: None,
            viewport_extent_override: None,
            input_baseline_center_x: None,
            step_baseline_center_x: None,
            input_delta_x: None,
            step_delta_x: None,
            concurrent_view_frames: 0,
            scene_view_id: None,
            closed_game_image: None,
            reopened_game_view_id: None,
            scene_before_game_resize: None,
        }
    }

    pub(super) fn advance(
        &mut self,
        frame_input: EditorGameViewCanaryFrameInput<'_>,
        input: &mut EditorGameInputRouter,
        window: &Window,
    ) -> Result<EditorGameViewCanaryAdvance, EditorRunError> {
        let EditorGameViewCanaryFrameInput {
            frame,
            observation,
            active_start,
            ui_authoring_frame,
            ui_observation,
            ui_presenter,
            scene_receipt,
            workspace,
            gpu,
            game_texture_registered,
        } = frame_input;
        if Instant::now() >= self.deadline {
            return Err(self.timeout_error(observation, active_start));
        }

        let mut actions = Vec::new();
        match self.stage {
            EditorGameViewCanaryStage::Start => {
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    EditorRunError::Platform(
                        "the Game View canary requires one active startup Scene".to_owned(),
                    )
                })?;
                actions.push(EditorAction::Play(EditorPlayAction::Play {
                    document: scene.document,
                    base_revision: scene.revision,
                }));
                self.stage = EditorGameViewCanaryStage::AwaitingInitial;
            }
            EditorGameViewCanaryStage::AwaitingInitial => {
                if frame.play().state() == EditorPlayState::Running
                    && let Some(observation) = observation
                {
                    self.require_combined_product(observation)?;
                    let Some(scene_receipt) = scene_receipt else {
                        window.request_redraw();
                        return Ok(EditorGameViewCanaryAdvance {
                            actions,
                            exit: false,
                        });
                    };
                    self.require_concurrent_views(scene_receipt, observation.receipt())?;
                    self.scene_view_id = Some(scene_receipt.image().view_id().get());
                    if observation.runtime_ui_blocks_unhandled_input() {
                        return Ok(EditorGameViewCanaryAdvance {
                            actions,
                            exit: false,
                        });
                    }
                    let center = self.require_tracked_center(observation)?;
                    self.initial = Some(EditorGameViewCanaryFrame::from_observation(observation));
                    self.input_baseline_center_x = Some(center.x);
                    if !input.capture_and_press_key(self.request.input_key) {
                        return Err(EditorRunError::Platform(
                            "the initial Game composite did not admit captured input".to_owned(),
                        ));
                    }
                    window.request_redraw();
                    self.stage = EditorGameViewCanaryStage::AwaitingInputResponse;
                }
            }
            EditorGameViewCanaryStage::AwaitingInputResponse => {
                let Some(initial) = self.initial else {
                    return Err(EditorRunError::Platform(
                        "the Game View canary lost its initial composite".to_owned(),
                    ));
                };
                if frame.play().state() == EditorPlayState::Running
                    && let Some(observation) = observation
                    && observation.runtime_frame_index() > initial.runtime_frame_index()
                {
                    self.require_combined_product(observation)?;
                    let center = self.require_tracked_center(observation)?;
                    let delta_x = center.x
                        - self.input_baseline_center_x.ok_or_else(|| {
                            EditorRunError::Platform(
                                "the Game View canary lost its pre-input sprite position"
                                    .to_owned(),
                            )
                        })?;
                    if delta_x > f32::EPSILON {
                        if !delta_x.is_finite() {
                            return Err(EditorRunError::Platform(
                                "the captured input produced a non-finite sprite response"
                                    .to_owned(),
                            ));
                        }
                        self.input_response =
                            Some(EditorGameViewCanaryFrame::from_observation(observation));
                        self.input_delta_x = Some(delta_x);
                        self.closed_game_image = Some(observation.receipt().image());
                        let panel = game_panel_id(workspace)?;
                        workspace.layout_mut().close(panel);
                        input.release_to_editor();
                        window.request_redraw();
                        self.stage = EditorGameViewCanaryStage::AwaitingHidden { redraws: 0 };
                    }
                }
            }
            EditorGameViewCanaryStage::AwaitingHidden { redraws } => {
                let closed = self
                    .closed_game_image
                    .expect("the close stage retains its exact image");
                if frame.play().state() != EditorPlayState::Running
                    || input.is_captured()
                    || game_texture_registered
                    || gpu.viewport_target_matches(
                        closed.view_id(),
                        closed.source_generation(),
                        closed.target(),
                    )
                {
                    return Err(EditorRunError::Platform(
                        "closing Game did not retire only its live texture and input while Play continued".to_owned(),
                    ));
                }
                if let Some(scene) = scene_receipt {
                    if Some(scene.image().view_id().get()) != self.scene_view_id {
                        return Err(EditorRunError::Platform(
                            "closing Game replaced the Scene view owner".to_owned(),
                        ));
                    }
                    if redraws >= 2 {
                        let panel = game_panel_id(workspace)?;
                        workspace.layout_mut().reopen(panel);
                        self.stage = EditorGameViewCanaryStage::AwaitingReopened;
                    } else {
                        self.stage = EditorGameViewCanaryStage::AwaitingHidden {
                            redraws: redraws + 1,
                        };
                    }
                }
                window.request_redraw();
            }
            EditorGameViewCanaryStage::AwaitingReopened => {
                if let Some(observation) = observation
                    && let Some(scene) = scene_receipt
                    && frame.play().state() == EditorPlayState::Running
                {
                    self.require_concurrent_views(scene, observation.receipt())?;
                    let closed = self
                        .closed_game_image
                        .expect("the reopen stage retains the closed image");
                    let reopened = observation.receipt().image();
                    let previous = self
                        .input_response
                        .expect("reopening follows the input response");
                    if reopened.view_id() == closed.view_id()
                        || Some(scene.image().view_id().get()) != self.scene_view_id
                        || observation.runtime_frame_index() <= previous.runtime_frame_index()
                    {
                        return Err(EditorRunError::Platform(
                            "reopening Game reused a retired view, replaced Scene, or stopped the hidden Runtime clock".to_owned(),
                        ));
                    }
                    self.reopened_game_view_id = Some(reopened.view_id().get());
                    let baseline_center_x = self.require_tracked_center(observation)?.x;
                    if !input.capture_and_press_key(self.request.input_key) {
                        return Err(EditorRunError::Platform(
                            "the reopened Game composite did not admit fresh captured input"
                                .to_owned(),
                        ));
                    }
                    window.request_redraw();
                    self.stage = EditorGameViewCanaryStage::AwaitingReopenedInputResponse {
                        baseline_center_x,
                    };
                }
            }
            EditorGameViewCanaryStage::AwaitingReopenedInputResponse { baseline_center_x } => {
                if frame.play().state() == EditorPlayState::Running
                    && let Some(observation) = observation
                    && self.require_tracked_center(observation)?.x
                        > baseline_center_x + f32::EPSILON
                {
                    self.require_combined_product(observation)?;
                    actions.push(EditorAction::Play(EditorPlayAction::Pause));
                    self.stage = EditorGameViewCanaryStage::AwaitingPause;
                }
            }
            EditorGameViewCanaryStage::AwaitingPause => {
                let Some(input_response) = self.input_response else {
                    return Err(EditorRunError::Platform(
                        "the Game View canary lost its visible input response".to_owned(),
                    ));
                };
                if frame.play().state() == EditorPlayState::Paused
                    && let Some(observation) = observation
                    && observation.runtime_frame_index() > input_response.runtime_frame_index()
                {
                    self.require_combined_product(observation)?;
                    self.require_tracked_center(observation)?;
                    let paused = EditorGameViewCanaryFrame::from_observation(observation);
                    self.paused = Some(paused);
                    self.scene_before_game_resize = scene_receipt.map(|receipt| receipt.image());
                    input.release_to_editor();
                    window.request_redraw();
                    self.stage = EditorGameViewCanaryStage::AwaitingPausedRefocus;
                }
            }
            EditorGameViewCanaryStage::AwaitingPausedRefocus => {
                if frame.play().state() == EditorPlayState::Paused
                    && let Some(observation) = observation
                {
                    if !input.capture_and_press_key(self.request.input_key) {
                        return Err(EditorRunError::Platform(
                            "the retained paused Game image could not recapture input after editing"
                                .to_owned(),
                        ));
                    }
                    let paused = self.paused.expect("refocus follows a visible paused image");
                    let current = observation.stamp().logical_window().resolution;
                    let width = current.physical_width.checked_add(64).ok_or_else(|| {
                        EditorRunError::Platform(
                            "the Game View canary resize width exhausted".to_owned(),
                        )
                    })?;
                    let height = current.physical_height.checked_add(48).ok_or_else(|| {
                        EditorRunError::Platform(
                            "the Game View canary resize height exhausted".to_owned(),
                        )
                    })?;
                    let resized_extent = Extent2d::new(width, height).ok_or_else(|| {
                        EditorRunError::Platform(
                            "the Game View canary produced a zero resize extent".to_owned(),
                        )
                    })?;
                    self.viewport_extent_override = Some(resized_extent);
                    window.request_redraw();
                    self.stage = EditorGameViewCanaryStage::AwaitingResize {
                        geometry_generation: paused.geometry_generation(),
                    };
                }
            }
            EditorGameViewCanaryStage::AwaitingStep => {
                let Some(resized) = self.resized else {
                    return Err(EditorRunError::Platform(
                        "the Game View canary lost its resized composite".to_owned(),
                    ));
                };
                let step_applied = matches!(
                    frame.play().result(),
                    Some(EditorPlayOperationResult::Applied {
                        operation: EditorPlayOperation::StepFixedTick,
                        generation: Some(_),
                    })
                );
                if frame.play().state() == EditorPlayState::Paused
                    && step_applied
                    && let Some(observation) = observation
                    && observation.runtime_frame_index() > resized.runtime_frame_index()
                {
                    self.require_combined_product(observation)?;
                    let center = self.require_tracked_center(observation)?;
                    let delta_x = center.x
                        - self.step_baseline_center_x.ok_or_else(|| {
                            EditorRunError::Platform(
                                "the Game View canary lost its pre-Step sprite position".to_owned(),
                            )
                        })?;
                    if !delta_x.is_finite() || delta_x <= f32::EPSILON {
                        return Err(EditorRunError::Platform(
                            "the exact fixed Step produced no visible sprite response".to_owned(),
                        ));
                    }
                    let stepped = EditorGameViewCanaryFrame::from_observation(observation);
                    self.step_delta_x = Some(delta_x);
                    self.stepped = Some(stepped);
                    window.request_redraw();
                    self.stage = EditorGameViewCanaryStage::AwaitingStopRequest;
                }
            }
            EditorGameViewCanaryStage::AwaitingResize {
                geometry_generation,
            } => {
                if frame.play().state() == EditorPlayState::Paused
                    && let Some(observation) = observation
                    && observation.stamp().geometry_generation() > geometry_generation
                {
                    self.require_combined_product(observation)?;
                    let center = self.require_tracked_center(observation)?;
                    let scene = scene_receipt.ok_or_else(|| {
                        EditorRunError::Platform(
                            "Game resize omitted the concurrent Scene composite".to_owned(),
                        )
                    })?;
                    self.require_concurrent_views(scene, observation.receipt())?;
                    let before = self.scene_before_game_resize.ok_or_else(|| {
                        EditorRunError::Platform(
                            "the native canary lost the Scene target before Game resize".to_owned(),
                        )
                    })?;
                    if scene.image().view_id() != before.view_id()
                        || scene.image().target() != before.target()
                        || scene.image().target_generation() != before.target_generation()
                    {
                        return Err(EditorRunError::Platform(
                            "resizing Game replaced or resized Scene".to_owned(),
                        ));
                    }
                    self.resized = Some(EditorGameViewCanaryFrame::from_observation(observation));
                    self.step_baseline_center_x = Some(center.x);
                    actions.push(EditorAction::Play(EditorPlayAction::StepFixedTick));
                    self.stage = EditorGameViewCanaryStage::AwaitingStep;
                }
            }
            EditorGameViewCanaryStage::AwaitingStopRequest => {
                if frame.play().state() == EditorPlayState::Paused {
                    actions.push(EditorAction::Play(EditorPlayAction::Stop));
                    self.stage = EditorGameViewCanaryStage::AwaitingStop;
                }
            }
            EditorGameViewCanaryStage::AwaitingStop => {
                if frame.play().state() == EditorPlayState::Empty {
                    self.stage = EditorGameViewCanaryStage::Complete;
                }
            }
            EditorGameViewCanaryStage::Complete => {}
        }

        if self.stage == EditorGameViewCanaryStage::Complete
            && let Some(ui_authoring) = self.ui_authoring.as_mut()
            && !ui_authoring.is_complete()
        {
            actions.extend(ui_authoring.advance(
                frame,
                ui_authoring_frame,
                ui_observation,
                ui_presenter,
            )?);
            return Ok(EditorGameViewCanaryAdvance {
                actions,
                exit: false,
            });
        }
        Ok(EditorGameViewCanaryAdvance {
            actions,
            exit: self.stage == EditorGameViewCanaryStage::Complete,
        })
    }

    pub(super) const fn viewport_extent_override(&self) -> Option<Extent2d> {
        self.viewport_extent_override
    }

    pub(super) const fn tracked_sprite_sort_key(&self) -> i32 {
        self.request.tracked_sprite_sort_key
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn timeout_error(
        &self,
        observation: Option<&EditorGamePresentedObservation<'_>>,
        active_start: Option<EditorGameStartRequest>,
    ) -> EditorRunError {
        let active_geometry = active_start.map(EditorGameStartRequest::geometry_generation);
        let observed = observation.map(|observation| {
            (
                observation.stamp().geometry_generation(),
                observation.runtime_frame_index(),
            )
        });
        let ui_stage = self
            .ui_authoring
            .as_ref()
            .filter(|ui| !ui.is_complete())
            .map(|ui| format!("{:?}", ui.stage()));
        EditorRunError::Platform(format!(
            "the bounded Editor canary timed out in {:?}; UI stage={ui_stage:?}, active geometry={active_geometry:?}, last composite={observed:?}",
            self.stage,
        ))
    }

    pub(super) fn finish(self) -> Result<EditorGameViewCanaryEvidence, EditorRunError> {
        if self.stage != EditorGameViewCanaryStage::Complete {
            return Err(EditorRunError::Platform(format!(
                "the native Editor exited before the Game View canary completed ({:?})",
                self.stage
            )));
        }
        let ui_authoring = self
            .ui_authoring
            .map(EditorUiAuthoringCanaryHost::finish)
            .transpose()?;
        Ok(EditorGameViewCanaryEvidence {
            ui_authoring,
            initial: self
                .initial
                .expect("a completed canary retains initial facts"),
            input_response: self
                .input_response
                .expect("a completed canary retains visible input facts"),
            paused: self
                .paused
                .expect("a completed canary retains paused facts"),
            stepped: self.stepped.expect("a completed canary retains Step facts"),
            resized: self
                .resized
                .expect("a completed canary retains resize facts"),
            input_delta_x: self
                .input_delta_x
                .expect("a completed canary retains the visible input response"),
            step_delta_x: self
                .step_delta_x
                .expect("a completed canary retains the visible Step response"),
            concurrent_view_frames: self.concurrent_view_frames,
            scene_view_id: self
                .scene_view_id
                .expect("completed concurrent views retain Scene identity"),
            closed_game_view_id: self
                .closed_game_image
                .expect("completed close retains Game identity")
                .view_id()
                .get(),
            reopened_game_view_id: self
                .reopened_game_view_id
                .expect("completed reopen retains the new identity"),
        })
    }

    fn require_concurrent_views(
        &mut self,
        scene: WgpuEditorCompositeReceipt,
        game: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        if scene.window() != game.window() || scene.image().view_id() == game.image().view_id() {
            return Err(EditorRunError::Platform(
                "Scene and Game did not sample independent exact textures in the same native window frame".to_owned(),
            ));
        }
        self.concurrent_view_frames += 1;
        Ok(())
    }

    fn require_combined_product(
        &self,
        observation: &EditorGamePresentedObservation<'_>,
    ) -> Result<(), EditorRunError> {
        if observation.total_sprite_instances() == 0 || !observation.runtime_ui_present() {
            return Err(EditorRunError::Platform(
                "the compiled-product canary requires combined sprite and Runtime UI content"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    fn require_tracked_center(
        &self,
        observation: &EditorGamePresentedObservation<'_>,
    ) -> Result<nara::prelude::Vec2, EditorRunError> {
        observation.tracked_center().ok_or_else(|| {
            EditorRunError::Platform(format!(
                "the compiled-product canary requires exactly one sprite at sort key {}",
                self.request.tracked_sprite_sort_key
            ))
        })
    }
}

fn game_panel_id(
    workspace: &crate::workspace::EditorPanelWorkspace,
) -> Result<crate::workspace::EditorPanelInstanceId, EditorRunError> {
    workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.game-view")
        .map(|panel| panel.id())
        .ok_or_else(|| EditorRunError::Platform("the native canary has no Game panel".to_owned()))
}
