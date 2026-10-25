use std::{collections::BTreeMap, error::Error, fmt, ops::Bound};

use nara_reflect::{
    ComponentFieldId, ComponentFieldPathError, ComponentFieldSchema, ComponentTypeId,
    ComponentValue,
};

use super::{
    AuthoringDocumentState, AuthoringRecordFact, AuthoringRecordReadiness, AuthoringSchemaEntry,
};
use crate::{SceneEntityId, SceneEntityRecord};

/// Refusal of a borrowed authoring projection, distinct from a missing optional value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringReadError {
    /// The session has no registry-free authoring admission.
    NotAdmitted,
    /// No direct entity has the requested source identity.
    EntityNotFound,
    /// The actual entity lacks this component, even if its Schema is in the creation catalog.
    ComponentNotFound,
    /// Presence does not authorize interpretation of an unavailable record.
    UnavailableComponent { readiness: AuthoringRecordReadiness },
    /// The current admitted component Schema has no such root ID.
    FieldNotFound,
    /// A non-missing source path is incompatible with the admitted Schema.
    InvalidFieldPath,
}

impl fmt::Display for AuthoringReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotAdmitted => "source has no authoring admission",
            Self::EntityNotFound => "direct source entity was not found",
            Self::ComponentNotFound => "actual source component was not found",
            Self::UnavailableComponent { .. } => {
                "source component is not available for semantic reading"
            }
            Self::FieldNotFound => "declared root field was not found",
            Self::InvalidFieldPath => "source value is incompatible with its admitted field path",
        })
    }
}

impl Error for AuthoringReadError {}

/// A borrowed root observation. Explicit Null is Present, not Missing or Defaulted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AuthoringRootValue<'a> {
    Present(&'a ComponentValue),
    Defaulted(&'a ComponentValue),
    Missing,
}

/// Direct actual components of one admitted entity, including unavailable records.
///
/// This borrow owns no document or history. It cannot survive mutation of its source session.
#[derive(Clone, Copy)]
pub struct AuthoringEntityView<'a> {
    state: &'a AuthoringDocumentState,
    entity: &'a SceneEntityRecord,
    index: Option<&'a BTreeMap<ComponentTypeId, usize>>,
}

impl fmt::Debug for AuthoringEntityView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringEntityView")
            .field("entity", &self.entity.id)
            .finish_non_exhaustive()
    }
}

impl<'a> AuthoringEntityView<'a> {
    #[must_use]
    pub fn component_count(&self) -> usize {
        self.entity.components.len()
    }

    /// Streams actual component headers in ID order, exclusively after an optional ID.
    /// No catalog creation entries, Schema copies, values or expanded Prefab records are visited.
    pub fn components_after(
        &self,
        after: Option<&ComponentTypeId>,
    ) -> impl Iterator<Item = AuthoringComponentView<'a>> + '_ {
        let lower = after.map_or(Bound::Unbounded, Bound::Excluded);
        self.index
            .map(|index| index.range((lower, Bound::Unbounded)))
            .into_iter()
            .flatten()
            .map(|(_, fact)| AuthoringComponentView {
                state: self.state,
                entity: self.entity,
                fact: &self.state.record_facts[*fact],
            })
    }

    pub fn component(
        &self,
        component: &ComponentTypeId,
    ) -> Result<AuthoringComponentView<'a>, AuthoringReadError> {
        let fact = self
            .index
            .and_then(|index| index.get(component))
            .ok_or(AuthoringReadError::ComponentNotFound)?;
        Ok(AuthoringComponentView {
            state: self.state,
            entity: self.entity,
            fact: &self.state.record_facts[*fact],
        })
    }
}

/// An actual source record's readiness and, only when Available, its declared semantic roots.
#[derive(Clone, Copy)]
pub struct AuthoringComponentView<'a> {
    state: &'a AuthoringDocumentState,
    entity: &'a SceneEntityRecord,
    fact: &'a AuthoringRecordFact,
}

impl fmt::Debug for AuthoringComponentView<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringComponentView")
            .field("component", self.fact.component())
            .field("readiness", &self.fact.readiness())
            .finish()
    }
}

impl<'a> AuthoringComponentView<'a> {
    #[must_use]
    pub const fn fact(&self) -> &'a AuthoringRecordFact {
        self.fact
    }

    /// Borrows the admitted root declaration sequence without materializing an effective value.
    pub fn root_fields(&self) -> Result<&'a [ComponentFieldSchema], AuthoringReadError> {
        Ok(self.schema()?.schema.fields())
    }

    /// Looks up one root by admitted ID and borrows only its source subtree or declared default.
    /// No sibling value is copied or traversed; shape/native validation already belongs to admission.
    pub fn root_value(
        &self,
        field: &ComponentFieldId,
    ) -> Result<AuthoringRootValue<'a>, AuthoringReadError> {
        let entry = self.schema()?;
        let fields = entry.schema.fields();
        let index = fields
            .binary_search_by(|candidate| candidate.id().cmp(field))
            .map_err(|_| AuthoringReadError::FieldNotFound)?;
        let field = &fields[index];
        let record = self
            .entity
            .components
            .get(self.fact.component())
            .expect("indexed component belongs to its source record");
        match record.value.get_path(field.path()) {
            Ok(value) => Ok(AuthoringRootValue::Present(value)),
            Err(
                ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. },
            ) => Ok(field
                .default_value()
                .map_or(AuthoringRootValue::Missing, AuthoringRootValue::Defaulted)),
            Err(_) => Err(AuthoringReadError::InvalidFieldPath),
        }
    }

    fn schema(&self) -> Result<&'a AuthoringSchemaEntry, AuthoringReadError> {
        if !self.fact.readiness().is_available() {
            return Err(AuthoringReadError::UnavailableComponent {
                readiness: self.fact.readiness(),
            });
        }
        Ok(self
            .state
            .basis
            .schemas
            .get(self.fact.component())
            .expect("Available record retains its admitted Schema"))
    }
}

impl AuthoringDocumentState {
    pub(crate) fn entity_view<'a>(
        &'a self,
        entities: &'a [SceneEntityRecord],
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'a>, AuthoringReadError> {
        let record = entities
            .binary_search_by(|record| record.id.cmp(entity))
            .ok()
            .and_then(|index| entities.get(index))
            .ok_or(AuthoringReadError::EntityNotFound)?;
        Ok(AuthoringEntityView {
            state: self,
            entity: record,
            index: self.direct_record_facts.get(entity),
        })
    }
}
