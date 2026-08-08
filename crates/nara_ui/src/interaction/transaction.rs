use std::collections::{BTreeMap, BTreeSet};

use nara_ecs::{Mut, Resource, World};
use nara_input::{InputTimeline, InputUiCursorError, InputUiDisposition};
use nara_ui_kernel::{
    UiDispatchBatch, UiDispatchError, UiInputEvent, UiInteractionSession, UiJournalError, UiKey,
    UiNonActivation, UiRouteStamp, UiSurfaceId,
};
use nara_ui_widgets::{UiButtonState, UiIntent, UiIntentBatch};
use thiserror::Error;

use crate::runtime::{UiAdapterError, UiRuntimeState};

use super::journal::{UiActionAdmissionError, UiActionDraft, UiActionJournal};

const MAX_UI_ROUTE_REJECTIONS: usize = 64;

pub(crate) fn execute(
    world: &mut World,
    route: UiRouteStamp,
    input: UiInputEvent,
    input_sequence: Option<u64>,
) -> UiRouteDisposition {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        world.resource_scope(|world, mut journal: Mut<UiActionJournal>| {
            let transaction = match UiInteractionTransaction::prepare(
                &state,
                &journal,
                route,
                input,
                input_sequence,
            ) {
                Ok(transaction) => transaction,
                Err(rejection) => {
                    let rejection = input_sequence.map_or(rejection.clone(), |sequence| {
                        reject_timeline_entry(world, sequence, rejection)
                    });
                    return UiRouteDisposition::RejectedClosed(rejection);
                }
            };
            let input_disposition = if transaction.is_rejected() {
                InputUiDisposition::RejectedClosed
            } else {
                InputUiDisposition::Consumed
            };
            if let Some(sequence) = input_sequence
                && let Err(error) = world
                    .resource_mut::<InputTimeline>()
                    .__finalize_ui_entry(sequence, input_disposition)
            {
                return UiRouteDisposition::RejectedClosed(UiRouteRejection::InputFinalization(
                    error,
                ));
            }
            transaction.commit(&mut state, &mut journal)
        })
    })
}

pub(crate) fn reject_timeline_entry(
    world: &mut World,
    sequence: u64,
    rejection: UiRouteRejection,
) -> UiRouteRejection {
    match world
        .resource_mut::<InputTimeline>()
        .__finalize_ui_entry(sequence, InputUiDisposition::RejectedClosed)
    {
        Ok(()) => rejection,
        Err(error) => UiRouteRejection::InputFinalization(error),
    }
}

/// Why a normalized event could not enter one exact presented UI transaction.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiRouteRejection {
    #[error("UI runtime adapter is not installed")]
    RuntimeUnavailable,
    #[error("UI route stamp is incomplete or malformed")]
    InvalidRouteStamp,
    #[error("UI input projection failed: {0}")]
    InvalidProjection(UiInputProjectionIssue),
    #[error("UI route endpoint does not match the retained presented session")]
    RouteEndpointMismatch,
    #[error("the active modal UI session blocks an input kind it does not handle")]
    UnhandledInputBlocked,
    #[error("UI surface {surface} is unknown")]
    UnknownSurface { surface: u64 },
    #[error("UI surface {surface} was retired")]
    RetiredSurface { surface: u64 },
    #[error(transparent)]
    PresentedHistory(#[from] UiJournalError),
    #[error(
        "surface {surface} presentation epoch {presentation_epoch} is missing behavior for {target:?}"
    )]
    PresentedBehaviorMissing {
        surface: u64,
        presentation_epoch: u64,
        target: Option<UiKey>,
    },
    #[error("UI kernel rejected activation: {0:?}")]
    NonActivation(UiNonActivation),
    #[error(transparent)]
    Dispatch(#[from] UiDispatchError),
    #[error("UI widget reducer rejected the transaction: {0}")]
    Widget(String),
    #[error(transparent)]
    IntentJournal(#[from] UiActionAdmissionError),
    #[error("input timeline finalization failed: {0}")]
    InputFinalization(InputUiCursorError),
}

/// Stable categories for adapter-level input projection failures.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum UiInputProjectionIssue {
    #[error("a pointer event has no pointer identity")]
    MissingPointer,
    #[error("a pointer move has no logical position")]
    MissingPosition,
    #[error("a pointer position is non-finite")]
    NonFinitePosition,
}

/// Typed result of routing one event at the runtime UI boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRouteDisposition {
    PassThrough,
    Consumed(UiDispatchBatch),
    RejectedClosed(UiRouteRejection),
}

/// One bounded fail-closed routing fact retained for diagnostics and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiRouteRejectionRecord {
    input_sequence: Option<u64>,
    rejection: UiRouteRejection,
}

impl UiRouteRejectionRecord {
    #[must_use]
    pub const fn input_sequence(&self) -> Option<u64> {
        self.input_sequence
    }

    #[must_use]
    pub const fn rejection(&self) -> &UiRouteRejection {
        &self.rejection
    }
}

/// Bounded newest-first rejection log; it carries no gameplay or callback authority.
#[derive(Debug, Default, Resource)]
pub struct UiRouteRejectionLog {
    entries: Vec<UiRouteRejectionRecord>,
}

impl UiRouteRejectionLog {
    #[must_use]
    pub fn as_slice(&self) -> &[UiRouteRejectionRecord] {
        &self.entries
    }

    pub(crate) fn record(&mut self, input_sequence: Option<u64>, rejection: UiRouteRejection) {
        if self.entries.len() == MAX_UI_ROUTE_REJECTIONS {
            self.entries.pop();
        }
        self.entries.insert(
            0,
            UiRouteRejectionRecord {
                input_sequence,
                rejection,
            },
        );
    }
}

pub(crate) struct UiInteractionTransaction {
    surface_id: UiSurfaceId,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
    journal: UiActionJournal,
    dispatch: UiDispatchBatch,
    rejection: Option<UiRouteRejection>,
}

impl UiInteractionTransaction {
    pub(crate) fn prepare(
        state: &UiRuntimeState,
        journal: &UiActionJournal,
        route: UiRouteStamp,
        input: UiInputEvent,
        input_sequence: Option<u64>,
    ) -> Result<Self, UiRouteRejection> {
        let surface_id = route.surface_id();
        if state.is_retired(surface_id) {
            return Err(UiRouteRejection::RetiredSurface {
                surface: surface_id.get(),
            });
        }
        let surface = state
            .surface(surface_id)
            .ok_or(UiRouteRejection::UnknownSurface {
                surface: surface_id.get(),
            })?;
        let presented = surface
            .resolve_presented(route)
            .map_err(map_adapter_error)?;
        let mut interaction = surface.interaction().clone();
        let dispatch =
            interaction.dispatch_resolved(presented.target(), presented.latest(), route, input)?;
        let rejection = dispatch
            .non_activation()
            .cloned()
            .map(UiRouteRejection::NonActivation);

        let mut reducers = surface.reducer_states().clone();
        let mut intents = UiIntentBatch::default();
        let mut targets = BTreeSet::new();
        targets.extend(dispatch.events().iter().map(|event| event.target().clone()));
        if rejection.is_none() {
            for target in targets {
                let button = presented.behavior().button(&target).ok_or_else(|| {
                    UiRouteRejection::PresentedBehaviorMissing {
                        surface: surface_id.get(),
                        presentation_epoch: route.presentation_epoch(),
                        target: Some(target.clone()),
                    }
                })?;
                reducers
                    .entry(target)
                    .or_default()
                    .reduce_batch(button, &dispatch, &mut intents)
                    .map_err(|error| UiRouteRejection::Widget(error.to_string()))?;
            }
        }

        let drafts = if rejection.is_none() {
            intents
                .intents()
                .iter()
                .map(|intent| match intent {
                    UiIntent::Activate {
                        target,
                        action,
                        source,
                    } => UiActionDraft::new(
                        input_sequence,
                        route,
                        dispatch.frame_revision().get(),
                        target.clone(),
                        action.clone(),
                        *source,
                    ),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut journal = journal.clone();
        journal.admit(&drafts)?;

        Ok(Self {
            surface_id,
            interaction,
            reducers,
            journal,
            dispatch,
            rejection,
        })
    }

    pub(crate) const fn is_rejected(&self) -> bool {
        self.rejection.is_some()
    }

    pub(crate) fn commit(
        self,
        state: &mut UiRuntimeState,
        journal: &mut UiActionJournal,
    ) -> UiRouteDisposition {
        let Self {
            surface_id,
            interaction,
            reducers,
            journal: candidate_journal,
            dispatch,
            rejection,
        } = self;
        state
            .surface_mut(surface_id)
            .expect("a prepared UI transaction retains its surface")
            .commit_interaction(interaction, reducers);
        *journal = candidate_journal;
        rejection.map_or_else(
            || UiRouteDisposition::Consumed(dispatch),
            UiRouteDisposition::RejectedClosed,
        )
    }
}

pub(crate) fn map_adapter_error(error: UiAdapterError) -> UiRouteRejection {
    match error {
        UiAdapterError::RetiredSurface { surface } => UiRouteRejection::RetiredSurface { surface },
        UiAdapterError::UnknownSurface { surface } => UiRouteRejection::UnknownSurface { surface },
        UiAdapterError::PresentedBehaviorMissing {
            surface,
            presentation_epoch,
        } => UiRouteRejection::PresentedBehaviorMissing {
            surface,
            presentation_epoch,
            target: None,
        },
        UiAdapterError::Journal(error) => UiRouteRejection::PresentedHistory(error),
        UiAdapterError::Dispatch(error) => UiRouteRejection::Dispatch(error),
        UiAdapterError::Widget(error) => UiRouteRejection::Widget(error),
        UiAdapterError::RuntimeUnavailable => UiRouteRejection::RuntimeUnavailable,
        UiAdapterError::RuntimeSetup(_)
        | UiAdapterError::SourceRevisionExhausted
        | UiAdapterError::SurfaceLimitExceeded
        | UiAdapterError::SurfaceIdentitySpaceExhausted
        | UiAdapterError::InputEndpointConflict { .. }
        | UiAdapterError::SurfaceAlreadyInstalled { .. }
        | UiAdapterError::FrameNotLatest { .. }
        | UiAdapterError::Environment(_)
        | UiAdapterError::SurfaceMapping(_)
        | UiAdapterError::Input(_) => UiRouteRejection::InvalidRouteStamp,
    }
}
