use std::collections::BTreeSet;

use thiserror::Error;

use crate::{
    UiActionId, UiKey, UiLimits, UiLimitsError, UiLocalKey, UiRevision, UiRootKey, UiStyle,
    UiStyleError, UiText, UiVisual,
};

/// Closed root layers shared by paint, hit, focus, and semantic order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiLayer {
    Base,
    Overlay,
    Modal,
}

/// Generic geometry participation needed by the kernel before widget reducers are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiInteraction {
    pub(crate) hit_testable: bool,
    pub(crate) focusable: bool,
    pub(crate) enabled: bool,
}

impl UiInteraction {
    #[must_use]
    pub const fn hit_target() -> Self {
        Self {
            hit_testable: true,
            focusable: false,
            enabled: true,
        }
    }

    #[must_use]
    pub const fn focus_target() -> Self {
        Self {
            hit_testable: true,
            focusable: true,
            enabled: true,
        }
    }

    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSemanticRole {
    Group,
    Text,
    Button,
    Status,
}

/// Optional explicit semantics for non-widget nodes and early kernel proofs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSemantics {
    pub(crate) role: UiSemanticRole,
    pub(crate) name: String,
    pub(crate) value: Option<String>,
    pub(crate) selected: Option<bool>,
    pub(crate) expanded: Option<bool>,
    pub(crate) actions: Vec<UiActionId>,
}

impl UiSemantics {
    pub fn new(role: UiSemanticRole, name: impl Into<String>) -> Result<Self, UiAuthoringError> {
        let name = name.into();
        validate_semantic_text("name", &name)?;
        Ok(Self {
            role,
            name,
            value: None,
            selected: None,
            expanded: None,
            actions: Vec::new(),
        })
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Result<Self, UiAuthoringError> {
        let value = value.into();
        validate_semantic_text("value", &value)?;
        self.value = Some(value);
        Ok(self)
    }

    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    #[must_use]
    pub const fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn with_action(mut self, action: UiActionId) -> Result<Self, UiAuthoringError> {
        if self.actions.len() == 16 {
            return Err(UiAuthoringError::SemanticActionLimitExceeded { maximum: 16 });
        }
        if !self.actions.contains(&action) {
            self.actions.push(action);
        }
        Ok(self)
    }
}

/// One ordered authoring node. The nested shape is input data, not retained execution state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    pub(crate) key: Option<UiLocalKey>,
    pub(crate) style: UiStyle,
    pub(crate) visual: Option<UiVisual>,
    pub(crate) text: Option<UiText>,
    pub(crate) interaction: UiInteraction,
    pub(crate) semantics: Option<UiSemantics>,
    pub(crate) children: Vec<Self>,
}

impl UiNode {
    #[must_use]
    pub const fn new(style: UiStyle) -> Self {
        Self {
            key: None,
            style,
            visual: None,
            text: None,
            interaction: UiInteraction {
                hit_testable: false,
                focusable: false,
                enabled: false,
            },
            semantics: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_key(mut self, key: UiLocalKey) -> Self {
        self.key = Some(key);
        self
    }

    #[must_use]
    pub fn with_visual(mut self, visual: UiVisual) -> Self {
        self.visual = Some(visual);
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: UiText) -> Self {
        self.text = Some(text);
        self
    }

    #[must_use]
    pub const fn with_interaction(mut self, interaction: UiInteraction) -> Self {
        self.interaction = interaction;
        self
    }

    #[must_use]
    pub fn with_semantics(mut self, semantics: UiSemantics) -> Self {
        self.semantics = Some(semantics);
        self
    }

    #[must_use]
    pub fn with_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children.extend(children);
        self
    }
}

/// One stable root candidate and its ordered authored tree.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRoot {
    pub(crate) key: UiRootKey,
    pub(crate) layer: UiLayer,
    pub(crate) node: UiNode,
}

impl UiRoot {
    #[must_use]
    pub const fn new(key: UiRootKey, layer: UiLayer, style: UiStyle) -> Self {
        Self {
            key,
            layer,
            node: UiNode::new(style),
        }
    }

    #[must_use]
    pub fn with_visual(mut self, visual: UiVisual) -> Self {
        self.node.visual = Some(visual);
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: UiText) -> Self {
        self.node.text = Some(text);
        self
    }

    #[must_use]
    pub const fn with_interaction(mut self, interaction: UiInteraction) -> Self {
        self.node.interaction = interaction;
        self
    }

    #[must_use]
    pub fn with_semantics(mut self, semantics: UiSemantics) -> Self {
        self.node.semantics = Some(semantics);
        self
    }

    #[must_use]
    pub fn with_child(mut self, child: UiNode) -> Self {
        self.node.children.push(child);
        self
    }

    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = UiNode>) -> Self {
        self.node.children.extend(children);
        self
    }

    #[must_use]
    pub const fn key(&self) -> &UiRootKey {
        &self.key
    }

    #[must_use]
    pub const fn layer(&self) -> UiLayer {
        self.layer
    }
}

/// Immutable, bounded source snapshot consumed by one compile operation.
#[derive(Clone, Debug, PartialEq)]
pub struct UiAuthoringSnapshot {
    pub(crate) source_revision: UiRevision,
    pub(crate) roots: Vec<UiRoot>,
}

impl UiAuthoringSnapshot {
    #[must_use]
    pub const fn source_revision(&self) -> UiRevision {
        self.source_revision
    }

    #[must_use]
    pub fn roots(&self) -> &[UiRoot] {
        &self.roots
    }
}

/// Concrete fallible construction boundary for one authoring snapshot.
#[derive(Clone, Debug)]
pub struct UiAuthoringSnapshotBuilder {
    source_revision: u64,
    limits: UiLimits,
    roots: Vec<UiRoot>,
}

impl UiAuthoringSnapshotBuilder {
    #[must_use]
    pub fn new(source_revision: u64) -> Self {
        Self {
            source_revision,
            limits: UiLimits::default(),
            roots: Vec::new(),
        }
    }

    #[must_use]
    pub const fn with_limits(mut self, limits: UiLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_root(mut self, root: UiRoot) -> Self {
        self.roots.push(root);
        self
    }

    pub fn build(self) -> Result<UiAuthoringSnapshot, UiAuthoringError> {
        self.limits.validate()?;
        let source_revision =
            UiRevision::new(self.source_revision).ok_or(UiAuthoringError::ZeroSourceRevision)?;
        if self.roots.len() > self.limits.max_roots {
            return Err(UiAuthoringError::RootLimitExceeded {
                actual: self.roots.len(),
                maximum: self.limits.max_roots,
            });
        }

        let mut node_count = 0usize;
        for root in &self.roots {
            if root.key.encoded_len() > self.limits.max_key_bytes {
                return Err(UiAuthoringError::KeyLimitExceeded {
                    actual: root.key.encoded_len(),
                    maximum: self.limits.max_key_bytes,
                });
            }
            let root_key = UiKey::root(root.key.clone()).map_err(UiAuthoringError::Key)?;
            validate_node(&root.node, &root_key, 1, &mut node_count, self.limits)?;
        }
        Ok(UiAuthoringSnapshot {
            source_revision,
            roots: self.roots,
        })
    }
}

fn validate_node(
    root: &UiNode,
    root_key: &UiKey,
    root_depth: usize,
    node_count: &mut usize,
    limits: UiLimits,
) -> Result<(), UiAuthoringError> {
    let mut pending = vec![(root, root_depth)];
    while let Some((node, depth)) = pending.pop() {
        *node_count = node_count.saturating_add(1);
        if *node_count > limits.max_nodes {
            return Err(UiAuthoringError::NodeLimitExceeded {
                actual: *node_count,
                maximum: limits.max_nodes,
            });
        }
        if depth > limits.max_depth {
            return Err(UiAuthoringError::DepthLimitExceeded {
                actual: depth,
                maximum: limits.max_depth,
            });
        }
        node.style.validate()?;
        if let Some(visual) = &node.visual {
            visual.validate()?;
        }
        if let Some(text) = &node.text {
            text.validate()?;
        }
        if let Some(local) = &node.key {
            let key = UiKey::scoped(root_key.root_key().clone(), local.clone())
                .map_err(UiAuthoringError::Key)?;
            if key.encoded_len() > limits.max_key_bytes {
                return Err(UiAuthoringError::KeyLimitExceeded {
                    actual: key.encoded_len(),
                    maximum: limits.max_key_bytes,
                });
            }
        }
        pending.extend(node.children.iter().rev().map(|child| (child, depth + 1)));
    }
    Ok(())
}

fn validate_semantic_text(field: &'static str, value: &str) -> Result<(), UiAuthoringError> {
    if value.is_empty() {
        return Err(UiAuthoringError::EmptySemanticText { field });
    }
    if value.len() > 16 * 1024 {
        return Err(UiAuthoringError::SemanticTextLimitExceeded {
            field,
            actual: value.len(),
            maximum: 16 * 1024,
        });
    }
    Ok(())
}

pub(crate) fn duplicate_root_keys(roots: &[UiRoot]) -> BTreeSet<UiRootKey> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for root in roots {
        if !seen.insert(root.key.clone()) {
            duplicates.insert(root.key.clone());
        }
    }
    duplicates
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiAuthoringError {
    #[error(transparent)]
    Limits(#[from] UiLimitsError),
    #[error(transparent)]
    Key(#[from] crate::UiKeyError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error("UI source revision must be non-zero")]
    ZeroSourceRevision,
    #[error("UI snapshot has {actual} roots; maximum is {maximum}")]
    RootLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot has at least {actual} nodes; maximum is {maximum}")]
    NodeLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot depth is at least {actual}; maximum is {maximum}")]
    DepthLimitExceeded { actual: usize, maximum: usize },
    #[error("UI key has {actual} encoded bytes; configured maximum is {maximum}")]
    KeyLimitExceeded { actual: usize, maximum: usize },
    #[error("UI semantic {field} is empty")]
    EmptySemanticText { field: &'static str },
    #[error("UI semantic {field} has {actual} bytes; maximum is {maximum}")]
    SemanticTextLimitExceeded {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    #[error("UI semantic node exceeds {maximum} actions")]
    SemanticActionLimitExceeded { maximum: usize },
}
