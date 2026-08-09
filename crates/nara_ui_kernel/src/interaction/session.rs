use std::collections::BTreeMap;

use thiserror::Error;

use super::{
    UiAccessibilityInput, UiActivationSource, UiButtonPhase, UiDispatchBatch, UiDispatchOutcome,
    UiInputEvent, UiNonActivation, UiPointerButton, UiSemanticFact, UiSemanticSnapshot,
    UiSemanticState,
    focus::{activate_target, committed_focus, is_focusable_key, navigate_focus, set_focus},
    pointer::{pointer_press, pointer_release, set_hover},
    routing::{KeyAvailability, active_modal_root, hit_target, key_availability, target_rejection},
    state::{
        PointerPosition, SurfaceState, cancel_all_pointers, cancel_pointer, cancel_seat,
        cancel_surface, preflight_input_identity, reconcile_pointer_input_generation,
        reconcile_presented_pointer_state, reconcile_surface,
    },
};
use crate::{
    UiFrame, UiFrameJournal, UiJournalError, UiKey, UiLimits, UiLimitsError, UiPoint, UiRouteStamp,
    UiRouteTarget, UiSurfaceId,
};

/// Bounded geometry-coupled state for presented-frame interaction.
#[derive(Clone, Debug)]
pub struct UiInteractionSession {
    limits: UiLimits,
    surfaces: BTreeMap<UiSurfaceId, SurfaceState>,
}

/// Candidate interaction state prepared without mutating the presented session.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct UiCandidateReconciliation {
    session: UiInteractionSession,
    dispatch: UiDispatchBatch,
}

impl UiCandidateReconciliation {
    /// Returns the cleanup events that must be reduced before this state can be adopted.
    #[must_use]
    pub const fn dispatch(&self) -> &UiDispatchBatch {
        &self.dispatch
    }

    /// Consumes the staged result after the owning adapter has prepared dependent reducers.
    #[must_use]
    pub fn into_parts(self) -> (UiInteractionSession, UiDispatchBatch) {
        (self.session, self.dispatch)
    }
}

impl UiInteractionSession {
    pub fn with_limits(limits: UiLimits) -> Result<Self, UiDispatchError> {
        limits.validate()?;
        Ok(Self {
            limits,
            surfaces: BTreeMap::new(),
        })
    }

    /// Routes one normalized event against its exact submitted frame/receipt pair.
    pub fn dispatch(
        &mut self,
        journal: &UiFrameJournal,
        route: UiRouteStamp,
        input: UiInputEvent,
    ) -> Result<UiDispatchOutcome, UiDispatchError> {
        let target = journal.resolve(route)?;
        let latest = journal.latest(route.surface_id())?;
        self.dispatch_resolved(&target, &latest, route, input)
    }

    /// Routes one receipt-stamped accessibility request through the same atomic transaction.
    pub fn dispatch_accessibility(
        &mut self,
        journal: &UiFrameJournal,
        action: UiAccessibilityInput,
    ) -> Result<UiDispatchOutcome, UiDispatchError> {
        let (route, input) = action.into_input();
        self.dispatch(journal, route, input)
    }

    /// Routes one normalized event against already resolved exact presented targets.
    pub fn dispatch_resolved(
        &mut self,
        target: &UiRouteTarget,
        latest: &UiRouteTarget,
        route: UiRouteStamp,
        input: UiInputEvent,
    ) -> Result<UiDispatchOutcome, UiDispatchError> {
        let latest_route = UiRouteStamp::from_receipt(latest.receipt());
        if UiRouteStamp::from_receipt(target.receipt()) != route
            || latest_route.surface_id() != route.surface_id()
            || latest_route.slot_generation() != route.slot_generation()
            || latest_route.presentation_epoch() < route.presentation_epoch()
            || latest_route.route_revision() < route.route_revision()
            || latest_route.geometry_generation() < route.geometry_generation()
        {
            return Err(UiDispatchError::ResolvedRouteMismatch);
        }
        self.preflight_surface(route.surface_id())?;

        let initial = self
            .surfaces
            .get(&route.surface_id())
            .cloned()
            .unwrap_or_default();
        let mut reconciled = initial.clone();
        let mut reconciliation = UiDispatchBatch::reconciliation(
            latest.revision(),
            latest.receipt().presentation_epoch(),
        );
        reconcile_surface(
            &mut reconciled,
            latest.frame(),
            latest.frame(),
            &mut reconciliation,
        );
        reconcile_pointer_input_generation(
            &mut reconciled,
            &input,
            route.presentation_epoch(),
            latest.frame(),
            &mut reconciliation,
        );
        let reconciliation_changed = reconciled != initial;
        let reconciliation_generation_differs = target.revision() != latest.revision()
            || route.presentation_epoch() != latest.receipt().presentation_epoch();
        if reconciliation_changed {
            reconciled.presentation_epoch = Some(latest.receipt().presentation_epoch());
            reconciled.presented_route = Some(UiRouteStamp::from_receipt(latest.receipt()));
            self.ensure_event_limit(&reconciliation)?;
            if reconciliation_generation_differs || !reconciliation.events().is_empty() {
                self.commit_surface(route.surface_id(), reconciled);
                return Ok(UiDispatchOutcome::Consumed(reconciliation));
            }
        }

        if matches!(
            input,
            UiInputEvent::PointerButton { button, .. } if button != UiPointerButton::Primary
        ) {
            return Ok(self.finish_reconciliation_or_rejection(
                route.surface_id(),
                reconciled,
                reconciliation,
                reconciliation_changed,
                UiNonActivation::UnsupportedPointerButton,
            ));
        }
        if is_stale_geometry(&initial, route, &input) {
            return Ok(self.finish_reconciliation_or_rejection(
                route.surface_id(),
                reconciled,
                reconciliation,
                reconciliation_changed,
                UiNonActivation::StaleGeometry,
            ));
        }

        let mut candidate = reconciled.clone();
        candidate.presentation_epoch = Some(latest.receipt().presentation_epoch());
        candidate.presented_route = Some(UiRouteStamp::from_receipt(latest.receipt()));
        if let Err(error) = preflight_input_identity(&mut candidate, &input) {
            if reconciliation_changed {
                self.commit_surface(route.surface_id(), reconciled);
                return Ok(UiDispatchOutcome::Consumed(reconciliation));
            }
            return Err(error);
        }
        let mut input_batch = UiDispatchBatch::new(target.revision(), route.presentation_epoch());
        let application = match apply_input(
            &mut candidate,
            target.frame(),
            latest.frame(),
            route,
            input,
            &mut input_batch,
        ) {
            Ok(application) => application,
            Err(_) if reconciliation_changed => {
                self.commit_surface(route.surface_id(), reconciled);
                return Ok(UiDispatchOutcome::Consumed(reconciliation));
            }
            Err(error) => return Err(error),
        };

        match application {
            UiInputApplication::Applied { remove_surface } => {
                if let Err(error) = self.ensure_event_limit(&input_batch) {
                    if reconciliation_changed {
                        self.commit_surface(route.surface_id(), reconciled);
                        return Ok(UiDispatchOutcome::Consumed(reconciliation));
                    }
                    return Err(error);
                }
                if remove_surface {
                    self.surfaces.remove(&route.surface_id());
                } else {
                    self.commit_surface(route.surface_id(), candidate);
                }
                Ok(UiDispatchOutcome::Consumed(input_batch))
            }
            UiInputApplication::Rejected(_) if reconciliation_changed => {
                self.commit_surface(route.surface_id(), reconciled);
                Ok(UiDispatchOutcome::Consumed(reconciliation))
            }
            UiInputApplication::Rejected(reason) => Ok(UiDispatchOutcome::Rejected(reason)),
        }
    }

    /// Reconciles interaction state with the journal's latest committed presentation.
    ///
    /// Callers that own presentation publication can apply this to a cloned session, reduce the
    /// returned cleanup batch, and publish both candidates atomically. `invalidated_keys` names
    /// stable controls whose owning behavior changed; their in-flight pointer gestures are
    /// cancelled before structural reconciliation, while focus remains frame-owned.
    pub fn reconcile_latest_presentation(
        &mut self,
        journal: &UiFrameJournal,
        surface: UiSurfaceId,
        invalidated_keys: &[UiKey],
    ) -> Result<Option<UiDispatchBatch>, UiDispatchError> {
        let target = journal.latest(surface)?;
        self.preflight_surface(surface)?;
        let initial = self.surfaces.get(&surface).cloned().unwrap_or_default();
        let mut candidate = initial.clone();
        let mut reconciliation = UiDispatchBatch::reconciliation(
            target.revision(),
            target.receipt().presentation_epoch(),
        );
        let previous = initial.presented_route.and_then(|route| {
            journal
                .resolve(route)
                .ok()
                .map(|target| (target, route.presentation_epoch()))
        });
        reconcile_presented_pointer_state(
            &mut candidate,
            previous
                .as_ref()
                .map(|(target, epoch)| (target.frame().as_ref(), *epoch)),
            target.frame(),
            Some(target.receipt().presentation_epoch()),
            invalidated_keys,
            &mut reconciliation,
        );
        reconcile_surface(
            &mut candidate,
            target.frame(),
            target.frame(),
            &mut reconciliation,
        );
        candidate.presentation_epoch = Some(target.receipt().presentation_epoch());
        candidate.presented_route = Some(UiRouteStamp::from_receipt(target.receipt()));
        if candidate == initial {
            return Ok(None);
        }
        self.ensure_event_limit(&reconciliation)?;
        self.commit_surface(surface, candidate);
        Ok(Some(reconciliation))
    }

    /// Stages state cleanup against an unpresented candidate without granting it route authority.
    ///
    /// First-party adapters use this before final visual compilation so candidate paint, semantics,
    /// reducers, and the eventual presented session all describe the same interaction state. The
    /// presented session is not mutated; the returned staged session can be adopted only after
    /// dependent reducers succeed. The committed presentation epoch remains unchanged until backend
    /// acceptance is published.
    #[doc(hidden)]
    pub fn stage_candidate_presentation(
        &self,
        surface: UiSurfaceId,
        previous: Option<&UiFrame>,
        candidate_frame: &UiFrame,
        invalidated_keys: &[UiKey],
    ) -> Result<Option<UiCandidateReconciliation>, UiDispatchError> {
        if candidate_frame.environment().surface_id() != surface
            || previous.is_some_and(|frame| frame.environment().surface_id() != surface)
        {
            return Err(UiDispatchError::ResolvedRouteMismatch);
        }
        self.preflight_surface(surface)?;
        let initial = self.surfaces.get(&surface).cloned().unwrap_or_default();
        let mut candidate = initial.clone();
        let current_epoch = initial.presentation_epoch;
        let mut reconciliation =
            UiDispatchBatch::reconciliation(candidate_frame.revision(), current_epoch.unwrap_or(1));
        reconcile_presented_pointer_state(
            &mut candidate,
            previous.zip(current_epoch),
            candidate_frame,
            None,
            invalidated_keys,
            &mut reconciliation,
        );
        reconcile_surface(
            &mut candidate,
            candidate_frame,
            candidate_frame,
            &mut reconciliation,
        );
        if candidate == initial {
            return Ok(None);
        }
        self.ensure_event_limit(&reconciliation)?;
        let mut session = self.clone();
        session.commit_surface(surface, candidate);
        Ok(Some(UiCandidateReconciliation {
            session,
            dispatch: reconciliation,
        }))
    }

    /// Projects descriptive semantics from the latest submitted frame and current interaction
    /// state. Actions are omitted until an exact presented behavior owner authorizes them.
    pub fn semantic_snapshot(
        &self,
        journal: &UiFrameJournal,
        route: UiRouteStamp,
        seat: super::UiSeatId,
    ) -> Result<UiSemanticSnapshot, UiDispatchError> {
        self.semantic_snapshot_with_action_authority(journal, route, seat, |_, _| false)
    }

    /// Projects actionable semantics only when the exact presented behavior owner resolves them.
    ///
    /// The predicate is evaluated against authored semantic actions after exact receipt resolution;
    /// returning `false` keeps the descriptive node while withholding the platform action.
    pub fn semantic_snapshot_with_action_authority(
        &self,
        journal: &UiFrameJournal,
        route: UiRouteStamp,
        seat: super::UiSeatId,
        mut owns_action: impl FnMut(&UiKey, &crate::UiActionId) -> bool,
    ) -> Result<UiSemanticSnapshot, UiDispatchError> {
        let target = journal.resolve(route)?;
        let latest = journal.latest(route.surface_id())?;
        if target.receipt().presentation_epoch() != latest.receipt().presentation_epoch() {
            return Err(UiDispatchError::SemanticSnapshotNotLatest {
                requested: target.receipt().presentation_epoch(),
                latest: latest.receipt().presentation_epoch(),
            });
        }

        let frame = target.frame();
        let latest_frame = latest.frame();
        let state = self.surfaces.get(&route.surface_id());
        let modal_root = active_modal_root(frame);
        let focused = committed_focus(state, seat, frame, latest_frame, modal_root.as_ref());
        let nodes = frame
            .semantics()
            .iter()
            .filter_map(|node| {
                let rect = node.visible_rect()?;
                let physical_rect = frame.environment().physical_rect(rect)?;
                let availability = key_availability(latest_frame, node.key());
                let inert = modal_root
                    .as_ref()
                    .is_some_and(|root| node.key().root_key() != root);
                let active = availability == KeyAvailability::Active;
                let focusable = active
                    && !inert
                    && frame
                        .focus_order()
                        .iter()
                        .any(|fact| fact.key() == node.key() && fact.enabled());
                let pressed = node.enabled()
                    && active
                    && !inert
                    && state.is_some_and(|state| {
                        state.pointers.iter().any(|((pointer_seat, _), pointer)| {
                            *pointer_seat == seat
                                && pointer.captured.as_ref().is_some_and(|capture| {
                                    capture.presentation_epoch == route.presentation_epoch()
                                        && &capture.target == node.key()
                                })
                        })
                    });
                let actions = node
                    .actions()
                    .iter()
                    .filter(|action| owns_action(node.key(), action))
                    .cloned()
                    .collect();
                Some(UiSemanticFact::from_frame_node(
                    node,
                    rect,
                    physical_rect,
                    actions,
                    UiSemanticState::new(
                        node.enabled() && active,
                        inert || !active,
                        node.selected(),
                        node.expanded(),
                        focusable,
                        focused.as_ref() == Some(node.key()),
                        pressed,
                    ),
                ))
            })
            .collect();
        Ok(UiSemanticSnapshot::new(
            route,
            target.revision(),
            seat,
            nodes,
        ))
    }

    /// Returns whether a surface retains a pointer position, hover or capture fact.
    #[must_use]
    pub fn has_pointer_facts(&self, surface: UiSurfaceId) -> bool {
        self.surfaces
            .get(&surface)
            .is_some_and(SurfaceState::has_pointer_facts)
    }

    /// Clears every interaction fact owned by one retired platform endpoint.
    ///
    /// Moving a Surface Slot to another window or seat must not transfer focus, capture, pointer,
    /// or modal session state to that new endpoint.
    pub fn clear_surface_state(&mut self, surface: UiSurfaceId) {
        self.surfaces.remove(&surface);
    }

    fn preflight_surface(&self, surface: UiSurfaceId) -> Result<(), UiDispatchError> {
        if !self.surfaces.contains_key(&surface) && self.surfaces.len() == self.limits.max_surfaces
        {
            return Err(UiDispatchError::SurfaceLimitExceeded {
                maximum: self.limits.max_surfaces,
            });
        }
        Ok(())
    }

    fn ensure_event_limit(&self, batch: &UiDispatchBatch) -> Result<(), UiDispatchError> {
        // One pointer press can replace hover (two routes), replace focus (two routes), and route
        // Pressed. Every route may contain capture, target, and bubble facts across the full tree.
        const MAX_ROUTED_TRANSITIONS_PER_INPUT: usize = 5;
        const MAX_ROUTE_PHASES_PER_NODE: usize = 2;
        let maximum = self
            .limits
            .max_nodes
            .saturating_mul(MAX_ROUTED_TRANSITIONS_PER_INPUT)
            .saturating_mul(MAX_ROUTE_PHASES_PER_NODE);
        if batch.events().len() > maximum {
            return Err(UiDispatchError::RoutedEventLimitExceeded { maximum });
        }
        Ok(())
    }

    fn commit_surface(&mut self, surface: UiSurfaceId, state: SurfaceState) {
        self.surfaces.insert(surface, state);
    }

    fn finish_reconciliation_or_rejection(
        &mut self,
        surface: UiSurfaceId,
        reconciled: SurfaceState,
        reconciliation: UiDispatchBatch,
        reconciliation_changed: bool,
        rejection: UiNonActivation,
    ) -> UiDispatchOutcome {
        if reconciliation_changed {
            self.commit_surface(surface, reconciled);
            UiDispatchOutcome::Consumed(reconciliation)
        } else {
            UiDispatchOutcome::Rejected(rejection)
        }
    }
}

impl Default for UiInteractionSession {
    fn default() -> Self {
        Self::with_limits(UiLimits::default()).expect("default UI limits are valid")
    }
}

fn apply_input(
    state: &mut SurfaceState,
    frame: &UiFrame,
    latest: &UiFrame,
    route: UiRouteStamp,
    input: UiInputEvent,
    batch: &mut UiDispatchBatch,
) -> Result<UiInputApplication, UiDispatchError> {
    match input {
        UiInputEvent::PointerMoved {
            seat,
            pointer,
            position,
        } => {
            validate_position(position)?;
            state
                .pointers
                .get_mut(&(seat, pointer))
                .expect("input identity preflight creates pointer state")
                .position = Some(PointerPosition {
                point: position,
                presentation_epoch: route.presentation_epoch(),
            });
            let hovered = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
                Ok(target) => target,
                Err(reason) => return Ok(UiInputApplication::Rejected(reason)),
            };
            set_hover(state, seat, pointer, hovered, frame, batch);
        }
        UiInputEvent::PointerButton {
            seat,
            pointer,
            button: _,
            phase,
            position,
        } => {
            if let Some(position) = position {
                validate_position(position)?;
                state
                    .pointers
                    .get_mut(&(seat, pointer))
                    .expect("input identity preflight creates pointer state")
                    .position = Some(PointerPosition {
                    point: position,
                    presentation_epoch: route.presentation_epoch(),
                });
            }
            match phase {
                UiButtonPhase::Pressed => {
                    if let Err(reason) = pointer_press(
                        state,
                        seat,
                        pointer,
                        frame,
                        latest,
                        route.presentation_epoch(),
                        batch,
                    ) {
                        return Ok(UiInputApplication::Rejected(reason));
                    }
                }
                UiButtonPhase::Released => {
                    if let Err(reason) = pointer_release(
                        state,
                        seat,
                        pointer,
                        frame,
                        latest,
                        route.presentation_epoch(),
                        batch,
                    ) {
                        return Ok(UiInputApplication::Rejected(reason));
                    }
                }
            }
        }
        UiInputEvent::PointerCancelled { seat, pointer } => {
            cancel_pointer(state, seat, pointer, frame, batch, true);
        }
        UiInputEvent::Navigate { seat, direction } => {
            if let Err(reason) = navigate_focus(state, seat, direction, frame, latest, batch) {
                return Ok(UiInputApplication::Rejected(reason));
            }
        }
        UiInputEvent::ActivateFocused { seat, source } => {
            let Some(target) = state.seats.get(&seat).and_then(|seat| seat.focused.clone()) else {
                return Ok(UiInputApplication::Rejected(UiNonActivation::NoFocusTarget));
            };
            if let Err(reason) = activate_target(
                &target,
                None,
                source,
                frame,
                latest,
                state.modal_root.as_ref(),
                batch,
            ) {
                return Ok(UiInputApplication::Rejected(reason));
            }
        }
        UiInputEvent::FocusTarget { seat, target } => {
            if is_focusable_key(frame, latest, state.modal_root.as_ref(), &target) {
                set_focus(state, seat, Some(target), frame, batch);
            } else if let Some(reason) =
                target_rejection(frame, latest, state.modal_root.as_ref(), &target)
            {
                return Ok(UiInputApplication::Rejected(reason));
            } else if frame
                .focus_order()
                .iter()
                .any(|fact| fact.key() == &target && !fact.enabled())
            {
                return Ok(UiInputApplication::Rejected(
                    UiNonActivation::TargetDisabled { target },
                ));
            } else {
                return Ok(UiInputApplication::Rejected(UiNonActivation::NoFocusTarget));
            }
        }
        UiInputEvent::SemanticAction {
            seat: _,
            target,
            action,
        } => {
            if let Err(reason) = activate_target(
                &target,
                Some(action),
                UiActivationSource::Accessibility,
                frame,
                latest,
                state.modal_root.as_ref(),
                batch,
            ) {
                return Ok(UiInputApplication::Rejected(reason));
            }
        }
        UiInputEvent::SeatCancelled { seat } => {
            cancel_seat(state, seat, frame, batch);
        }
        UiInputEvent::GeometryInvalidated => {
            cancel_all_pointers(state, frame, batch);
            state.invalidate_geometry(route);
        }
        UiInputEvent::SurfaceCancelled => {
            cancel_surface(state, frame, batch);
            return Ok(UiInputApplication::Applied {
                remove_surface: true,
            });
        }
    }
    Ok(UiInputApplication::Applied {
        remove_surface: false,
    })
}

enum UiInputApplication {
    Applied { remove_surface: bool },
    Rejected(UiNonActivation),
}

fn is_stale_geometry(state: &SurfaceState, route: UiRouteStamp, input: &UiInputEvent) -> bool {
    let targets_geometry = matches!(
        input,
        UiInputEvent::PointerMoved { .. }
            | UiInputEvent::PointerButton { .. }
            | UiInputEvent::Navigate { .. }
            | UiInputEvent::ActivateFocused { .. }
            | UiInputEvent::FocusTarget { .. }
            | UiInputEvent::SemanticAction { .. }
    );
    targets_geometry && state.geometry_is_invalidated_for(route)
}

fn validate_position(position: UiPoint) -> Result<(), UiDispatchError> {
    if !position.is_finite() {
        return Err(UiDispatchError::NonFinitePointerPosition);
    }
    Ok(())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiDispatchError {
    #[error(transparent)]
    Limits(#[from] UiLimitsError),
    #[error(transparent)]
    Journal(#[from] UiJournalError),
    #[error("UI interaction session exceeds {maximum} active surfaces")]
    SurfaceLimitExceeded { maximum: usize },
    #[error("UI interaction surface exceeds {maximum} seats")]
    SeatLimitExceeded { maximum: usize },
    #[error("UI interaction surface exceeds {maximum} pointers")]
    PointerLimitExceeded { maximum: usize },
    #[error("UI pointer position must be finite")]
    NonFinitePointerPosition,
    #[error("resolved UI targets do not match the requested route")]
    ResolvedRouteMismatch,
    #[error("UI dispatch exceeds {maximum} routed events")]
    RoutedEventLimitExceeded { maximum: usize },
    #[error("semantic snapshot requested epoch {requested}, but latest is {latest}")]
    SemanticSnapshotNotLatest { requested: u64, latest: u64 },
}
