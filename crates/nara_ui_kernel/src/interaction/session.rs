use std::collections::BTreeMap;

use thiserror::Error;

use super::{
    UiActivationSource, UiButtonPhase, UiDispatchBatch, UiInputEvent, UiNonActivation,
    UiPointerButton, UiSemanticFact, UiSemanticSnapshot, UiSemanticState,
    focus::{activate_target, effective_focus, is_focusable_key, navigate_focus, set_focus},
    pointer::{pointer_press, pointer_release, set_hover},
    routing::{KeyAvailability, active_modal_root, hit_target, key_availability, target_rejection},
    state::{
        SurfaceState, cancel_all_pointers, cancel_pointer, cancel_seat, cancel_surface,
        preflight_input_identity, reconcile_surface,
    },
};
use crate::{
    UiFrame, UiFrameJournal, UiJournalError, UiLimits, UiLimitsError, UiPoint, UiRouteStamp,
    UiSurfaceId,
};

/// Bounded geometry-coupled state for presented-frame interaction.
#[derive(Clone, Debug)]
pub struct UiInteractionSession {
    limits: UiLimits,
    surfaces: BTreeMap<UiSurfaceId, SurfaceState>,
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
    ) -> Result<UiDispatchBatch, UiDispatchError> {
        let target = journal.resolve(route)?;
        let latest = journal.latest(route.surface_id())?;
        self.preflight_surface(route.surface_id())?;

        let mut candidate = self
            .surfaces
            .get(&route.surface_id())
            .cloned()
            .unwrap_or_default();
        if candidate
            .invalidated_epoch
            .is_some_and(|epoch| route.presentation_epoch() > epoch)
        {
            candidate.invalidated_epoch = None;
        }

        let mut batch = UiDispatchBatch::new(target.revision(), route.presentation_epoch());
        if matches!(
            input,
            UiInputEvent::PointerButton { button, .. } if button != UiPointerButton::Primary
        ) {
            batch.reject(UiNonActivation::UnsupportedPointerButton);
            return Ok(batch);
        }
        if is_stale_geometry(&candidate, route, &input) {
            batch.reject(UiNonActivation::StaleGeometry);
            return Ok(batch);
        }
        preflight_input_identity(&mut candidate, &input)?;
        reconcile_surface(&mut candidate, target.frame(), latest.frame(), &mut batch);

        let remove_surface = apply_input(
            &mut candidate,
            target.frame(),
            latest.frame(),
            route,
            input,
            &mut batch,
        )?;
        self.ensure_event_limit(&batch)?;
        if remove_surface {
            self.surfaces.remove(&route.surface_id());
        } else {
            self.commit_surface(route.surface_id(), candidate);
        }
        Ok(batch)
    }

    /// Projects semantics from the latest submitted frame and current interaction state.
    pub fn semantic_snapshot(
        &self,
        journal: &UiFrameJournal,
        route: UiRouteStamp,
        seat: super::UiSeatId,
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
        let focused = effective_focus(state, seat, frame, latest_frame, modal_root.as_ref());
        let nodes = frame
            .semantics()
            .iter()
            .map(|node| {
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
                            *pointer_seat == seat && pointer.captured.as_ref() == Some(node.key())
                        })
                    });
                UiSemanticFact::from_frame_node(
                    node,
                    UiSemanticState::new(
                        node.enabled() && active,
                        inert || !active,
                        node.selected(),
                        node.expanded(),
                        focusable,
                        focused.as_ref() == Some(node.key()),
                        pressed,
                    ),
                )
            })
            .collect();
        Ok(UiSemanticSnapshot::new(
            route.surface_id(),
            target.revision(),
            route.presentation_epoch(),
            seat,
            nodes,
        ))
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
        let maximum = self.limits.max_nodes.saturating_mul(2);
        if batch.events().len() > maximum {
            return Err(UiDispatchError::RoutedEventLimitExceeded { maximum });
        }
        Ok(())
    }

    fn commit_surface(&mut self, surface: UiSurfaceId, state: SurfaceState) {
        self.surfaces.insert(surface, state);
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
) -> Result<bool, UiDispatchError> {
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
                .position = Some(position);
            let hovered = match hit_target(frame, latest, state.modal_root.as_ref(), position) {
                Ok(target) => target,
                Err(reason) => {
                    batch.reject(reason);
                    None
                }
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
                    .position = Some(position);
            }
            match phase {
                UiButtonPhase::Pressed => {
                    pointer_press(state, seat, pointer, frame, latest, batch);
                }
                UiButtonPhase::Released => {
                    pointer_release(state, seat, pointer, frame, latest, batch);
                }
            }
        }
        UiInputEvent::PointerCancelled { seat, pointer } => {
            cancel_pointer(state, seat, pointer, frame, batch, true);
        }
        UiInputEvent::Navigate { seat, direction } => {
            navigate_focus(state, seat, direction, frame, latest, batch);
        }
        UiInputEvent::ActivateFocused { seat, source } => {
            let Some(target) = state.seats.get(&seat).and_then(|seat| seat.focused.clone()) else {
                batch.reject(UiNonActivation::NoFocusTarget);
                return Ok(false);
            };
            activate_target(
                &target,
                None,
                source,
                frame,
                latest,
                state.modal_root.as_ref(),
                batch,
            );
        }
        UiInputEvent::FocusTarget { seat, target } => {
            if is_focusable_key(frame, latest, state.modal_root.as_ref(), &target) {
                set_focus(state, seat, Some(target), frame, batch);
            } else if let Some(reason) =
                target_rejection(frame, latest, state.modal_root.as_ref(), &target)
            {
                batch.reject(reason);
            } else if frame
                .focus_order()
                .iter()
                .any(|fact| fact.key() == &target && !fact.enabled())
            {
                batch.reject(UiNonActivation::TargetDisabled { target });
            } else {
                batch.reject(UiNonActivation::NoFocusTarget);
            }
        }
        UiInputEvent::SemanticAction {
            seat: _,
            target,
            action,
        } => {
            activate_target(
                &target,
                Some(action),
                UiActivationSource::Accessibility,
                frame,
                latest,
                state.modal_root.as_ref(),
                batch,
            );
        }
        UiInputEvent::SeatCancelled { seat } => {
            cancel_seat(state, seat, frame, batch);
        }
        UiInputEvent::GeometryInvalidated => {
            cancel_all_pointers(state, frame, batch);
            state.invalidated_epoch = Some(route.presentation_epoch());
        }
        UiInputEvent::SurfaceCancelled => {
            cancel_surface(state, frame, batch);
            return Ok(true);
        }
    }
    Ok(false)
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
    targets_geometry
        && state
            .invalidated_epoch
            .is_some_and(|epoch| route.presentation_epoch() <= epoch)
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
    #[error("UI dispatch exceeds {maximum} routed events")]
    RoutedEventLimitExceeded { maximum: usize },
    #[error("semantic snapshot requested epoch {requested}, but latest is {latest}")]
    SemanticSnapshotNotLatest { requested: u64, latest: u64 },
}
