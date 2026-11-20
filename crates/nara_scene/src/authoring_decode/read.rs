use std::{collections::BTreeMap, error::Error, fmt, ops::Bound};

use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldSchema, ComponentMemberPath, ComponentMemberPathSegment, ComponentPropertyError,
    ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertySchemaClaim, ComponentPropertyStep, ComponentSchema, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    resolve_component_property_in_root, resolve_component_property_read_path_in_root,
    resolve_component_property_schema_in_root,
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

/// Borrowed direct Record-member declarations and their root-relative tombstones.
///
/// The view keeps the source Schema's declaration order. Tooling applies its own scan and row
/// limits while advancing through the two borrowed slices; no complete member inventory is
/// allocated merely to discover one page.
#[derive(Debug, Clone, Copy)]
pub struct AuthoringDirectMembers<'a> {
    members: &'a [ComponentShapeMember],
    tombstones: &'a [ComponentMemberPath],
    edit_capability: bool,
}

/// Position for the source-owned direct-child property stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringPropertyChildrenPosition {
    Initial,
    Empty,
    Record {
        next_member: usize,
        next_tombstone: usize,
    },
    Map {
        after: Option<String>,
    },
    KeyedList {
        next_item: usize,
    },
}

/// One source-owned durable child selector. The parent path is supplied by the caller and the
/// child step is admitted by the same Schema/value owner that serves property reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuthoringPropertyChild<'a> {
    step: PropertyChildStep<'a>,
    pub kind: Option<ComponentValueKind>,
    pub edit_capability: bool,
    pub retired: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PropertyChildStep<'a> {
    Member(&'a ComponentFieldId),
    MapEntry(&'a str),
    ListItem {
        by: &'a ComponentFieldId,
        key: &'a ComponentValue,
    },
}

impl AuthoringPropertyChild<'_> {
    /// Logical identity bytes copied by `to_step`, excluding the caller's parent selector.
    #[must_use]
    pub fn metadata_bytes(&self) -> usize {
        match self.step {
            PropertyChildStep::Member(id) => id.as_str().len(),
            PropertyChildStep::MapEntry(key) => key.len(),
            PropertyChildStep::ListItem { by, key } => {
                by.as_str().len().saturating_add(match key {
                    ComponentValue::String(value) => value.len(),
                    _ => 8,
                })
            }
        }
    }

    /// Copies this admitted child identity after the caller admits its complete owned page.
    #[must_use]
    pub fn to_step(&self) -> ComponentPropertyStep {
        match self.step {
            PropertyChildStep::Member(id) => ComponentPropertyStep::Member(id.clone()),
            PropertyChildStep::MapEntry(key) => ComponentPropertyStep::MapEntry(key.to_owned()),
            PropertyChildStep::ListItem { by, key } => ComponentPropertyStep::ListItem {
                by: by.clone(),
                key: ComponentPropertyKey::from_value(key)
                    .expect("borrowed key was admitted by the source owner"),
            },
        }
    }
}

/// Borrowed successor position; copying a map anchor is deferred until page admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoringPropertyChildrenContinuation<'a>(PropertyChildrenNext<'a>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropertyChildrenNext<'a> {
    Record {
        next_member: usize,
        next_tombstone: usize,
    },
    Map {
        after: &'a str,
    },
    KeyedList {
        next_item: usize,
    },
}

impl AuthoringPropertyChildrenContinuation<'_> {
    #[must_use]
    pub const fn metadata_bytes(&self) -> usize {
        match self.0 {
            PropertyChildrenNext::Map { after } => after.len(),
            _ => 0,
        }
    }

    #[must_use]
    pub fn to_position(&self) -> AuthoringPropertyChildrenPosition {
        match self.0 {
            PropertyChildrenNext::Record {
                next_member,
                next_tombstone,
            } => AuthoringPropertyChildrenPosition::Record {
                next_member,
                next_tombstone,
            },
            PropertyChildrenNext::Map { after } => AuthoringPropertyChildrenPosition::Map {
                after: Some(after.to_owned()),
            },
            PropertyChildrenNext::KeyedList { next_item } => {
                AuthoringPropertyChildrenPosition::KeyedList { next_item }
            }
        }
    }
}

/// Bounded borrowed direct-child page. It copies no identities, values or mutation authority.
#[derive(Debug, PartialEq)]
pub struct AuthoringPropertyChildrenPage<'a> {
    pub children: Vec<AuthoringPropertyChild<'a>>,
    pub next: Option<AuthoringPropertyChildrenContinuation<'a>>,
    pub scanned_entries: usize,
    pub scanned_logical_bytes: usize,
}

fn unwrap_authoring_shape<'a>(
    mut shape: &'a ComponentValueShape,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<&'a ComponentValueShape, AuthoringReadError> {
    while let ComponentValueShape::Optional { value } = shape {
        budget.observe_entry(0)?;
        shape = value;
    }
    Ok(shape)
}

fn admit_child_scan(
    budget: &mut ComponentPropertyReadBudget,
    initial_entries: usize,
    logical_bytes: usize,
) -> Result<bool, AuthoringReadError> {
    match budget.observe_entry(logical_bytes) {
        Ok(()) => Ok(true),
        Err(ComponentPropertyError::ReadBudgetExceeded)
            if budget.scanned_entries() > initial_entries =>
        {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

fn authoring_property_value(value: AuthoringPropertyValue<'_>) -> Option<&ComponentValue> {
    match value {
        AuthoringPropertyValue::Present(value) | AuthoringPropertyValue::Defaulted(value)
            if !matches!(value, ComponentValue::Null) =>
        {
            Some(value)
        }
        AuthoringPropertyValue::Present(_)
        | AuthoringPropertyValue::Defaulted(_)
        | AuthoringPropertyValue::Missing => None,
    }
}

fn keyed_list_member<'a>(
    shape: &'a ComponentValueShape,
    key: &ComponentFieldId,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<Option<&'a ComponentShapeMember>, AuthoringReadError> {
    let ComponentValueShape::Record { members } = unwrap_authoring_shape(shape, budget)? else {
        return Ok(None);
    };
    for member in members {
        budget.observe_entry(
            member
                .name
                .len()
                .saturating_add(member.id.as_ref().map_or(0, |id| id.as_str().len())),
        )?;
        if member.id.as_ref() == Some(key) {
            return Ok(Some(member));
        }
    }
    Ok(None)
}

fn property_step_matches_member_path(
    step: &ComponentPropertyStep,
    segment: &ComponentMemberPathSegment,
) -> bool {
    match (step, segment) {
        (ComponentPropertyStep::Member(left), ComponentMemberPathSegment::Member(right)) => {
            left == right
        }
        (ComponentPropertyStep::ListItem { .. }, ComponentMemberPathSegment::ListItem)
        | (ComponentPropertyStep::MapEntry(_), ComponentMemberPathSegment::MapEntry) => true,
        _ => false,
    }
}

fn tombstone_direct_child<'a>(
    tombstone: &'a ComponentMemberPath,
    property: &ComponentPropertyPath,
) -> Option<&'a ComponentFieldId> {
    let segments = tombstone.segments();
    if segments.len() != property.steps.len() + 1
        || !property
            .steps
            .iter()
            .zip(segments.iter())
            .all(|(step, segment)| property_step_matches_member_path(step, segment))
    {
        return None;
    }
    match segments.last() {
        Some(ComponentMemberPathSegment::Member(id)) => Some(id),
        _ => None,
    }
}

fn member_path_logical_bytes(path: &ComponentMemberPath) -> usize {
    path.segments()
        .iter()
        .map(|segment| match segment {
            ComponentMemberPathSegment::Member(id) => id.as_str().len(),
            ComponentMemberPathSegment::ListItem | ComponentMemberPathSegment::MapEntry => 1,
        })
        .fold(0usize, usize::saturating_add)
}

impl<'a> AuthoringDirectMembers<'a> {
    #[must_use]
    pub const fn members(&self) -> &'a [ComponentShapeMember] {
        self.members
    }

    #[must_use]
    pub const fn tombstones(&self) -> &'a [ComponentMemberPath] {
        self.tombstones
    }

    #[must_use]
    pub const fn edit_capability(&self) -> bool {
        self.edit_capability
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

    /// Borrows one current Record root's directly addressable members and direct tombstones.
    ///
    /// Members without a durable Schema ID are retained in the borrowed declaration slice so a
    /// caller can charge inspecting them, but must not fall back to their wire names. No value
    /// tree or recursive member shape is interpreted or copied.
    pub fn direct_members(
        &self,
        root: &ComponentFieldId,
    ) -> Result<AuthoringDirectMembers<'a>, AuthoringReadError> {
        let entry = self.schema()?;
        if entry.schema.field_tombstones().binary_search(root).is_ok() {
            return Err(ComponentPropertyError::RetiredField(root.clone()).into());
        }
        let fields = entry.schema.fields();
        let field = fields
            .binary_search_by(|candidate| candidate.id().cmp(root))
            .ok()
            .and_then(|index| fields.get(index))
            .ok_or(AuthoringReadError::FieldNotFound)?;

        let mut shape = field.shape();
        while let ComponentValueShape::Optional { value } = shape {
            shape = value;
        }
        let ComponentValueShape::Record { members } = shape else {
            return Err(ComponentPropertyError::ExpectedRecord { step: 0 }.into());
        };

        Ok(AuthoringDirectMembers {
            members,
            tombstones: &field.member_tombstones,
            edit_capability: field.has_capability(ComponentCapability::Edit),
        })
    }

    /// Borrows one admitted root or descendant, preserving explicit/default origin.
    /// Scan limits apply to selector traversal, not root/storage index lookups, allocation or time.
    /// Complete shape/native validation belongs to admission; no effective component is copied.
    pub fn property_value(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<AuthoringPropertyValue<'a>, AuthoringReadError> {
        self.resolve_property_value(property, budget, false)
            .map(|(value, _)| value)
    }

    /// Captures the observed wire position relative to the admitted root. Resuming a read
    /// through this position requires the caller to fence the exact source and Schema basis.
    #[doc(hidden)]
    pub fn property_value_with_read_path(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<(AuthoringPropertyValue<'a>, ComponentFieldPath), AuthoringReadError> {
        self.resolve_property_value(property, budget, true)
    }

    /// Borrows the same explicit or defaulted root for a read fenced to the original basis.
    #[doc(hidden)]
    pub fn property_root_value(
        &self,
        root: &ComponentFieldId,
    ) -> Result<AuthoringPropertyValue<'a>, AuthoringReadError> {
        let field = property_root_field(&self.schema()?.schema, root)?;
        self.root_value(field, true)
    }

    fn resolve_property_value(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
        capture_read_path: bool,
    ) -> Result<(AuthoringPropertyValue<'a>, ComponentFieldPath), AuthoringReadError> {
        let field = property_root(&self.schema()?.schema, property)?;
        let root = self.root_value(field, property.steps.is_empty())?;
        let (value, defaulted) = match root {
            AuthoringPropertyValue::Present(value) => (value, false),
            AuthoringPropertyValue::Defaulted(value) => (value, true),
            AuthoringPropertyValue::Missing => return Ok((root, ComponentFieldPath::empty())),
        };
        let (selected, read_path) = if capture_read_path {
            resolve_component_property_read_path_in_root(field, value.into(), property, budget)?
        } else {
            (
                resolve_component_property_in_root(field, value, property, budget)?.value,
                ComponentFieldPath::empty(),
            )
        };
        let selected = selected
            .as_value()
            .ok_or(AuthoringReadError::InvalidFieldPath)?;
        let value = if defaulted {
            AuthoringPropertyValue::Defaulted(selected)
        } else {
            AuthoringPropertyValue::Present(selected)
        };
        Ok((value, read_path))
    }

    fn root_value(
        &self,
        field: &'a ComponentFieldSchema,
        allow_missing: bool,
    ) -> Result<AuthoringPropertyValue<'a>, AuthoringReadError> {
        let record = self
            .entity
            .components
            .get(self.fact.component())
            .expect("indexed component belongs to its source record");
        match record.value().get_path(field.path()) {
            Ok(value) => Ok(AuthoringPropertyValue::Present(value)),
            Err(
                error @ (ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. }),
            ) => match record
                .contextual_defaults
                .get(field.id())
                .or(field.default_value())
            {
                Some(value) => Ok(AuthoringPropertyValue::Defaulted(value)),
                None if allow_missing => Ok(AuthoringPropertyValue::Missing),
                None => Err(ComponentPropertyError::ValueUnavailable(error).into()),
            },
            Err(_) => Err(AuthoringReadError::InvalidFieldPath),
        }
    }

    /// Borrows the current Schema claim for a root or descendant selector.
    ///
    /// This observes declarations through the same owner and lookup budget as a property read;
    /// it does not inspect a target value, choose a default, or authorize a write.
    pub fn property_schema_claim(
        &self,
        property: &ComponentPropertyPath,
        budget: &mut ComponentPropertyReadBudget,
    ) -> Result<ComponentPropertySchemaClaim<'a>, AuthoringReadError> {
        let entry = self.schema()?;
        Ok(resolve_component_property_schema_in_root(
            property_root(&entry.schema, property)?,
            property,
            budget,
        )?)
    }

    /// Enumerates directly addressable children of a root or descendant through the source owner.
    ///
    /// Record declarations and retained tombstones are schema-owned. Map entries and keyed-list
    /// items are value-owned; ordinary list positions refuse because they have no durable
    /// selectors. Record declarations can be observed below an absent optional parent; their
    /// availability does not assert value presence. Collection identities require the actual
    /// selected explicit/default value. Lookup and child scans share the supplied allowance.
    /// An exhausted scan returns a continuation only after actual child-stream progress.
    pub fn property_children(
        &self,
        property: &ComponentPropertyPath,
        position: AuthoringPropertyChildrenPosition,
        max_scan_entries: usize,
        max_rows: usize,
        max_logical_bytes: usize,
    ) -> Result<AuthoringPropertyChildrenPage<'a>, AuthoringReadError> {
        property_children_in_schema(
            || self.schema().map(|entry| &entry.schema),
            property,
            position,
            max_scan_entries,
            max_rows,
            max_logical_bytes,
            |_, budget| {
                self.property_value(property, budget)
                    .map(authoring_property_value)
            },
        )
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

pub(crate) fn property_root<'a>(
    schema: &'a ComponentSchema,
    property: &ComponentPropertyPath,
) -> Result<&'a ComponentFieldSchema, AuthoringReadError> {
    property.validate()?;
    property_root_field(schema, &property.root)
}

fn property_root_field<'a>(
    schema: &'a ComponentSchema,
    root: &ComponentFieldId,
) -> Result<&'a ComponentFieldSchema, AuthoringReadError> {
    if schema.field_tombstones().binary_search(root).is_ok() {
        return Err(ComponentPropertyError::RetiredField(root.clone()).into());
    }
    let fields = schema.fields();
    let index = fields
        .binary_search_by(|candidate| candidate.id().cmp(root))
        .map_err(|_| AuthoringReadError::FieldNotFound)?;
    Ok(&fields[index])
}

pub(crate) fn property_children_in_schema<'a>(
    schema: impl FnOnce() -> Result<&'a ComponentSchema, AuthoringReadError>,
    property: &ComponentPropertyPath,
    position: AuthoringPropertyChildrenPosition,
    max_scan_entries: usize,
    max_rows: usize,
    max_logical_bytes: usize,
    value: impl FnOnce(
        &'a ComponentFieldSchema,
        &mut ComponentPropertyReadBudget,
    ) -> Result<Option<&'a ComponentValue>, AuthoringReadError>,
) -> Result<AuthoringPropertyChildrenPage<'a>, AuthoringReadError> {
    if max_scan_entries == 0 || max_rows == 0 || max_logical_bytes == 0 {
        return Err(ComponentPropertyError::ReadBudgetExceeded.into());
    }
    let mut budget = ComponentPropertyReadBudget::new(
        nara_core::ItemLimit::new(max_scan_entries)
            .expect("property child scan entries are positive"),
        nara_core::ByteLimit::new(max_logical_bytes)
            .expect("property child logical bytes are positive"),
    );
    let field = property_root(schema()?, property)?;
    let claim = resolve_component_property_schema_in_root(field, property, &mut budget)?;
    let shape = unwrap_authoring_shape(claim.shape, &mut budget)?;
    if matches!(shape, ComponentValueShape::List { .. }) {
        return Err(ComponentPropertyError::UnkeyedList {
            step: property.steps.len(),
        }
        .into());
    }
    if matches!(
        shape,
        ComponentValueShape::Record { .. }
            | ComponentValueShape::Map { .. }
            | ComponentValueShape::KeyedList { .. }
    ) && property.steps.len() == ComponentPropertyPath::MAX_STEPS
    {
        return Err(ComponentPropertyError::PathDepthLimit.into());
    }
    let value = if matches!(
        shape,
        ComponentValueShape::Map { .. } | ComponentValueShape::KeyedList { .. }
    ) || property.steps.iter().any(|step| {
        matches!(
            step,
            ComponentPropertyStep::MapEntry(_) | ComponentPropertyStep::ListItem { .. }
        )
    }) {
        value(field, &mut budget)?
    } else {
        None
    };
    let position = match (position, shape) {
        (AuthoringPropertyChildrenPosition::Initial, ComponentValueShape::Record { .. }) => {
            AuthoringPropertyChildrenPosition::Record {
                next_member: 0,
                next_tombstone: 0,
            }
        }
        (AuthoringPropertyChildrenPosition::Initial, ComponentValueShape::Map { .. }) => {
            AuthoringPropertyChildrenPosition::Map { after: None }
        }
        (AuthoringPropertyChildrenPosition::Initial, ComponentValueShape::KeyedList { .. }) => {
            AuthoringPropertyChildrenPosition::KeyedList { next_item: 0 }
        }
        (AuthoringPropertyChildrenPosition::Initial, _) => AuthoringPropertyChildrenPosition::Empty,
        (position, _) => position,
    };
    let initial_entries = budget.scanned_entries();
    let mut children = Vec::new();
    let next = match (position, shape) {
        (
            AuthoringPropertyChildrenPosition::Record {
                mut next_member,
                mut next_tombstone,
            },
            ComponentValueShape::Record { members },
        ) => {
            if next_member > members.len() || next_tombstone > claim.field.member_tombstones.len() {
                return Err(AuthoringReadError::InvalidFieldPath);
            }
            while children.len() < max_rows {
                if let Some(member) = members.get(next_member) {
                    if !admit_child_scan(
                        &mut budget,
                        initial_entries,
                        member
                            .id
                            .as_ref()
                            .map_or(member.name.len(), |id| id.as_str().len()),
                    )? {
                        break;
                    }
                    next_member += 1;
                    let Some(id) = member.id.as_ref() else {
                        continue;
                    };
                    children.push(AuthoringPropertyChild {
                        step: PropertyChildStep::Member(id),
                        kind: Some(member.shape.value_kind()),
                        edit_capability: claim.field.has_capability(ComponentCapability::Edit),
                        retired: false,
                    });
                } else if let Some(tombstone) = claim.field.member_tombstones.get(next_tombstone) {
                    if !admit_child_scan(
                        &mut budget,
                        initial_entries,
                        member_path_logical_bytes(tombstone),
                    )? {
                        break;
                    }
                    next_tombstone += 1;
                    let direct = tombstone_direct_child(tombstone, property);
                    if let Some(id) = direct {
                        children.push(AuthoringPropertyChild {
                            step: PropertyChildStep::Member(id),
                            kind: None,
                            edit_capability: false,
                            retired: true,
                        });
                    }
                } else {
                    break;
                }
            }
            let has_more =
                next_member < members.len() || next_tombstone < claim.field.member_tombstones.len();
            has_more.then_some(AuthoringPropertyChildrenContinuation(
                PropertyChildrenNext::Record {
                    next_member,
                    next_tombstone,
                },
            ))
        }
        (
            AuthoringPropertyChildrenPosition::Map { after },
            ComponentValueShape::Map { value: value_shape },
        ) => {
            let Some(ComponentValue::Map(values)) = value else {
                return Ok(AuthoringPropertyChildrenPage {
                    children,
                    next: None,
                    scanned_entries: budget.scanned_entries(),
                    scanned_logical_bytes: budget.scanned_logical_bytes(),
                });
            };
            let mut last = match after.as_deref() {
                Some(after) => Some(
                    values
                        .get_key_value(after)
                        .ok_or(AuthoringReadError::InvalidFieldPath)?
                        .0
                        .as_str(),
                ),
                None => None,
            };
            let mut entries = match last {
                Some(after) => values.range::<str, _>((
                    std::ops::Bound::Excluded(after),
                    std::ops::Bound::Unbounded,
                )),
                None => {
                    values.range::<str, _>((std::ops::Bound::Unbounded, std::ops::Bound::Unbounded))
                }
            };
            let mut pending = false;
            while children.len() < max_rows {
                let Some((key, _)) = entries.next() else {
                    break;
                };
                if !admit_child_scan(&mut budget, initial_entries, key.len())? {
                    pending = true;
                    break;
                }
                last = Some(key.as_str());
                children.push(AuthoringPropertyChild {
                    step: PropertyChildStep::MapEntry(key),
                    kind: Some(value_shape.value_kind()),
                    edit_capability: claim.field.has_capability(ComponentCapability::Edit),
                    retired: false,
                });
            }
            let has_more = pending || entries.next().is_some();
            has_more.then(|| {
                AuthoringPropertyChildrenContinuation(PropertyChildrenNext::Map {
                    after: last.expect("a continuation advances"),
                })
            })
        }
        (
            AuthoringPropertyChildrenPosition::KeyedList { mut next_item },
            ComponentValueShape::KeyedList { item, key },
        ) => {
            let Some(ComponentValue::List(values)) = value else {
                return Ok(AuthoringPropertyChildrenPage {
                    children,
                    next: None,
                    scanned_entries: budget.scanned_entries(),
                    scanned_logical_bytes: budget.scanned_logical_bytes(),
                });
            };
            let Some(key_member) = keyed_list_member(item, key, &mut budget)? else {
                return Err(AuthoringReadError::InvalidFieldPath);
            };
            if next_item > values.len() {
                return Err(AuthoringReadError::InvalidFieldPath);
            }
            let initial_entries = budget.scanned_entries();
            while next_item < values.len() && children.len() < max_rows {
                let index = next_item;
                let item_value = &values[index];
                let key_source = item_value.get(key_member.name.as_str()).ok_or(
                    AuthoringReadError::Property(ComponentPropertyError::InvalidListItemKey {
                        step: 0,
                        index,
                    }),
                )?;
                let key_logical_bytes = match key_source {
                    ComponentValue::String(value) => value.len(),
                    ComponentValue::I64(_) | ComponentValue::U64(_) => 8,
                    _ => {
                        return Err(AuthoringReadError::Property(
                            ComponentPropertyError::InvalidListItemKey { step: 0, index },
                        ));
                    }
                };
                if !admit_child_scan(
                    &mut budget,
                    initial_entries,
                    key.as_str().len().saturating_add(key_logical_bytes),
                )? {
                    break;
                }
                if key_source.kind() != key_member.shape.value_kind() {
                    return Err(AuthoringReadError::Property(
                        ComponentPropertyError::KeyTypeMismatch {
                            step: 0,
                            expected: key_member.shape.value_kind(),
                            actual: key_source.kind(),
                        },
                    ));
                }
                next_item += 1;
                children.push(AuthoringPropertyChild {
                    step: PropertyChildStep::ListItem {
                        by: key,
                        key: key_source,
                    },
                    kind: Some(item.value_kind()),
                    edit_capability: claim.field.has_capability(ComponentCapability::Edit),
                    retired: false,
                });
            }
            (next_item < values.len()).then_some(AuthoringPropertyChildrenContinuation(
                PropertyChildrenNext::KeyedList { next_item },
            ))
        }
        (AuthoringPropertyChildrenPosition::Empty, shape)
            if !matches!(
                shape,
                ComponentValueShape::Record { .. }
                    | ComponentValueShape::Map { .. }
                    | ComponentValueShape::KeyedList { .. }
            ) =>
        {
            None
        }
        _ => return Err(AuthoringReadError::InvalidFieldPath),
    };
    Ok(AuthoringPropertyChildrenPage {
        children,
        next,
        scanned_entries: budget.scanned_entries(),
        scanned_logical_bytes: budget.scanned_logical_bytes(),
    })
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
