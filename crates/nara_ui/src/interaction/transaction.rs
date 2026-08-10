use std::collections::BTreeMap;

use nara_ecs::{Mut, Resource, World};
use nara_input::{InputTimeline, InputUiCursorError, InputUiDisposition};
use nara_ui_kernel::{
    UiButtonPhase, UiDispatchBatch, UiDispatchError, UiDispatchOutcome, UiInputEvent,
    UiInteractionSession, UiJournalError, UiKey, UiNonActivation, UiRouteStamp, UiSurfaceId,
};
use nara_ui_widgets::{UiButtonState, UiIntent};
use thiserror::Error;

use crate::runtime::{UiAdapterError, UiRuntimeState, state::UiReducerPreparationError};

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
            if let Some(sequence) = input_sequence
                && let Err(error) = world
                    .resource_mut::<InputTimeline>()
                    .__finalize_ui_entry(sequence, InputUiDisposition::Consumed)
            {
                return UiRouteDisposition::RejectedClosed(UiRouteRejection::InputFinalization(
                    error,
                ));
            }
            let (disposition, diagnostic) = transaction.commit(&mut state, &mut journal);
            if let Some(rejection) = diagnostic
                && let Some(mut log) = world.get_resource_mut::<UiRouteRejectionLog>()
            {
                log.record(input_sequence, rejection);
            }
            disposition
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
    #[error("UI route generations do not match the current Surface Slot")]
    SurfaceGenerationMismatch,
    #[error("UI pointer does not match the Surface Slot pointer owner")]
    PointerOwnerMismatch,
    #[error("UI seat does not match the Surface Slot endpoint owner")]
    SeatOwnerMismatch,
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
    diagnostic: Option<UiRouteRejection>,
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
        let release_must_commit = matches!(
            &input,
            UiInputEvent::PointerButton {
                phase: UiButtonPhase::Released,
                ..
            }
        );
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
        let slot = surface.slot();
        if slot.slot_generation() != route.slot_generation().get()
            || slot.route_generation() != route.route_revision().get()
            || slot.geometry_generation() != route.geometry_generation().get()
        {
            return Err(UiRouteRejection::SurfaceGenerationMismatch);
        }
        let presented = surface
            .resolve_presented(route)
            .map_err(map_adapter_error)?;
        let mut interaction = surface.interaction().clone();
        let dispatch = match interaction.dispatch_resolved(
            presented.target(),
            presented.latest(),
            route,
            input,
        )? {
            UiDispatchOutcome::Consumed(dispatch) => dispatch,
            UiDispatchOutcome::Rejected(reason) => {
                return Err(UiRouteRejection::NonActivation(reason));
            }
        };
        let (reducers, intents) = surface
            .prepare_dispatch_reducers(&presented, &dispatch)
            .map_err(|error| match error {
                UiReducerPreparationError::PresentedBehaviorMissing(target) => {
                    UiRouteRejection::PresentedBehaviorMissing {
                        surface: surface_id.get(),
                        presentation_epoch: route.presentation_epoch(),
                        target: Some(target),
                    }
                }
                UiReducerPreparationError::Widget(error) => UiRouteRejection::Widget(error),
            })?;

        let drafts = intents
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
            .collect::<Vec<_>>();
        let mut journal = journal.clone();
        let diagnostic = match journal.admit(&drafts) {
            Ok(_) => None,
            Err(error) if release_must_commit => Some(UiRouteRejection::IntentJournal(error)),
            Err(error) => return Err(UiRouteRejection::IntentJournal(error)),
        };

        Ok(Self {
            surface_id,
            interaction,
            reducers,
            journal,
            dispatch,
            diagnostic,
        })
    }

    pub(crate) fn commit(
        self,
        state: &mut UiRuntimeState,
        journal: &mut UiActionJournal,
    ) -> (UiRouteDisposition, Option<UiRouteRejection>) {
        let Self {
            surface_id,
            interaction,
            reducers,
            journal: candidate_journal,
            dispatch,
            diagnostic,
        } = self;
        state
            .surface_mut(surface_id)
            .expect("a prepared UI transaction retains its surface")
            .commit_interaction(interaction, reducers);
        *journal = candidate_journal;
        (UiRouteDisposition::Consumed(dispatch), diagnostic)
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
        UiAdapterError::SeatOwnerMismatch { .. } => UiRouteRejection::SeatOwnerMismatch,
        UiAdapterError::RuntimeSetup(_)
        | UiAdapterError::SourceRevisionExhausted
        | UiAdapterError::SurfaceLimitExceeded
        | UiAdapterError::SurfaceIdentitySpaceExhausted
        | UiAdapterError::PresentationTargetConflict { .. }
        | UiAdapterError::PresentationTargetOwnerMismatch { .. }
        | UiAdapterError::SurfaceAlreadyInstalled { .. }
        | UiAdapterError::SurfaceAlreadyActive { .. }
        | UiAdapterError::SurfaceSuspended { .. }
        | UiAdapterError::PresentationAuthorityMissing { .. }
        | UiAdapterError::FrameNotLatest { .. }
        | UiAdapterError::Environment(_)
        | UiAdapterError::SurfaceSlot(_)
        | UiAdapterError::Input(_) => UiRouteRejection::InvalidRouteStamp,
    }
}
