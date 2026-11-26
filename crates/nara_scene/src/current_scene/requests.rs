//! Preparation and explicit publication of one change to a World's active Scenes.

mod preparation;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use nara_app::RuntimeGeneration;
use nara_core::diagnostic::DiagnosticReport;
use nara_ecs::World;

use super::{CurrentScene, SceneSpawnReport, SpawnedSceneInstance, rejected};

pub use preparation::ScenePreparation;

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

/// Identity of a request made against a specific runtime generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneRequestId {
    generation: RuntimeGeneration,
    sequence: u64,
}

impl SceneRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.sequence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneRequestError<E> {
    Unavailable,
    Busy,
    UndeclaredScene,
    TaskUnavailable,
    IdExhausted,
    Stale,
    Preparation(E),
    CommitRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneRequestState<E> {
    Preparing,
    /// The request is closed to publication. Worker captures are still returning.
    Cancelling,
    Ready,
    Cancelled,
    Rejected(SceneRequestError<E>),
    Committed,
}

/// The content-specific rules for a Scene request. The Scene owner independently checks the
/// active Scene receipt, runtime generation and source instance at observation and publication.
pub trait SceneRequestSource {
    type Target;
    type Content;
    type Error: Copy;
    type Failure;

    fn admits(&self, target: &Self::Target) -> bool;
    fn matches_current(&self, current: &CurrentScene) -> bool;
    fn matches_prepared(&self, target: &Self::Target, content: &Self::Content) -> bool;
}

struct Admission<S: SceneRequestSource> {
    source: Arc<S>,
    id: SceneRequestId,
    expected: SpawnedSceneInstance,
    target: S::Target,
}

#[derive(Clone, Copy)]
enum Discard {
    Cancelled,
    Stale,
}

enum Slot<S: SceneRequestSource> {
    Preparing {
        admission: Admission<S>,
        preparation: ScenePreparation<S::Content, S::Failure>,
        discard: Option<Discard>,
    },
    Ready {
        admission: Admission<S>,
        content: S::Content,
    },
    Terminal {
        id: SceneRequestId,
        state: SceneRequestState<S::Error>,
        diagnostics: DiagnosticReport,
    },
}

impl<S: SceneRequestSource> Slot<S> {
    fn id(&self) -> SceneRequestId {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => admission.id,
            Self::Terminal { id, .. } => *id,
        }
    }

    fn admission(&self) -> Option<&Admission<S>> {
        match self {
            Self::Preparing { admission, .. } | Self::Ready { admission, .. } => Some(admission),
            Self::Terminal { .. } => None,
        }
    }

    fn stale(id: SceneRequestId) -> Self {
        Self::Terminal {
            id,
            state: SceneRequestState::Rejected(SceneRequestError::Stale),
            diagnostics: rejected(
                "project.scene.request-stale",
                "Project Scene request was rejected",
            )
            .diagnostics,
        }
    }

    fn cancelled(id: SceneRequestId) -> Self {
        Self::Terminal {
            id,
            state: SceneRequestState::Cancelled,
            diagnostics: DiagnosticReport::default(),
        }
    }
}

/// Single pending, ready or unacknowledged Scene request for an admitted content source.
///
/// Preparation never mutates the World. A ready result is published only by an explicit commit
/// against the original active Scene. A cancelled worker must return before its result can be
/// acknowledged and the slot reused.
pub struct SceneRequests<S: SceneRequestSource> {
    source: Arc<S>,
    closing: Arc<AtomicBool>,
    slot: Option<Slot<S>>,
}

impl<S: SceneRequestSource> SceneRequests<S> {
    pub fn new(source: S, closing: Arc<AtomicBool>) -> Self {
        Self {
            source: Arc::new(source),
            closing,
            slot: None,
        }
    }

    #[must_use]
    pub fn source(&self) -> &Arc<S> {
        &self.source
    }

    /// Replaces the admitted source. Preparation captured from the previous source becomes stale.
    pub fn replace_source(&mut self, source: S) {
        self.source = Arc::new(source);
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn is_occupied(&self) -> bool {
        self.slot.is_some()
    }

    #[must_use]
    pub fn active_request(&self) -> Option<SceneRequestId> {
        self.slot
            .as_ref()?
            .admission()
            .map(|admission| admission.id)
    }

    pub fn request(
        &mut self,
        current: &CurrentScene,
        generation: RuntimeGeneration,
        target: S::Target,
        start: impl FnOnce(Arc<S>, &S::Target) -> Option<ScenePreparation<S::Content, S::Failure>>,
    ) -> Result<SceneRequestId, SceneRequestError<S::Error>> {
        if !self.source.matches_current(current) {
            return Err(SceneRequestError::Stale);
        }
        if self.is_closing() {
            return Err(SceneRequestError::Unavailable);
        }
        if self.is_occupied() {
            return Err(SceneRequestError::Busy);
        }
        if !self.source.admits(&target) {
            return Err(SceneRequestError::UndeclaredScene);
        }
        let sequence = NEXT_REQUEST
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| SceneRequestError::IdExhausted)?;
        let id = SceneRequestId {
            generation,
            sequence,
        };
        let preparation =
            start(Arc::clone(&self.source), &target).ok_or(SceneRequestError::TaskUnavailable)?;
        self.slot = Some(Slot::Preparing {
            admission: Admission {
                source: Arc::clone(&self.source),
                id,
                expected: current.receipt().clone(),
                target,
            },
            preparation,
            discard: None,
        });
        Ok(id)
    }

    pub fn cancel(&mut self, id: SceneRequestId) -> bool {
        match self.slot.as_mut().filter(|slot| slot.id() == id) {
            Some(Slot::Preparing {
                preparation,
                discard,
                ..
            }) if discard.is_none() => {
                *discard = Some(Discard::Cancelled);
                preparation.cancel();
                true
            }
            Some(Slot::Ready { .. }) => {
                self.slot = Some(Slot::cancelled(id));
                true
            }
            _ => false,
        }
    }

    /// Revokes publication and asks the task to return. The owning task pool drains on shutdown.
    pub fn close(&self) {
        self.closing.store(true, Ordering::Release);
        if let Some(Slot::Preparing { preparation, .. }) = &self.slot {
            preparation.cancel();
        }
    }

    #[must_use]
    pub fn state(
        &self,
        id: SceneRequestId,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentScene>,
    ) -> Option<SceneRequestState<S::Error>> {
        let slot = self.slot.as_ref().filter(|slot| slot.id() == id)?;
        Some(match slot {
            Slot::Preparing {
                admission, discard, ..
            } => {
                if discard.is_some() || !self.matches(admission, generation, current) {
                    SceneRequestState::Cancelling
                } else {
                    SceneRequestState::Preparing
                }
            }
            Slot::Ready { admission, .. } => {
                if self.matches(admission, generation, current) {
                    SceneRequestState::Ready
                } else {
                    SceneRequestState::Rejected(SceneRequestError::Stale)
                }
            }
            Slot::Terminal { state, .. } => *state,
        })
    }

    #[must_use]
    pub fn prepared(
        &self,
        id: SceneRequestId,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentScene>,
    ) -> Option<(&S::Target, &SpawnedSceneInstance, &S::Content)> {
        let Slot::Ready { admission, content } = self.slot.as_ref()? else {
            return None;
        };
        (admission.id == id
            && self.matches(admission, generation, current)
            && self.source.matches_prepared(&admission.target, content))
        .then_some((&admission.target, &admission.expected, content))
    }

    #[must_use]
    pub fn diagnostics(&self, id: SceneRequestId) -> Option<&DiagnosticReport> {
        match self.slot.as_ref()? {
            Slot::Terminal {
                id: terminal,
                diagnostics,
                ..
            } if *terminal == id => Some(diagnostics),
            _ => None,
        }
    }

    pub fn acknowledge(
        &mut self,
        id: SceneRequestId,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentScene>,
    ) -> bool {
        let terminal = match &self.slot {
            Some(Slot::Ready { admission, .. }) if admission.id == id => {
                !self.matches(admission, generation, current)
            }
            Some(Slot::Terminal { id: terminal, .. }) => *terminal == id,
            _ => false,
        };
        if terminal {
            self.slot = None;
        }
        terminal
    }

    /// Observes physical worker return. Source-specific preparation failures can update the
    /// caller's source observations before their compact diagnostics become the terminal result.
    pub fn poll(
        &mut self,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentScene>,
        task_unavailable: impl FnOnce() -> S::Failure,
        on_failure: impl FnOnce(SceneRequestId, S::Failure) -> (S::Error, DiagnosticReport),
    ) {
        let Some(mut slot) = self.slot.take() else {
            return;
        };
        let valid = slot
            .admission()
            .is_none_or(|admission| self.matches(admission, generation, current));
        if let Slot::Preparing {
            admission,
            preparation,
            discard,
        } = &mut slot
        {
            if !valid && discard.is_none() {
                *discard = Some(Discard::Stale);
                preparation.cancel();
            }
            if let Some(result) = preparation.poll(task_unavailable) {
                let discarded = discard.take();
                let id = admission.id;
                slot = match (discarded, result) {
                    (Some(Discard::Cancelled), _) => Slot::cancelled(id),
                    (Some(Discard::Stale), _) => Slot::stale(id),
                    (None, Ok(content)) => {
                        let Slot::Preparing { admission, .. } = slot else {
                            unreachable!()
                        };
                        if self.source.matches_prepared(&admission.target, &content) {
                            Slot::Ready { admission, content }
                        } else {
                            Slot::stale(id)
                        }
                    }
                    (None, Err(failure)) => {
                        let (error, diagnostics) = on_failure(id, failure);
                        Slot::Terminal {
                            id,
                            state: SceneRequestState::Rejected(SceneRequestError::Preparation(
                                error,
                            )),
                            diagnostics,
                        }
                    }
                };
            }
        } else if !valid {
            slot = Slot::stale(slot.id());
        }
        self.slot = Some(slot);
    }

    /// Publishes one ready Scene request at an exclusive World boundary. The publication
    /// operation supplies the staged Scene and any domain resource transaction it depends on.
    pub fn commit(
        &mut self,
        world: &mut World,
        id: SceneRequestId,
        matches_purpose: impl FnOnce(&S::Target) -> bool,
        publish: impl FnOnce(&mut World, &SpawnedSceneInstance, &S::Content) -> SceneSpawnReport,
    ) -> SceneSpawnReport {
        let Some(slot) = self.slot.as_ref().filter(|slot| slot.id() == id) else {
            return rejected(
                "project.scene.request-mismatch",
                "Project Scene request identity does not match",
            );
        };
        let valid = slot.admission().is_some_and(|admission| {
            self.matches(
                admission,
                world.get_resource::<RuntimeGeneration>().copied(),
                CurrentScene::get(world),
            )
        });
        if !valid {
            match &mut self.slot {
                Some(Slot::Preparing {
                    preparation,
                    discard,
                    ..
                }) if discard.is_none() => {
                    *discard = Some(Discard::Stale);
                    preparation.cancel();
                }
                Some(Slot::Ready { .. }) => self.slot = Some(Slot::stale(id)),
                _ => {}
            }
            return rejected(
                "project.scene.request-stale",
                "Project Scene request inputs changed before commit",
            );
        }
        let Slot::Ready { admission, .. } = slot else {
            return rejected(
                "project.scene.request-not-ready",
                "Project Scene preparation has not returned ready content",
            );
        };
        if !matches_purpose(&admission.target) {
            return rejected(
                "project.scene.request-purpose-mismatch",
                "Scene request must be published through the operation that admitted it",
            );
        }
        let Some(Slot::Ready { admission, content }) = self.slot.take() else {
            unreachable!()
        };
        let report = if self.source.matches_prepared(&admission.target, &content) {
            publish(world, &admission.expected, &content)
        } else {
            rejected(
                "project.scene.prepared-input-mismatch",
                "Prepared Scene does not match its captured Host authority",
            )
        };
        self.slot = Some(Slot::Terminal {
            id,
            state: if !report.diagnostics.has_errors() && report.instance.is_some() {
                SceneRequestState::Committed
            } else {
                SceneRequestState::Rejected(SceneRequestError::CommitRejected)
            },
            diagnostics: report.diagnostics.clone(),
        });
        report
    }

    fn matches(
        &self,
        admission: &Admission<S>,
        generation: Option<RuntimeGeneration>,
        current: Option<&CurrentScene>,
    ) -> bool {
        !self.is_closing()
            && generation == Some(admission.id.generation)
            && Arc::ptr_eq(&self.source, &admission.source)
            && current.is_some_and(|current| {
                current.receipt() == &admission.expected && self.source.matches_current(current)
            })
    }
}
