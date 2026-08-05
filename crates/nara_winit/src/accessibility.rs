use std::collections::{BTreeMap, BTreeSet};

use accesskit::{Action, ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};
use nara_ui_kernel::{
    UiFrameRevision, UiInputEvent, UiKey, UiSeatId, UiSemanticRole, UiSemanticSnapshot, UiSurfaceId,
};
use thiserror::Error;

const ROOT_NODE_ID: NodeId = NodeId(1);
const FIRST_CONTENT_NODE_ID: u64 = 2;
const MAX_NODE_IDENTITIES: usize = 4_096;

/// Bounded optional adapter from Nara semantic facts to AccessKit schema values.
#[derive(Clone, Debug)]
pub struct WinitAccessibilityBridge {
    surface_id: UiSurfaceId,
    next_node_id: u64,
    node_ids: BTreeMap<UiKey, NodeId>,
    active_keys: BTreeMap<NodeId, UiKey>,
    active_actions: BTreeMap<NodeId, nara_ui_kernel::UiActionId>,
}

impl WinitAccessibilityBridge {
    #[must_use]
    pub const fn new(surface_id: UiSurfaceId) -> Self {
        Self {
            surface_id,
            next_node_id: FIRST_CONTENT_NODE_ID,
            node_ids: BTreeMap::new(),
            active_keys: BTreeMap::new(),
            active_actions: BTreeMap::new(),
        }
    }

    /// Maps one latest semantic snapshot without changing kernel or widget contracts.
    pub fn update(
        &mut self,
        snapshot: &UiSemanticSnapshot,
        scale_factor: f64,
    ) -> Result<WinitAccessibilityUpdate, WinitAccessibilityError> {
        if snapshot.surface_id() != self.surface_id {
            return Err(WinitAccessibilityError::SurfaceMismatch {
                expected: self.surface_id,
                actual: snapshot.surface_id(),
            });
        }
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(WinitAccessibilityError::InvalidScaleFactor);
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

        let mut candidate = self.clone();
        candidate.active_keys.clear();
        candidate.active_actions.clear();
        for fact in snapshot.nodes() {
            let node_id = candidate.admit_key(fact.key())?;
            candidate.active_keys.insert(node_id, fact.key().clone());
            if let Some(action) = fact.actions().first() {
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
            let rect = fact.rect();
            let max = rect.max();
            node.set_bounds(Rect::new(
                f64::from(rect.origin().x()) * scale_factor,
                f64::from(rect.origin().y()) * scale_factor,
                f64::from(max.x()) * scale_factor,
                f64::from(max.y()) * scale_factor,
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
        *self = candidate;
        Ok(WinitAccessibilityUpdate {
            surface_id: snapshot.surface_id(),
            frame_revision: snapshot.frame_revision(),
            presentation_epoch: snapshot.presentation_epoch(),
            update,
        })
    }

    /// Converts a supported AccessKit request back into a neutral kernel event.
    pub fn map_action(
        &self,
        request: &ActionRequest,
        seat: UiSeatId,
    ) -> Result<UiInputEvent, WinitAccessibilityError> {
        if request.target_tree != TreeId::ROOT {
            return Err(WinitAccessibilityError::ForeignTree);
        }
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
                Ok(UiInputEvent::SemanticAction {
                    seat,
                    target,
                    action,
                })
            }
            Action::Focus => Ok(UiInputEvent::FocusTarget { seat, target }),
            action => Err(WinitAccessibilityError::UnsupportedAction { action }),
        }
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

/// AccessKit payload paired with the exact Nara presentation authority it represents.
#[derive(Clone, Debug, PartialEq)]
pub struct WinitAccessibilityUpdate {
    surface_id: UiSurfaceId,
    frame_revision: UiFrameRevision,
    presentation_epoch: u64,
    update: TreeUpdate,
}

impl WinitAccessibilityUpdate {
    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn frame_revision(&self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn accesskit_update(&self) -> &TreeUpdate {
        &self.update
    }

    #[must_use]
    pub fn into_accesskit_update(self) -> TreeUpdate {
        self.update
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum WinitAccessibilityError {
    #[error(
        "accessibility snapshot surface {actual:?} does not match adapter surface {expected:?}"
    )]
    SurfaceMismatch {
        expected: UiSurfaceId,
        actual: UiSurfaceId,
    },
    #[error("accessibility scale factor must be finite and positive")]
    InvalidScaleFactor,
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
    #[error("accessibility action {action:?} is unsupported")]
    UnsupportedAction { action: Action },
}
