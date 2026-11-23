//! Source declaration selectors and disposable compilation observations.

use nara_reflect::{
    __private::component_values_exactly_match, ComponentSchemaVersion, ComponentTypeId,
};

use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim,
    PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent, SceneComponentRecord, SceneEntityId,
    SourceDocumentKey,
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

impl PrefabInstance {
    pub(crate) fn declarations_exactly_match(left: &Option<Self>, right: &Option<Self>) -> bool {
        match (left, right) {
            (None, None) => true,
            (Some(left), Some(right)) => left.exactly_matches(right),
            _ => false,
        }
    }

    fn exactly_matches(&self, other: &Self) -> bool {
        let Self {
            source,
            binding,
            arguments,
            overrides,
        } = self;
        source == &other.source
            && binding == &other.binding
            && arguments == &other.arguments
            && match (overrides, &other.overrides) {
                (None, None) => true,
                (Some(left), Some(right)) => left.exactly_matches(right),
                _ => false,
            }
    }
}

impl PrefabOverlay {
    fn exactly_matches(&self, other: &Self) -> bool {
        let Self {
            base,
            additions,
            instances,
            components,
            placements,
            suppressions,
        } = self;
        base == &other.base
            && placements == &other.placements
            && suppressions == &other.suppressions
            && additions.len() == other.additions.len()
            && instances.len() == other.instances.len()
            && components.len() == other.components.len()
            && additions
                .iter()
                .zip(&other.additions)
                .all(|(left, right)| left.exactly_matches(right))
            && instances
                .iter()
                .zip(&other.instances)
                .all(|(left, right)| left.exactly_matches(right))
            && components
                .iter()
                .zip(&other.components)
                .all(|(left, right)| left.exactly_matches(right))
    }
}

impl PrefabAddition {
    fn exactly_matches(&self, other: &Self) -> bool {
        let Self {
            id,
            components,
            prefab,
        } = self;
        id == &other.id
            && components.len() == other.components.len()
            && components.iter().zip(&other.components).all(
                |((component, record), (other_component, other_record))| {
                    component == other_component
                        && component_records_exactly_match(record, other_record)
                },
            )
            && PrefabInstance::declarations_exactly_match(prefab, &other.prefab)
    }
}

impl PrefabInstanceClaim {
    fn exactly_matches(&self, other: &Self) -> bool {
        let Self { target, value } = self;
        target == &other.target && PrefabInstance::declarations_exactly_match(value, &other.value)
    }
}

fn component_records_exactly_match(
    left: &SceneComponentRecord,
    right: &SceneComponentRecord,
) -> bool {
    left.owner == right.owner
        && left.version == right.version
        && component_values_exactly_match(left.value(), right.value())
        && left.contextual_defaults.len() == right.contextual_defaults.len()
        && left
            .contextual_defaults
            .iter()
            .zip(&right.contextual_defaults)
            .all(|((field, value), (other_field, other_value))| {
                field == other_field && component_values_exactly_match(value, other_value)
            })
}

impl PrefabComponentIntent {
    pub(crate) fn address(&self) -> (&PrefabEntityTarget, &ComponentTypeId) {
        match self {
            Self::Set {
                target, component, ..
            }
            | Self::Suppress { target, component }
            | Self::Properties {
                target, component, ..
            } => (target, component),
        }
    }

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
                    && component_records_exactly_match(record, other_record)
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
