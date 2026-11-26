use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

use nara_app::tasks::TaskPools;
use nara_scene::current_scene::requests::ScenePreparation;

#[cfg(feature = "project-source")]
use super::ProjectContentErrorKind;
use super::{
    Context, Diagnostic, DiagnosticCode, DiagnosticReport, PreparedSceneContent,
    ProjectSceneRequestError, ProjectSceneSelection, RequestPurpose, SafeSummary,
    error_diagnostics,
};

#[derive(Debug)]
pub(super) struct PreparationFailure {
    pub(super) error: ProjectSceneRequestError,
    pub(super) diagnostics: DiagnosticReport,
    #[cfg(feature = "project-source")]
    pub(super) observations: Option<crate::project_content::ProjectSourceObservations>,
}

impl PreparationFailure {
    pub(super) fn unavailable() -> Self {
        let error = ProjectSceneRequestError::TaskUnavailable;
        Self {
            error,
            diagnostics: error_diagnostics(error),
            #[cfg(feature = "project-source")]
            observations: None,
        }
    }

    #[cfg(feature = "project-source")]
    fn source_content(
        error: crate::project_host::ProjectContentError,
        observations: crate::project_content::ProjectSourceObservations,
    ) -> Self {
        let undeclared = error.kind() == ProjectContentErrorKind::HostAuthorityRejected
            && error.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.content.scene-not-declared"
            });
        Self {
            error: if undeclared {
                ProjectSceneRequestError::UndeclaredScene
            } else {
                ProjectSceneRequestError::Content(error.kind())
            },
            diagnostics: error.diagnostics().clone(),
            observations: Some(observations),
        }
    }

    fn runtime_content(error: crate::runtime_content::RuntimeContentError) -> Self {
        let kind = ProjectSceneRequestError::RuntimeContent;
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::new(kind.code()).expect("engine-owned diagnostic code is valid"),
            SafeSummary::new(error.summary()).expect("Runtime content errors have safe summaries"),
        ));
        Self {
            error: kind,
            diagnostics,
            #[cfg(feature = "project-source")]
            observations: None,
        }
    }
}

pub(super) type Preparation = ScenePreparation<PreparedSceneContent, PreparationFailure>;

pub(super) fn start_preparation(
    context: Arc<Context>,
    selection: impl Into<ProjectSceneSelection>,
    purpose: RequestPurpose,
    pools: &TaskPools,
    #[cfg(test)] mut gate: Option<TestGate>,
) -> Result<Preparation, ProjectSceneRequestError> {
    let selection = selection.into();
    ScenePreparation::spawn(pools, move |cancellation| {
        #[cfg(test)]
        wait_at(&mut gate, TestPhase::BeforeRead);
        if cancellation.is_cancelled() {
            return None;
        }
        // Startup already established executable schema agreement. Ordinary targets use the
        // same frozen native plan; they are not new startup attestations or schema owners.
        let result = match (context.as_ref(), &selection) {
            #[cfg(feature = "project-source")]
            (
                Context::Source {
                    candidate,
                    plan,
                    loader,
                    input,
                },
                selection,
            ) => {
                let attempt = match (selection, &purpose) {
                    (ProjectSceneSelection::Source(path), RequestPurpose::Travel) => loader
                        .load_scene_observed_reusing_input(candidate, plan, path, input.as_ref()),
                    (ProjectSceneSelection::Product(product), RequestPurpose::Travel) => loader
                        .load_scene_product_observed_reusing_input(
                            candidate,
                            plan,
                            *product,
                            input.as_ref(),
                        ),
                    (
                        ProjectSceneSelection::Product(product),
                        RequestPurpose::AdditiveSource(path),
                    ) => loader.load_deferred_scene_observed_reusing_input(
                        candidate,
                        plan,
                        *product,
                        path,
                        input.as_ref(),
                    ),
                    _ => unreachable!("source request purpose is selected by the Host"),
                };
                attempt
                    .result
                    .map_err(|error| {
                        PreparationFailure::source_content(error, attempt.observations)
                    })
                    .and_then(|snapshot| {
                        // Required dependencies are data, not public travel roots.
                        if !purpose.is_additive()
                            && !context.declares_source_path(snapshot.selected_scene_path())
                        {
                            let error = ProjectSceneRequestError::UndeclaredScene;
                            return Err(PreparationFailure {
                                error,
                                diagnostics: error_diagnostics(error),
                                observations: None,
                            });
                        }
                        Ok(PreparedSceneContent::Source(snapshot))
                    })
            }
            (Context::Processed { base }, ProjectSceneSelection::Product(product)) => {
                let prepared = if matches!(purpose, RequestPurpose::AdditiveProcessed) {
                    base.prepare_scene_closure(*product)
                } else {
                    base.prepare_scene_content(*product)
                };
                prepared
                    .map(PreparedSceneContent::Processed)
                    .map_err(PreparationFailure::runtime_content)
            }
            #[cfg(feature = "project-source")]
            (Context::Processed { .. }, ProjectSceneSelection::Source(_)) => {
                let error = ProjectSceneRequestError::UndeclaredScene;
                Err(PreparationFailure {
                    error,
                    diagnostics: error_diagnostics(error),
                    observations: None,
                })
            }
        };
        #[cfg(test)]
        wait_at(&mut gate, TestPhase::AfterPreparation);
        Some(result)
    })
    .ok_or(ProjectSceneRequestError::TaskUnavailable)
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TestPhase {
    BeforeRead,
    AfterPreparation,
}

#[cfg(test)]
pub(super) struct TestGate {
    phase: TestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    panic_after_release: bool,
}

#[cfg(test)]
pub(super) struct TestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl TestGate {
    pub(super) fn new(phase: TestPhase) -> (Self, TestControl) {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        (
            Self {
                phase,
                started: started_tx,
                release: Mutex::new(release_rx),
                panic_after_release: false,
            },
            TestControl {
                started,
                release: Some(release),
            },
        )
    }

    #[cfg(feature = "project-source")]
    pub(super) fn panicking(phase: TestPhase) -> (Self, TestControl) {
        let (mut gate, control) = Self::new(phase);
        gate.panic_after_release = true;
        (gate, control)
    }
}

#[cfg(test)]
impl TestControl {
    pub(super) fn wait_until_started(&self) -> bool {
        self.started
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    }
    pub(super) fn release(mut self) {
        self.release.take();
    }
    #[cfg(feature = "project-source")]
    pub(super) fn into_channels(
        mut self,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        (
            self.started,
            self.release
                .take()
                .expect("the test gate has one release sender"),
        )
    }
}

#[cfg(test)]
fn wait_at(gate: &mut Option<TestGate>, phase: TestPhase) {
    if gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        let gate = gate.take().expect("matching test gate");
        let _ = gate.started.send(());
        let _ = gate.release.lock().unwrap().recv();
        assert!(
            !gate.panic_after_release,
            "project Scene preparation panic probe"
        );
    }
}
