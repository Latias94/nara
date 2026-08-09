use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_ecs::Resource;
use nara_input::InputRouteStamp as PresentedInputRouteStamp;
use nara_text::FontFace;
use nara_ui_kernel::{
    UiCompileError, UiCompiler, UiDispatchBatch, UiDispatchError, UiDispatchOutcome, UiEnvironment,
    UiFrame, UiInputEvent, UiInteractionSession, UiKey, UiLimits, UiPresentationReceipt,
    UiRouteStamp, UiSeatId, UiSemanticSnapshot, UiSurfaceRetirementToken, UiTheme,
};
use nara_ui_widgets::{UiButtonState, UiIntentBatch};
use thiserror::Error;

use super::{
    UiAdapterError,
    candidate::{
        UiBehaviorSnapshot, UiBehaviorSnapshotError, UiCandidateStaging, UiCompiledCandidate,
        UiPresentationFacts,
    },
    identity::UiSurfaceIdentityLedger,
    presentation::{UiPresentedHistory, UiPresentedSession},
    projection::UiProjectedAuthoring,
    surface::{UiSurfaceRetirement, UiSurfaceSlot},
};

pub(super) const MAX_RUNTIME_SURFACES: usize = 16;

#[derive(Debug, Error)]
pub(super) enum UiCandidatePreparationError {
    #[error(transparent)]
    Compile(#[from] UiCompileError),
    #[error(transparent)]
    Behavior(#[from] UiBehaviorSnapshotError),
    #[error(transparent)]
    Dispatch(#[from] UiDispatchError),
    #[error("candidate widget reconciliation failed: {0}")]
    Widget(String),
    #[error("candidate reconciliation emitted an action intent")]
    ReconciliationIntent,
    #[error("reducer-only reprojection changed candidate interaction facts")]
    ReprojectionInteractionMismatch,
    #[error("reducer-only reprojection changed candidate behavior")]
    ReprojectionBehaviorMismatch,
}

pub(crate) enum UiReducerPreparationError {
    PresentedBehaviorMissing(UiKey),
    Widget(String),
}

pub(super) struct UiPreparedPresentation {
    presented: UiPresentedHistory,
    receipt: UiPresentationReceipt,
    input_route: Option<PresentedInputRouteStamp>,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
}

pub(super) struct UiPreparedSlotUpdate {
    slot: UiSurfaceSlot,
    released_input_route: Option<PresentedInputRouteStamp>,
    presented: UiPresentedHistory,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
}

impl UiPreparedPresentation {
    pub(super) const fn receipt(&self) -> UiPresentationReceipt {
        self.receipt
    }

    pub(super) fn bind_input_route(&mut self, route: PresentedInputRouteStamp) {
        debug_assert!(route.is_ui_routable());
        self.input_route = Some(route);
    }
}

impl UiPreparedSlotUpdate {
    pub(super) const fn released_input_route(&self) -> Option<PresentedInputRouteStamp> {
        self.released_input_route
    }
}

/// All candidate, presentation, interaction, and reducer state for one surface lineage.
pub(crate) struct UiSurfaceRuntime {
    slot: UiSurfaceSlot,
    compiler: UiCompiler,
    candidate: Option<UiCompiledCandidate>,
    presented: UiPresentedHistory,
    presentation_authority: nara_ui_kernel::__private::UiFrameJournalAuthority,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
    input_route: Option<PresentedInputRouteStamp>,
}

impl UiSurfaceRuntime {
    pub(super) fn new(
        slot: UiSurfaceSlot,
        font: FontFace,
        limits: UiLimits,
    ) -> Result<Self, String> {
        let (presented, presentation_authority) =
            UiPresentedHistory::with_limits(limits, slot).map_err(|error| error.to_string())?;
        Ok(Self {
            slot,
            compiler: UiCompiler::new(font, limits).map_err(|error| error.to_string())?,
            candidate: None,
            presented,
            presentation_authority,
            interaction: UiInteractionSession::with_limits(limits)
                .map_err(|error| error.to_string())?,
            reducers: BTreeMap::new(),
            input_route: None,
        })
    }

    pub(crate) const fn slot(&self) -> UiSurfaceSlot {
        self.slot
    }

    pub(super) fn prepare_slot_update(
        &self,
        slot: UiSurfaceSlot,
    ) -> Result<UiPreparedSlotUpdate, UiAdapterError> {
        debug_assert_eq!(self.slot.surface_id(), slot.surface_id());
        let mut presented = self.presented.clone();
        let mut interaction = self.interaction.clone();
        let mut reducers = self.reducers.clone();

        let endpoint_changed = self.slot.endpoint() != slot.endpoint();
        if endpoint_changed {
            interaction.clear_surface_state(self.slot.surface_id());
            reducers.clear();
        } else if let Some(session) = self.presented.latest_session()? {
            let route = UiRouteStamp::from_receipt(session.latest().receipt());
            let dispatch = match interaction.dispatch_resolved(
                session.latest(),
                session.latest(),
                route,
                UiInputEvent::GeometryInvalidated,
            )? {
                UiDispatchOutcome::Consumed(dispatch) => dispatch,
                UiDispatchOutcome::Rejected(reason) => {
                    return Err(UiAdapterError::Widget(format!(
                        "surface slot invalidation was rejected: {reason:?}"
                    )));
                }
            };
            let (prepared_reducers, intents) = Self::prepare_reducers(
                &session, &dispatch, reducers,
            )
            .map_err(|error| match error {
                UiReducerPreparationError::PresentedBehaviorMissing(target) => {
                    UiAdapterError::Widget(format!(
                        "surface slot invalidation is missing behavior for {target:?}"
                    ))
                }
                UiReducerPreparationError::Widget(error) => UiAdapterError::Widget(error),
            })?;
            if !intents.intents().is_empty() {
                return Err(UiAdapterError::Widget(
                    "surface slot invalidation emitted an action intent".to_owned(),
                ));
            }
            reducers = prepared_reducers;
        }

        presented.advance_slot(self.slot, slot)?;
        Ok(UiPreparedSlotUpdate {
            slot,
            released_input_route: self.input_route,
            presented,
            interaction,
            reducers,
        })
    }

    pub(super) fn commit_slot_update(&mut self, prepared: UiPreparedSlotUpdate) {
        self.slot = prepared.slot;
        self.presented = prepared.presented;
        self.interaction = prepared.interaction;
        self.reducers = prepared.reducers;
        self.input_route = None;
        self.discard_candidate();
    }

    /// Retires work that has not acquired presentation authority.
    pub(super) fn discard_candidate(&mut self) {
        self.candidate = None;
    }

    pub(crate) fn reducer_states(&self) -> &BTreeMap<UiKey, UiButtonState> {
        &self.reducers
    }

    pub(super) fn prepare_candidate(
        &mut self,
        projected: UiProjectedAuthoring,
        environment: UiEnvironment,
        theme: UiTheme,
    ) -> Result<Option<UiCandidateStaging>, UiCandidatePreparationError> {
        let (snapshot, buttons) = projected.into_parts();
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        let behavior = Arc::new(UiBehaviorSnapshot::from_compiled_frame(&frame, buttons)?);
        let Some(staging) = self.stage_candidate_state(&frame, &behavior)? else {
            self.candidate = Some(UiCompiledCandidate::new(
                frame,
                behavior,
                self.interaction.clone(),
                self.reducers.clone(),
            ));
            return Ok(None);
        };
        Ok(Some(staging))
    }

    pub(super) fn finish_candidate(
        &mut self,
        staging: UiCandidateStaging,
        projected: UiProjectedAuthoring,
        environment: UiEnvironment,
        theme: UiTheme,
    ) -> Result<(), UiCandidatePreparationError> {
        let (snapshot, buttons) = projected.into_parts();
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        let behavior = Arc::new(UiBehaviorSnapshot::from_compiled_frame(&frame, buttons)?);
        if staging.behavior().as_ref() != behavior.as_ref() {
            return Err(UiCandidatePreparationError::ReprojectionBehaviorMismatch);
        }
        if !interaction_facts_match(staging.probe(), &frame) {
            return Err(UiCandidatePreparationError::ReprojectionInteractionMismatch);
        }
        let (interaction, reducers) = staging.into_parts();
        self.candidate = Some(UiCompiledCandidate::new(
            frame,
            behavior,
            interaction,
            reducers,
        ));
        Ok(())
    }

    pub(super) fn candidate(&self) -> Option<&UiCompiledCandidate> {
        self.candidate.as_ref()
    }

    fn stage_candidate_state(
        &self,
        frame: &Arc<UiFrame>,
        behavior: &Arc<UiBehaviorSnapshot>,
    ) -> Result<Option<UiCandidateStaging>, UiCandidatePreparationError> {
        let invalidated_keys = self.presented.behavior_invalidations(behavior);
        let reconciliation = self.interaction.stage_candidate_presentation(
            self.slot.surface_id(),
            self.presented
                .latest_target()
                .map(|target| target.frame().as_ref()),
            frame,
            &invalidated_keys,
        )?;
        let Some(reconciliation) = reconciliation else {
            return Ok(None);
        };
        let (interaction, reconciliation) = reconciliation.into_parts();
        let mut reducer_seed = self.reducers.clone();
        for key in &invalidated_keys {
            if let Some(state) = reducer_seed.get_mut(key) {
                state.cancel_pointer_interaction();
            }
        }
        let (mut reducers, intents) =
            Self::prepare_reconciliation_reducers(behavior, &reconciliation, reducer_seed)?;
        if !intents.intents().is_empty() {
            return Err(UiCandidatePreparationError::ReconciliationIntent);
        }
        reducers.retain(|key, _| behavior.button(key).is_some());
        Ok(Some(UiCandidateStaging::new(
            Arc::clone(frame),
            Arc::clone(behavior),
            interaction,
            reducers,
        )))
    }

    pub(super) fn prepare_presentation(
        &self,
        presentation_epoch: u64,
    ) -> Result<UiPreparedPresentation, UiAdapterError> {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(UiAdapterError::FrameNotLatest {
                surface: self.slot.surface_id().get(),
            })?;
        self.prepare_candidate_presentation(candidate, presentation_epoch)
    }

    pub(super) fn prepare_retained_presentation(
        &self,
        retained: &UiPresentationFacts,
        presentation_epoch: u64,
    ) -> Result<Option<UiPreparedPresentation>, UiAdapterError> {
        let Some(latest) = self
            .candidate
            .as_ref()
            .filter(|latest| retained.matches_candidate(latest))
        else {
            return Ok(None);
        };
        let rebound = latest.rebind_presentation_identity(retained);
        self.prepare_candidate_presentation(&rebound, presentation_epoch)
            .map(Some)
    }

    fn prepare_candidate_presentation(
        &self,
        candidate: &UiCompiledCandidate,
        presentation_epoch: u64,
    ) -> Result<UiPreparedPresentation, UiAdapterError> {
        let mut presented = self.presented.clone();
        let receipt = presented.submit(
            &self.presentation_authority,
            candidate,
            self.slot,
            presentation_epoch,
        )?;
        let mut interaction = candidate.interaction().clone();
        let reconciliation = interaction.reconcile_latest_presentation(
            presented.journal(),
            self.slot.surface_id(),
            &[],
        )?;
        if reconciliation
            .as_ref()
            .is_some_and(|dispatch| !dispatch.events().is_empty())
        {
            return Err(UiAdapterError::Widget(
                "candidate interaction facts changed after final visual compilation".to_owned(),
            ));
        }
        Ok(UiPreparedPresentation {
            presented,
            receipt,
            input_route: None,
            interaction,
            reducers: candidate.reducers().clone(),
        })
    }

    pub(super) fn commit_presentation(&mut self, prepared: UiPreparedPresentation) {
        debug_assert!(prepared.input_route.is_some());
        self.presented = prepared.presented;
        self.interaction = prepared.interaction;
        self.reducers = prepared.reducers;
        self.input_route = prepared.input_route;
    }

    pub(super) fn prepare_retirement(
        &self,
        retirement_generation: u64,
    ) -> Result<UiSurfaceRetirementToken, UiAdapterError> {
        let mut presented = self.presented.clone();
        presented
            .retire(self.slot.surface_id(), retirement_generation)
            .map_err(UiAdapterError::from)
            .and_then(|token| {
                token.ok_or(UiAdapterError::PresentationAuthorityMissing {
                    surface: self.slot.surface_id().get(),
                })
            })
    }

    pub(crate) fn resolve_presented(
        &self,
        route: UiRouteStamp,
    ) -> Result<UiPresentedSession, UiAdapterError> {
        self.presented
            .resolve(route)?
            .ok_or(UiAdapterError::PresentedBehaviorMissing {
                surface: route.surface_id().get(),
                presentation_epoch: route.presentation_epoch(),
            })
    }

    pub(crate) const fn interaction(&self) -> &UiInteractionSession {
        &self.interaction
    }

    pub(super) const fn input_route(&self) -> Option<PresentedInputRouteStamp> {
        self.input_route
    }

    #[cfg(test)]
    pub(crate) fn presented_receipt(
        &self,
        presentation_epoch: u64,
    ) -> Option<UiPresentationReceipt> {
        self.presented.receipt(presentation_epoch)
    }

    pub(crate) fn prepare_dispatch_reducers(
        &self,
        presented: &UiPresentedSession,
        dispatch: &UiDispatchBatch,
    ) -> Result<(BTreeMap<UiKey, UiButtonState>, UiIntentBatch), UiReducerPreparationError> {
        Self::prepare_reducers(presented, dispatch, self.reducers.clone())
    }

    pub(crate) fn commit_interaction(
        &mut self,
        interaction: UiInteractionSession,
        reducers: BTreeMap<UiKey, UiButtonState>,
    ) {
        self.interaction = interaction;
        self.reducers = reducers;
        self.discard_candidate();
    }

    pub(super) fn semantic_snapshot(
        &self,
        route: UiRouteStamp,
        seat: UiSeatId,
    ) -> Result<UiSemanticSnapshot, UiAdapterError> {
        let presented = self.resolve_presented(route)?;
        self.interaction
            .semantic_snapshot_with_action_authority(
                self.presented.journal(),
                route,
                seat,
                |key, action| {
                    presented
                        .behavior()
                        .button(key)
                        .is_some_and(|button| button.action() == action)
                },
            )
            .map_err(UiAdapterError::from)
    }

    fn prepare_reducers(
        presented: &UiPresentedSession,
        dispatch: &UiDispatchBatch,
        mut reducers: BTreeMap<UiKey, UiButtonState>,
    ) -> Result<(BTreeMap<UiKey, UiButtonState>, UiIntentBatch), UiReducerPreparationError> {
        let mut intents = UiIntentBatch::default();
        let targets = dispatch
            .events()
            .iter()
            .map(|event| event.target().clone())
            .collect::<BTreeSet<_>>();
        for target in targets {
            let button = if dispatch.is_reconciliation() {
                let Some(button) = presented.latest_behavior().button(&target) else {
                    reducers.remove(&target);
                    continue;
                };
                Some(button)
            } else {
                presented.behavior().button(&target)
            };
            let button = button.ok_or_else(|| {
                UiReducerPreparationError::PresentedBehaviorMissing(target.clone())
            })?;
            reducers
                .entry(target)
                .or_default()
                .reduce_batch(button, dispatch, &mut intents)
                .map_err(|error| UiReducerPreparationError::Widget(error.to_string()))?;
        }
        Ok((reducers, intents))
    }

    fn prepare_reconciliation_reducers(
        behavior: &UiBehaviorSnapshot,
        dispatch: &UiDispatchBatch,
        mut reducers: BTreeMap<UiKey, UiButtonState>,
    ) -> Result<(BTreeMap<UiKey, UiButtonState>, UiIntentBatch), UiCandidatePreparationError> {
        let mut intents = UiIntentBatch::default();
        let targets = dispatch
            .events()
            .iter()
            .map(|event| event.target().clone())
            .collect::<BTreeSet<_>>();
        for target in targets {
            let Some(button) = behavior.button(&target) else {
                reducers.remove(&target);
                continue;
            };
            reducers
                .entry(target)
                .or_default()
                .reduce_batch(button, dispatch, &mut intents)
                .map_err(|error| UiCandidatePreparationError::Widget(error.to_string()))?;
        }
        Ok((reducers, intents))
    }
}

fn interaction_facts_match(probe: &UiFrame, final_frame: &UiFrame) -> bool {
    probe.environment() == final_frame.environment()
        && probe.layouts() == final_frame.layouts()
        && probe.hit_regions() == final_frame.hit_regions()
        && probe.focus_order() == final_frame.focus_order()
        && probe.semantics() == final_frame.semantics()
        && probe.root_outcomes() == final_frame.root_outcomes()
}

/// Runtime UI adapter state; all mutable surface-local values live inside `UiSurfaceRuntime`.
#[derive(Resource)]
pub(crate) struct UiRuntimeState {
    font: FontFace,
    limits: UiLimits,
    surfaces: BTreeMap<nara_ui_kernel::UiSurfaceId, UiSurfaceRuntime>,
    identities: UiSurfaceIdentityLedger,
    next_source_revision: u64,
}

impl UiRuntimeState {
    pub(crate) fn new() -> Result<Self, String> {
        Ok(Self {
            font: nara_text::bundled_font_face().map_err(|error| error.to_string())?,
            limits: UiLimits::default(),
            surfaces: BTreeMap::new(),
            identities: UiSurfaceIdentityLedger::default(),
            next_source_revision: 1,
        })
    }

    pub(crate) fn surface_slot(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<UiSurfaceSlot> {
        self.surfaces.get(&surface_id).map(UiSurfaceRuntime::slot)
    }

    pub(crate) fn surface(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<&UiSurfaceRuntime> {
        self.surfaces.get(&surface_id)
    }

    pub(crate) fn surface_mut(
        &mut self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<&mut UiSurfaceRuntime> {
        self.surfaces.get_mut(&surface_id)
    }

    pub(super) fn surfaces(
        &self,
    ) -> impl Iterator<Item = (&nara_ui_kernel::UiSurfaceId, &UiSurfaceRuntime)> {
        self.surfaces.iter()
    }

    pub(super) fn surfaces_mut(
        &mut self,
    ) -> impl Iterator<Item = (&nara_ui_kernel::UiSurfaceId, &mut UiSurfaceRuntime)> {
        self.surfaces.iter_mut()
    }

    pub(super) const fn source_revision(&self) -> u64 {
        self.next_source_revision
    }

    pub(super) fn advance_source_revision(&mut self) -> Result<(), UiAdapterError> {
        self.next_source_revision = self
            .next_source_revision
            .checked_add(1)
            .ok_or(UiAdapterError::SourceRevisionExhausted)?;
        Ok(())
    }

    pub(super) fn prepare_surface_install(
        &self,
        slot: UiSurfaceSlot,
    ) -> Result<UiSurfaceRuntime, UiAdapterError> {
        let surface_id = slot.surface_id();
        if self.surfaces.contains_key(&surface_id) {
            return Err(UiAdapterError::SurfaceAlreadyInstalled {
                surface: surface_id.get(),
            });
        }
        self.identities.validate_new(surface_id)?;
        if self.surfaces.len() == MAX_RUNTIME_SURFACES {
            return Err(UiAdapterError::SurfaceLimitExceeded);
        }
        UiSurfaceRuntime::new(slot, self.font.clone(), self.limits)
            .map_err(UiAdapterError::RuntimeSetup)
    }

    pub(super) fn conflicting_presentation_target(
        &self,
        slot: UiSurfaceSlot,
    ) -> Option<nara_ui_kernel::UiSurfaceId> {
        self.surfaces.iter().find_map(|(surface_id, surface)| {
            (*surface_id != slot.surface_id()
                && surface.slot().endpoint().window() == slot.endpoint().window())
            .then_some(*surface_id)
        })
    }

    pub(super) fn commit_surface_install(
        &mut self,
        slot: UiSurfaceSlot,
        prepared: UiSurfaceRuntime,
    ) {
        let replaced = self.surfaces.insert(slot.surface_id(), prepared);
        debug_assert!(replaced.is_none(), "validated UI surface must be new");
    }

    pub(super) fn prepare_surface_slot_update(
        &self,
        slot: UiSurfaceSlot,
    ) -> Result<UiPreparedSlotUpdate, UiAdapterError> {
        self.surfaces
            .get(&slot.surface_id())
            .ok_or(UiAdapterError::UnknownSurface {
                surface: slot.surface_id().get(),
            })?
            .prepare_slot_update(slot)
    }

    pub(super) fn commit_surface_slot_update(&mut self, prepared: UiPreparedSlotUpdate) {
        self.surfaces
            .get_mut(&prepared.slot.surface_id())
            .expect("validated UI surface remains installed")
            .commit_slot_update(prepared);
    }

    pub(super) fn discard_all_candidates(&mut self) {
        for surface in self.surfaces.values_mut() {
            surface.discard_candidate();
        }
    }

    pub(super) fn prepare_surface_retirement(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Result<Option<UiSurfaceRetirement>, UiAdapterError> {
        let Some(runtime) = self.surface(surface_id) else {
            return Ok(None);
        };
        let slot = runtime.slot();
        let token = runtime.prepare_retirement(slot.next_retirement_generation()?)?;
        Ok(Some(UiSurfaceRetirement::new(
            slot.endpoint(),
            token,
            runtime.input_route(),
        )))
    }

    pub(super) fn commit_surface_retirement(&mut self, retirement: UiSurfaceRetirement) {
        let surface_id = retirement.surface_id();
        let removed = self.surfaces.remove(&surface_id);
        debug_assert!(removed.is_some(), "validated UI surface remains installed");
        self.identities.retire(retirement.token());
    }

    pub(crate) fn is_retired(&self, surface_id: nara_ui_kernel::UiSurfaceId) -> bool {
        self.identities.is_retired(surface_id)
    }
}

pub(super) fn candidate_frame(candidate: &UiCompiledCandidate) -> Arc<UiFrame> {
    Arc::clone(candidate.frame())
}
