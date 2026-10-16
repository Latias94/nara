use std::{
    env,
    ffi::OsStr,
    time::{Duration, Instant},
};

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

use super::{EditorRunError, NativeEditorCanaryAdvance};

mod fixture;

use fixture::GuardedSupervisedPlayProject;

const SUPERVISED_PLAY_CANARY_ENV: &str = "NARA_EDITOR_SUPERVISED_PLAY_CANARY";
const SUPERVISED_PLAY_CANARY_DEADLINE: Duration = Duration::from_secs(600);
const FIRST_EDITED_WEAPON_X: f64 = 2.75;
const SUCCESSOR_EDITED_WEAPON_X: f64 = 3.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupervisedPlayCanaryStage {
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
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorSupervisedPlayCanaryEvidence {
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

impl EditorSupervisedPlayCanaryEvidence {
    #[must_use]
    pub const fn first_saved_revision(self) -> u64 {
        self.first_saved_revision
    }

    #[must_use]
    pub const fn successor_saved_revision(self) -> u64 {
        self.successor_saved_revision
    }

    #[must_use]
    pub const fn first_generation(self) -> u64 {
        self.first_generation
    }

    #[must_use]
    pub const fn successor_generation(self) -> u64 {
        self.successor_generation
    }

    #[must_use]
    pub const fn last_good_generation(self) -> u64 {
        self.last_good_generation
    }

    #[must_use]
    pub const fn first_output_sequence(self) -> u64 {
        self.first_output_sequence
    }

    #[must_use]
    pub const fn successor_output_sequence(self) -> u64 {
        self.successor_output_sequence
    }

    #[must_use]
    pub const fn first_output_truncated(self) -> bool {
        self.first_output_truncated
    }

    #[must_use]
    pub const fn successor_output_truncated(self) -> bool {
        self.successor_output_truncated
    }
}

pub(super) fn requested() -> bool {
    env::var_os(SUPERVISED_PLAY_CANARY_ENV).as_deref() == Some(OsStr::new("1"))
}

pub(super) struct SupervisedPlayCanaryHost {
    project: GuardedSupervisedPlayProject,
    stage: SupervisedPlayCanaryStage,
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

impl SupervisedPlayCanaryHost {
    pub(super) fn new(
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, Self), EditorRunError> {
        let source_root = options.project_root().ok_or_else(|| {
            EditorRunError::usage(format!(
                "{SUPERVISED_PLAY_CANARY_ENV}=1 requires one source project root"
            ))
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
                stage: SupervisedPlayCanaryStage::ProjectManager,
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

    fn transition(&mut self, next: SupervisedPlayCanaryStage) {
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
            SupervisedPlayCanaryStage::ProjectManager => {
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
                self.transition(SupervisedPlayCanaryStage::AwaitingOpen);
                Ok(true)
            }
            SupervisedPlayCanaryStage::AwaitingOpen => Ok(false),
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
    ) -> Result<NativeEditorCanaryAdvance, EditorRunError> {
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
            SupervisedPlayCanaryStage::AwaitingOpen => {
                if matches!(
                    application.state(),
                    EditorApplicationState::AuthoringReady
                        | EditorApplicationState::AuthoringDegraded
                ) {
                    self.transition(SupervisedPlayCanaryStage::AwaitingFirstEdit);
                    Ok(NativeEditorCanaryAdvance::redraw())
                } else {
                    Ok(NativeEditorCanaryAdvance::idle())
                }
            }
            SupervisedPlayCanaryStage::AwaitingFirstEdit => {
                let (document, action) = edit_action(
                    &frame,
                    FIRST_EDITED_WEAPON_X,
                    application.state(),
                    "the first saved edit",
                )?;
                self.document = Some(document);
                self.transition(SupervisedPlayCanaryStage::AwaitingFirstSave);
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            SupervisedPlayCanaryStage::AwaitingFirstSave => {
                let document = self.require_document(application.state())?;
                let action = save_action(
                    &frame,
                    document,
                    application.state(),
                    "the first saved edit",
                )?;
                self.transition(SupervisedPlayCanaryStage::AwaitingFirstSaveReceipt);
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            SupervisedPlayCanaryStage::AwaitingFirstSaveReceipt => {
                let document = self.require_document(application.state())?;
                let Some(revision) = observe_saved_revision(
                    &frame,
                    document,
                    application.state(),
                    "the first saved edit",
                )?
                else {
                    return Ok(NativeEditorCanaryAdvance::idle());
                };
                self.first_saved_revision = Some(revision);
                self.transition(SupervisedPlayCanaryStage::AwaitingFirstSaveAcknowledgement);
                Ok(NativeEditorCanaryAdvance::action(
                    EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
                ))
            }
            SupervisedPlayCanaryStage::AwaitingFirstSaveAcknowledgement => {
                if frame.persistence().result().is_some() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                self.transition(SupervisedPlayCanaryStage::AwaitingPlay);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            SupervisedPlayCanaryStage::AwaitingPlay => {
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(application.state(), "the project has no startup Scene")
                })?;
                self.transition(SupervisedPlayCanaryStage::AwaitingReady);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayCanaryStage::AwaitingReady => {
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
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                let generation = external.generation().ok_or_else(|| {
                    self.state_error(application.state(), "READY did not publish a generation")
                })?;
                self.first_generation = Some(generation);
                self.first_output_sequence = external.output_sequence();
                self.first_output_truncated = external.output_truncated();
                self.transition(SupervisedPlayCanaryStage::AwaitingStop);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayCanaryStage::AwaitingStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(NativeEditorCanaryAdvance::idle());
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
                self.transition(SupervisedPlayCanaryStage::AwaitingBuildFailure);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayCanaryStage::AwaitingBuildFailure => {
                if external.state() == EditorExternalPlayState::Running {
                    return Err(self.state_error(
                        application.state(),
                        "the injected build failure was falsely admitted as Running",
                    ));
                }
                if external.state() != EditorExternalPlayState::Faulted {
                    return Ok(NativeEditorCanaryAdvance::idle());
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
                self.transition(SupervisedPlayCanaryStage::AwaitingLastGoodReady);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::RunLastGood,
                )))
            }
            SupervisedPlayCanaryStage::AwaitingLastGoodReady => {
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
                    return Ok(NativeEditorCanaryAdvance::idle());
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
                self.transition(SupervisedPlayCanaryStage::AwaitingLastGoodStop);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayCanaryStage::AwaitingLastGoodStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorEdit);
                Ok(NativeEditorCanaryAdvance::redraw())
            }
            SupervisedPlayCanaryStage::AwaitingSuccessorEdit => {
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
                self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorSave);
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            SupervisedPlayCanaryStage::AwaitingSuccessorSave => {
                let document = self.require_document(application.state())?;
                let action = save_action(
                    &frame,
                    document,
                    application.state(),
                    "the successor saved edit",
                )?;
                self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorSaveReceipt);
                Ok(NativeEditorCanaryAdvance::action(action))
            }
            SupervisedPlayCanaryStage::AwaitingSuccessorSaveReceipt => {
                let document = self.require_document(application.state())?;
                let Some(revision) = observe_saved_revision(
                    &frame,
                    document,
                    application.state(),
                    "the successor saved edit",
                )?
                else {
                    return Ok(NativeEditorCanaryAdvance::idle());
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
                self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorSaveAcknowledgement);
                Ok(NativeEditorCanaryAdvance::action(
                    EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
                ))
            }
            SupervisedPlayCanaryStage::AwaitingSuccessorSaveAcknowledgement => {
                if frame.persistence().result().is_some() {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                let scene = frame.workspace().active_scene.as_ref().ok_or_else(|| {
                    self.state_error(
                        application.state(),
                        "the Scene disappeared before successor Play",
                    )
                })?;
                self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorPlay);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )))
            }
            SupervisedPlayCanaryStage::AwaitingSuccessorPlay => match frame.play().result() {
                Some(result) if is_accepted(result, EditorPlayOperation::Play) => {
                    self.transition(SupervisedPlayCanaryStage::AwaitingSuccessorReady);
                    Ok(NativeEditorCanaryAdvance::redraw())
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
                _ => Ok(NativeEditorCanaryAdvance::idle()),
            },
            SupervisedPlayCanaryStage::AwaitingSuccessorReady => {
                if external.state() == EditorExternalPlayState::Faulted {
                    return Err(self.state_error(
                        application.state(),
                        "the successor external Play attempt faulted before READY",
                    ));
                }
                if external.state() != EditorExternalPlayState::Running {
                    return Ok(NativeEditorCanaryAdvance::idle());
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
                self.transition(SupervisedPlayCanaryStage::AwaitingFinalStop);
                Ok(NativeEditorCanaryAdvance::action(EditorAction::Play(
                    EditorPlayAction::Stop,
                )))
            }
            SupervisedPlayCanaryStage::AwaitingFinalStop => {
                if external.state() != EditorExternalPlayState::Idle {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                if !is_applied(frame.play().result(), EditorPlayOperation::Stop) {
                    return Ok(NativeEditorCanaryAdvance::idle());
                }
                self.transition(SupervisedPlayCanaryStage::Complete);
                Ok(NativeEditorCanaryAdvance::request_exit())
            }
            SupervisedPlayCanaryStage::Complete => Ok(NativeEditorCanaryAdvance::idle()),
            SupervisedPlayCanaryStage::ProjectManager => Err(self.state_error(
                application.state(),
                "the supervised Play canary reached an invalid workspace stage",
            )),
        }
    }

    pub(super) fn finish(self) -> Result<EditorSupervisedPlayCanaryEvidence, EditorRunError> {
        if self.stage != SupervisedPlayCanaryStage::Complete {
            return Err(self.state_error(
                EditorApplicationState::ProjectManager,
                "the native supervised Play trajectory did not reach clean close",
            ));
        }
        Ok(EditorSupervisedPlayCanaryEvidence {
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
