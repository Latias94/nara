use std::{collections::BTreeMap, error::Error, fmt, ops::Bound};

use nara_reflect::{
    ComponentFieldPathError, ComponentFieldSchema, ComponentPropertyError, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentTypeId, ComponentValue,
    resolve_component_property_in_root,
};

use super::{
    AuthoringDocumentState, AuthoringRecordFact, AuthoringRecordReadiness, AuthoringSchemaEntry,
};
use crate::{SceneEntityId, SceneEntityRecord};

/// Refusal of a borrowed authoring projection, distinct from a missing optional value.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Exact selector/lookup-work refusal from the shared Schema owner.
    Property(ComponentPropertyError),
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
            Self::Property(error) => return error.fmt(formatter),
        })
    }
}

impl Error for AuthoringReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Property(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ComponentPropertyError> for AuthoringReadError {
    fn from(error: ComponentPropertyError) -> Self {
        Self::Property(error)
    }
}

/// A borrowed selected property. Explicit Null is Present, not Missing or Defaulted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AuthoringPropertyValue<'a> {
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

    /// Borrows one admitted root or descendant, preserving explicit/default origin.
    /// Scan limits apply to selector traversal, not root/storage index lookups, allocation or time.
    /// Complete shape/native validation belongs to admission; no effective component is copied.
    pub fn property_value(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<AuthoringPropertyValue<'a>, AuthoringReadError> {
        let entry = self.schema()?;
        property.validate()?;
        if entry
            .schema
            .field_tombstones()
            .binary_search(&property.root)
            .is_ok()
        {
            return Err(ComponentPropertyError::RetiredField(property.root.clone()).into());
        }
        let fields = entry.schema.fields();
        let index = fields
            .binary_search_by(|candidate| candidate.id().cmp(&property.root))
            .map_err(|_| AuthoringReadError::FieldNotFound)?;
        let field = &fields[index];
        let record = self
            .entity
            .components
            .get(self.fact.component())
            .expect("indexed component belongs to its source record");
        let (value, defaulted) = match record.value.get_path(field.path()) {
            Ok(value) => (value, false),
            Err(
                error @ (ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. }),
            ) => match field.default_value() {
                Some(value) => (value, true),
                None if property.steps.is_empty() => return Ok(AuthoringPropertyValue::Missing),
                None => return Err(ComponentPropertyError::ValueUnavailable(error).into()),
            },
            Err(_) => return Err(AuthoringReadError::InvalidFieldPath),
        };
        let selected = resolve_component_property_in_root(field, value, property, budget)?.value;
        Ok(if defaulted {
            AuthoringPropertyValue::Defaulted(selected)
        } else {
            AuthoringPropertyValue::Present(selected)
        })
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
