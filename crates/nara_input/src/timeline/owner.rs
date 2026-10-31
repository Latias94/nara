use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use nara_app::{__RuntimeDriverPort, RuntimeState};
use nara_core::Vec2;
use nara_ecs::Resource;

use super::{
    capacity::{InputCapacity, InputCapacityError, InputCapacityPlan},
    topology::{EndpointId, InputTopology, Member},
    *,
};
use crate::{
    ButtonDriverInput, ButtonTransitionPhase, InputBinding, KeyCode, MouseButton,
    PointerDriverInput, StagedActionRevision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionRetention {
    Unscanned,
    Deferred,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionDelivery {
    OwnUiDisposition,
    Gameplay,
    Ui,
    FollowPress(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldInputDisposition {
    Pending,
    Gameplay,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeldInput {
    press_sequence: u64,
    route: InputRouteStamp,
    disposition: HeldInputDisposition,
}

impl HeldInput {
    fn new(press_sequence: u64, route: InputRouteStamp) -> Self {
        Self {
            press_sequence,
            route,
            disposition: if route.is_ui_routable() {
                HeldInputDisposition::Pending
            } else {
                HeldInputDisposition::Gameplay
            },
        }
    }

    fn action_delivery(self) -> ActionDelivery {
        match self.disposition {
            HeldInputDisposition::Pending => ActionDelivery::FollowPress(self.press_sequence),
            HeldInputDisposition::Gameplay => ActionDelivery::Gameplay,
            HeldInputDisposition::Ui => ActionDelivery::Ui,
        }
    }

    fn routes_through(self, route: InputRouteStamp) -> bool {
        self.disposition != HeldInputDisposition::Gameplay
            && self.route.is_ui_routable()
            && route.is_ui_routable()
            && self.route.window() == route.window()
            && self.route.seat() == route.seat()
            && self.route.pointer() == route.pointer()
            && self.route.surface() == route.surface()
            && self.route.slot_generation() == route.slot_generation()
            && self.route.route_generation() == route.route_generation()
            && self.route.geometry_generation() == route.geometry_generation()
    }
}

#[derive(Debug)]
struct RetainedInput {
    entry: InputTimelineEntry,
    projection_pending: bool,
    observation_pending: bool,
    frame_pending: bool,
    ui_pending: bool,
    ui_consumed: bool,
    action: ActionRetention,
    action_delivery: ActionDelivery,
    action_revision: Option<Arc<StagedActionRevision>>,
}

#[derive(Debug, Clone, Copy)]
struct RouteState {
    route: InputPresentedRoute,
    binding_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RouteDeclaration {
    route: InputPresentedRoute,
    endpoint: EndpointId,
    binding_generation: u64,
}

#[derive(Debug)]
struct RouteReservation {
    declaration: RouteDeclaration,
    invalidation: Option<InputInvalidationReason>,
}

/// Exact route admission retained until its matching commit, abort or supersede.
#[must_use = "settle the exact prepared input route"]
#[derive(Debug)]
pub struct PreparedInputRoutePublication {
    owner: Arc<()>,
    declaration: RouteDeclaration,
}

/// Identity of one connected endpoint for coordinated owners such as a UI Surface.
///
/// This retains no mutable slot. A retired/recycled label cannot retarget the original claim.
/// Equality compares the original owner and endpoint lifetime, independently of raw labels.
#[derive(Debug, Clone)]
pub struct InputEndpointClaim {
    owner: Arc<()>,
    endpoint: EndpointId,
    window: InputWindowId,
    seat: InputSeatId,
}

impl PartialEq for InputEndpointClaim {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.endpoint == other.endpoint
    }
}

impl Eq for InputEndpointClaim {}

impl InputEndpointClaim {
    #[must_use]
    pub const fn window(&self) -> InputWindowId {
        self.window
    }
    #[must_use]
    pub const fn seat(&self) -> InputSeatId {
        self.seat
    }
}

/// Backend completion cannot reinstate authority revoked after preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRouteCommit {
    Published(InputRouteStamp),
    Invalidated { reason: InputInvalidationReason },
}

#[derive(Debug, Clone, Copy)]
struct InputDraft {
    route: InputRouteStamp,
    origin: InputEventOrigin,
    logical_position: Option<Vec2>,
    kind: InputEventKind,
    invalidation: Option<InputInvalidationReason>,
    press_origin: Option<(u64, InputRouteStamp)>,
    action_delivery: ActionDelivery,
}

/// One finite owner of accepted physical/synthetic members, terminal promises and consumer progress.
#[derive(Debug, Resource)]
pub struct InputTimeline {
    owner: Arc<()>,
    limits: InputLimits,
    pub(super) topology: InputTopology,
    pub(super) capacity: InputCapacity,
    retained: VecDeque<RetainedInput>,
    scratch: Vec<InputDraft>,
    observation_cursor: u64,
    ui_cursor: u64,
    action_scanned_cursor: u64,
    routes: BTreeMap<(InputWindowId, InputSeatId), RouteState>,
    prepared_routes: BTreeMap<(InputWindowId, InputSeatId), RouteReservation>,
    pub(super) next_binding_generation: u64,
}

impl Default for InputTimeline {
    fn default() -> Self {
        Self::with_limits(InputLimits::default())
            .expect("default Input storage is finite and allocatable")
    }
}

impl InputTimeline {
    fn claim_endpoint(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<InputEndpointClaim, InputIngressError> {
        let index = self.topology.connected_endpoint(window, seat)?;
        Ok(InputEndpointClaim {
            owner: Arc::clone(&self.owner),
            endpoint: self.topology.endpoints[index].unwrap().id,
            window,
            seat,
        })
    }

    fn owns_claim(&self, claim: &InputEndpointClaim) -> Result<bool, InputIngressError> {
        if !Arc::ptr_eq(&self.owner, &claim.owner) {
            return Err(InputIngressError::ForeignEndpointClaim);
        }
        Ok(self
            .topology
            .endpoint_index(claim.window, claim.seat)
            .is_some_and(|index| self.topology.endpoints[index].unwrap().id == claim.endpoint))
    }

    pub fn with_limits(limits: InputLimits) -> Result<Self, InputConfigurationError> {
        let mut retained = VecDeque::new();
        retained
            .try_reserve_exact(limits.events())
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(limits.events())
            .map_err(|_| InputConfigurationError::AllocationRefused)?;
        let mut timeline = Self {
            owner: Arc::new(()),
            limits,
            topology: InputTopology::new(limits)?,
            capacity: InputCapacity::new(limits.events()),
            retained,
            scratch,
            observation_cursor: 0,
            ui_cursor: 0,
            action_scanned_cursor: 0,
            routes: BTreeMap::new(),
            prepared_routes: BTreeMap::new(),
            next_binding_generation: 0,
        };
        timeline
            .connect_endpoint(InputWindowId::UNSPECIFIED, InputSeatId::PRIMARY)
            .expect("validated event capacity reserves default lifecycle controls");
        timeline
            .connect_pointer(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            )
            .expect("validated pointer capacity includes the default synthetic pointer");
        Ok(timeline)
    }

    #[must_use]
    pub const fn limits(&self) -> InputLimits {
        self.limits
    }
    #[must_use]
    pub fn retained_len(&self) -> usize {
        self.retained.len()
    }

    pub fn entries(&self) -> impl Iterator<Item = &InputTimelineEntry> {
        self.retained.iter().map(|record| &record.entry)
    }
    pub fn pending_ui_entries(&self) -> impl Iterator<Item = &InputTimelineEntry> {
        self.retained
            .iter()
            .filter(|record| record.ui_pending)
            .map(|record| &record.entry)
    }

    #[must_use]
    pub fn cursors(&self) -> InputConsumerCursors {
        InputConsumerCursors {
            observation: self.observation_cursor,
            ui: self.ui_cursor,
            action_scanned: self.action_scanned_cursor,
            oldest_deferred_action: self
                .retained
                .iter()
                .find(|record| record.action == ActionRetention::Deferred)
                .map(|record| record.entry.sequence()),
        }
    }

    /// Connects one raw adapter endpoint. Pending retirement must finish before label reuse.
    pub fn connect_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
    ) -> Result<(), InputIngressError> {
        if let Some(index) = self.topology.endpoint_index(window, seat) {
            return self
                .topology
                .connected_endpoint(window, seat)
                .map(|connected| {
                    debug_assert_eq!(index, connected);
                });
        }
        let slot = self.topology.free_endpoint()?;
        let identity = self.topology.next_identity()?;
        let plan = self.plan(0, self.topology.obligations() + 3)?;
        self.commit_capacity(plan);
        self.topology.install_endpoint(slot, identity, window, seat);
        self.assert_accounting();
        Ok(())
    }

    pub fn connect_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    ) -> Result<(), InputIngressError> {
        let endpoint = self.topology.connected_endpoint(window, seat)?;
        if self
            .topology
            .pointer_index(self.topology.endpoints[endpoint].unwrap().id, pointer)
            .is_some()
        {
            return self
                .topology
                .connected_pointer(endpoint, pointer)
                .map(|_| ());
        }
        let slot = self.topology.free_pointer()?;
        let identity = self.topology.next_identity()?;
        self.topology.install_pointer(
            slot,
            identity,
            self.topology.endpoints[endpoint].unwrap().id,
            pointer,
        );
        Ok(())
    }

    pub fn publish_presented_route(
        &mut self,
        route: InputPresentedRoute,
    ) -> Result<InputRouteStamp, InputIngressError> {
        let prepared = self.prepare_presented_route(route)?;
        match self.commit_presented_route(prepared) {
            InputRouteCommit::Published(stamp) => Ok(stamp),
            InputRouteCommit::Invalidated { .. } => {
                unreachable!("immediate route publication cannot interleave platform loss")
            }
        }
    }

    fn prepare_presented_route(
        &mut self,
        route: InputPresentedRoute,
    ) -> Result<PreparedInputRoutePublication, InputIngressError> {
        let key = (route.window(), route.seat());
        let endpoint = self.topology.connected_endpoint(key.0, key.1)?;
        if self.prepared_routes.contains_key(&key) {
            return Err(InputIngressError::RouteReservationConflict {
                window: key.0,
                seat: key.1,
            });
        }
        if !self.routes.contains_key(&key)
            && self.routes.len()
                + self
                    .prepared_routes
                    .keys()
                    .filter(|key| !self.routes.contains_key(key))
                    .count()
                >= MAX_INPUT_ROUTES
        {
            return Err(InputIngressError::RouteCapacityExceeded {
                limit: MAX_INPUT_ROUTES,
            });
        }
        let previous = self.routes.get(&key).copied();
        if let Some(current) = previous {
            if current.route.surface() != route.surface() {
                return Err(InputIngressError::RouteConflict {
                    window: key.0,
                    seat: key.1,
                    current_surface: current.route.surface(),
                    requested_surface: route.surface(),
                });
            }
            if route.slot_generation() < current.route.slot_generation()
                || route.route_generation() < current.route.route_generation()
                || route.geometry_generation() < current.route.geometry_generation()
                || (route.slot_generation() == current.route.slot_generation()
                    && route.presentation_epoch() < current.route.presentation_epoch())
            {
                return Err(InputIngressError::PresentedRouteRegression {
                    window: key.0,
                    seat: key.1,
                });
            }
        }
        let exact = previous.filter(|current| current.route == route);
        let generation = exact.map_or_else(
            || {
                self.next_binding_generation
                    .checked_add(1)
                    .ok_or(InputIngressError::RouteBindingGenerationExhausted)
            },
            |current| Ok(current.binding_generation),
        )?;
        let record = self.topology.endpoints[endpoint].unwrap();
        let after = self.topology.obligations() - record.obligations() + 4;
        let plan = self.plan(0, after)?;
        let declaration = RouteDeclaration {
            route,
            endpoint: record.id,
            binding_generation: generation,
        };
        self.commit_capacity(plan);
        self.topology.endpoints[endpoint].as_mut().unwrap().rearm();
        self.topology.endpoints[endpoint].as_mut().unwrap().geometry = true;
        self.next_binding_generation = self.next_binding_generation.max(generation);
        self.prepared_routes.insert(
            key,
            RouteReservation {
                declaration,
                invalidation: None,
            },
        );
        self.assert_accounting();
        Ok(PreparedInputRoutePublication {
            owner: Arc::clone(&self.owner),
            declaration,
        })
    }

    fn route_reservation(&self, token: &PreparedInputRoutePublication) -> &RouteReservation {
        assert!(
            Arc::ptr_eq(&self.owner, &token.owner),
            "a prepared route belongs to its original Input owner"
        );
        let key = (
            token.declaration.route.window(),
            token.declaration.route.seat(),
        );
        let active = self
            .prepared_routes
            .get(&key)
            .expect("an exact token retains its reservation");
        assert_eq!(
            active.declaration, token.declaration,
            "route completion consumes its exact declaration"
        );
        active
    }

    fn consume_route(&mut self, token: PreparedInputRoutePublication) -> RouteReservation {
        self.route_reservation(&token);
        let key = (
            token.declaration.route.window(),
            token.declaration.route.seat(),
        );
        self.prepared_routes.remove(&key).unwrap()
    }

    fn commit_presented_route(&mut self, token: PreparedInputRoutePublication) -> InputRouteCommit {
        let reservation = self.consume_route(token);
        let route = reservation.declaration.route;
        if let Some(reason) = reservation.invalidation {
            self.refund_unused_geometry(route.window(), route.seat());
            self.reclaim_completed();
            return InputRouteCommit::Invalidated { reason };
        }
        let endpoint = self
            .topology
            .endpoint_index(route.window(), route.seat())
            .expect("a valid reservation retains its connected endpoint");
        let record = self.topology.endpoints[endpoint].as_mut().unwrap();
        assert_eq!(record.id, reservation.declaration.endpoint);
        record.last_binding_generation = reservation.declaration.binding_generation;
        self.routes.insert(
            (route.window(), route.seat()),
            RouteState {
                route,
                binding_generation: reservation.declaration.binding_generation,
            },
        );
        InputRouteCommit::Published(self.pointer_route(route.window(), route.seat(), None))
    }

    fn release_route(&mut self, token: PreparedInputRoutePublication) {
        let reservation = self.consume_route(token);
        let declaration = reservation.declaration;
        self.refund_unused_geometry(declaration.route.window(), declaration.route.seat());
        self.reclaim_completed();
    }

    fn ensure_unreserved(
        &self,
        key: (InputWindowId, InputSeatId),
    ) -> Result<(), InputIngressError> {
        if self
            .prepared_routes
            .get(&key)
            .is_some_and(|reservation| reservation.invalidation.is_none())
        {
            return Err(InputIngressError::RouteReservationConflict {
                window: key.0,
                seat: key.1,
            });
        }
        Ok(())
    }

    fn validate_route(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        expected: Option<InputRouteStamp>,
    ) -> Result<(), InputIngressError> {
        if expected.is_some_and(|stamp| stamp.window() != window || stamp.seat() != seat)
            || expected.is_some_and(|stamp| {
                !stamp.is_ui_routable()
                    || self
                        .topology
                        .endpoint_index(window, seat)
                        .is_some_and(|index| {
                            self.topology.endpoints[index]
                                .unwrap()
                                .last_binding_generation
                                != stamp.binding_generation()
                        })
            })
            || (self.routes.contains_key(&(window, seat))
                && expected
                    .is_none_or(|stamp| self.pointer_route(window, seat, stamp.pointer()) != stamp))
        {
            return Err(InputIngressError::PresentedRouteMismatch { window, seat });
        }
        Ok(())
    }

    pub fn unbind_presented_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        expected: Option<InputRouteStamp>,
    ) -> Result<(), InputIngressError> {
        self.ensure_unreserved((window, seat))?;
        self.validate_route(window, seat, expected)?;
        self.routes.remove(&(window, seat));
        if let Some(index) = self.topology.endpoint_index(window, seat) {
            let id = self.topology.endpoints[index].unwrap().id;
            for pointer in self
                .topology
                .pointers
                .iter_mut()
                .flatten()
                .filter(|pointer| pointer.endpoint == id)
            {
                pointer.position = None;
            }
            self.refund_unused_geometry(window, seat);
        }
        Ok(())
    }

    pub fn retire_presented_endpoint(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        expected: Option<InputRouteStamp>,
    ) -> Result<InputEndpointRetirement, InputIngressError> {
        self.ensure_unreserved((window, seat))?;
        self.validate_route(window, seat, expected)?;
        if self.topology.endpoint_index(window, seat).is_none() {
            return Ok(InputEndpointRetirement::new(
                InputAdmission::IGNORED,
                Vec::new(),
                Vec::new(),
            ));
        }
        let output = self.apply_window(window, seat, InputWindowEvent::SurfaceRetired)?;
        Ok(InputEndpointRetirement::new(
            output.admission,
            output.released_keys,
            output.released_mouse_buttons,
        ))
    }

    pub fn admit_keyboard(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        input: ButtonDriverInput<KeyCode>,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_keyboard(window, seat, input, InputEventOrigin::Platform)
            .map(|output| output.admission)
    }
    pub fn admit_mouse_button(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: ButtonDriverInput<MouseButton>,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_mouse(window, seat, pointer, input, InputEventOrigin::Platform)
            .map(|output| output.admission)
    }
    pub fn admit_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: PointerDriverInput,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_pointer(
            window,
            seat,
            pointer,
            input,
            InputEventOrigin::Platform,
            false,
        )
        .map(|output| output.admission)
    }
    pub fn retire_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Left,
            InputEventOrigin::Platform,
            true,
        )
        .map(|output| output.admission)
    }
    pub fn admit_window_event(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        event: InputWindowEvent,
    ) -> Result<InputAdmission, InputIngressError> {
        self.apply_window(window, seat, event)
            .map(|output| output.admission)
    }

    fn pointer_route(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: Option<InputPointerId>,
    ) -> InputRouteStamp {
        let Some(state) = self.routes.get(&(window, seat)) else {
            return InputRouteStamp::unrouted(window, seat, pointer);
        };
        let route = state.route;
        InputRouteStamp {
            window,
            seat,
            pointer,
            surface: Some(route.surface()),
            presentation_epoch: Some(route.presentation_epoch()),
            slot_generation: Some(route.slot_generation()),
            route_generation: Some(route.route_generation()),
            geometry_generation: Some(route.geometry_generation()),
            binding_generation: state.binding_generation,
        }
    }

    fn ordinary_draft(
        &self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: Option<InputPointerId>,
        kind: InputEventKind,
        origin: InputEventOrigin,
    ) -> InputDraft {
        let logical_position = pointer.and_then(|label| {
            let endpoint = self.topology.endpoint_index(window, seat)?;
            let pointer = self
                .topology
                .pointer_index(self.topology.endpoints[endpoint]?.id, label)?;
            self.topology.pointers[pointer]?.position
        });
        InputDraft {
            route: self.pointer_route(window, seat, pointer),
            origin,
            logical_position,
            kind,
            invalidation: None,
            press_origin: None,
            action_delivery: ActionDelivery::OwnUiDisposition,
        }
    }

    fn release_draft(&self, member: Member, reason: Option<InputInvalidationReason>) -> InputDraft {
        let original = member.held.route;
        let current = self.pointer_route(original.window(), original.seat(), original.pointer());
        let route = if member.held.routes_through(current) {
            current
        } else {
            InputRouteStamp::unrouted(original.window(), original.seat(), original.pointer())
        };
        let kind = match member.input {
            InputBinding::Key(key) => InputEventKind::Key {
                key,
                phase: ButtonTransitionPhase::Released,
            },
            InputBinding::Mouse(button) => InputEventKind::PointerButton {
                button,
                phase: ButtonTransitionPhase::Released,
            },
        };
        let mut draft = self.ordinary_draft(
            route.window(),
            route.seat(),
            route.pointer(),
            kind,
            member.origin,
        );
        draft.route = route;
        draft.invalidation = reason;
        draft.press_origin = Some((member.held.press_sequence, original));
        draft.action_delivery = member.held.action_delivery();
        draft
    }

    fn plan(
        &self,
        drafts: usize,
        obligations: usize,
    ) -> Result<InputCapacityPlan, InputIngressError> {
        self.capacity
            .plan(drafts, obligations)
            .map_err(|error| self.capacity_error(error, drafts, obligations))
    }
    fn terminal_plan(&self, drafts: usize, obligations: usize) -> InputCapacityPlan {
        self.capacity
            .plan_terminal(drafts, obligations)
            .expect("accepted lifetimes reserve every terminal record and its sequence")
    }
    fn capacity_error(
        &self,
        error: InputCapacityError,
        drafts: usize,
        obligations: usize,
    ) -> InputIngressError {
        match error {
            InputCapacityError::Capacity => InputIngressError::CapacityExceeded {
                limit: self.limits.events(),
                retained: self.retained.len(),
                requested: drafts.saturating_add(obligations),
            },
            InputCapacityError::Sequence | InputCapacityError::Arithmetic => {
                InputIngressError::SequenceExhausted {
                    requested: drafts.saturating_add(obligations),
                }
            }
        }
    }
    fn commit_capacity(&mut self, plan: InputCapacityPlan) {
        self.capacity
            .commit(plan)
            .expect("one immediate owner mutation consumes its own capacity plan");
    }

    fn publish(&mut self, plan: InputCapacityPlan, drafts: &[InputDraft]) -> InputAdmission {
        let first = plan.first_sequence();
        let last = plan.last_sequence();
        self.commit_capacity(plan);
        for (offset, draft) in drafts.iter().copied().enumerate() {
            let sequence =
                first.expect("nonempty draft batch has a first sequence") + offset as u64;
            let press = match draft.kind {
                InputEventKind::Key {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
                | InputEventKind::PointerButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                } => Some((sequence, draft.route)),
                _ => draft.press_origin,
            };
            self.retained.push_back(RetainedInput {
                entry: InputTimelineEntry {
                    sequence,
                    origin: draft.origin,
                    route: draft.route,
                    logical_position: draft.logical_position,
                    kind: draft.kind,
                    invalidation: draft.invalidation,
                    press_sequence: press.map(|origin| origin.0),
                    press_route: press.map(|origin| origin.1),
                },
                projection_pending: true,
                observation_pending: true,
                frame_pending: true,
                ui_pending: draft.route.is_ui_routable(),
                ui_consumed: false,
                action: ActionRetention::Unscanned,
                action_delivery: draft.action_delivery,
                action_revision: None,
            });
        }
        InputAdmission {
            first_sequence: first,
            last_sequence: last,
            admitted: drafts.len(),
        }
    }

    fn with_drafts<R>(&mut self, action: impl FnOnce(&mut Self, &mut Vec<InputDraft>) -> R) -> R {
        let mut scratch = std::mem::take(&mut self.scratch);
        debug_assert!(scratch.is_empty());
        let result = action(self, &mut scratch);
        scratch.clear();
        self.scratch = scratch;
        result
    }

    fn assert_accounting(&self) {
        debug_assert_eq!(self.topology.obligations(), self.capacity.obligations());
        debug_assert_eq!(self.retained.len(), self.capacity.retained());
    }
}

impl InputTimeline {
    fn apply_keyboard(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        input: ButtonDriverInput<KeyCode>,
        origin: InputEventOrigin,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.connected_endpoint(window, seat)?;
        match input {
            ButtonDriverInput::Press(key) => {
                self.press(endpoint, None, InputBinding::Key(key), origin)
            }
            ButtonDriverInput::Release(key) => self.release(endpoint, None, InputBinding::Key(key)),
            ButtonDriverInput::ReleaseAll => self.release_all(endpoint, None, false),
        }
    }

    fn apply_mouse(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        input: ButtonDriverInput<MouseButton>,
        origin: InputEventOrigin,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.connected_endpoint(window, seat)?;
        let pointer = self.topology.connected_pointer(endpoint, pointer)?;
        match input {
            ButtonDriverInput::Press(button) => {
                self.press(endpoint, Some(pointer), InputBinding::Mouse(button), origin)
            }
            ButtonDriverInput::Release(button) => {
                self.release(endpoint, Some(pointer), InputBinding::Mouse(button))
            }
            ButtonDriverInput::ReleaseAll => self.release_all(endpoint, Some(pointer), true),
        }
    }

    fn press(
        &mut self,
        endpoint_index: usize,
        pointer_index: Option<usize>,
        input: InputBinding,
        origin: InputEventOrigin,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let pointer = pointer_index.map(|index| self.topology.pointers[index].unwrap());
        if self
            .topology
            .member_index(endpoint.id, pointer.map(|record| record.id), input)
            .is_some()
        {
            return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
        }
        let member_slot = self.topology.free_member()?;
        let after = self.topology.obligations() - endpoint.obligations()
            + endpoint.rearmed_obligations()
            + 1
            + usize::from(pointer.is_some_and(|pointer| !pointer.leave));
        let plan = self.plan(1, after)?;
        let kind = match input {
            InputBinding::Key(key) => InputEventKind::Key {
                key,
                phase: ButtonTransitionPhase::Pressed,
            },
            InputBinding::Mouse(button) => InputEventKind::PointerButton {
                button,
                phase: ButtonTransitionPhase::Pressed,
            },
        };
        let draft = self.ordinary_draft(
            endpoint.window,
            endpoint.seat,
            pointer.map(|record| record.label),
            kind,
            origin,
        );
        let admission = self.publish(plan, &[draft]);
        self.topology.endpoints[endpoint_index]
            .as_mut()
            .unwrap()
            .rearm();
        if let Some(index) = pointer_index {
            self.topology.pointers[index].as_mut().unwrap().leave = true;
        }
        self.topology.members[member_slot] = Some(Member {
            endpoint: endpoint.id,
            pointer: pointer.map(|record| record.id),
            input,
            pressed: true,
            held: HeldInput::new(admission.first_sequence().unwrap(), draft.route),
            origin,
        });
        self.assert_accounting();
        Ok(__InputDriverOutput::new(admission))
    }

    fn release(
        &mut self,
        endpoint_index: usize,
        pointer_index: Option<usize>,
        input: InputBinding,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let pointer = pointer_index.map(|index| self.topology.pointers[index].unwrap().id);
        let Some(index) = self.topology.member_index(endpoint.id, pointer, input) else {
            return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
        };
        let draft = self.release_draft(self.topology.members[index].unwrap(), None);
        let plan = self.terminal_plan(1, self.topology.obligations() - 1);
        let admission = self.publish(plan, &[draft]);
        self.topology.members[index].as_mut().unwrap().pressed = false;
        self.assert_accounting();
        Ok(__InputDriverOutput::new(admission))
    }

    fn release_all(
        &mut self,
        endpoint_index: usize,
        pointer_index: Option<usize>,
        mouse: bool,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap().id;
        let pointer = pointer_index.map(|index| self.topology.pointers[index].unwrap().id);
        self.with_drafts(|owner, drafts| {
            for member in owner
                .topology
                .members
                .iter()
                .flatten()
                .copied()
                .filter(|member| {
                    member.pressed
                        && member.endpoint == endpoint
                        && member.pointer == pointer
                        && matches!(
                            (member.input, mouse),
                            (InputBinding::Key(_), false) | (InputBinding::Mouse(_), true)
                        )
                })
            {
                drafts.push(owner.release_draft(member, None));
            }
            drafts.sort_by_key(|draft| draft.press_origin.unwrap().0);
            let plan =
                owner.terminal_plan(drafts.len(), owner.topology.obligations() - drafts.len());
            let admission = owner.publish(plan, drafts);
            for member in owner
                .topology
                .members
                .iter_mut()
                .flatten()
                .filter(|member| {
                    member.pressed
                        && member.endpoint == endpoint
                        && member.pointer == pointer
                        && matches!(
                            (member.input, mouse),
                            (InputBinding::Key(_), false) | (InputBinding::Mouse(_), true)
                        )
                })
            {
                member.pressed = false;
            }
            owner.assert_accounting();
            Ok(Self::release_output(admission, drafts))
        })
    }

    fn release_output(admission: InputAdmission, drafts: &[InputDraft]) -> __InputDriverOutput {
        __InputDriverOutput {
            admission,
            released_keys: drafts
                .iter()
                .filter_map(|draft| match draft.kind {
                    InputEventKind::Key {
                        key,
                        phase: ButtonTransitionPhase::Released,
                    } => Some(key),
                    _ => None,
                })
                .collect(),
            released_mouse_buttons: drafts
                .iter()
                .filter_map(|draft| match draft.kind {
                    InputEventKind::PointerButton {
                        button,
                        phase: ButtonTransitionPhase::Released,
                    } => Some(button),
                    _ => None,
                })
                .collect(),
        }
    }

    fn apply_pointer(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        label: InputPointerId,
        input: PointerDriverInput,
        origin: InputEventOrigin,
        retire: bool,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint_index = self.topology.connected_endpoint(window, seat)?;
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let pointer_index = self.topology.connected_pointer(endpoint_index, label)?;
        let pointer = self.topology.pointers[pointer_index].unwrap();
        match input {
            PointerDriverInput::Moved(position) => {
                if !position.is_finite() {
                    return Err(InputIngressError::NonFinitePosition);
                }
                let after = self.topology.obligations() - endpoint.obligations()
                    + 4
                    + usize::from(!pointer.leave);
                let plan = self.plan(1, after)?;
                let mut draft = self.ordinary_draft(
                    window,
                    seat,
                    Some(label),
                    InputEventKind::PointerMoved,
                    origin,
                );
                draft.logical_position = Some(position);
                let admission = self.publish(plan, &[draft]);
                self.topology.endpoints[endpoint_index]
                    .as_mut()
                    .unwrap()
                    .rearm();
                self.topology.endpoints[endpoint_index]
                    .as_mut()
                    .unwrap()
                    .geometry = true;
                let pointer = self.topology.pointers[pointer_index].as_mut().unwrap();
                pointer.position = Some(position);
                pointer.leave = true;
                self.assert_accounting();
                Ok(__InputDriverOutput::new(admission))
            }
            PointerDriverInput::Left => self.with_drafts(|owner, drafts| {
                for member in owner
                    .topology
                    .members
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|member| member.pressed && member.pointer == Some(pointer.id))
                {
                    drafts.push(
                        owner.release_draft(member, Some(InputInvalidationReason::PointerLeft)),
                    );
                }
                drafts.sort_by_key(|draft| draft.press_origin.unwrap().0);
                let releases = drafts.len();
                if pointer.leave {
                    let mut draft = owner.ordinary_draft(
                        window,
                        seat,
                        Some(label),
                        InputEventKind::PointerLeft,
                        origin,
                    );
                    draft.invalidation = Some(InputInvalidationReason::PointerLeft);
                    drafts.push(draft);
                }
                let invalidates_prepared = pointer.leave || releases != 0;
                let geometry_ends = endpoint.geometry
                    && !owner.routes.contains_key(&(window, seat))
                    && !owner
                        .prepared_routes
                        .get(&(window, seat))
                        .is_some_and(|reservation| {
                            reservation.invalidation.is_none() && !invalidates_prepared
                        })
                    && !owner.topology.pointers.iter().flatten().any(|other| {
                        other.id != pointer.id
                            && other.endpoint == endpoint.id
                            && other.position.is_some()
                    });
                let after = owner.topology.obligations()
                    - releases
                    - usize::from(pointer.leave)
                    - usize::from(geometry_ends);
                let plan = owner.terminal_plan(drafts.len(), after);
                let admission = owner.publish(plan, drafts);
                if invalidates_prepared
                    && let Some(reservation) = owner.prepared_routes.get_mut(&(window, seat))
                {
                    reservation
                        .invalidation
                        .get_or_insert(InputInvalidationReason::PointerLeft);
                }
                for member in owner
                    .topology
                    .members
                    .iter_mut()
                    .flatten()
                    .filter(|member| member.pointer == Some(pointer.id))
                {
                    member.pressed = false;
                }
                let state = owner.topology.pointers[pointer_index].as_mut().unwrap();
                state.position = None;
                state.leave = false;
                state.retiring = retire;
                if geometry_ends {
                    owner.topology.endpoints[endpoint_index]
                        .as_mut()
                        .unwrap()
                        .geometry = false;
                }
                owner.assert_accounting();
                owner.reclaim_completed();
                Ok(Self::release_output(admission, drafts))
            }),
        }
    }

    pub fn admit_wheel(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        pointer: InputPointerId,
        delta: Vec2,
    ) -> Result<InputAdmission, InputIngressError> {
        if !delta.is_finite() {
            return Err(InputIngressError::NonFinitePosition);
        }
        let endpoint = self.topology.connected_endpoint(window, seat)?;
        self.topology.connected_pointer(endpoint, pointer)?;
        self.admit_ordinary(
            endpoint,
            Some(pointer),
            InputEventKind::Wheel { delta },
            InputEventOrigin::Platform,
        )
    }

    pub fn admit_gamepad_button(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
        button: u16,
        phase: ButtonTransitionPhase,
    ) -> Result<InputAdmission, InputIngressError> {
        self.gamepad(
            window,
            seat,
            gamepad,
            button,
            phase,
            InputEventOrigin::Platform,
        )
    }

    fn gamepad(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        gamepad: u16,
        button: u16,
        phase: ButtonTransitionPhase,
        origin: InputEventOrigin,
    ) -> Result<InputAdmission, InputIngressError> {
        let endpoint = self.topology.connected_endpoint(window, seat)?;
        self.admit_ordinary(
            endpoint,
            None,
            InputEventKind::GamepadButton {
                gamepad,
                button,
                phase,
            },
            origin,
        )
    }

    fn admit_ordinary(
        &mut self,
        endpoint_index: usize,
        pointer: Option<InputPointerId>,
        kind: InputEventKind,
        origin: InputEventOrigin,
    ) -> Result<InputAdmission, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let after =
            self.topology.obligations() - endpoint.obligations() + endpoint.rearmed_obligations();
        let plan = self.plan(1, after)?;
        let draft = self.ordinary_draft(endpoint.window, endpoint.seat, pointer, kind, origin);
        let admission = self.publish(plan, &[draft]);
        self.topology.endpoints[endpoint_index]
            .as_mut()
            .unwrap()
            .rearm();
        self.assert_accounting();
        Ok(admission)
    }

    fn apply_window(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        event: InputWindowEvent,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint_index = self
            .topology
            .endpoint_index(window, seat)
            .ok_or(InputIngressError::UnknownEndpoint { window, seat })?;
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        if endpoint.retiring {
            return Ok(__InputDriverOutput::new(InputAdmission::IGNORED));
        }
        match event {
            InputWindowEvent::Focused(true) => {
                let admission = self.admit_ordinary(
                    endpoint_index,
                    None,
                    InputEventKind::FocusChanged { focused: true },
                    InputEventOrigin::Platform,
                )?;
                for record in self
                    .topology
                    .endpoints
                    .iter_mut()
                    .flatten()
                    .filter(|record| record.seat == seat)
                {
                    record.focused = record.id == endpoint.id;
                }
                Ok(__InputDriverOutput::new(admission))
            }
            InputWindowEvent::Resized { logical_size } => {
                if !logical_size.is_finite() || logical_size.x < 0.0 || logical_size.y < 0.0 {
                    return Err(InputIngressError::InvalidLogicalSize);
                }
                self.geometry_loss(
                    endpoint_index,
                    InputEventKind::Resized { logical_size },
                    InputInvalidationReason::Resized,
                )
            }
            InputWindowEvent::ScaleFactorChanged { scale_factor } => {
                if !scale_factor.is_finite() || scale_factor <= 0.0 {
                    return Err(InputIngressError::InvalidScaleFactor);
                }
                self.geometry_loss(
                    endpoint_index,
                    InputEventKind::ScaleFactorChanged { scale_factor },
                    InputInvalidationReason::ScaleFactorChanged,
                )
            }
            InputWindowEvent::Focused(false) => self.physical_loss(
                endpoint_index,
                InputEventKind::FocusChanged { focused: false },
                InputInvalidationReason::FocusLost,
            ),
            InputWindowEvent::CloseRequested => self.physical_loss(
                endpoint_index,
                InputEventKind::CloseRequested,
                InputInvalidationReason::CloseRequested,
            ),
            InputWindowEvent::SurfaceRetired => self.physical_loss(
                endpoint_index,
                InputEventKind::SurfaceRetired,
                InputInvalidationReason::SurfaceRetired,
            ),
        }
    }

    fn revoke_route(
        &mut self,
        window: InputWindowId,
        seat: InputSeatId,
        reason: InputInvalidationReason,
    ) {
        self.routes.remove(&(window, seat));
        if let Some(reservation) = self.prepared_routes.get_mut(&(window, seat)) {
            reservation.invalidation.get_or_insert(reason);
        }
    }

    fn geometry_loss(
        &mut self,
        endpoint_index: usize,
        kind: InputEventKind,
        reason: InputInvalidationReason,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        if !endpoint.geometry {
            return self
                .admit_ordinary(endpoint_index, None, kind, InputEventOrigin::Platform)
                .map(__InputDriverOutput::new);
        }
        let mut draft = self.ordinary_draft(
            endpoint.window,
            endpoint.seat,
            None,
            kind,
            InputEventOrigin::Platform,
        );
        draft.invalidation = Some(reason);
        let plan = self.terminal_plan(1, self.topology.obligations() - 1);
        let admission = self.publish(plan, &[draft]);
        self.revoke_route(endpoint.window, endpoint.seat, reason);
        for pointer in self
            .topology
            .pointers
            .iter_mut()
            .flatten()
            .filter(|pointer| pointer.endpoint == endpoint.id)
        {
            pointer.position = None;
        }
        self.topology.endpoints[endpoint_index]
            .as_mut()
            .unwrap()
            .geometry = false;
        self.assert_accounting();
        Ok(__InputDriverOutput::new(admission))
    }

    fn physical_loss(
        &mut self,
        endpoint_index: usize,
        kind: InputEventKind,
        reason: InputInvalidationReason,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let endpoint = self.topology.endpoints[endpoint_index].unwrap();
        let retiring = reason == InputInvalidationReason::SurfaceRetired;
        let control = match reason {
            InputInvalidationReason::FocusLost => endpoint.focus_loss,
            InputInvalidationReason::CloseRequested => endpoint.close,
            InputInvalidationReason::SurfaceRetired => endpoint.retirement,
            _ => unreachable!("physical loss has one terminal control"),
        };
        self.with_drafts(|owner, drafts| {
            for member in owner
                .topology
                .members
                .iter()
                .flatten()
                .copied()
                .filter(|member| member.endpoint == endpoint.id && member.pressed)
            {
                drafts.push(owner.release_draft(member, Some(reason)));
            }
            drafts.sort_by_key(|draft| draft.press_origin.unwrap().0);
            let releases = drafts.len();
            let leaves = owner
                .topology
                .pointers
                .iter()
                .flatten()
                .filter(|pointer| pointer.endpoint == endpoint.id && pointer.leave)
                .count();
            for pointer in owner
                .topology
                .pointers
                .iter()
                .flatten()
                .filter(|pointer| pointer.endpoint == endpoint.id && pointer.position.is_some())
            {
                let mut draft = owner.ordinary_draft(
                    endpoint.window,
                    endpoint.seat,
                    Some(pointer.label),
                    InputEventKind::PointerLeft,
                    InputEventOrigin::Platform,
                );
                draft.invalidation = Some(reason);
                drafts.push(draft);
            }
            if control {
                let mut draft = owner.ordinary_draft(
                    endpoint.window,
                    endpoint.seat,
                    None,
                    kind,
                    InputEventOrigin::Platform,
                );
                draft.invalidation = Some(reason);
                drafts.push(draft);
            }
            let controls_after = if retiring {
                0
            } else {
                endpoint.obligations() - usize::from(control) - usize::from(endpoint.geometry)
            };
            let after = owner.topology.obligations() - releases - leaves - endpoint.obligations()
                + controls_after;
            let plan = owner.terminal_plan(drafts.len(), after);
            let admission = owner.publish(plan, drafts);
            owner.revoke_route(endpoint.window, endpoint.seat, reason);
            for member in owner
                .topology
                .members
                .iter_mut()
                .flatten()
                .filter(|member| member.endpoint == endpoint.id)
            {
                member.pressed = false;
            }
            for pointer in owner
                .topology
                .pointers
                .iter_mut()
                .flatten()
                .filter(|pointer| pointer.endpoint == endpoint.id)
            {
                pointer.position = None;
                pointer.leave = false;
                pointer.retiring |= retiring;
            }
            let state = owner.topology.endpoints[endpoint_index].as_mut().unwrap();
            state.focused = false;
            state.geometry = false;
            state.retiring = retiring;
            match reason {
                InputInvalidationReason::FocusLost => state.focus_loss = false,
                InputInvalidationReason::CloseRequested => state.close = false,
                InputInvalidationReason::SurfaceRetired => {
                    state.focus_loss = false;
                    state.close = false;
                    state.retirement = false;
                }
                _ => unreachable!(),
            }
            owner.assert_accounting();
            owner.reclaim_completed();
            Ok(Self::release_output(admission, drafts))
        })
    }

    fn refund_unused_geometry(&mut self, window: InputWindowId, seat: InputSeatId) {
        let Some(index) = self.topology.endpoint_index(window, seat) else {
            return;
        };
        let endpoint = self.topology.endpoints[index].unwrap();
        if endpoint.geometry
            && !self.routes.contains_key(&(window, seat))
            && self
                .prepared_routes
                .get(&(window, seat))
                .is_none_or(|reservation| reservation.invalidation.is_some())
            && !self.topology.has_pointer_positions(endpoint.id)
        {
            let plan = self.terminal_plan(0, self.topology.obligations() - 1);
            self.commit_capacity(plan);
            self.topology.endpoints[index].as_mut().unwrap().geometry = false;
        }
        self.assert_accounting();
    }
}

impl InputTimeline {
    #[doc(hidden)]
    pub fn __finalize_ui_entry(
        &mut self,
        sequence: u64,
        disposition: InputUiDisposition,
    ) -> Result<(), InputUiCursorError> {
        let Some(record) = self
            .retained
            .iter_mut()
            .find(|record| record.entry.sequence() == sequence)
        else {
            return Err(InputUiCursorError::UnknownSequence { sequence });
        };
        if !record.ui_pending {
            return Err(InputUiCursorError::AlreadyFinalized { sequence });
        }
        record.ui_pending = false;
        record.ui_consumed = matches!(
            disposition,
            InputUiDisposition::Consumed | InputUiDisposition::RejectedClosed
        );
        let entry = record.entry;
        self.resolve_held_press(entry, true);
        Ok(())
    }

    fn resolve_held_press(&mut self, entry: InputTimelineEntry, consumed: bool) {
        if !matches!(
            entry.kind(),
            InputEventKind::Key {
                phase: ButtonTransitionPhase::Pressed,
                ..
            } | InputEventKind::PointerButton {
                phase: ButtonTransitionPhase::Pressed,
                ..
            }
        ) {
            return;
        }
        let sequence = entry.sequence();
        if let Some(member) = self
            .topology
            .members
            .iter_mut()
            .flatten()
            .find(|member| member.held.press_sequence == sequence)
        {
            member.held.disposition = if consumed {
                HeldInputDisposition::Ui
            } else {
                HeldInputDisposition::Gameplay
            };
        }
        for record in &mut self.retained {
            if record.action_delivery == ActionDelivery::FollowPress(sequence) {
                record.action_delivery = if consumed {
                    ActionDelivery::Ui
                } else {
                    ActionDelivery::Gameplay
                };
            }
        }
    }

    pub(crate) fn projection_entries(&self) -> Vec<InputTimelineEntry> {
        let mut entries = Vec::with_capacity(self.retained.len());
        entries.extend(
            self.retained
                .iter()
                .filter(|record| record.projection_pending)
                .map(|record| record.entry),
        );
        entries
    }
    pub(crate) fn finish_projection(&mut self) {
        for record in &mut self.retained {
            if record.projection_pending {
                record.frame_pending = true;
            }
            record.projection_pending = false;
        }
    }

    /// Frame copies pin their original records until every projection/observation buffer clears.
    pub(crate) fn finish_frame_observations(&mut self) {
        for record in &mut self.retained {
            record.frame_pending = false;
        }
    }

    pub(crate) fn publish_observations(&mut self, observations: &mut InputFrameObservations) {
        observations.entries.clear();
        for record in &mut self.retained {
            if record.observation_pending {
                record.frame_pending = true;
                observations.entries.push(record.entry);
                record.observation_pending = false;
                self.observation_cursor = record.entry.sequence();
            }
        }
    }

    pub(crate) fn finalize_ui_routing(&mut self) {
        let mut presses = Vec::with_capacity(self.retained.len());
        for record in &mut self.retained {
            if record.ui_pending {
                record.ui_pending = false;
                presses.push(record.entry);
            }
            self.ui_cursor = self.ui_cursor.max(record.entry.sequence());
        }
        for entry in presses {
            self.resolve_held_press(entry, false);
        }
    }

    pub(crate) fn stage_actions(&mut self, revision: &Arc<StagedActionRevision>) {
        for record in &mut self.retained {
            if record.action != ActionRetention::Unscanned {
                continue;
            }
            // Every button edge participates, including a release after its mapping/context ended.
            // Semantic reconciliation must finish before its admitted member can be recycled.
            record.action = if record.entry.action_transition().is_some() {
                ActionRetention::Deferred
            } else {
                ActionRetention::Done
            };
            if record.action == ActionRetention::Deferred {
                record.action_revision = Some(Arc::clone(revision));
            }
            self.action_scanned_cursor = record.entry.sequence();
        }
    }

    pub(crate) fn take_deferred_action_inputs(
        &mut self,
    ) -> Vec<(InputTimelineEntry, Arc<StagedActionRevision>)> {
        let mut entries = Vec::with_capacity(self.retained.len());
        for record in &mut self.retained {
            if record.action != ActionRetention::Deferred || record.ui_pending {
                continue;
            }
            let deliver = match record.action_delivery {
                ActionDelivery::OwnUiDisposition => !record.ui_consumed,
                ActionDelivery::Gameplay => true,
                ActionDelivery::Ui => false,
                ActionDelivery::FollowPress(sequence) => {
                    self.topology.members.iter().flatten().any(|member| {
                        member.held.press_sequence == sequence
                            && member.held.disposition == HeldInputDisposition::Gameplay
                    })
                }
            };
            record.action = ActionRetention::Done;
            let revision = record
                .action_revision
                .take()
                .expect("an admitted action edge retains its actual staging basis");
            if deliver {
                record.frame_pending = true;
                entries.push((record.entry, revision));
            }
        }
        entries
    }

    pub(crate) fn reclaim_completed(&mut self) {
        let mut removed = 0;
        while self.retained.front().is_some_and(|record| {
            !record.projection_pending
                && !record.observation_pending
                && !record.frame_pending
                && !record.ui_pending
                && record.action == ActionRetention::Done
        }) {
            self.retained.pop_front();
            removed += 1;
        }
        self.capacity
            .reclaim(removed)
            .expect("only actually removed records release retained credits");
        for slot in &mut self.topology.members {
            if slot.is_some_and(|member| {
                !member.pressed
                    && !self.retained.iter().any(|record| {
                        record.entry.press_sequence() == Some(member.held.press_sequence)
                    })
            }) {
                *slot = None;
            }
        }
        for slot in &mut self.topology.pointers {
            let Some(pointer) = *slot else {
                continue;
            };
            if !pointer.retiring
                || self
                    .topology
                    .members
                    .iter()
                    .flatten()
                    .any(|member| member.pointer == Some(pointer.id))
            {
                continue;
            }
            let endpoint = self
                .topology
                .endpoints
                .iter()
                .flatten()
                .find(|endpoint| endpoint.id == pointer.endpoint)
                .expect("a connected pointer retains its endpoint");
            if self.retained.iter().any(|record| {
                record.entry.route().window() == endpoint.window
                    && record.entry.route().seat() == endpoint.seat
                    && record.entry.route().pointer() == Some(pointer.label)
            }) {
                continue;
            }
            *slot = None;
        }
        for slot in &mut self.topology.endpoints {
            let Some(endpoint) = *slot else {
                continue;
            };
            if !endpoint.retiring
                || self
                    .prepared_routes
                    .contains_key(&(endpoint.window, endpoint.seat))
                || self
                    .topology
                    .members
                    .iter()
                    .flatten()
                    .any(|member| member.endpoint == endpoint.id)
                || self
                    .topology
                    .pointers
                    .iter()
                    .flatten()
                    .any(|pointer| pointer.endpoint == endpoint.id)
                || self.retained.iter().any(|record| {
                    record.entry.route().window() == endpoint.window
                        && record.entry.route().seat() == endpoint.seat
                })
            {
                continue;
            }
            *slot = None;
        }
        self.assert_accounting();
    }

    fn apply_driver_event(
        &mut self,
        event: __InputDriverEvent,
    ) -> Result<__InputDriverOutput, InputIngressError> {
        let synthetic_window = InputWindowId::UNSPECIFIED;
        let synthetic_seat = InputSeatId::PRIMARY;
        match event {
            __InputDriverEvent::ConnectEndpoint { window, seat } => {
                self.connect_endpoint(window, seat)?;
                Ok(__InputDriverOutput::new(InputAdmission::IGNORED))
            }
            __InputDriverEvent::ConnectPointer {
                window,
                seat,
                pointer,
            } => {
                self.connect_pointer(window, seat, pointer)?;
                Ok(__InputDriverOutput::new(InputAdmission::IGNORED))
            }
            __InputDriverEvent::RetirePointer {
                window,
                seat,
                pointer,
            } => self.apply_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Left,
                InputEventOrigin::Platform,
                true,
            ),
            __InputDriverEvent::Keyboard {
                window,
                seat,
                input,
            } => self.apply_keyboard(window, seat, input, InputEventOrigin::Platform),
            __InputDriverEvent::Mouse {
                window,
                seat,
                pointer,
                input,
            } => self.apply_mouse(window, seat, pointer, input, InputEventOrigin::Platform),
            __InputDriverEvent::Pointer {
                window,
                seat,
                pointer,
                input,
            } => self.apply_pointer(
                window,
                seat,
                pointer,
                input,
                InputEventOrigin::Platform,
                false,
            ),
            __InputDriverEvent::Gamepad {
                window,
                seat,
                gamepad,
                button,
                phase,
            } => self
                .gamepad(
                    window,
                    seat,
                    gamepad,
                    button,
                    phase,
                    InputEventOrigin::Platform,
                )
                .map(__InputDriverOutput::new),
            __InputDriverEvent::Window {
                window,
                seat,
                event,
            } => self.apply_window(window, seat, event),
            __InputDriverEvent::SyntheticKeyboard { input } => self.apply_keyboard(
                synthetic_window,
                synthetic_seat,
                input,
                InputEventOrigin::Synthetic,
            ),
            __InputDriverEvent::SyntheticMouse { input } => self.apply_mouse(
                synthetic_window,
                synthetic_seat,
                InputPointerId::PRIMARY,
                input,
                InputEventOrigin::Synthetic,
            ),
            __InputDriverEvent::SyntheticPointer { input } => self.apply_pointer(
                synthetic_window,
                synthetic_seat,
                InputPointerId::PRIMARY,
                input,
                InputEventOrigin::Synthetic,
                false,
            ),
            __InputDriverEvent::SyntheticGamepad {
                gamepad,
                button,
                phase,
            } => self
                .gamepad(
                    synthetic_window,
                    synthetic_seat,
                    gamepad,
                    button,
                    phase,
                    InputEventOrigin::Synthetic,
                )
                .map(__InputDriverOutput::new),
        }
    }
}

impl __RuntimeDriverPort for InputTimeline {
    type Input = __InputDriverEvent;
    type Output = Result<__InputDriverOutput, InputIngressError>;

    fn accepts_driver_input(state: RuntimeState, input: &Self::Input) -> bool {
        matches!(state, RuntimeState::Running | RuntimeState::Paused)
            || matches!(
                (state, input),
                (
                    RuntimeState::Faulted | RuntimeState::Stopping | RuntimeState::CloseIncomplete,
                    __InputDriverEvent::Window {
                        event: InputWindowEvent::SurfaceRetired,
                        ..
                    }
                )
            )
    }
    fn apply_driver_input(&mut self, event: Self::Input) -> Self::Output {
        self.apply_driver_event(event)
    }
}

/// Reserves the exact route before backend submission.
#[doc(hidden)]
pub fn prepare_presented_route(
    timeline: &mut InputTimeline,
    route: InputPresentedRoute,
) -> Result<PreparedInputRoutePublication, InputIngressError> {
    timeline.prepare_presented_route(route)
}
#[doc(hidden)]
pub fn commit_presented_route(
    timeline: &mut InputTimeline,
    token: PreparedInputRoutePublication,
) -> InputRouteCommit {
    timeline.commit_presented_route(token)
}
/// Reads exact Input invalidation before the UI owner decides its total publication outcome.
#[doc(hidden)]
pub fn prepared_route_invalidation(
    timeline: &InputTimeline,
    token: &PreparedInputRoutePublication,
) -> Option<InputInvalidationReason> {
    timeline.route_reservation(token).invalidation
}

/// Checks original route custody without consuming the token or asserting on a foreign owner.
#[doc(hidden)]
#[must_use]
pub fn owns_prepared_route(
    timeline: &InputTimeline,
    token: &PreparedInputRoutePublication,
) -> bool {
    Arc::ptr_eq(&timeline.owner, &token.owner)
        && timeline
            .prepared_routes
            .get(&(
                token.declaration.route.window(),
                token.declaration.route.seat(),
            ))
            .is_some_and(|reservation| reservation.declaration == token.declaration)
}

#[doc(hidden)]
pub fn claim_input_endpoint(
    timeline: &InputTimeline,
    window: InputWindowId,
    seat: InputSeatId,
) -> Result<InputEndpointClaim, InputIngressError> {
    timeline.claim_endpoint(window, seat)
}

#[doc(hidden)]
pub fn prepare_claimed_route(
    timeline: &mut InputTimeline,
    claim: &InputEndpointClaim,
    route: InputPresentedRoute,
) -> Result<PreparedInputRoutePublication, InputIngressError> {
    if route.window() != claim.window
        || route.seat() != claim.seat
        || !timeline.owns_claim(claim)?
    {
        return Err(InputIngressError::EndpointClaimExpired {
            window: claim.window,
            seat: claim.seat,
        });
    }
    timeline.prepare_presented_route(route)
}

#[doc(hidden)]
pub fn unbind_claimed_endpoint(
    timeline: &mut InputTimeline,
    claim: &InputEndpointClaim,
    expected: Option<InputRouteStamp>,
) -> Result<(), InputIngressError> {
    if !timeline.owns_claim(claim)? {
        return Ok(());
    }
    timeline.unbind_presented_endpoint(claim.window, claim.seat, expected)
}

#[doc(hidden)]
pub fn retire_claimed_endpoint(
    timeline: &mut InputTimeline,
    claim: &InputEndpointClaim,
    expected: Option<InputRouteStamp>,
) -> Result<InputEndpointRetirement, InputIngressError> {
    if !timeline.owns_claim(claim)? {
        return Ok(InputEndpointRetirement::new(
            InputAdmission::IGNORED,
            Vec::new(),
            Vec::new(),
        ));
    }
    timeline.retire_presented_endpoint(claim.window, claim.seat, expected)
}
#[doc(hidden)]
pub fn abort_presented_route(timeline: &mut InputTimeline, token: PreparedInputRoutePublication) {
    timeline.release_route(token);
}
#[doc(hidden)]
pub fn supersede_presented_route(
    timeline: &mut InputTimeline,
    token: PreparedInputRoutePublication,
) {
    timeline.release_route(token);
}
