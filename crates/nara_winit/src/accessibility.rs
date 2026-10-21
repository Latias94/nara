use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};

use accesskit::{Action, ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use nara_ui_kernel::{
    UiAccessibilityInput, UiFrameRevision, UiKey, UiRevision, UiRouteStamp, UiSeatId,
    UiSemanticRole, UiSemanticSnapshot, UiSurfaceId, UiSurfaceRetirementToken,
};
use thiserror::Error;

#[cfg(feature = "platform-accessibility")]
mod bridge;
#[cfg(feature = "platform-accessibility")]
mod platform;

#[cfg(feature = "platform-accessibility")]
pub(crate) use platform::WinitPlatformAccessibilitySession;

#[cfg(feature = "platform-accessibility")]
#[doc(hidden)]
pub mod __private {
    pub use super::bridge::{
        WinitAccessibilityAccess, WinitAccessibilityActivation, WinitAccessibilityActivationError,
        WinitAccessibilityBridgeError, WinitAccessibilityInputBatch,
        WinitAccessibilityPlatformUpdate, capture_winit_accessibility_activation,
        drain_winit_accessibility_inputs, install_winit_accessibility_contribution,
        publish_winit_accessibility_update, queue_winit_accessibility_inputs,
        take_winit_accessibility_updates,
    };

    #[cfg(test)]
    pub(crate) use super::bridge::install_partial_winit_accessibility_contribution;
}

const ROOT_NODE_ID: NodeId = NodeId(1);
const FIRST_CONTENT_NODE_ID: u64 = 2;
const FIRST_PUBLICATION_GENERATION: u64 = 1;
const MAX_NODE_IDENTITIES: usize = 4_096;
static NEXT_PUBLISHER_ID: AtomicU64 = AtomicU64::new(1);

/// Opaque process-unique identity for one Host-owned accessibility publisher lifetime.
///
/// The adapter allocates this identity when the Host creates a publisher and never accepts a
/// caller-provided value. It is carried by every publication receipt so coincidentally equal
/// AccessKit node identifiers from another publisher cannot be resolved against this publisher's
/// current tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WinitAccessibilityPublisherId(u64);

impl WinitAccessibilityPublisherId {
    fn allocate() -> Result<Self, WinitAccessibilityError> {
        NEXT_PUBLISHER_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(Self)
            .map_err(|_| WinitAccessibilityError::PublisherIdExhausted)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Host-owned synchronous publisher from Nara semantic facts to a bounded AccessKit tree record.
///
/// A publication receipt proves that this publisher recorded the exact tree and atomically replaced
/// its callback mapping. It does not claim that an operating-system accessibility client observed
/// the record.
#[derive(Debug)]
pub struct WinitAccessibilityPublisher {
    publisher_id: WinitAccessibilityPublisherId,
    surface_id: UiSurfaceId,
    next_node_id: u64,
    node_ids: BTreeMap<UiKey, NodeId>,
    active_keys: BTreeMap<NodeId, UiKey>,
    active_actions: BTreeMap<NodeId, nara_ui_kernel::UiActionId>,
    active_focusable: BTreeSet<NodeId>,
    route: Option<UiRouteStamp>,
    seat: Option<UiSeatId>,
    next_publication_generation: u64,
    committed_update: Option<WinitAccessibilityUpdate>,
    suspension: Option<WinitAccessibilitySuspensionReceipt>,
    retirement: Option<WinitAccessibilityRetirementReceipt>,
    invalidation_floor: Option<UiRevision>,
}

#[cfg(feature = "platform-accessibility")]
#[derive(Debug)]
pub(super) struct WinitAccessibilityCallbackMapping {
    publisher: WinitAccessibilityPublisher,
    receipt: WinitAccessibilityReceipt,
}

#[cfg(feature = "platform-accessibility")]
impl WinitAccessibilityCallbackMapping {
    pub(super) fn map_action(
        &self,
        request: ActionRequest,
    ) -> Result<UiAccessibilityInput, WinitAccessibilityError> {
        self.publisher
            .map_action(&self.receipt.bind_action(request))
    }
}

impl WinitAccessibilityPublisher {
    pub fn new(surface_id: UiSurfaceId) -> Result<Self, WinitAccessibilityError> {
        Self::with_node_id_floor(surface_id, FIRST_CONTENT_NODE_ID)
    }

    pub(super) fn with_node_id_floor(
        surface_id: UiSurfaceId,
        next_node_id: u64,
    ) -> Result<Self, WinitAccessibilityError> {
        if next_node_id < FIRST_CONTENT_NODE_ID {
            return Err(WinitAccessibilityError::InvalidNodeIdFloor {
                minimum: FIRST_CONTENT_NODE_ID,
                actual: next_node_id,
            });
        }
        Ok(Self {
            publisher_id: WinitAccessibilityPublisherId::allocate()?,
            surface_id,
            next_node_id,
            node_ids: BTreeMap::new(),
            active_keys: BTreeMap::new(),
            active_actions: BTreeMap::new(),
            active_focusable: BTreeSet::new(),
            route: None,
            seat: None,
            next_publication_generation: FIRST_PUBLICATION_GENERATION,
            committed_update: None,
            suspension: None,
            retirement: None,
            invalidation_floor: None,
        })
    }

    #[cfg(feature = "platform-accessibility")]
    pub(super) const fn next_node_id_floor(&self) -> u64 {
        self.next_node_id
    }

    #[must_use]
    pub const fn publisher_id(&self) -> WinitAccessibilityPublisherId {
        self.publisher_id
    }

    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }

    #[cfg(feature = "platform-accessibility")]
    pub(super) fn callback_mapping(
        &self,
        receipt: WinitAccessibilityReceipt,
    ) -> WinitAccessibilityCallbackMapping {
        let mut publisher = self.candidate_clone();
        publisher.committed_update =
            self.committed_update
                .as_ref()
                .map(|update| WinitAccessibilityUpdate {
                    publisher_id: update.publisher_id,
                    route: update.route,
                    frame_revision: update.frame_revision,
                    publication_generation: update.publication_generation,
                    update: update.update.clone(),
                });
        WinitAccessibilityCallbackMapping { publisher, receipt }
    }

    /// Records one semantic snapshot and commits its callback mapping atomically.
    ///
    /// The complete candidate is validated before a unique publication generation is reserved. The
    /// exact AccessKit payload is then stored in this publisher before the callback mapping becomes
    /// observable. No external callback or publishable token can delay, omit or reorder that record.
    pub fn publish_update(
        &mut self,
        snapshot: &UiSemanticSnapshot,
    ) -> Result<WinitAccessibilityReceipt, WinitAccessibilityError> {
        let publication_generation = self.next_publication_generation;
        let publication = self.build_publication(snapshot, publication_generation)?;
        self.next_publication_generation = self
            .next_publication_generation
            .checked_add(1)
            .ok_or(WinitAccessibilityError::PublicationGenerationExhausted)?;
        let receipt = publication.update.receipt();
        self.commit_candidate(publication.candidate, publication.update);
        Ok(receipt)
    }

    fn build_publication(
        &self,
        snapshot: &UiSemanticSnapshot,
        publication_generation: u64,
    ) -> Result<WinitAccessibilityPublication, WinitAccessibilityError> {
        if snapshot.surface_id() != self.surface_id {
            return Err(WinitAccessibilityError::SurfaceMismatch {
                expected: self.surface_id,
                actual: snapshot.surface_id(),
            });
        }
        if self.retirement.is_some() {
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
        if let Some(suspension) = self.suspension
            && snapshot.route().slot_generation() <= suspension.slot_generation()
        {
            return Err(WinitAccessibilityError::SnapshotDoesNotSucceedSuspension {
                suspended: suspension.slot_generation(),
                candidate: snapshot.route(),
            });
        }
        if let Some(current) = self.route {
            let next = snapshot.route();
            if next.slot_generation() < current.slot_generation()
                || (next.slot_generation() == current.slot_generation()
                    && next.presentation_epoch() < current.presentation_epoch())
            {
                return Err(WinitAccessibilityError::StaleSnapshot {
                    current,
                    candidate: next,
                });
            }
            if next.slot_generation() == current.slot_generation()
                && (next.route_revision() != current.route_revision()
                    || next.geometry_generation() != current.geometry_generation()
                    || (next.presentation_epoch() == current.presentation_epoch()
                        && next != current))
            {
                return Err(WinitAccessibilityError::ConflictingSnapshot {
                    current,
                    candidate: next,
                });
            }
        }
        if let Some(floor) = self.invalidation_floor
            && snapshot.route().slot_generation() < floor
        {
            return Err(WinitAccessibilityError::SnapshotPredatesInvalidation {
                floor,
                candidate: snapshot.route(),
            });
        }
        if snapshot.nodes().len() > MAX_NODE_IDENTITIES {
            return Err(WinitAccessibilityError::NodeLimitExceeded {
                maximum: MAX_NODE_IDENTITIES,
            });
        }

        let mut keys = BTreeSet::new();
        for fact in snapshot.nodes() {
            if !keys.insert(fact.key().clone()) {
                return Err(WinitAccessibilityError::DuplicateKey {
                    key: fact.key().clone(),
                });
            }
        }
        for fact in snapshot.nodes() {
            if let Some(parent) = fact.parent()
                && !keys.contains(parent)
            {
                return Err(WinitAccessibilityError::MissingParent {
                    key: fact.key().clone(),
                    parent: parent.clone(),
                });
            }
        }

        let mut candidate = self.candidate_clone();
        candidate.active_keys.clear();
        candidate.active_actions.clear();
        candidate.active_focusable.clear();
        candidate.route = Some(snapshot.route());
        candidate.seat = Some(snapshot.seat());
        candidate.suspension = None;
        for fact in snapshot.nodes() {
            let node_id = candidate.admit_key(fact.key())?;
            candidate.active_keys.insert(node_id, fact.key().clone());
            if fact.state().enabled() && !fact.state().inert() && fact.state().focusable() {
                candidate.active_focusable.insert(node_id);
            }
            if fact.state().enabled()
                && !fact.state().inert()
                && let Some(action) = fact.actions().first()
            {
                candidate.active_actions.insert(node_id, action.clone());
            }
        }

        let mut children = BTreeMap::<Option<UiKey>, Vec<NodeId>>::new();
        for fact in snapshot.nodes() {
            let node_id = candidate.node_ids[fact.key()];
            children
                .entry(fact.parent().cloned())
                .or_default()
                .push(node_id);
        }

        let mut nodes = Vec::with_capacity(snapshot.nodes().len() + 1);
        let mut root = Node::new(Role::Window);
        root.set_children(children.get(&None).cloned().unwrap_or_default());
        nodes.push((ROOT_NODE_ID, root));
        let mut focus = ROOT_NODE_ID;
        for fact in snapshot.nodes() {
            let node_id = candidate.node_ids[fact.key()];
            let mut node = Node::new(map_role(fact.role()));
            match fact.role() {
                UiSemanticRole::Text | UiSemanticRole::Status => {
                    node.set_value(fact.value().unwrap_or(fact.name()));
                }
                UiSemanticRole::Group | UiSemanticRole::Button => {
                    node.set_label(fact.name());
                    if let Some(value) = fact.value() {
                        node.set_value(value);
                    }
                }
            }
            node.set_children(
                children
                    .get(&Some(fact.key().clone()))
                    .cloned()
                    .unwrap_or_default(),
            );
            let rect = fact.physical_rect();
            node.set_bounds(Rect::new(
                f64::from(rect.min_x()),
                f64::from(rect.min_y()),
                f64::from(rect.max_x()),
                f64::from(rect.max_y()),
            ));
            if !fact.state().enabled() || fact.state().inert() {
                node.set_disabled();
            } else {
                if fact.state().focusable() {
                    node.add_action(Action::Focus);
                }
                if !fact.actions().is_empty() {
                    node.add_action(Action::Click);
                }
            }
            if let Some(selected) = fact.state().selected() {
                node.set_selected(selected);
            }
            if let Some(expanded) = fact.state().expanded() {
                node.set_expanded(expanded);
            }
            if fact.state().focused() {
                focus = node_id;
            }
            nodes.push((node_id, node));
        }

        let mut tree = Tree::new(ROOT_NODE_ID);
        tree.toolkit_name = Some("Nara".to_owned());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
        let update = TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus,
        };
        Ok(WinitAccessibilityPublication {
            candidate,
            update: WinitAccessibilityUpdate {
                publisher_id: self.publisher_id,
                route: snapshot.route(),
                frame_revision: snapshot.frame_revision(),
                publication_generation,
                update,
            },
        })
    }

    fn commit_candidate(
        &mut self,
        candidate: WinitAccessibilityPublisher,
        update: WinitAccessibilityUpdate,
    ) {
        let next_publication_generation = self.next_publication_generation;
        *self = candidate;
        self.next_publication_generation = next_publication_generation;
        self.committed_update = Some(update);
    }

    fn candidate_clone(&self) -> Self {
        Self {
            publisher_id: self.publisher_id,
            surface_id: self.surface_id,
            next_node_id: self.next_node_id,
            node_ids: self.node_ids.clone(),
            active_keys: self.active_keys.clone(),
            active_actions: self.active_actions.clone(),
            active_focusable: self.active_focusable.clone(),
            route: self.route,
            seat: self.seat,
            next_publication_generation: self.next_publication_generation,
            committed_update: None,
            suspension: self.suspension,
            retirement: self.retirement,
            invalidation_floor: self.invalidation_floor,
        }
    }

    #[cfg(feature = "platform-accessibility")]
    pub(super) fn invalidate_before(
        &mut self,
        surface_id: UiSurfaceId,
        successor_slot_generation: UiRevision,
    ) -> Result<(), WinitAccessibilityError> {
        if surface_id != self.surface_id {
            return Err(WinitAccessibilityError::SurfaceMismatch {
                expected: self.surface_id,
                actual: surface_id,
            });
        }
        if self.retirement.is_some() {
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
        if let Some(current) = self.route
            && successor_slot_generation <= current.slot_generation()
        {
            return Err(WinitAccessibilityError::InvalidationGenerationMismatch {
                current: current.slot_generation(),
                successor: successor_slot_generation,
            });
        }
        if let Some(current) = self.invalidation_floor
            && successor_slot_generation < current
        {
            return Err(WinitAccessibilityError::StaleInvalidation {
                current,
                candidate: successor_slot_generation,
            });
        }
        self.active_keys.clear();
        self.active_actions.clear();
        self.active_focusable.clear();
        self.route = None;
        self.seat = None;
        self.committed_update = None;
        self.invalidation_floor = Some(successor_slot_generation);
        Ok(())
    }

    /// Converts a supported AccessKit request into the exact presented-session input.
    pub fn map_action(
        &self,
        request: &WinitAccessibilityActionRequest,
    ) -> Result<UiAccessibilityInput, WinitAccessibilityError> {
        let receipt = request.receipt();
        if receipt.publisher_id() != self.publisher_id {
            return Err(WinitAccessibilityError::ForeignPublisher {
                expected: self.publisher_id,
                actual: receipt.publisher_id(),
            });
        }
        if receipt.surface_id() != self.surface_id {
            return Err(WinitAccessibilityError::ActionSurfaceMismatch {
                expected: self.surface_id,
                actual: receipt.surface_id(),
            });
        }
        if self.retirement.is_some() {
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
        if let Some(suspension) = self.suspension {
            return Err(WinitAccessibilityError::SurfaceSuspended {
                surface: self.surface_id,
                slot_generation: suspension.slot_generation(),
            });
        }
        let current = self
            .committed_update
            .as_ref()
            .map(WinitAccessibilityUpdate::receipt)
            .ok_or(WinitAccessibilityError::NoPresentedSnapshot {
                surface: self.surface_id,
            })?;
        if receipt != current {
            return Err(WinitAccessibilityError::ActionPublicationMismatch {
                current_generation: current.publication_generation(),
                candidate_generation: receipt.publication_generation(),
            });
        }
        let request = request.accesskit_request();
        if request.target_tree != TreeId::ROOT {
            return Err(WinitAccessibilityError::ForeignTree);
        }
        let route = self
            .route
            .ok_or(WinitAccessibilityError::NoPresentedSnapshot {
                surface: self.surface_id,
            })?;
        let seat = self
            .seat
            .ok_or(WinitAccessibilityError::NoPresentedSnapshot {
                surface: self.surface_id,
            })?;
        let target = self.active_keys.get(&request.target_node).cloned().ok_or(
            WinitAccessibilityError::UnknownNode {
                node: request.target_node,
            },
        )?;
        match request.action {
            Action::Click => {
                let action = self
                    .active_actions
                    .get(&request.target_node)
                    .cloned()
                    .ok_or(WinitAccessibilityError::ActionUnavailable {
                        node: request.target_node,
                    })?;
                Ok(UiAccessibilityInput::semantic_action_from_route(
                    route, seat, target, action,
                ))
            }
            Action::Focus => {
                if !self.active_focusable.contains(&request.target_node) {
                    return Err(WinitAccessibilityError::FocusUnavailable {
                        node: request.target_node,
                    });
                }
                Ok(UiAccessibilityInput::focus_target_from_route(
                    route, seat, target,
                ))
            }
            action => Err(WinitAccessibilityError::UnsupportedAction { action }),
        }
    }

    /// Clears the committed platform tree and action mapping for a temporarily unavailable slot.
    ///
    /// The supplied fact is a Host-observed successor generation, not a request to suspend later.
    /// Repeating the same fact is idempotent. A later publication must belong to that generation or
    /// a successor before this publisher becomes active again.
    pub fn suspend(
        &mut self,
        suspension: WinitAccessibilitySuspension,
    ) -> Result<WinitAccessibilitySuspensionReceipt, WinitAccessibilityError> {
        if suspension.surface_id() != self.surface_id {
            return Err(WinitAccessibilityError::SuspensionSurfaceMismatch {
                expected: self.surface_id,
                actual: suspension.surface_id(),
            });
        }
        if self.retirement.is_some() {
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
        if let Some(committed) = self.suspension {
            if committed.suspension() == suspension {
                return Ok(committed);
            }
            if suspension.slot_generation() < committed.slot_generation() {
                return Err(WinitAccessibilityError::StaleSuspension {
                    current: committed.slot_generation(),
                    candidate: suspension.slot_generation(),
                });
            }
        }
        if let Some(route) = self.route
            && suspension.slot_generation() <= route.slot_generation()
        {
            return Err(WinitAccessibilityError::SuspensionGenerationMismatch {
                current: route.slot_generation(),
                candidate: suspension.slot_generation(),
            });
        }
        if let Some(floor) = self.invalidation_floor
            && suspension.slot_generation() < floor
        {
            return Err(WinitAccessibilityError::SuspensionPredatesInvalidation {
                floor,
                candidate: suspension.slot_generation(),
            });
        }
        let publication_generation = self.next_publication_generation;
        let next_publication_generation = self
            .next_publication_generation
            .checked_add(1)
            .ok_or(WinitAccessibilityError::PublicationGenerationExhausted)?;
        let receipt = WinitAccessibilitySuspensionReceipt {
            publisher_id: self.publisher_id,
            suspension,
            publication_generation,
        };
        self.next_publication_generation = next_publication_generation;
        self.active_keys.clear();
        self.active_actions.clear();
        self.active_focusable.clear();
        self.route = None;
        self.seat = None;
        self.committed_update = None;
        self.suspension = Some(receipt);
        Ok(receipt)
    }

    /// Records one explicit endpoint retirement and clears the exact tree record and mapping.
    ///
    /// `endpoint` is a Host-observed postcondition, not a request for a later asynchronous action.
    /// The same exact retirement is idempotent; a different retirement after the publisher closed
    /// fails closed.
    pub fn retire(
        &mut self,
        retirement: UiSurfaceRetirementToken,
        endpoint: WinitAccessibilityEndpointRetirement,
    ) -> Result<WinitAccessibilityRetirementReceipt, WinitAccessibilityError> {
        if retirement.surface_id() != self.surface_id {
            return Err(WinitAccessibilityError::RetirementSurfaceMismatch {
                expected: self.surface_id,
                actual: retirement.surface_id(),
            });
        }
        if let Some(committed) = self.retirement {
            if committed.token() == retirement && committed.endpoint() == endpoint {
                return Ok(committed);
            }
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
        if let Some(current) = self.route.map(UiRouteStamp::slot_generation) {
            if retirement.slot_generation() < current {
                return Err(WinitAccessibilityError::RetirementPredatesActiveRoute {
                    current,
                    candidate: retirement.slot_generation(),
                });
            }
        } else if let Some(suspension) = self.suspension
            && retirement.slot_generation() < suspension.slot_generation()
        {
            return Err(WinitAccessibilityError::RetirementPredatesSuspension {
                suspended: suspension.slot_generation(),
                candidate: retirement.slot_generation(),
            });
        } else if let Some(floor) = self.invalidation_floor
            && retirement.slot_generation() < floor
        {
            return Err(WinitAccessibilityError::RetirementPredatesInvalidation {
                floor,
                candidate: retirement.slot_generation(),
            });
        }
        let publication_generation = self.next_publication_generation;
        self.next_publication_generation = self
            .next_publication_generation
            .checked_add(1)
            .ok_or(WinitAccessibilityError::PublicationGenerationExhausted)?;
        let receipt = WinitAccessibilityRetirementReceipt {
            publisher_id: self.publisher_id,
            token: retirement,
            endpoint,
            publication_generation,
        };
        self.active_keys.clear();
        self.active_actions.clear();
        self.active_focusable.clear();
        self.route = None;
        self.seat = None;
        self.committed_update = None;
        self.suspension = None;
        self.retirement = Some(receipt);
        Ok(receipt)
    }

    /// Returns the exact tree record committed by the latest successful publication.
    #[must_use]
    pub fn committed_update(&self) -> Option<&WinitAccessibilityUpdate> {
        self.committed_update.as_ref()
    }

    /// Returns the exact tombstone after this publisher has synchronously retired.
    #[must_use]
    pub const fn retirement(&self) -> Option<WinitAccessibilityRetirementReceipt> {
        self.retirement
    }

    /// Returns the exact temporary tree-removal record while this publisher is suspended.
    #[must_use]
    pub const fn suspension(&self) -> Option<WinitAccessibilitySuspensionReceipt> {
        self.suspension
    }

    #[must_use]
    pub fn node_id(&self, key: &UiKey) -> Option<NodeId> {
        self.node_ids.get(key).copied()
    }

    fn admit_key(&mut self, key: &UiKey) -> Result<NodeId, WinitAccessibilityError> {
        if let Some(node_id) = self.node_ids.get(key) {
            return Ok(*node_id);
        }
        if self.node_ids.len() == MAX_NODE_IDENTITIES {
            return Err(WinitAccessibilityError::NodeIdentityLimitExceeded {
                maximum: MAX_NODE_IDENTITIES,
            });
        }
        let node_id = NodeId(self.next_node_id);
        self.next_node_id = self
            .next_node_id
            .checked_add(1)
            .ok_or(WinitAccessibilityError::NodeIdExhausted)?;
        self.node_ids.insert(key.clone(), node_id);
        Ok(node_id)
    }
}

fn map_role(role: UiSemanticRole) -> Role {
    match role {
        UiSemanticRole::Group => Role::GenericContainer,
        UiSemanticRole::Text => Role::Label,
        UiSemanticRole::Button => Role::Button,
        UiSemanticRole::Status => Role::Status,
    }
}

/// One internal publication candidate. It never crosses the public API boundary.
#[derive(Debug)]
struct WinitAccessibilityPublication {
    candidate: WinitAccessibilityPublisher,
    update: WinitAccessibilityUpdate,
}

/// AccessKit payload stored by the publisher with the exact Nara presentation authority it
/// represents.
#[derive(Debug, PartialEq)]
pub struct WinitAccessibilityUpdate {
    publisher_id: WinitAccessibilityPublisherId,
    route: UiRouteStamp,
    frame_revision: UiFrameRevision,
    publication_generation: u64,
    update: TreeUpdate,
}

impl WinitAccessibilityUpdate {
    #[must_use]
    pub const fn publisher_id(&self) -> WinitAccessibilityPublisherId {
        self.publisher_id
    }

    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.route.surface_id()
    }

    #[must_use]
    pub const fn route(&self) -> UiRouteStamp {
        self.route
    }

    #[must_use]
    pub const fn frame_revision(&self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn publication_generation(&self) -> u64 {
        self.publication_generation
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.route.presentation_epoch()
    }

    #[must_use]
    pub const fn accesskit_update(&self) -> &TreeUpdate {
        &self.update
    }

    fn receipt(&self) -> WinitAccessibilityReceipt {
        WinitAccessibilityReceipt {
            publisher_id: self.publisher_id,
            route: self.route,
            frame_revision: self.frame_revision,
            publication_generation: self.publication_generation,
        }
    }
}

/// Exact evidence that one accessibility tree record and callback mapping were committed together.
///
/// This receipt does not prove that an operating-system client or player observed the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinitAccessibilityReceipt {
    publisher_id: WinitAccessibilityPublisherId,
    route: UiRouteStamp,
    frame_revision: UiFrameRevision,
    publication_generation: u64,
}

impl WinitAccessibilityReceipt {
    #[must_use]
    pub const fn publisher_id(self) -> WinitAccessibilityPublisherId {
        self.publisher_id
    }

    #[must_use]
    pub const fn route(self) -> UiRouteStamp {
        self.route
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.route.surface_id()
    }

    #[must_use]
    pub const fn frame_revision(self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn publication_generation(self) -> u64 {
        self.publication_generation
    }

    /// Binds an incoming AccessKit callback to this exact committed publication.
    #[must_use]
    pub fn bind_action(self, request: ActionRequest) -> WinitAccessibilityActionRequest {
        WinitAccessibilityActionRequest::new(self, request)
    }
}

/// Incoming AccessKit action bound to the exact publication that created its node mapping.
#[derive(Debug)]
pub struct WinitAccessibilityActionRequest {
    receipt: WinitAccessibilityReceipt,
    request: ActionRequest,
}

impl WinitAccessibilityActionRequest {
    #[must_use]
    pub const fn new(receipt: WinitAccessibilityReceipt, request: ActionRequest) -> Self {
        Self { receipt, request }
    }

    #[must_use]
    pub const fn receipt(&self) -> WinitAccessibilityReceipt {
        self.receipt
    }

    #[must_use]
    pub const fn accesskit_request(&self) -> &ActionRequest {
        &self.request
    }
}

/// Host-observed successor generation whose platform accessibility tree is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinitAccessibilitySuspension {
    surface_id: UiSurfaceId,
    slot_generation: UiRevision,
}

impl WinitAccessibilitySuspension {
    #[must_use]
    pub const fn new(surface_id: UiSurfaceId, slot_generation: UiRevision) -> Self {
        Self {
            surface_id,
            slot_generation,
        }
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.slot_generation
    }
}

/// Exact evidence that one publisher synchronously removed its tree and action mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinitAccessibilitySuspensionReceipt {
    publisher_id: WinitAccessibilityPublisherId,
    suspension: WinitAccessibilitySuspension,
    publication_generation: u64,
}

impl WinitAccessibilitySuspensionReceipt {
    #[must_use]
    pub const fn publisher_id(self) -> WinitAccessibilityPublisherId {
        self.publisher_id
    }

    #[must_use]
    pub const fn suspension(self) -> WinitAccessibilitySuspension {
        self.suspension
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.suspension.surface_id()
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.suspension.slot_generation()
    }

    #[must_use]
    pub const fn publication_generation(self) -> u64 {
        self.publication_generation
    }
}

/// Host-observed endpoint postcondition that authorizes accessibility retirement.
///
/// These variants describe work already completed by the Host. They are not deferred requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinitAccessibilityEndpointRetirement {
    PlatformTreeRemoved,
    NativeWindowDestroyed,
}

/// Exact tombstone recorded after the publisher cleared its tree record and callback mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinitAccessibilityRetirementReceipt {
    publisher_id: WinitAccessibilityPublisherId,
    token: UiSurfaceRetirementToken,
    endpoint: WinitAccessibilityEndpointRetirement,
    publication_generation: u64,
}

impl WinitAccessibilityRetirementReceipt {
    #[must_use]
    pub const fn publisher_id(self) -> WinitAccessibilityPublisherId {
        self.publisher_id
    }

    #[must_use]
    pub const fn token(self) -> UiSurfaceRetirementToken {
        self.token
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.token.surface_id()
    }

    #[must_use]
    pub const fn endpoint(self) -> WinitAccessibilityEndpointRetirement {
        self.endpoint
    }

    #[must_use]
    pub const fn publication_generation(self) -> u64 {
        self.publication_generation
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum WinitAccessibilityError {
    #[error("accessibility publisher identities are exhausted")]
    PublisherIdExhausted,
    #[error("accessibility node identifier floor {actual} is below minimum {minimum}")]
    InvalidNodeIdFloor { minimum: u64, actual: u64 },
    #[error(
        "accessibility snapshot surface {actual:?} does not match publisher surface {expected:?}"
    )]
    SurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error("accessibility surface {surface:?} has been retired")]
    SurfaceRetired { surface: UiSurfaceId },
    #[error(
        "accessibility surface {surface:?} is suspended at slot generation {slot_generation:?}"
    )]
    SurfaceSuspended {
        surface: UiSurfaceId,
        slot_generation: UiRevision,
    },
    #[error("accessibility surface {surface:?} has no presented semantic snapshot")]
    NoPresentedSnapshot { surface: UiSurfaceId },
    #[error("accessibility snapshot route {candidate:?} predates active route {current:?}")]
    StaleSnapshot {
        current: UiRouteStamp,
        candidate: UiRouteStamp,
    },
    #[error("accessibility snapshot route {candidate:?} conflicts with active route {current:?}")]
    ConflictingSnapshot {
        current: UiRouteStamp,
        candidate: UiRouteStamp,
    },
    #[error(
        "accessibility snapshot route {candidate:?} does not succeed suspended slot generation {suspended:?}"
    )]
    SnapshotDoesNotSucceedSuspension {
        suspended: UiRevision,
        candidate: UiRouteStamp,
    },
    #[error(
        "accessibility snapshot route {candidate:?} predates invalidated slot generation {floor:?}"
    )]
    SnapshotPredatesInvalidation {
        floor: UiRevision,
        candidate: UiRouteStamp,
    },
    #[error("accessibility publication generation was exhausted")]
    PublicationGenerationExhausted,
    #[error("accessibility snapshot exceeds {maximum} semantic nodes")]
    NodeLimitExceeded { maximum: usize },
    #[error("accessibility node identity retention exceeds {maximum} keys")]
    NodeIdentityLimitExceeded { maximum: usize },
    #[error("accessibility node identifiers are exhausted")]
    NodeIdExhausted,
    #[error("accessibility snapshot contains duplicate key {key:?}")]
    DuplicateKey { key: UiKey },
    #[error("accessibility node {key:?} references missing parent {parent:?}")]
    MissingParent { key: UiKey, parent: UiKey },
    #[error("accessibility action targets a foreign tree")]
    ForeignTree,
    #[error("accessibility action publisher {actual:?} does not match publisher {expected:?}")]
    ForeignPublisher {
        expected: WinitAccessibilityPublisherId,
        actual: WinitAccessibilityPublisherId,
    },
    #[error("accessibility action surface {actual:?} does not match surface {expected:?}")]
    ActionSurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error(
        "accessibility action publication generation {candidate_generation} does not match current generation {current_generation}"
    )]
    ActionPublicationMismatch {
        current_generation: u64,
        candidate_generation: u64,
    },
    #[error("accessibility action targets unknown node {node:?}")]
    UnknownNode { node: NodeId },
    #[error("accessibility node {node:?} has no semantic action")]
    ActionUnavailable { node: NodeId },
    #[error("accessibility node {node:?} is not an enabled focus target")]
    FocusUnavailable { node: NodeId },
    #[error("accessibility action {action:?} is unsupported")]
    UnsupportedAction { action: Action },
    #[error("suspension targets surface {actual:?}, expected {expected:?}")]
    SuspensionSurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error("suspension slot generation {candidate:?} must succeed active generation {current:?}")]
    SuspensionGenerationMismatch {
        current: UiRevision,
        candidate: UiRevision,
    },
    #[error("suspension slot generation {candidate:?} predates committed suspension {current:?}")]
    StaleSuspension {
        current: UiRevision,
        candidate: UiRevision,
    },
    #[error(
        "invalidation slot generation {successor:?} must succeed active generation {current:?}"
    )]
    InvalidationGenerationMismatch {
        current: UiRevision,
        successor: UiRevision,
    },
    #[error("invalidation slot generation {candidate:?} predates committed floor {current:?}")]
    StaleInvalidation {
        current: UiRevision,
        candidate: UiRevision,
    },
    #[error("suspension slot generation {candidate:?} predates invalidation floor {floor:?}")]
    SuspensionPredatesInvalidation {
        floor: UiRevision,
        candidate: UiRevision,
    },
    #[error("retirement slot generation {candidate:?} predates suspended generation {suspended:?}")]
    RetirementPredatesSuspension {
        suspended: UiRevision,
        candidate: UiRevision,
    },
    #[error("retirement slot generation {candidate:?} predates invalidation floor {floor:?}")]
    RetirementPredatesInvalidation {
        floor: UiRevision,
        candidate: UiRevision,
    },
    #[error("retirement token targets surface {actual:?}, expected {expected:?}")]
    RetirementSurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error(
        "retirement token slot generation {candidate:?} predates active route generation {current:?}"
    )]
    RetirementPredatesActiveRoute {
        current: nara_ui_kernel::UiRevision,
        candidate: nara_ui_kernel::UiRevision,
    },
}
