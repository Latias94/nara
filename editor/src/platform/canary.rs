use std::time::{Duration, Instant};

use nara::{
    input::KeyCode,
    project_host::EditorGameStartRequest,
    render::Extent2d,
    tooling::{
        EditorAction, EditorFrameModel, EditorPlayAction, EditorPlayOperation,
        EditorPlayOperationResult, EditorPlayState,
    },
};
use winit::window::Window;

use super::{
    EditorRunError, game_input::EditorGameInputRouter, viewport::EditorGamePresentedObservation,
};

const CANARY_DEADLINE: Duration = Duration::from_secs(20);

/// Product-provided facts for one bounded first-party Game View native canary.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorGameViewCanaryRequest {
    tracked_sprite_sort_key: i32,
    input_key: KeyCode,
}

impl EditorGameViewCanaryRequest {
    #[must_use]
    pub const fn new(tracked_sprite_sort_key: i32, input_key: KeyCode) -> Self {
        Self {
            tracked_sprite_sort_key,
            input_key,
        }
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
    initial: EditorGameViewCanaryFrame,
    input_response: EditorGameViewCanaryFrame,
    paused: EditorGameViewCanaryFrame,
    stepped: EditorGameViewCanaryFrame,
    resized: EditorGameViewCanaryFrame,
    input_delta_x: f32,
    step_delta_x: f32,
}

impl EditorGameViewCanaryEvidence {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorGameViewCanaryStage {
    Start,
    AwaitingInitial,
    AwaitingInputResponse,
    AwaitingPause,
    AwaitingStep,
    AwaitingResize { geometry_generation: u64 },
    AwaitingStopRequest,
    AwaitingStop,
    Complete,
}

pub(super) struct EditorGameViewCanaryHost {
    request: EditorGameViewCanaryRequest,
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
}

pub(super) struct EditorGameViewCanaryAdvance {
    pub(super) action: Option<EditorAction>,
    pub(super) exit: bool,
}

impl EditorGameViewCanaryHost {
    pub(super) fn new(request: EditorGameViewCanaryRequest) -> Self {
        Self {
            request,
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
        }
    }

    pub(super) fn advance(
        &mut self,
        frame: &EditorFrameModel,
        observation: Option<&EditorGamePresentedObservation<'_>>,
        active_start: Option<EditorGameStartRequest>,
        input: &mut EditorGameInputRouter,
        window: &Window,
    ) -> Result<EditorGameViewCanaryAdvance, EditorRunError> {
        if Instant::now() >= self.deadline {
            return Err(self.timeout_error(observation, active_start));
        }

        let mut action = None;
        let mut exit = false;
        match self.stage {
            EditorGameViewCanaryStage::Start => {
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    EditorRunError::Platform(
                        "the Game View canary requires one active startup Scene".to_owned(),
                    )
                })?;
                action = Some(EditorAction::Play(EditorPlayAction::Play {
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
                    if observation.runtime_ui_blocks_unhandled_input() {
                        return Ok(EditorGameViewCanaryAdvance {
                            action: None,
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
                        action = Some(EditorAction::Play(EditorPlayAction::Pause));
                        self.stage = EditorGameViewCanaryStage::AwaitingPause;
                    }
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
                    self.resized = Some(EditorGameViewCanaryFrame::from_observation(observation));
                    self.step_baseline_center_x = Some(center.x);
                    action = Some(EditorAction::Play(EditorPlayAction::StepFixedTick));
                    self.stage = EditorGameViewCanaryStage::AwaitingStep;
                }
            }
            EditorGameViewCanaryStage::AwaitingStopRequest => {
                if frame.play().state() == EditorPlayState::Paused {
                    action = Some(EditorAction::Play(EditorPlayAction::Stop));
                    self.stage = EditorGameViewCanaryStage::AwaitingStop;
                }
            }
            EditorGameViewCanaryStage::AwaitingStop => {
                if frame.play().state() == EditorPlayState::Empty {
                    self.stage = EditorGameViewCanaryStage::Complete;
                    exit = true;
                }
            }
            EditorGameViewCanaryStage::Complete => exit = true,
        }
        Ok(EditorGameViewCanaryAdvance { action, exit })
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
        EditorRunError::Platform(format!(
            "the bounded Game View canary timed out in {:?}; active geometry={active_geometry:?}, last composite={observed:?}",
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
        Ok(EditorGameViewCanaryEvidence {
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
        })
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
