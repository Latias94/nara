//! UI-neutral observations and actions for a Session-owned finite source tool.

use std::{collections::BTreeMap, sync::Arc};

use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentSchemaOwnerSnapshot, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{LocalConstructionEntity, SceneEntityId, ScenePatchDocument};

use crate::{
    EditorActionOutcome, EditorDocumentId, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorPropertyTarget, EditorWorkspaceCommandReport, PreparedEditorDocumentConstruction,
};

macro_rules! operation_id {
    ($name:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name(Arc<()>);

        impl $name {
            /// Issues a fresh nominal identity. Only an identity retained by its Session is admitted.
            #[doc(hidden)]
            #[must_use]
            pub fn issue() -> Self {
                Self(Arc::new(()))
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                Arc::ptr_eq(&self.0, &other.0)
            }
        }
        impl Eq for $name {}
    };
}

operation_id!(EditorSourceToolAttemptId);
operation_id!(EditorSourceToolRequestId);
operation_id!(EditorSourceToolActivationId);

mod selection;
pub use selection::*;
mod artifacts;
pub use artifacts::*;
mod template_import;
pub use template_import::*;

/// Explicit source intent retained by the Host rather than returned by the child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolIntent {
    Attach { entity: SceneEntityId },
    Edit { entity: SceneEntityId },
    Spawn { name: String },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditorSourceToolError {
    #[error("the source tool launch or component declaration is invalid")]
    Definition,
    #[error("no Editor workspace is available")]
    WorkspaceUnavailable,
    #[error("the project is closing or resolving a workspace intent")]
    Closing,
    #[error("no source tool is enabled")]
    Disabled,
    #[error("source tool work is still physically pending")]
    Busy,
    #[error("the tool request or output exceeded its finite allowance")]
    Limit,
    #[error("the source tool task was not admitted")]
    TaskRejected,
    #[error("the source tool task returned with failed executor cleanup")]
    TaskCleanupFailed,
    #[error("the source tool child failed: {0}")]
    Process(&'static str),
    #[error("the source tool response does not match its finite protocol")]
    Protocol,
    #[error("the actual child declaration does not match the selected catalog")]
    Declaration,
    #[error("the document selects another component owner or declaration")]
    Schema,
    #[error("the active tool does not provide this integer property facet")]
    PropertyFacet,
    #[error("the retained tool activation is no longer active")]
    StaleActivation,
    #[error("the retained activation attempt is no longer pending")]
    StaleAttempt,
    #[error("the retained request is no longer the newest request")]
    StaleRequest,
    #[error("the retained source observation was rejected: {0}")]
    QueryBasis(EditorDocumentQueryRejection),
    #[error("the edit requires its specialized document-domain contract")]
    StructureContractMismatch { document: EditorDocumentId },
    #[error("the source owner rejected tool preparation")]
    Preparation(Box<EditorWorkspaceCommandReport>),
    #[error("the source owner rejected construction intent")]
    Draft,
    #[error("the source owner rejected tool publication")]
    Commit(Box<EditorActionOutcome>),
}

/// One finite property contribution. This describes a proposal parameter, not a source assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolIntegerProperty {
    field: ComponentFieldId,
    parameter: String,
    label: String,
    initial: i64,
}

impl EditorSourceToolIntegerProperty {
    pub fn new(
        field: ComponentFieldId,
        parameter: String,
        label: String,
        initial: i64,
    ) -> Result<Self, EditorSourceToolError> {
        if field.as_str().len() > ComponentFieldId::MAX_BYTES
            || ComponentFieldId::try_new(field.as_str()).is_err()
            || parameter.is_empty()
            || parameter.len() > 64
            || !parameter
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || b"_.-".contains(&value))
            || label.trim().is_empty()
            || label.len() > 256
            || label.chars().any(char::is_control)
        {
            return Err(EditorSourceToolError::Declaration);
        }
        Ok(Self {
            field,
            parameter,
            label,
            initial,
        })
    }

    #[must_use]
    pub fn field(&self) -> &ComponentFieldId {
        &self.field
    }
    #[must_use]
    pub fn parameter(&self) -> &str {
        &self.parameter
    }
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
    #[must_use]
    pub const fn initial(&self) -> i64 {
        self.initial
    }
}

/// Immutable metadata for one admitted executable activation. It owns no process or candidate.
#[derive(Debug, Clone)]
pub struct EditorSourceToolActive {
    id: EditorSourceToolActivationId,
    owner: ComponentSchemaOwnerSnapshot,
    component: ComponentTypeId,
    integer_property: Option<EditorSourceToolIntegerProperty>,
}

impl PartialEq for EditorSourceToolActive {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner.ptr_eq(&other.owner)
            && self.component == other.component
            && self.integer_property == other.integer_property
    }
}

impl EditorSourceToolActive {
    /// Admits metadata for a fresh Host activation; it grants no authority until Session publication.
    #[doc(hidden)]
    pub fn admit(
        id: EditorSourceToolActivationId,
        owner: ComponentSchemaOwnerSnapshot,
        component: ComponentTypeId,
        integer_property: Option<EditorSourceToolIntegerProperty>,
    ) -> Result<Self, EditorSourceToolError> {
        let schema = owner
            .schema(&component)
            .ok_or(EditorSourceToolError::Declaration)?;
        if let Some(property) = &integer_property {
            let field = schema
                .fields()
                .iter()
                .find(|field| field.id == property.field)
                .ok_or(EditorSourceToolError::Declaration)?;
            if !schema.capabilities.contains(&ComponentCapability::Edit)
                || !field.capabilities.contains(&ComponentCapability::Edit)
                || !matches!(
                    field.shape,
                    ComponentValueShape::Scalar {
                        kind: ComponentValueKind::I64,
                        ..
                    }
                )
            {
                return Err(EditorSourceToolError::Declaration);
            }
        }
        Ok(Self {
            id,
            owner,
            component,
            integer_property,
        })
    }

    #[must_use]
    pub fn id(&self) -> &EditorSourceToolActivationId {
        &self.id
    }
    #[must_use]
    pub fn owner(&self) -> &PersistentComponentSchemaOwnerId {
        self.owner.owner_id()
    }
    #[must_use]
    pub fn component(&self) -> &ComponentTypeId {
        &self.component
    }
    #[must_use]
    pub fn integer_property(&self) -> Option<&EditorSourceToolIntegerProperty> {
        self.integer_property.as_ref()
    }

    /// Compares the selected component declaration without asserting current source readiness.
    #[must_use]
    pub fn matches_schema(&self, basis: &EditorDocumentQueryBasis) -> bool {
        basis.schema.as_ref().is_some_and(|schema| {
            schema.component_schema_owner(&self.component) == Some(self.owner())
                && schema.schema(&self.component) == self.owner.schema(&self.component)
        })
    }

    /// Matches admitted metadata only. The Session still validates current source and activation.
    #[must_use]
    pub fn matches_integer_property(&self, target: &EditorPropertyTarget) -> bool {
        let Some(property) = &self.integer_property else {
            return false;
        };
        target.component == self.component
            && target.property.root == property.field
            && target.property.steps.is_empty()
            && self.matches_schema(&target.basis)
    }
}

/// Bounded review observation; the actual prepared candidate remains in the Session.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorSourceToolReviewSummary {
    pub request: EditorSourceToolRequestId,
    pub basis: EditorDocumentQueryBasis,
    pub intent: EditorSourceToolIntent,
    pub title: Arc<str>,
    pub operations: usize,
    pub created_entity: Option<SceneEntityId>,
}

/// Terminal observations and physical work are independent, with no duplicate enabled flag.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorSourceToolView {
    pub active: Option<EditorSourceToolActive>,
    pub probing: bool,
    pub working: bool,
    pub retiring: bool,
    /// Includes deferred selection work and cleanup retained by superseded file Hosts.
    pub physical_work_pending: bool,
    /// Sticky Session-local failure, independent of pending work and never a clean-return proof.
    pub process_cleanup_failed: bool,
    pub attempt: Option<EditorSourceToolAttemptId>,
    pub request: Option<EditorSourceToolRequestId>,
    pub review: Option<EditorSourceToolReviewSummary>,
    pub attempt_error: Option<EditorSourceToolError>,
    pub request_error: Option<EditorSourceToolError>,
    pub selection: Option<EditorSourceToolSelectionView>,
    pub artifacts: Option<EditorSourceToolArtifactView>,
    pub template_import: Option<EditorSourceToolTemplateImportView>,
}

impl EditorSourceToolView {
    #[must_use]
    pub fn has_pending_work(&self) -> bool {
        self.physical_work_pending
            || self.probing
            || self.working
            || self.retiring
            || self.selection.as_ref().is_some_and(|selection| {
                matches!(
                    selection.phase,
                    EditorSourceToolSelectionPhase::Loading
                        | EditorSourceToolSelectionPhase::Probing
                        | EditorSourceToolSelectionPhase::Publishing
                        | EditorSourceToolSelectionPhase::Reconciling
                )
            })
            || self
                .artifacts
                .as_ref()
                .is_some_and(|artifacts| artifacts.physical_work_pending)
            || self
                .template_import
                .as_ref()
                .is_some_and(|import| import.physical_work_pending)
    }
}

/// Borrowed access to the Session's exact candidate; no frontend owns a second inverse executor.
pub struct EditorSourceToolReview<'a> {
    request: &'a EditorSourceToolRequestId,
    title: &'a str,
    prepared: &'a PreparedEditorDocumentConstruction,
    created: Option<&'a LocalConstructionEntity>,
}

impl<'a> EditorSourceToolReview<'a> {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        request: &'a EditorSourceToolRequestId,
        title: &'a str,
        prepared: &'a PreparedEditorDocumentConstruction,
        created: Option<&'a LocalConstructionEntity>,
    ) -> Self {
        Self {
            request,
            title,
            prepared,
            created,
        }
    }
    #[must_use]
    pub fn request(&self) -> &EditorSourceToolRequestId {
        self.request
    }
    #[must_use]
    pub fn title(&self) -> &str {
        self.title
    }
    #[must_use]
    pub fn patch(&self) -> &ScenePatchDocument {
        self.prepared.patch()
    }
    #[must_use]
    pub fn inverse(&self) -> &ScenePatchDocument {
        self.prepared.inverse()
    }
    #[must_use]
    pub fn created_entity(&self) -> Option<&SceneEntityId> {
        self.created.and_then(|local| self.prepared.entity(local))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSourceToolCommit {
    pub report: EditorWorkspaceCommandReport,
    pub created_entity: Option<SceneEntityId>,
}

/// All deferred operations retain their original execution/source observations.
#[derive(Debug, Clone)]
pub enum EditorSourceToolAction {
    Request {
        activation: EditorSourceToolActivationId,
        basis: EditorDocumentQueryBasis,
        intent: EditorSourceToolIntent,
        parameters: BTreeMap<String, ComponentValue>,
    },
    IntegerProperty {
        activation: EditorSourceToolActivationId,
        target: EditorPropertyTarget,
        value: i64,
    },
    Accept {
        request: EditorSourceToolRequestId,
    },
    Disable {
        activation: EditorSourceToolActivationId,
    },
    CancelAttempt {
        attempt: EditorSourceToolAttemptId,
    },
    DisableInstalled {
        basis: EditorSourceToolSelectionBasis,
    },
    ReconcileSelection {
        basis: EditorSourceToolSelectionBasis,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorSourceToolActionOutcome {
    Requested(Result<EditorSourceToolRequestId, EditorSourceToolError>),
    Accepted(Result<EditorSourceToolCommit, EditorSourceToolError>),
    Disabled(Result<(), EditorSourceToolError>),
    Cancelled(Result<(), EditorSourceToolError>),
    Selection(Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError>),
}

#[cfg(test)]
mod tests;
