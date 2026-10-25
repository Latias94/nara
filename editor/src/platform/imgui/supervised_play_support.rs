use std::time::{Duration, Instant};

use nara::{
    identity::SceneEntityId,
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::AuthoringFieldEdit,
    tooling::{
        EditorAction, EditorDocumentId, EditorDocumentMutation, EditorExternalPlayState,
        EditorFrameModel, EditorPersistenceAction, EditorPersistenceResult, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorRuntimeAvailability,
    },
};
use winit::window::WindowId;

use crate::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

use crate::platform::EditorRunError;

pub(super) struct SupervisedPlayTestAdvance {
    pub(super) action: Option<EditorAction>,
    pub(super) redraw: bool,
    pub(super) request_exit: bool,
    pub(super) exit: bool,
}

impl SupervisedPlayTestAdvance {
    const fn idle() -> Self {
        Self {
            action: None,
            redraw: false,
            request_exit: false,
            exit: false,
        }
    }

    const fn redraw() -> Self {
        Self {
            action: None,
            redraw: true,
            request_exit: false,
            exit: false,
        }
    }

    fn action(action: EditorAction) -> Self {
        Self {
            action: Some(action),
            redraw: true,
            request_exit: false,
            exit: false,
        }
    }

    const fn request_exit() -> Self {
        Self {
            action: None,
            redraw: true,
            request_exit: true,
            exit: false,
        }
    }
}

pub(super) mod fixture;

use fixture::GuardedSupervisedPlayProject;

const SUPERVISED_PLAY_CANARY_DEADLINE: Duration = Duration::from_secs(600);
const FIRST_EDITED_WEAPON_X: f64 = 2.75;
const SUCCESSOR_EDITED_WEAPON_X: f64 = 3.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupervisedPlayTestStage {
    ProjectManager,
    AwaitingOpen,
    AwaitingFirstEdit,
    AwaitingFirstSave,
    AwaitingFirstSaveReceipt,
    AwaitingFirstSaveAcknowledgement,
    AwaitingPlay,
    AwaitingReady,
    AwaitingStop,
    AwaitingBuildFailure,
    AwaitingLastGoodReady,
    AwaitingLastGoodStop,
    AwaitingSuccessorEdit,
    AwaitingSuccessorSave,
    AwaitingSuccessorSaveReceipt,
    AwaitingSuccessorSaveAcknowledgement,
    AwaitingSuccessorPlay,
    AwaitingSuccessorReady,
    AwaitingFinalStop,
    Complete,
}

/// Bounded evidence from one real external-window supervised Play trajectory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SupervisedPlayTestEvidence {
    first_saved_revision: u64,
    successor_saved_revision: u64,
    first_generation: u64,
    last_good_generation: u64,
    successor_generation: u64,
    first_output_sequence: u64,
    successor_output_sequence: u64,
    first_output_truncated: bool,
    successor_output_truncated: bool,
}

impl SupervisedPlayTestEvidence {
    #[must_use]
    pub(super) const fn first_saved_revision(self) -> u64 {
        self.first_saved_revision
    }

    #[must_use]
    pub(super) const fn successor_saved_revision(self) -> u64 {
        self.successor_saved_revision
    }

    #[must_use]
    pub(super) const fn first_generation(self) -> u64 {
        self.first_generation
    }

    #[must_use]
    pub(super) const fn successor_generation(self) -> u64 {
        self.successor_generation
    }

    #[must_use]
    pub(super) const fn last_good_generation(self) -> u64 {
        self.last_good_generation
    }

    #[must_use]
    pub(super) const fn first_output_sequence(self) -> u64 {
        self.first_output_sequence
    }

    #[must_use]
    pub(super) const fn successor_output_sequence(self) -> u64 {
        self.successor_output_sequence
    }

    #[must_use]
    pub(super) const fn first_output_truncated(self) -> bool {
        self.first_output_truncated
    }

    #[must_use]
    pub(super) const fn successor_output_truncated(self) -> bool {
        self.successor_output_truncated
    }
}

pub(super) struct SupervisedPlayTestFixture {
    project: GuardedSupervisedPlayProject,
    stage: SupervisedPlayTestStage,
    deadline: Instant,
    document: Option<EditorDocumentId>,
    first_saved_revision: Option<u64>,
    successor_saved_revision: Option<u64>,
    first_generation: Option<u64>,
    last_good_generation: Option<u64>,
    successor_generation: Option<u64>,
    first_output_sequence: u64,
    successor_output_sequence: u64,
    first_output_truncated: bool,
    successor_output_truncated: bool,
}

impl SupervisedPlayTestFixture {
    pub(super) fn new(
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, Self), EditorRunError> {
        let source_root = options.project_root().ok_or_else(|| {
            EditorRunError::usage("the supervised Play test requires one source project root")
        })?;
        let project = GuardedSupervisedPlayProject::prepare(source_root).map_err(|error| {
            EditorRunError::Platform(format!(
                "the supervised Play canary could not prepare its guarded project: {error}"
            ))
        })?;
        let launch = EditorLaunchOptions::new(project.root())
            .with_optional_profile(options.profile().map(str::to_owned));
        Ok((
            EditorApplication::new(launch),
            Self {
                project,
                stage: SupervisedPlayTestStage::ProjectManager,
                deadline: Instant::now() + SUPERVISED_PLAY_CANARY_DEADLINE,
                document: None,
                first_saved_revision: None,
                successor_saved_revision: None,
                first_generation: None,
                last_good_generation: None,
                successor_generation: None,
                first_output_sequence: 0,
                successor_output_sequence: 0,
                first_output_truncated: false,
                successor_output_truncated: false,
            },
        ))
    }

    pub(super) const fn deadline(&self) -> Instant {
        self.deadline
    }

    fn transition(&mut self, next: SupervisedPlayTestStage) {
        self.stage = next;
    }

    pub(super) fn advance_project_shell(
        &mut self,
        rendered_state: EditorApplicationState,
        application: &mut EditorApplication,
        _window_id: WindowId,
    ) -> Result<bool, EditorRunError> {
        self.require_before_deadline(application)?;
        match self.stage {
            SupervisedPlayTestStage::ProjectManager => {
                if rendered_state != EditorApplicationState::ProjectManager {
                    return Err(
                        self.state_error(application.state(), "Project Manager was skipped")
                    );
                }
                let update = application.native_shell_ready();
                if !update.actions_observed()
                    || application.state() != EditorApplicationState::OpeningInitial
                {
                    return Err(self.state_error(
                        application.state(),
                        "the initial project request was not armed",
                    ));
                }
                self.transition(SupervisedPlayTestStage::AwaitingOpen);
                Ok(true)
            }
            SupervisedPlayTestStage::AwaitingOpen => Ok(false),
            _ => Err(self.state_error(
                application.state(),
                "the supervised Play canary returned to the project shell",
            )),
        }
    }

    pub(super) fn advance_workspace(
        &mut self,
        application: &mut EditorApplication,
        _window_id: WindowId,
    ) -> Result<SupervisedPlayTestAdvance, EditorRunError> {
        self.require_before_deadline(application)?;
        let Some(frame) = application.current_frame() else {
            return Err(self.state_error(
                application.state(),
                "the workspace frame disappeared during supervised Play",
            ));
        };
        if !matches!(
            frame.runtime_availability(),
            EditorRuntimeAvailability::ExternalSupervised
        ) {
            return Err(self.state_error(
                application.state(),
                "the opened project did not advertise ExternalSupervised",
            ));
        }
        let external = frame.play().external().ok_or_else(|| {
            self.state_error(
                application.state(),
                "the external Play projection was not published",
            )
        })?;
        match self.stage {
            SupervisedPlayTestStage::AwaitingOpen => {
                if matches!(
                    application.state(),
                    EditorApplicationState::AuthoringReady
                        | EditorApplicationState::AuthoringDegraded
                ) {
                    self.transition(SupervisedPlayTestStage::AwaitingFirstEdit);
                    Ok(SupervisedPlayTestAdvance::redraw())
                } else {
                    Ok(SupervisedPlayTestAdvance::idle())
                }
            }
            SupervisedPlayTestStage::AwaitingFirstEdit => {
                let (document, action) = edit_action(
                    &frame,
                    FIRST_EDITED_WEAPON_X,
                    application.state(),
                    "the first saved edit",
                )?;
                self.document = Some(document);
                self.transition(SupervisedPlayTestStage::AwaitingFirstSave);
                Ok(SupervisedPlayTestAdvance::action(action))
            }
            SupervisedPlayTestStage::AwaitingFirstSave => {
                let document = self.require_document(application.state())?;
                let action = save_action(
                    &frame,
                    document,
                    application.state(),
                    "the first saved edit",
                )?;
                self.transition(SupervisedPlayTestStage::AwaitingFirstSaveReceipt);
                Ok(SupervisedPlayTestAdvance::action(action))
            }
            SupervisedPlayTestStage::AwaitingFirstSaveReceipt => {
                let document = self.require_document(application.state())?;
                let Some(revision) = observe_saved_revision(
                    &frame,
                    document,
                    application.state(),
                    "the first saved edit",
                )?
                else {
                    return Ok(SupervisedPlayTestAdvance::idle());
                };
                self.first_saved_revision = Some(revision);
                self.transition(SupervisedPlayTestStage::AwaitingFirstSaveAcknowledgement);
                Ok(SupervisedPlayTestAdvance::action(
                    EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                        attempt: frame
                            .persistence()
                            .attempt()
                            .expect("the observed Save is terminal"),
                    }),
                ))
            }
            SupervisedPlayTestStage::AwaitingFirstSaveAcknowledgement => {
                if frame.persistence().result().is_some() {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                self.transition(SupervisedPlayTestStage::AwaitingPlay);
                Ok(SupervisedPlayTestAdvance::redraw())
            }
            SupervisedPlayTestStage::AwaitingPlay => {
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(application.state(), "the project has no startup Scene")
                })?;
                self.transition(SupervisedPlayTestStage::AwaitingReady);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayTestStage::AwaitingReady => {
                if let Some(result) = frame.play().result()
                    && !is_accepted(result, EditorPlayOperation::Play)
                {
                    return Err(EditorRunError::Platform(format!(
                        "supervised Play canary: the first external Play request was not accepted; result={result:?}; stage={:?}; state={:?}",
                        self.stage,
                        application.state(),
                    )));
                }
                if external.state() == EditorExternalPlayState::Faulted {
                    return Err(self.state_error(
                        application.state(),
                        "the first external Play attempt faulted before READY",
                    ));
                }
                if external.state() != EditorExternalPlayState::Running {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                let generation = external.generation().ok_or_else(|| {
                    self.state_error(application.state(), "READY did not publish a generation")
                })?;
                self.first_generation = Some(generation);
                self.first_output_sequence = external.output_sequence();
                self.first_output_truncated = external.output_truncated();
                self.transition(SupervisedPlayTestStage::AwaitingStop);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayTestStage::AwaitingStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                self.project
                    .inject_desktop_build_failure()
                    .map_err(|error| {
                        EditorRunError::Platform(format!(
                            "supervised Play canary: could not inject the bounded build failure: {error}"
                        ))
                    })?;
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(
                        application.state(),
                        "the Scene disappeared before the injected build failure",
                    )
                })?;
                self.transition(SupervisedPlayTestStage::AwaitingBuildFailure);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayTestStage::AwaitingBuildFailure => {
                if external.state() == EditorExternalPlayState::Running {
                    return Err(self.state_error(
                        application.state(),
                        "the injected build failure was falsely admitted as Running",
                    ));
                }
                if external.state() != EditorExternalPlayState::Faulted {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                if !matches!(
                    frame.play().result(),
                    Some(EditorPlayOperationResult::Failed {
                        operation: EditorPlayOperation::Play,
                        ..
                    })
                ) || !external.last_good_available()
                    || external.generation().is_some()
                {
                    return Err(self.state_error(
                        application.state(),
                        "the injected build failure did not retain only explicit last-good authority",
                    ));
                }
                self.project.restore_desktop_source().map_err(|error| {
                    EditorRunError::Platform(format!(
                        "supervised Play canary: could not restore the guarded desktop source: {error}"
                    ))
                })?;
                self.transition(SupervisedPlayTestStage::AwaitingLastGoodReady);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::RunLastGood,
                )))
            }
            SupervisedPlayTestStage::AwaitingLastGoodReady => {
                if matches!(
                    frame.play().result(),
                    Some(
                        EditorPlayOperationResult::Rejected {
                            operation: EditorPlayOperation::RunLastGood,
                            ..
                        } | EditorPlayOperationResult::Failed {
                            operation: EditorPlayOperation::RunLastGood,
                            ..
                        } | EditorPlayOperationResult::Cancelled {
                            operation: EditorPlayOperation::RunLastGood,
                        }
                    )
                ) {
                    return Err(self.state_error(
                        application.state(),
                        "the explicit Run Last Good request did not start",
                    ));
                }
                if external.state() != EditorExternalPlayState::Running {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                let generation = external.generation().ok_or_else(|| {
                    self.state_error(
                        application.state(),
                        "Run Last Good reached Running without a generation",
                    )
                })?;
                if Some(generation) != self.first_generation {
                    return Err(self.state_error(
                        application.state(),
                        "Run Last Good did not relaunch the admitted immutable generation",
                    ));
                }
                self.last_good_generation = Some(generation);
                self.transition(SupervisedPlayTestStage::AwaitingLastGoodStop);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayTestStage::AwaitingLastGoodStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                self.transition(SupervisedPlayTestStage::AwaitingSuccessorEdit);
                Ok(SupervisedPlayTestAdvance::redraw())
            }
            SupervisedPlayTestStage::AwaitingSuccessorEdit => {
                let document = self.require_document(application.state())?;
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(application.state(), "the Scene disappeared after Stop")
                })?;
                if scene.document != document {
                    return Err(self.state_error(
                        application.state(),
                        "the active Scene changed before the successor edit",
                    ));
                }
                let (_, action) = edit_action(
                    &frame,
                    SUCCESSOR_EDITED_WEAPON_X,
                    application.state(),
                    "the successor saved edit",
                )?;
                self.transition(SupervisedPlayTestStage::AwaitingSuccessorSave);
                Ok(SupervisedPlayTestAdvance::action(action))
            }
            SupervisedPlayTestStage::AwaitingSuccessorSave => {
                let document = self.require_document(application.state())?;
                let action = save_action(
                    &frame,
                    document,
                    application.state(),
                    "the successor saved edit",
                )?;
                self.transition(SupervisedPlayTestStage::AwaitingSuccessorSaveReceipt);
                Ok(SupervisedPlayTestAdvance::action(action))
            }
            SupervisedPlayTestStage::AwaitingSuccessorSaveReceipt => {
                let document = self.require_document(application.state())?;
                let Some(revision) = observe_saved_revision(
                    &frame,
                    document,
                    application.state(),
                    "the successor saved edit",
                )?
                else {
                    return Ok(SupervisedPlayTestAdvance::idle());
                };
                if self
                    .first_saved_revision
                    .is_some_and(|first| revision <= first)
                {
                    return Err(self.state_error(
                        application.state(),
                        "the successor Save did not advance the Scene revision",
                    ));
                }
                self.successor_saved_revision = Some(revision);
                self.transition(SupervisedPlayTestStage::AwaitingSuccessorSaveAcknowledgement);
                Ok(SupervisedPlayTestAdvance::action(
                    EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                        attempt: frame
                            .persistence()
                            .attempt()
                            .expect("the observed Save is terminal"),
                    }),
                ))
            }
            SupervisedPlayTestStage::AwaitingSuccessorSaveAcknowledgement => {
                if frame.persistence().result().is_some() {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(
                        application.state(),
                        "the Scene disappeared before successor Play",
                    )
                })?;
                self.transition(SupervisedPlayTestStage::AwaitingSuccessorPlay);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayTestStage::AwaitingSuccessorPlay => match frame.play().result() {
                Some(result) if is_accepted(result, EditorPlayOperation::Play) => {
                    self.transition(SupervisedPlayTestStage::AwaitingSuccessorReady);
                    Ok(SupervisedPlayTestAdvance::redraw())
                }
                Some(EditorPlayOperationResult::Rejected { operation, .. })
                | Some(EditorPlayOperationResult::Failed { operation, .. })
                | Some(EditorPlayOperationResult::Cancelled { operation })
                    if operation == EditorPlayOperation::Play =>
                {
                    Err(self.state_error(
                        application.state(),
                        "the successor external Play request was not accepted",
                    ))
                }
                _ => Ok(SupervisedPlayTestAdvance::idle()),
            },
            SupervisedPlayTestStage::AwaitingSuccessorReady => {
                if external.state() == EditorExternalPlayState::Faulted {
                    return Err(self.state_error(
                        application.state(),
                        "the successor external Play attempt faulted before READY",
                    ));
                }
                if external.state() != EditorExternalPlayState::Running {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                let generation = external.generation().ok_or_else(|| {
                    self.state_error(
                        application.state(),
                        "the successor READY omitted its generation",
                    )
                })?;
                if Some(generation) == self.first_generation {
                    return Err(self.state_error(
                        application.state(),
                        "successor Play reused the first immutable generation",
                    ));
                }
                self.successor_generation = Some(generation);
                self.successor_output_sequence = external.output_sequence();
                self.successor_output_truncated = external.output_truncated();
                self.transition(SupervisedPlayTestStage::AwaitingFinalStop);
                Ok(SupervisedPlayTestAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayTestStage::AwaitingFinalStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(SupervisedPlayTestAdvance::idle());
                }
                self.transition(SupervisedPlayTestStage::Complete);
                Ok(SupervisedPlayTestAdvance::request_exit())
            }
            SupervisedPlayTestStage::Complete => Ok(SupervisedPlayTestAdvance::idle()),
            SupervisedPlayTestStage::ProjectManager => Err(self.state_error(
                application.state(),
                "the supervised Play canary reached an invalid workspace stage",
            )),
        }
    }

    pub(super) fn finish(self) -> Result<SupervisedPlayTestEvidence, EditorRunError> {
        if self.stage != SupervisedPlayTestStage::Complete {
            return Err(self.state_error(
                EditorApplicationState::ProjectManager,
                "the native supervised Play trajectory did not reach clean close",
            ));
        }
        Ok(SupervisedPlayTestEvidence {
            first_saved_revision: self.first_saved_revision.ok_or_else(|| {
                self.state_error(
                    EditorApplicationState::ProjectManager,
                    "the first saved source revision evidence is missing",
                )
            })?,
            successor_saved_revision: self.successor_saved_revision.ok_or_else(|| {
                self.state_error(
                    EditorApplicationState::ProjectManager,
                    "the successor saved source revision evidence is missing",
                )
            })?,
            first_generation: self.first_generation.ok_or_else(|| {
                self.state_error(
                    EditorApplicationState::ProjectManager,
                    "the first generation evidence is missing",
                )
            })?,
            last_good_generation: self.last_good_generation.ok_or_else(|| {
                self.state_error(
                    EditorApplicationState::ProjectManager,
                    "the explicit last-good generation evidence is missing",
                )
            })?,
            successor_generation: self.successor_generation.ok_or_else(|| {
                self.state_error(
                    EditorApplicationState::ProjectManager,
                    "the successor generation evidence is missing",
                )
            })?,
            first_output_sequence: self.first_output_sequence,
            successor_output_sequence: self.successor_output_sequence,
            first_output_truncated: self.first_output_truncated,
            successor_output_truncated: self.successor_output_truncated,
        })
    }

    fn require_before_deadline(
        &self,
        application: &EditorApplication,
    ) -> Result<(), EditorRunError> {
        if Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(self.timeout_error(application))
        }
    }

    pub(super) fn timeout_error(&self, application: &EditorApplication) -> EditorRunError {
        let external_detail = application.current_frame().map_or_else(
            || "external=unavailable".to_owned(),
            |frame| {
                frame.play().external().map_or_else(
                    || "external=unavailable".to_owned(),
                    |external| {
                        format!(
                            "external={:?}, generation={:?}, output_sequence={}, output_truncated={}",
                            external.state(),
                            external.generation(),
                            external.output_sequence(),
                            external.output_truncated(),
                        )
                    },
                )
            },
        );
        let result_detail = application
            .current_frame()
            .and_then(|frame| frame.play().result())
            .map_or_else(
                || "result=None".to_owned(),
                |result| format!("result={result:?}"),
            );
        EditorRunError::Platform(format!(
            "supervised Play canary: deadline exceeded; stage={:?}; state={:?}; {external_detail}; {result_detail}",
            self.stage,
            application.state(),
        ))
    }

    fn state_error(&self, state: EditorApplicationState, detail: &'static str) -> EditorRunError {
        EditorRunError::Platform(format!(
            "supervised Play canary: {detail}; stage={:?}; state={state:?}",
            self.stage,
        ))
    }

    fn require_document(
        &self,
        state: EditorApplicationState,
    ) -> Result<EditorDocumentId, EditorRunError> {
        self.document.ok_or_else(|| {
            self.state_error(
                state,
                "the supervised Play canary lost its edited Scene document",
            )
        })
    }
}

fn edit_action(
    frame: &EditorFrameModel,
    translation_x: f64,
    state: EditorApplicationState,
    edit_name: &'static str,
) -> Result<(EditorDocumentId, EditorAction), EditorRunError> {
    let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
        EditorRunError::Platform(format!(
            "supervised Play canary: {edit_name} has no active Scene; state={state:?}"
        ))
    })?;
    if scene.dirty {
        return Err(EditorRunError::Platform(format!(
            "supervised Play canary: {edit_name} began from a dirty Scene; state={state:?}"
        )));
    }
    let entity = SceneEntityId::new("player-weapon").map_err(|_| {
        EditorRunError::Platform("supervised Play canary: invalid probe entity id".to_owned())
    })?;
    let value = ComponentValue::f64(translation_x).map_err(|_| {
        EditorRunError::Platform("supervised Play canary: invalid probe field value".to_owned())
    })?;
    let edit = AuthoringFieldEdit::single(
        entity,
        ComponentTypeId::new("nara.transform.Transform2d"),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("translation.x"),
        value,
    )
    .map_err(|_| {
        EditorRunError::Platform("supervised Play canary: invalid probe field edit".to_owned())
    })?;
    let mutation = EditorDocumentMutation::ApplyAuthoringFieldEdit {
        document: scene.document,
        base_revision: scene.revision,
        edit,
    };
    Ok((scene.document, EditorAction::Mutate(mutation)))
}

fn save_action(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    state: EditorApplicationState,
    edit_name: &'static str,
) -> Result<EditorAction, EditorRunError> {
    let tab = frame.workspace().document(document).ok_or_else(|| {
        EditorRunError::Platform(format!(
            "supervised Play canary: {edit_name} lost its Scene before Save; state={state:?}"
        ))
    })?;
    if !tab.dirty {
        return Err(EditorRunError::Platform(format!(
            "supervised Play canary: {edit_name} did not dirty the Scene; state={state:?}"
        )));
    }
    Ok(EditorAction::Persistence(EditorPersistenceAction::Save {
        document,
        base_revision: tab.revision,
    }))
}

fn observe_saved_revision(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    state: EditorApplicationState,
    edit_name: &'static str,
) -> Result<Option<u64>, EditorRunError> {
    match frame.persistence().result() {
        Some(EditorPersistenceResult::Saved {
            document: saved,
            revision,
            ..
        }) if saved == document => {
            let tab = frame.workspace().document(document).ok_or_else(|| {
                EditorRunError::Platform(format!(
                    "supervised Play canary: {edit_name} lost its Scene after Save; state={state:?}"
                ))
            })?;
            if tab.dirty || tab.saved_revision != tab.revision || tab.revision != revision {
                return Err(EditorRunError::Platform(format!(
                    "supervised Play canary: {edit_name} did not publish an exact clean checkpoint; state={state:?}"
                )));
            }
            Ok(Some(revision.generation()))
        }
        Some(
            EditorPersistenceResult::Rejected { .. }
            | EditorPersistenceResult::Failed { .. }
            | EditorPersistenceResult::PersistenceUncertain { .. }
            | EditorPersistenceResult::Opened { .. }
            | EditorPersistenceResult::Saved { .. },
        ) => Err(EditorRunError::Platform(format!(
            "supervised Play canary: {edit_name} did not complete safely; state={state:?}"
        ))),
        None => Ok(None),
    }
}

fn is_applied(result: Option<EditorPlayOperationResult>, operation: EditorPlayOperation) -> bool {
    matches!(
        result,
        Some(EditorPlayOperationResult::Applied {
            operation: observed,
            ..
        }) if observed == operation
    )
}

fn is_accepted(result: EditorPlayOperationResult, operation: EditorPlayOperation) -> bool {
    matches!(
        result,
        EditorPlayOperationResult::Pending {
            operation: observed,
        } | EditorPlayOperationResult::Applied {
            operation: observed,
            ..
        } if observed == operation
    )
}
