use std::collections::BTreeSet;

use thiserror::Error;

use crate::{
    UiActionId, UiKey, UiLimits, UiLimitsError, UiLocalKey, UiMesh, UiPreparedText, UiRevision,
    UiRootDiagnostic, UiRootKey, UiStyle, UiStyleError, UiText, UiVisual,
    limits::UiSnapshotProfile, mesh::MeshCounts,
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

    #[must_use]
    pub const fn hit_testable(self) -> bool {
        self.hit_testable
    }

    #[must_use]
    pub const fn focusable(self) -> bool {
        self.focusable
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSemanticRole {
    Group,
    Text,
    TextInput,
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
        if name.is_empty() {
            return Err(UiAuthoringError::EmptySemanticText { field: "name" });
        }
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

    #[must_use]
    pub const fn role(&self) -> UiSemanticRole {
        self.role
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    #[must_use]
    pub const fn selected(&self) -> Option<bool> {
        self.selected
    }

    #[must_use]
    pub const fn expanded(&self) -> Option<bool> {
        self.expanded
    }

    #[must_use]
    pub fn actions(&self) -> &[UiActionId] {
        &self.actions
    }
}

/// One ordered authoring node. The nested shape is input data, not retained execution state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    pub(crate) key: Option<UiLocalKey>,
    pub(crate) style: UiStyle,
    pub(crate) visual: Option<UiVisual>,
    pub(crate) mesh: Option<UiMesh>,
    pub(crate) text: Option<UiNodeText>,
    pub(crate) custom_layout: Option<crate::UiCustomLayout>,
    pub(crate) interaction: UiInteraction,
    pub(crate) semantics: Option<UiSemantics>,
    pub(crate) children: Vec<Self>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UiNodeText {
    Plain(UiText),
    Prepared(UiPreparedText),
}

impl UiNodeText {
    pub(crate) fn validate(&self) -> Result<(), UiStyleError> {
        match self {
            Self::Plain(text) => text.validate(),
            Self::Prepared(text) => text.validate(),
        }
    }

    pub(crate) fn brush(&self) -> &crate::UiBrush {
        match self {
            Self::Plain(text) => &text.brush,
            Self::Prepared(text) => &text.brush,
        }
    }

    pub(crate) fn semantic_text(&self) -> &str {
        match self {
            Self::Plain(text) => text.semantic_text.as_deref().unwrap_or(&text.content),
            Self::Prepared(text) => text
                .semantic_text
                .as_deref()
                .unwrap_or_else(|| text.snapshot.display_text()),
        }
    }
}

impl UiNode {
    #[must_use]
    pub const fn new(style: UiStyle) -> Self {
        Self {
            key: None,
            style,
            visual: None,
            mesh: None,
            text: None,
            custom_layout: None,
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

    /// Captures local content-space geometry between this node's background and text.
    ///
    /// Meshes provide no intrinsic size and are always clipped to the node's content box,
    /// intersected with inherited clipping. They do not establish hit or semantic regions.
    #[must_use]
    pub fn with_mesh(mut self, mesh: UiMesh) -> Self {
        self.mesh = Some(mesh);
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: UiText) -> Self {
        self.text = Some(UiNodeText::Plain(text));
        self
    }

    #[must_use]
    pub fn with_prepared_text(mut self, text: UiPreparedText) -> Self {
        self.text = Some(UiNodeText::Prepared(text));
        self
    }

    /// Replaces default child arrangement with a repeatable content-space layout description.
    ///
    /// The compiler retains outer sizing/padding and all projection ownership. Put text in a
    /// child node; a custom container cannot also own intrinsic text. Inner Flex gap/alignment
    /// rules are replaced by this description; the node's outer Flex participation remains.
    #[must_use]
    pub fn with_custom_layout(mut self, layout: crate::UiCustomLayout) -> Self {
        self.custom_layout = Some(layout);
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
///
/// The root establishes a layout scope at the surface's authored origin. Width, height and
/// padding apply here; use a positioned child to offset content within that scope. Root inset
/// values do not relocate the surface or its layout origin.
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

    /// Captures geometry under the same contract as [`UiNode::with_mesh`].
    #[must_use]
    pub fn with_mesh(mut self, mesh: UiMesh) -> Self {
        self.node.mesh = Some(mesh);
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: UiText) -> Self {
        self.node.text = Some(UiNodeText::Plain(text));
        self
    }

    #[must_use]
    pub fn with_prepared_text(mut self, text: UiPreparedText) -> Self {
        self.node.text = Some(UiNodeText::Prepared(text));
        self
    }

    /// Uses the same custom content arrangement as [`UiNode::with_custom_layout`].
    #[must_use]
    pub fn with_custom_layout(mut self, layout: crate::UiCustomLayout) -> Self {
        self.node.custom_layout = Some(layout);
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
    profile: UiSnapshotProfile,
    pub(crate) roots: Vec<UiRoot>,
    pub(crate) envelopes: Vec<UiRootEnvelope>,
}

impl UiAuthoringSnapshot {
    #[must_use]
    pub const fn source_revision(&self) -> UiRevision {
        self.source_revision
    }

    /// Returns admitted source nodes, including the original counts of rejected root bodies.
    ///
    /// Sums stored root-envelope counts without traversing node trees. This describes the
    /// constructed snapshot, independently of compilation, retained visuals, or presentation.
    #[must_use]
    pub fn source_node_count(&self) -> usize {
        self.envelopes.iter().map(UiRootEnvelope::node_count).sum()
    }

    pub(crate) const fn profile(&self) -> UiSnapshotProfile {
        self.profile
    }

    #[must_use]
    /// Returns root bodies that passed root-local authoring validation.
    ///
    /// Rejected root envelopes remain part of compilation order and appear in the compiled
    /// frame's root outcomes, but do not expose an invalid `UiRoot` body here.
    pub fn roots(&self) -> &[UiRoot] {
        &self.roots
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRootEnvelope {
    key: UiRootKey,
    layer: UiLayer,
    ordinal: usize,
    node_count: usize,
    mesh_counts: Option<MeshCounts>,
    body: UiRootEnvelopeBody,
}

impl UiRootEnvelope {
    pub(crate) const fn key(&self) -> &UiRootKey {
        &self.key
    }

    pub(crate) const fn layer(&self) -> UiLayer {
        self.layer
    }

    pub(crate) const fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub(crate) const fn node_count(&self) -> usize {
        self.node_count
    }

    pub(crate) const fn mesh_counts(&self) -> Option<MeshCounts> {
        self.mesh_counts
    }

    pub(crate) const fn body(&self) -> &UiRootEnvelopeBody {
        &self.body
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UiRootEnvelopeBody {
    Ready(usize),
    Rejected(UiRootDiagnostic),
}

#[derive(Clone, Debug)]
struct PendingRootEnvelope {
    key: UiRootKey,
    layer: UiLayer,
    ordinal: usize,
    node_count: usize,
    mesh_counts: Option<MeshCounts>,
    body: PendingRootEnvelopeBody,
}

#[derive(Clone, Debug)]
enum PendingRootEnvelopeBody {
    Ready,
    Rejected(UiRootDiagnostic),
}

/// Concrete fallible construction boundary for one authoring snapshot.
#[derive(Clone, Debug)]
pub struct UiAuthoringSnapshotBuilder {
    source_revision: u64,
    limits: UiLimits,
    ready_roots: Vec<UiRoot>,
    envelopes: Vec<PendingRootEnvelope>,
}

impl UiAuthoringSnapshotBuilder {
    #[must_use]
    pub fn new(source_revision: u64) -> Self {
        Self {
            source_revision,
            limits: UiLimits::default(),
            ready_roots: Vec::new(),
            envelopes: Vec::new(),
        }
    }

    #[must_use]
    pub const fn with_limits(mut self, limits: UiLimits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub fn with_root(mut self, root: UiRoot) -> Self {
        let (node_count, mesh_counts) = source_counts(&root.node);
        let ordinal = self.envelopes.len();
        self.envelopes.push(PendingRootEnvelope {
            key: root.key.clone(),
            layer: root.layer,
            ordinal,
            node_count,
            mesh_counts,
            body: PendingRootEnvelopeBody::Ready,
        });
        self.ready_roots.push(root);
        self
    }

    pub fn build(self) -> Result<UiAuthoringSnapshot, UiAuthoringError> {
        self.limits.validate()?;
        let source_revision =
            UiRevision::new(self.source_revision).ok_or(UiAuthoringError::ZeroSourceRevision)?;
        if self.envelopes.len() > self.limits.max_roots {
            return Err(UiAuthoringError::RootLimitExceeded {
                actual: self.envelopes.len(),
                maximum: self.limits.max_roots,
            });
        }

        let mut root_keys = BTreeSet::new();
        let mut node_count = 0usize;
        let mut mesh_counts = MeshCounts::default();
        for root in &self.envelopes {
            if root.key.encoded_len() > self.limits.max_key_bytes {
                return Err(UiAuthoringError::KeyLimitExceeded {
                    actual: root.key.encoded_len(),
                    maximum: self.limits.max_key_bytes,
                });
            }
            UiKey::root(root.key.clone()).map_err(UiAuthoringError::Key)?;
            if !root_keys.insert(root.key.clone()) {
                return Err(UiAuthoringError::DuplicateRootKey {
                    key: root.key.clone(),
                });
            }
            node_count = node_count.saturating_add(root.node_count);
            if node_count > self.limits.max_nodes {
                return Err(UiAuthoringError::NodeLimitExceeded {
                    actual: node_count,
                    maximum: self.limits.max_nodes,
                });
            }
            mesh_counts = mesh_counts
                .checked_add(
                    root.mesh_counts
                        .ok_or(UiAuthoringError::MeshCountOverflow)?,
                )
                .ok_or(UiAuthoringError::MeshCountOverflow)?;
            if mesh_counts.vertices > self.limits.max_mesh_vertices {
                return Err(UiAuthoringError::MeshVertexLimitExceeded {
                    actual: mesh_counts.vertices,
                    maximum: self.limits.max_mesh_vertices,
                });
            }
            if mesh_counts.indices > self.limits.max_mesh_indices {
                return Err(UiAuthoringError::MeshIndexLimitExceeded {
                    actual: mesh_counts.indices,
                    maximum: self.limits.max_mesh_indices,
                });
            }
        }

        let limits = self.limits;
        let mut pending_roots = self.ready_roots.into_iter();
        let mut roots = Vec::new();
        let envelopes = self
            .envelopes
            .into_iter()
            .map(|envelope| {
                let body = match envelope.body {
                    PendingRootEnvelopeBody::Ready => {
                        let root = pending_roots
                            .next()
                            .expect("each ready envelope owns one pending root");
                        match validate_root(&root, limits) {
                            Ok(()) => {
                                let index = roots.len();
                                roots.push(root);
                                UiRootEnvelopeBody::Ready(index)
                            }
                            Err(diagnostic) => UiRootEnvelopeBody::Rejected(diagnostic),
                        }
                    }
                    PendingRootEnvelopeBody::Rejected(diagnostic) => {
                        UiRootEnvelopeBody::Rejected(diagnostic)
                    }
                };
                UiRootEnvelope {
                    key: envelope.key,
                    layer: envelope.layer,
                    ordinal: envelope.ordinal,
                    node_count: envelope.node_count,
                    mesh_counts: envelope.mesh_counts,
                    body,
                }
            })
            .collect();
        debug_assert!(pending_roots.next().is_none());
        Ok(UiAuthoringSnapshot {
            source_revision,
            profile: limits.snapshot_profile(),
            roots,
            envelopes,
        })
    }
}

/// Adds a root whose stable envelope is known even though its adapter-owned body was rejected.
#[doc(hidden)]
pub fn with_rejected_root(
    mut builder: UiAuthoringSnapshotBuilder,
    key: UiRootKey,
    layer: UiLayer,
    node_count: usize,
    diagnostic: UiRootDiagnostic,
) -> UiAuthoringSnapshotBuilder {
    let ordinal = builder.envelopes.len();
    builder.envelopes.push(PendingRootEnvelope {
        key,
        layer,
        ordinal,
        node_count,
        mesh_counts: Some(MeshCounts::default()),
        body: PendingRootEnvelopeBody::Rejected(diagnostic),
    });
    builder
}

fn source_counts(root: &UiNode) -> (usize, Option<MeshCounts>) {
    let mut count = 0usize;
    let mut mesh_counts = Some(MeshCounts::default());
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        count = count.saturating_add(1);
        if let Some(mesh) = &node.mesh {
            mesh_counts = mesh_counts.and_then(|counts| counts.checked_add(MeshCounts::of(mesh)));
        }
        pending.extend(node.children.iter());
    }
    (count, mesh_counts)
}

fn validate_root(root: &UiRoot, limits: UiLimits) -> Result<(), UiRootDiagnostic> {
    let root_key =
        UiKey::root(root.key.clone()).map_err(|error| UiRootDiagnostic::KeyRejected {
            message: error.to_string(),
        })?;
    let mut keys = BTreeSet::new();
    let mut structural_index = 0usize;
    let mut pending = vec![(&root.node, 1usize, Some(root_key))];
    while let Some((node, depth, forced_key)) = pending.pop() {
        if depth > limits.max_depth {
            return Err(UiRootDiagnostic::DepthLimitExceeded {
                maximum: limits.max_depth,
            });
        }
        node.style
            .validate()
            .map_err(|error| UiRootDiagnostic::InvalidStyle {
                message: error.to_string(),
            })?;
        if let Some(visual) = &node.visual {
            visual
                .validate()
                .map_err(|error| UiRootDiagnostic::InvalidStyle {
                    message: error.to_string(),
                })?;
        }
        if node.custom_layout.is_some() && node.text.is_some() {
            return Err(UiRootDiagnostic::LayoutRejected {
                message: "a custom layout container must put intrinsic text in a child node".into(),
            });
        }
        if let Some(text) = &node.text {
            text.validate()
                .map_err(|error| UiRootDiagnostic::InvalidStyle {
                    message: error.to_string(),
                })?;
        }
        let key = match forced_key {
            Some(key) => Some(key),
            None => node
                .key
                .as_ref()
                .map(|local| UiKey::scoped(root.key.clone(), local.clone()))
                .transpose()
                .map_err(|error| UiRootDiagnostic::KeyRejected {
                    message: error.to_string(),
                })?,
        };
        if let Some(key) = &key {
            if key.encoded_len() > limits.max_key_bytes {
                return Err(UiRootDiagnostic::KeyRejected {
                    message: format!(
                        "key has {} encoded bytes; maximum is {}",
                        key.encoded_len(),
                        limits.max_key_bytes
                    ),
                });
            }
            if !keys.insert(key.clone()) {
                return Err(UiRootDiagnostic::DuplicateNodeKey { key: key.clone() });
            }
        }
        let requires_key = node.interaction.hit_testable
            || node.interaction.focusable
            || node.semantics.is_some()
            || node.text.is_some();
        if requires_key && key.is_none() {
            return Err(UiRootDiagnostic::MissingRequiredKey { structural_index });
        }
        structural_index = structural_index.saturating_add(1);
        pending.extend(
            node.children
                .iter()
                .rev()
                .map(|child| (child, depth + 1, None)),
        );
    }
    Ok(())
}

fn validate_semantic_text(field: &'static str, value: &str) -> Result<(), UiAuthoringError> {
    if value.len() > 16 * 1024 {
        return Err(UiAuthoringError::SemanticTextLimitExceeded {
            field,
            actual: value.len(),
            maximum: 16 * 1024,
        });
    }
    Ok(())
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
    #[error("UI snapshot contains duplicate root key {key:?}")]
    DuplicateRootKey { key: UiRootKey },
    #[error("UI snapshot has at least {actual} nodes; maximum is {maximum}")]
    NodeLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot has {actual} mesh vertices; maximum is {maximum}")]
    MeshVertexLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot has {actual} mesh indices; maximum is {maximum}")]
    MeshIndexLimitExceeded { actual: usize, maximum: usize },
    #[error("UI snapshot mesh occurrence counts overflow")]
    MeshCountOverflow,
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

#[cfg(test)]
mod root_budget_tests {
    use super::*;

    fn mesh() -> crate::UiMesh {
        let color = crate::UiColor::rgba(1.0, 0.5, 0.25, 0.5);
        crate::UiMesh::new(
            vec![
                crate::UiMeshVertex::new(crate::UiPoint::new(0.0, 0.0), color),
                crate::UiMeshVertex::new(crate::UiPoint::new(20.0, 0.0), color),
                crate::UiMeshVertex::new(crate::UiPoint::new(0.0, 20.0), color),
            ],
            vec![0, 1, 2],
        )
        .unwrap()
    }

    fn mesh_environment() -> crate::UiEnvironment {
        crate::UiEnvironment::new(
            crate::UiSurfaceId::new(1).unwrap(),
            200.0,
            100.0,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap()
    }

    #[test]
    fn compiler_rechecks_complete_mesh_source_counts_after_a_body_was_dropped() {
        let invalid = UiRoot::new(
            crate::UiRootKey::runtime("mesh-budget", "rejected").unwrap(),
            crate::UiLayer::Base,
            crate::UiStyle::fill().with_gap(f32::NAN),
        )
        .with_mesh(mesh())
        .with_child(UiNode::new(crate::UiStyle::fill()).with_mesh(mesh()));
        let source = UiAuthoringSnapshotBuilder::new(1)
            .with_root(invalid)
            .build()
            .unwrap();
        assert!(source.roots().is_empty());
        assert_eq!(
            source.envelopes[0].mesh_counts(),
            Some(MeshCounts {
                vertices: 6,
                indices: 6
            })
        );

        for (limits, vertex_refusal) in [
            (UiLimits::default().with_max_mesh_vertices(5).unwrap(), true),
            (UiLimits::default().with_max_mesh_indices(5).unwrap(), false),
        ] {
            let mut source = source.clone();
            source.profile = limits.snapshot_profile();
            let mut compiler =
                crate::UiCompiler::new(nara_text::bundled_font_face().unwrap(), limits).unwrap();
            let result = compiler.compile(&source, mesh_environment(), crate::UiTheme::default());
            assert!(matches!(
                (result, vertex_refusal),
                (
                    Err(crate::UiCompileError::SnapshotMeshVertexLimitExceeded {
                        actual: 6,
                        maximum: 5
                    }),
                    true
                ) | (
                    Err(crate::UiCompileError::SnapshotMeshIndexLimitExceeded {
                        actual: 6,
                        maximum: 5
                    }),
                    false
                )
            ));
            assert_eq!(compiler.work_stats().root_compile_attempts, 0);
            assert_eq!(compiler.work_stats().published_frames, 0);
            let recovery = UiAuthoringSnapshotBuilder::new(2)
                .with_limits(limits)
                .with_root(
                    UiRoot::new(
                        crate::UiRootKey::runtime("mesh-budget", "rejected").unwrap(),
                        crate::UiLayer::Base,
                        crate::UiStyle::fill(),
                    )
                    .with_mesh(mesh()),
                )
                .build()
                .unwrap();
            let frame = compiler
                .compile(&recovery, mesh_environment(), crate::UiTheme::default())
                .unwrap();
            assert_eq!(frame.revision().get(), 1);
            assert!(matches!(
                frame.paint()[0],
                crate::UiPaintPrimitive::Mesh { .. }
            ));
        }
    }

    #[test]
    fn unknown_mesh_source_counts_fail_globally_before_any_root_work() {
        let root = UiRoot::new(
            crate::UiRootKey::runtime("mesh-budget", "overflow").unwrap(),
            crate::UiLayer::Base,
            crate::UiStyle::fill(),
        )
        .with_mesh(mesh());
        let mut builder = UiAuthoringSnapshotBuilder::new(1).with_root(root.clone());
        builder.envelopes[0].mesh_counts = None;
        assert!(matches!(
            builder.build(),
            Err(UiAuthoringError::MeshCountOverflow)
        ));

        let mut source = UiAuthoringSnapshotBuilder::new(1)
            .with_root(root)
            .build()
            .unwrap();
        source.envelopes[0].mesh_counts = None;
        let mut compiler =
            crate::UiCompiler::new(nara_text::bundled_font_face().unwrap(), UiLimits::default())
                .unwrap();
        assert!(matches!(
            compiler.compile(&source, mesh_environment(), crate::UiTheme::default()),
            Err(crate::UiCompileError::SnapshotMeshCountOverflow),
        ));
        assert_eq!(compiler.work_stats().root_compile_attempts, 0);
        assert_eq!(compiler.work_stats().published_frames, 0);
    }

    #[test]
    fn compiler_rechecks_complete_root_cost_even_for_an_absent_body() {
        let root = UiRoot::new(
            crate::UiRootKey::runtime("a", "b").unwrap(),
            crate::UiLayer::Base,
            crate::UiStyle::fill(),
        )
        .with_child(
            crate::UiNode::new(crate::UiStyle::fill())
                .with_text(crate::UiText::new("unkeyed", 16.0).unwrap()),
        );
        let mut snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(root)
            .build()
            .unwrap();
        assert!(snapshot.roots().is_empty());
        let limits = crate::UiLimits::default().with_max_key_bytes(9).unwrap();
        snapshot.profile = limits.snapshot_profile();
        let mut compiler =
            crate::UiCompiler::new(nara_text::bundled_font_face().unwrap(), limits).unwrap();
        let environment = crate::UiEnvironment::new(
            crate::UiSurfaceId::new(1).unwrap(),
            200.0,
            100.0,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        assert!(matches!(
            compiler.compile(&snapshot, environment, crate::UiTheme::default()),
            Err(crate::UiCompileError::SnapshotKeyLimitExceeded {
                actual: 10,
                maximum: 9
            })
        ));
    }
}
