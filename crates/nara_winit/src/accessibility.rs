use std::collections::{BTreeMap, BTreeSet};

use accesskit::{Action, ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use nara_ui_kernel::{
    UiAccessibilityInput, UiFrameRevision, UiKey, UiRouteStamp, UiSeatId, UiSemanticRole,
    UiSemanticSnapshot, UiSurfaceId, UiSurfaceRetirementToken,
};
use thiserror::Error;

const ROOT_NODE_ID: NodeId = NodeId(1);
const FIRST_CONTENT_NODE_ID: u64 = 2;
const FIRST_PUBLICATION_GENERATION: u64 = 1;
const MAX_NODE_IDENTITIES: usize = 4_096;

/// Host-owned synchronous publisher from Nara semantic facts to a bounded AccessKit tree record.
///
/// A publication receipt proves that this publisher recorded the exact tree and atomically replaced
/// its callback mapping. It does not claim that an operating-system accessibility client observed
/// the record.
#[derive(Debug)]
pub struct WinitAccessibilityPublisher {
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
    retirement: Option<WinitAccessibilityRetirementReceipt>,
}

impl WinitAccessibilityPublisher {
    #[must_use]
    pub const fn new(surface_id: UiSurfaceId) -> Self {
        Self {
            surface_id,
            next_node_id: FIRST_CONTENT_NODE_ID,
            node_ids: BTreeMap::new(),
            active_keys: BTreeMap::new(),
            active_actions: BTreeMap::new(),
            active_focusable: BTreeSet::new(),
            route: None,
            seat: None,
            next_publication_generation: FIRST_PUBLICATION_GENERATION,
            committed_update: None,
            retirement: None,
        }
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
            retirement: self.retirement,
        }
    }

    /// Converts a supported AccessKit request into the exact presented-session input.
    pub fn map_action(
        &self,
        request: &ActionRequest,
    ) -> Result<UiAccessibilityInput, WinitAccessibilityError> {
        if self.retirement.is_some() {
            return Err(WinitAccessibilityError::SurfaceRetired {
                surface: self.surface_id,
            });
        }
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
        if let Some(route) = self.route
            && route.slot_generation() > retirement.slot_generation()
        {
            return Err(WinitAccessibilityError::RetirementGenerationMismatch {
                expected: route.slot_generation(),
                actual: retirement.slot_generation(),
            });
        }
        let publication_generation = self.next_publication_generation;
        self.next_publication_generation = self
            .next_publication_generation
            .checked_add(1)
            .ok_or(WinitAccessibilityError::PublicationGenerationExhausted)?;
        let receipt = WinitAccessibilityRetirementReceipt {
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
    route: UiRouteStamp,
    frame_revision: UiFrameRevision,
    publication_generation: u64,
    update: TreeUpdate,
}

impl WinitAccessibilityUpdate {
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
    route: UiRouteStamp,
    frame_revision: UiFrameRevision,
    publication_generation: u64,
}

impl WinitAccessibilityReceipt {
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
    token: UiSurfaceRetirementToken,
    endpoint: WinitAccessibilityEndpointRetirement,
    publication_generation: u64,
}

impl WinitAccessibilityRetirementReceipt {
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
    #[error(
        "accessibility snapshot surface {actual:?} does not match publisher surface {expected:?}"
    )]
    SurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error("accessibility surface {surface:?} has been retired")]
    SurfaceRetired { surface: UiSurfaceId },
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
    #[error("accessibility action targets unknown node {node:?}")]
    UnknownNode { node: NodeId },
    #[error("accessibility node {node:?} has no semantic action")]
    ActionUnavailable { node: NodeId },
    #[error("accessibility node {node:?} is not an enabled focus target")]
    FocusUnavailable { node: NodeId },
    #[error("accessibility action {action:?} is unsupported")]
    UnsupportedAction { action: Action },
    #[error("retirement token targets surface {actual:?}, expected {expected:?}")]
    RetirementSurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error(
        "retirement token slot generation {actual:?} does not match active route generation {expected:?}"
    )]
    RetirementGenerationMismatch {
        expected: nara_ui_kernel::UiRevision,
        actual: nara_ui_kernel::UiRevision,
    },
}
