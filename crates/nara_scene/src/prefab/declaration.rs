//! Source declaration selectors and disposable compilation observations.

use nara_reflect::{
    __private::component_values_exactly_match, ComponentSchemaVersion, ComponentTypeId,
};

use crate::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabPropertyClaim, PrefabPropertyIntent,
    SceneEntityId, SourceDocumentKey,
};

/// Why composition refused publication.
///
/// Source admission and work limits do not establish a conflicting or removable source intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCompilationFailureKind {
    SourceChanged,
    MissingTarget,
    DuplicateIntent,
    Placement,
    RootBinding,
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

/// A borrowed property declaration, including the authored route and the exact stored intent.
#[derive(Debug, Clone, Copy)]
pub struct PrefabPropertyClaimRef<'a> {
    pub(crate) slot: PrefabOverlaySlot,
    pub(crate) target: &'a PrefabEntityTarget,
    pub(crate) component: &'a ComponentTypeId,
    pub(crate) version: ComponentSchemaVersion,
    pub(crate) claim: &'a PrefabPropertyClaim,
}

impl<'a> PrefabPropertyClaimRef<'a> {
    #[must_use]
    pub const fn slot(self) -> PrefabOverlaySlot {
        self.slot
    }

    #[must_use]
    pub const fn component(self) -> &'a ComponentTypeId {
        self.component
    }

    #[must_use]
    pub const fn version(self) -> ComponentSchemaVersion {
        self.version
    }

    #[must_use]
    pub const fn claim(self) -> &'a PrefabPropertyClaim {
        self.claim
    }
}

impl PartialEq for PrefabPropertyClaimRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot
            && self.target == other.target
            && self.component == other.component
            && self.version == other.version
            && property_claims_exactly_match(self.claim, other.claim)
    }
}

impl Eq for PrefabPropertyClaimRef<'_> {}

fn property_claims_exactly_match(left: &PrefabPropertyClaim, right: &PrefabPropertyClaim) -> bool {
    std::ptr::eq(left, right)
        || (left.path == right.path
            && match (&left.intent, &right.intent) {
                (PrefabPropertyIntent::Absent, PrefabPropertyIntent::Absent) => true,
                (PrefabPropertyIntent::Set(left), PrefabPropertyIntent::Set(right)) => {
                    component_values_exactly_match(left, right)
                }
                _ => false,
            })
}

impl PrefabComponentIntent {
    pub(crate) fn exactly_matches(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Set {
                    mode,
                    target,
                    component,
                    record,
                },
                Self::Set {
                    mode: other_mode,
                    target: other_target,
                    component: other_component,
                    record: other_record,
                },
            ) => {
                mode == other_mode
                    && target == other_target
                    && component == other_component
                    && record.owner == other_record.owner
                    && record.version == other_record.version
                    && component_values_exactly_match(record.value(), other_record.value())
                    && record.contextual_defaults.len() == other_record.contextual_defaults.len()
                    && record
                        .contextual_defaults
                        .iter()
                        .zip(&other_record.contextual_defaults)
                        .all(|((field, value), (other_field, other_value))| {
                            field == other_field
                                && component_values_exactly_match(value, other_value)
                        })
            }
            (
                Self::Suppress { target, component },
                Self::Suppress {
                    target: other_target,
                    component: other_component,
                },
            ) => target == other_target && component == other_component,
            (
                Self::Properties {
                    target,
                    component,
                    version,
                    claims,
                },
                Self::Properties {
                    target: other_target,
                    component: other_component,
                    version: other_version,
                    claims: other_claims,
                },
            ) => {
                target == other_target
                    && component == other_component
                    && version == other_version
                    && claims.len() == other_claims.len()
                    && claims
                        .iter()
                        .zip(other_claims)
                        .all(|(left, right)| property_claims_exactly_match(left, right))
            }
            _ => false,
        }
    }
}

/// Explicit source changes, distinct from detaching or converting an instance.
#[derive(Debug, Clone, Copy)]
pub enum PrefabRevert<'a> {
    /// Removes an addition, instance claim, placement or entity suppression at this slot.
    Intent(PrefabOverlaySlot),
    /// Removes a whole component declaration when it matches the observed declaration.
    Component {
        index: usize,
        observed: &'a PrefabComponentIntent,
    },
    /// Removes a property declaration only when it agrees with the current source.
    Property(PrefabPropertyClaimRef<'a>),
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
pub struct SceneCompilationFailure {
    pub(crate) reason: SceneCompilationFailureKind,
    pub(crate) location: Option<PrefabInstanceLocation>,
    pub(crate) slot: Option<PrefabOverlaySlot>,
    pub(crate) entity: Option<SceneEntityId>,
}

impl SceneCompilationFailure {
    #[must_use]
    pub const fn reason(&self) -> &SceneCompilationFailureKind {
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

impl From<SceneCompilationFailureKind> for SceneCompilationFailure {
    fn from(reason: SceneCompilationFailureKind) -> Self {
        Self {
            reason,
            location: None,
            slot: None,
            entity: None,
        }
    }
}
