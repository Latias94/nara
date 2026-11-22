use std::sync::Arc;

use nara_reflect::{
    ComponentFieldId, ComponentPropertyPath, ComponentPropertyStep, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    AuthoringRecordReadiness, PrefabEntityTarget, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabOverlaySlot, SceneEntityId,
};

use crate::document_query::EditorDocumentQueryOwner;
use crate::{EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection};

mod value_page;

pub use value_page::{
    EditorPropertyPage, EditorPropertyPageCursor, EditorPropertyPageData, EditorPropertyPageItem,
    EditorPropertyPagePosition, EditorPropertyPageRead, EditorPropertyPageValue,
};
pub(crate) use value_page::{PropertyValuePagePosition, PropertyValuePageTarget};

pub const MAX_EDITOR_AUTHORING_QUERY_ENTRIES: usize = 256;
pub const MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_EDITOR_PROPERTY_READ_VALUE_NODES: usize = 4_096;
pub const MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES: usize = 256 * 1024;
pub const MAX_EDITOR_PROPERTY_READ_SCAN_ENTRIES: usize = 65_536;
pub const MAX_EDITOR_PROPERTY_READ_SCAN_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EDITOR_AUTHORING_QUERY_SCAN_BYTES: usize = 4 * 1024 * 1024;

/// Operation-local discovery limits. Metadata counts retained result and cursor identities.
/// This is a logical content allowance, not an allocator bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorAuthoringQueryLimits {
    max_scan_entries: usize,
    max_rows: usize,
    max_metadata_bytes: usize,
    max_scan_logical_bytes: usize,
}

impl EditorAuthoringQueryLimits {
    pub fn new(
        max_scan_entries: usize,
        max_rows: usize,
        max_metadata_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_AUTHORING_QUERY_ENTRIES).contains(&max_scan_entries)
            || !(1..=MAX_EDITOR_AUTHORING_QUERY_ENTRIES).contains(&max_rows)
            || !(1..=MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES).contains(&max_metadata_bytes)
        {
            return Err(EditorDocumentQueryRejection::InvalidAuthoringLimits);
        }
        Ok(Self {
            max_scan_entries,
            max_rows,
            max_metadata_bytes,
            max_scan_logical_bytes: MAX_EDITOR_AUTHORING_QUERY_SCAN_BYTES,
        })
    }
    #[must_use]
    pub const fn max_scan_entries(self) -> usize {
        self.max_scan_entries
    }
    #[must_use]
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
    #[must_use]
    pub const fn max_metadata_bytes(self) -> usize {
        self.max_metadata_bytes
    }

    /// Sets the logical bytes available to recursive property or Prefab claim discovery.
    ///
    /// Component, root and direct member queries do not need this allowance.
    pub fn with_scan_logical_bytes(
        mut self,
        logical_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_AUTHORING_QUERY_SCAN_BYTES).contains(&logical_bytes) {
            return Err(EditorDocumentQueryRejection::InvalidAuthoringLimits);
        }
        self.max_scan_logical_bytes = logical_bytes;
        Ok(self)
    }

    #[must_use]
    pub const fn max_scan_logical_bytes(self) -> usize {
        self.max_scan_logical_bytes
    }
}

/// Discovery copies compact metadata and durable selectors, never selected payload trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringProjection {
    Components,
    Roots {
        component: ComponentTypeId,
    },
    Members {
        component: ComponentTypeId,
        root: ComponentFieldId,
    },
    /// Enumerates the directly addressable children of an admitted root or descendant.
    ///
    /// Record members, map entries and keyed-list items have durable selectors. Ordinary list
    /// positions are intentionally not exposed because they have no stable source identity.
    Properties {
        component: ComponentTypeId,
        property: Arc<ComponentPropertyPath>,
    },
    /// Enumerates stored property claims of the instance at the target's direct anchor.
    /// Steps enter nested declarations owned by that same source. No base is loaded.
    PrefabPropertyClaims {
        steps: Vec<PrefabInstancePathSegment>,
    },
}

/// Exact source and projection to discover, independent of active navigation and selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringQueryTarget {
    pub basis: EditorDocumentQueryBasis,
    pub entity: SceneEntityId,
    pub projection: EditorAuthoringProjection,
}

impl EditorAuthoringQueryTarget {
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.entity
            .as_str()
            .len()
            .saturating_add(match &self.projection {
                EditorAuthoringProjection::Components => 0,
                EditorAuthoringProjection::Roots { component } => component.as_str().len(),
                EditorAuthoringProjection::Members { component, root } => {
                    component.as_str().len().saturating_add(root.as_str().len())
                }
                EditorAuthoringProjection::Properties {
                    component,
                    property,
                } => component
                    .as_str()
                    .len()
                    .saturating_add(property_metadata_bytes(property)),
                EditorAuthoringProjection::PrefabPropertyClaims { steps } => {
                    prefab_path_step_bytes(steps)
                }
            })
    }
}

pub(crate) fn prefab_path_step_bytes(steps: &[PrefabInstancePathSegment]) -> usize {
    steps.len().saturating_mul(9)
}

fn property_step_metadata_bytes(step: &ComponentPropertyStep) -> usize {
    match step {
        ComponentPropertyStep::Member(id) => id.as_str().len(),
        ComponentPropertyStep::ListItem { by, key } => {
            by.as_str().len().saturating_add(match key {
                nara_reflect::ComponentPropertyKey::String(value) => value.len(),
                nara_reflect::ComponentPropertyKey::I64(_)
                | nara_reflect::ComponentPropertyKey::U64(_) => 8,
            })
        }
        ComponentPropertyStep::MapEntry(key) => key.len(),
    }
}

pub(crate) fn property_metadata_bytes(property: &ComponentPropertyPath) -> usize {
    property_metadata_bytes_up_to(property, usize::MAX).unwrap_or(usize::MAX)
}

pub(crate) fn property_metadata_bytes_up_to(
    property: &ComponentPropertyPath,
    maximum: usize,
) -> Option<usize> {
    let root = property.root.as_str().len();
    if root > maximum {
        return None;
    }
    property.steps.iter().try_fold(root, |bytes, step| {
        let bytes = bytes.checked_add(property_step_metadata_bytes(step))?;
        (bytes <= maximum).then_some(bytes)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringQueryPosition {
    Start(EditorAuthoringQueryTarget),
    Continue(EditorAuthoringCursor),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringQuery {
    pub position: EditorAuthoringQueryPosition,
    pub limits: EditorAuthoringQueryLimits,
}

/// Replayable continuation for the original workspace or preparation. Its private target retains
/// the Schema admission without retaining the document or granting permission to publish edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringCursor {
    pub(crate) owner: EditorDocumentQueryOwner,
    pub(crate) kind: EditorDocumentKind,
    pub(crate) position: AuthoringCursorPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthoringCursorPosition {
    Direct {
        target: Arc<EditorAuthoringQueryTarget>,
        next: AuthoringPagePosition,
    },
    PrefabClaims {
        source: Arc<PrefabPropertyClaimSource>,
        component_index: usize,
        claim_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthoringPagePosition {
    Components {
        after: Option<ComponentTypeId>,
        visited: usize,
    },
    Roots {
        next_root: usize,
    },
    Members {
        next_member: usize,
        next_tombstone: usize,
    },
    Properties {
        position: PropertyPagePosition,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PropertyPagePosition {
    Initial,
    Record {
        next_member: usize,
        next_tombstone: usize,
    },
    Map {
        after: Arc<ComponentPropertyStep>,
    },
    KeyedList {
        next_item: usize,
    },
}

/// Availability of one discovered durable Record member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthoringMemberStatus {
    /// A current durable Schema declaration; this does not assert that a source value is present.
    Available,
    /// A retained declaration path with no current member shape.
    Retired,
}

/// Declaration or collection-identity availability in an exact source observation.
/// Value presence and explicit Null remain separate property-read facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthoringPropertyStatus {
    Available,
    Retired,
}

/// A durable child selector that shares its parent query path with the rest of the page.
/// Materialize the complete schema-relative path when issuing a selected read or edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringPropertySelector {
    pub(crate) parent: Arc<ComponentPropertyPath>,
    pub(crate) step: Arc<ComponentPropertyStep>,
}

impl EditorAuthoringPropertySelector {
    #[must_use]
    pub fn child_step(&self) -> &ComponentPropertyStep {
        self.step.as_ref()
    }

    #[must_use]
    pub fn to_path(&self) -> ComponentPropertyPath {
        self.parent
            .as_ref()
            .clone()
            .with_steps([self.child_step().clone()])
    }
}

/// Exact stored claim selected by a query. Cloning preserves its original workspace, basis,
/// instance path and source slot together. Late reads and Revert still require current admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPrefabPropertyClaimTarget {
    pub(crate) owner: EditorDocumentQueryOwner,
    pub(crate) source: Arc<PrefabPropertyClaimSource>,
    pub(crate) slot: PrefabOverlaySlot,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PrefabPropertyClaimSource {
    pub(crate) basis: EditorDocumentQueryBasis,
    pub(crate) path: PrefabInstancePath,
}

impl EditorPrefabPropertyClaimTarget {
    #[must_use]
    pub fn basis(&self) -> &EditorDocumentQueryBasis {
        &self.source.basis
    }

    #[must_use]
    pub fn path(&self) -> &PrefabInstancePath {
        &self.source.path
    }

    #[must_use]
    pub const fn slot(&self) -> PrefabOverlaySlot {
        self.slot
    }
}

/// Compact actual record/root/member metadata, not a Schema, creation capability or interpreted
/// payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringRow {
    Component {
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        readiness: AuthoringRecordReadiness,
    },
    Root {
        field: ComponentFieldId,
        kind: ComponentValueKind,
        required: bool,
        /// Declared Edit capability only; final candidate admission still decides an edit.
        edit_capability: bool,
    },
    Member {
        /// A Schema-relative selector with exactly one direct `Member` step.
        property: ComponentPropertyPath,
        /// Retired members have no current shape and therefore report `None`.
        kind: Option<ComponentValueKind>,
        /// Retired members are never editable.
        edit_capability: bool,
        status: EditorAuthoringMemberStatus,
    },
    Property {
        /// A durable selector with one additional child step relative to the query target.
        property: EditorAuthoringPropertySelector,
        /// Retired declarations have no current shape and therefore report `None`.
        kind: Option<ComponentValueKind>,
        /// The root field's declared Edit capability; final candidate admission still decides.
        edit_capability: bool,
        status: EditorAuthoringPropertyStatus,
    },
    /// Stored source intent. The claim retains this page's exact instance path and basis.
    /// The target route and property identify authored intent independently of activation.
    PrefabPropertyClaim {
        claim: EditorPrefabPropertyClaimTarget,
        target: PrefabEntityTarget,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        property: ComponentPropertyPath,
        /// An explicit Set payload, including explicit Null, is distinct from an Absent claim.
        has_value: bool,
    },
    /// The complete authored target/property header cannot fit even on a full-budget page.
    /// The original claim remains readable and revertible through its exact source slot.
    OversizedPrefabPropertyClaim {
        claim: EditorPrefabPropertyClaimTarget,
    },
}

/// Owned historical observation. A current page is not mutation authority after its basis changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringPage {
    pub target: Arc<EditorAuthoringQueryTarget>,
    pub rows: Vec<EditorAuthoringRow>,
    pub scanned_entries: usize,
    /// Logical identity work for property selectors, child identities and Prefab claim headers.
    /// A claim header that exceeds a limit charges the allowance examined before refusal.
    pub scanned_logical_bytes: usize,
    /// Aggregate retained logical identity bytes. Direct discovery shares its target, and Map
    /// continuations share the last returned key. Each shared identity is counted once.
    pub metadata_bytes: usize,
    pub next: Option<EditorAuthoringCursor>,
}

/// Separately capped metadata, selected-value copies and selector lookup work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPropertyReadLimits {
    max_metadata_bytes: usize,
    max_value_nodes: usize,
    max_logical_bytes: usize,
    max_scan_entries: usize,
    max_scan_logical_bytes: usize,
}

impl EditorPropertyReadLimits {
    pub fn new(
        max_metadata_bytes: usize,
        max_value_nodes: usize,
        max_logical_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES).contains(&max_metadata_bytes)
            || !(1..=MAX_EDITOR_PROPERTY_READ_VALUE_NODES).contains(&max_value_nodes)
            || !(1..=MAX_EDITOR_PROPERTY_READ_LOGICAL_BYTES).contains(&max_logical_bytes)
        {
            return Err(EditorDocumentQueryRejection::InvalidPropertyReadLimits);
        }
        Ok(Self {
            max_metadata_bytes,
            max_value_nodes,
            max_logical_bytes,
            max_scan_entries: MAX_EDITOR_PROPERTY_READ_SCAN_ENTRIES,
            max_scan_logical_bytes: MAX_EDITOR_PROPERTY_READ_SCAN_BYTES,
        })
    }
    #[must_use]
    pub const fn max_metadata_bytes(self) -> usize {
        self.max_metadata_bytes
    }
    #[must_use]
    pub const fn max_value_nodes(self) -> usize {
        self.max_value_nodes
    }
    #[must_use]
    pub const fn max_logical_bytes(self) -> usize {
        self.max_logical_bytes
    }

    pub fn with_scan_limits(
        mut self,
        entries: usize,
        logical_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_PROPERTY_READ_SCAN_ENTRIES).contains(&entries)
            || !(1..=MAX_EDITOR_PROPERTY_READ_SCAN_BYTES).contains(&logical_bytes)
        {
            return Err(EditorDocumentQueryRejection::InvalidPropertyReadLimits);
        }
        self.max_scan_entries = entries;
        self.max_scan_logical_bytes = logical_bytes;
        Ok(self)
    }

    #[must_use]
    pub const fn max_scan_entries(self) -> usize {
        self.max_scan_entries
    }

    #[must_use]
    pub const fn max_scan_logical_bytes(self) -> usize {
        self.max_scan_logical_bytes
    }

    pub(crate) fn scan_budget(self) -> nara_reflect::ComponentPropertyReadBudget {
        nara_reflect::ComponentPropertyReadBudget::new(
            nara_core::ItemLimit::new(self.max_scan_entries).expect("validated scan limit"),
            nara_core::ByteLimit::new(self.max_scan_logical_bytes).expect("validated scan limit"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPropertyTarget {
    pub basis: EditorDocumentQueryBasis,
    pub entity: SceneEntityId,
    pub component: ComponentTypeId,
    pub property: ComponentPropertyPath,
}

impl EditorPropertyTarget {
    pub(crate) fn admitted_metadata_bytes(
        &self,
        maximum: usize,
    ) -> Result<usize, EditorDocumentQueryRejection> {
        let ids = self
            .entity
            .as_str()
            .len()
            .saturating_add(self.component.as_str().len());
        let bytes = maximum
            .checked_sub(ids)
            .and_then(|remaining| property_metadata_bytes_up_to(&self.property, remaining))
            .ok_or(EditorDocumentQueryRejection::MetadataBudgetExceeded)?;
        if self.property.steps.len() > ComponentPropertyPath::MAX_STEPS {
            return Err(nara_scene::AuthoringReadError::from(
                nara_reflect::ComponentPropertyError::PathDepthLimit,
            )
            .into());
        }
        Ok(ids + bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPropertyRead {
    pub target: EditorPropertyTarget,
    pub limits: EditorPropertyReadLimits,
}

/// Explicit Null is Present; a defaulted Null is Defaulted. Missing never means an unknown field.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorPropertyValue {
    Present(ComponentValue),
    Defaulted(ComponentValue),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorPropertyObservation {
    pub target: EditorPropertyTarget,
    pub value: EditorPropertyValue,
    pub metadata_bytes: usize,
    pub value_nodes: usize,
    pub value_logical_bytes: usize,
    pub scanned_entries: usize,
    pub scanned_logical_bytes: usize,
}
