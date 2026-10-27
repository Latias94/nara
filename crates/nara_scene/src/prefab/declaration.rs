//! Source declaration selectors and disposable compilation observations.

use crate::{SceneEntityId, SourceDocumentKey};

/// Why composition refused publication.
///
/// Source admission and work limits do not establish a conflicting or removable source intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExpansionFailureKind {
    SourceChanged,
    MissingTarget,
    DuplicateIntent,
    Placement,
    Component,
    Reference,
    OwnerAdmission,
    WorkLimit,
}

/// One nested instance declaration in an observed source snapshot.
///
/// Indices refer to the stored declaration arrays, not to expanded entities. They must be
/// used with the originating document revision and Schema basis, never as durable identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrefabInstancePathSegment {
    Addition { index: usize },
    InstanceClaim { index: usize },
}

/// An instance declaration owned by one local source anchor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefabInstancePath {
    pub anchor: SceneEntityId,
    pub steps: Vec<PrefabInstancePathSegment>,
}

impl PrefabInstancePath {
    #[must_use]
    pub const fn new(anchor: SceneEntityId) -> Self {
        Self {
            anchor,
            steps: Vec::new(),
        }
    }
}

/// One authored intent in an observed overlay snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrefabOverlaySlot {
    Addition {
        index: usize,
    },
    InstanceClaim {
        index: usize,
    },
    Component {
        index: usize,
    },
    Property {
        component_index: usize,
        claim_index: usize,
    },
    Placement {
        index: usize,
    },
    Suppression {
        index: usize,
    },
}

/// Explicit source changes, distinct from detaching or converting an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabRevert {
    /// Removes exactly this authored intent; unrelated intent and the base pin remain.
    Intent(PrefabOverlaySlot),
    /// Removes the complete overlay, including its base pin, while preserving the source link.
    Overlay,
}

/// The source owning a declaration observed during composition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrefabDeclarationSource {
    /// The input document supplied by the caller.
    Containing,
    /// An actually resolved Prefab. The key identifies its source, not its content revision.
    Prefab(SourceDocumentKey),
}

/// A source declaration location, independent of its materialized occurrence.
///
/// This observation grants no mutation authority. Editing a resolved Prefab requires that
/// Prefab's own current session basis, not the containing document's revision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrefabInstanceLocation {
    pub(crate) source: PrefabDeclarationSource,
    pub(crate) path: PrefabInstancePath,
}

impl PrefabInstanceLocation {
    #[must_use]
    pub const fn source(&self) -> &PrefabDeclarationSource {
        &self.source
    }

    #[must_use]
    pub const fn path(&self) -> &PrefabInstancePath {
        &self.path
    }
}

/// The first rejected compilation step, with source location only when it is established.
///
/// Failure is not an exhaustive inventory of conflicting intent. Budget, loading and authority
/// errors may have no declaration location; callers must not invent a replacement anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabExpansionFailure {
    pub(crate) reason: PrefabExpansionFailureKind,
    pub(crate) location: Option<PrefabInstanceLocation>,
    pub(crate) slot: Option<PrefabOverlaySlot>,
}

impl PrefabExpansionFailure {
    #[must_use]
    pub const fn reason(&self) -> &PrefabExpansionFailureKind {
        &self.reason
    }

    #[must_use]
    pub const fn location(&self) -> Option<&PrefabInstanceLocation> {
        self.location.as_ref()
    }

    #[must_use]
    pub const fn slot(&self) -> Option<PrefabOverlaySlot> {
        self.slot
    }
}

impl From<PrefabExpansionFailureKind> for PrefabExpansionFailure {
    fn from(reason: PrefabExpansionFailureKind) -> Self {
        Self {
            reason,
            location: None,
            slot: None,
        }
    }
}
