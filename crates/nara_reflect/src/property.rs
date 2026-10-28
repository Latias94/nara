//! Schema-relative property identity, independent of documents and runtime incarnations.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    mem::size_of,
};

use nara_core::{ByteLimit, ItemLimit};

use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentMemberPath,
    ComponentMemberPathSegment, ComponentSchema, ComponentSchemaVersion, ComponentShapeError,
    ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    effective::validate_value_materialization_budget,
    schema::{
        MAX_SHAPE_BYTES, MAX_SHAPE_DEPTH, keyed_list_member, validate_component_shape_budget,
        validate_component_value_budget,
    },
    validate_component_value, validate_component_value_with_limits,
};

/// Exact logical identity within a keyed list. Reusing a key asserts identity continuity;
/// Reflect does not retain deleted item history or distinguish restoration from accidental reuse.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ComponentPropertyKey {
    String(String),
    I64(i64),
    U64(u64),
}

impl ComponentPropertyKey {
    #[must_use]
    pub fn from_value(value: &ComponentValue) -> Option<Self> {
        match value {
            ComponentValue::String(value) => Some(Self::String(value.clone())),
            ComponentValue::I64(value) => Some(Self::I64(*value)),
            ComponentValue::U64(value) => Some(Self::U64(*value)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn value_kind(&self) -> ComponentValueKind {
        match self {
            Self::String(_) => ComponentValueKind::String,
            Self::I64(_) => ComponentValueKind::I64,
            Self::U64(_) => ComponentValueKind::U64,
        }
    }

    fn matches(&self, value: &ComponentValue) -> bool {
        match (self, value) {
            (Self::String(key), ComponentValue::String(value)) => key == value,
            (Self::I64(key), ComponentValue::I64(value)) => key == value,
            (Self::U64(key), ComponentValue::U64(value)) => key == value,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ComponentPropertyStep {
    Member(ComponentFieldId),
    ListItem {
        by: ComponentFieldId,
        key: ComponentPropertyKey,
    },
    MapEntry(String),
}

/// A durable selector relative to one component schema, not a document or mutation authority.
///
/// Wire member names, list positions and schema versions are deliberately not identity.
/// Deserializers at file/transport boundaries must still enforce their allocation budgets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ComponentPropertyPath {
    pub root: ComponentFieldId,
    pub steps: Vec<ComponentPropertyStep>,
}

impl ComponentPropertyPath {
    pub const MAX_STEPS: usize = MAX_SHAPE_DEPTH;

    #[must_use]
    pub fn new(root: ComponentFieldId) -> Self {
        Self {
            root,
            steps: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_steps(mut self, steps: impl IntoIterator<Item = ComponentPropertyStep>) -> Self {
        self.steps.extend(steps);
        self
    }

    pub fn validate(&self) -> Result<(), ComponentPropertyError> {
        self.logical_bytes().map(|_| ())
    }

    /// Validates the selector and returns its logical size for an owner's aggregate work budget.
    /// This counts identity strings and scalar key bytes, not serialized transport overhead.
    pub fn logical_bytes(&self) -> Result<usize, ComponentPropertyError> {
        if self.steps.len() > Self::MAX_STEPS {
            return Err(ComponentPropertyError::PathDepthLimit);
        }
        let mut bytes = self.root.as_str().len();
        self.root
            .validate()
            .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
        for step in &self.steps {
            bytes = bytes.saturating_add(match step {
                ComponentPropertyStep::Member(id) => {
                    id.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
                    id.as_str().len()
                }
                ComponentPropertyStep::ListItem { by, key } => {
                    by.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId)?;
                    by.as_str().len().saturating_add(match key {
                        ComponentPropertyKey::String(key) => key.len(),
                        _ => 8,
                    })
                }
                ComponentPropertyStep::MapEntry(key) => key.len(),
            });
            if bytes > MAX_SHAPE_BYTES {
                return Err(ComponentPropertyError::PathByteLimit);
            }
        }
        Ok(bytes)
    }

    /// Replaces a property in an isolated, unpublished candidate without cloning its siblings.
    ///
    /// This internal owner seam consumes the candidate on success or failure. It shares selector,
    /// replacement shape and value-budget validation with `replace_component_property`. Complete
    /// candidate validation is deferred to its owner, including default interpretation and keyed
    /// list uniqueness. The owner must admit allocations and validate the result before publishing.
    #[doc(hidden)]
    pub fn replace_in_owned_value(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        mut value: ComponentValue,
        replacement: ComponentValue,
    ) -> Result<ComponentValue, ComponentPropertyError> {
        let (resolved, storage) = resolve_location(schema, version, &value, self, None)?;
        validate_property_replacement(resolved.shape, self, &replacement, None)?;
        replace_prepared_property(&mut value, &storage, replacement, None)?;
        Ok(value)
    }

    /// Replaces a property in an isolated candidate under its owner's explicit value limits.
    ///
    /// Unlike the arbitrary-value entry point, this uses the supplied logical node, byte and
    /// depth limits for the original, replacement and complete resulting value. It retains the
    /// same selector, replacement shape and storage semantics. The candidate is consumed on
    /// failure; complete candidate semantics, defaults and keyed-list uniqueness remain the
    /// owner's responsibility before publication. No sibling or rollback copy is allocated.
    #[doc(hidden)]
    pub fn replace_in_owned_value_with_limits(
        &self,
        schema: &ComponentSchema,
        version: ComponentSchemaVersion,
        mut value: ComponentValue,
        replacement: ComponentValue,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<ComponentValue, ComponentPropertyError> {
        let (resolved, storage) = resolve_location(schema, version, &value, self, Some(limits))?;
        validate_property_replacement(resolved.shape, self, &replacement, Some(limits))?;
        replace_prepared_property(&mut value, &storage, replacement, Some(limits))?;
        Ok(value)
    }
}

/// A borrowed observation tied to the exact supplied schema and value. No reusable storage
/// locator escapes resolution, and replacing the value requires resolving the selector again.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedComponentProperty<'schema, 'value> {
    pub shape: &'schema ComponentValueShape,
    pub value: &'value ComponentValue,
}

/// A selector's declaration in an admitted current schema, without any value observation.
///
/// The owning field retains its original identity, metadata and capabilities. The selected
/// shape describes a replacement payload; it does not prove that a target or value exists.
#[derive(Debug, Clone, Copy)]
pub struct ComponentPropertySchemaClaim<'schema> {
    pub field: &'schema ComponentFieldSchema,
    pub shape: &'schema ComponentValueShape,
}

/// Finite selector lookup work, independent of the selected value's copying budget.
/// Counts examined declarations, optional wrappers, tombstones, list keys and map lookups,
/// not allocator usage, elapsed time or internal BTreeMap comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentPropertyReadBudget {
    max_entries: usize,
    max_logical_bytes: usize,
    scanned_entries: usize,
    scanned_logical_bytes: usize,
}

impl ComponentPropertyReadBudget {
    #[must_use]
    pub const fn new(entries: ItemLimit, logical_bytes: ByteLimit) -> Self {
        Self {
            max_entries: entries.get(),
            max_logical_bytes: logical_bytes.get(),
            scanned_entries: 0,
            scanned_logical_bytes: 0,
        }
    }

    #[must_use]
    pub const fn scanned_entries(self) -> usize {
        self.scanned_entries
    }

    #[must_use]
    pub const fn scanned_logical_bytes(self) -> usize {
        self.scanned_logical_bytes
    }

    fn observe(&mut self, bytes: usize) -> Result<(), ComponentPropertyError> {
        let entries = self.scanned_entries.checked_add(1);
        let bytes = self.scanned_logical_bytes.checked_add(bytes);
        match (entries, bytes) {
            (Some(entries), Some(bytes))
                if entries <= self.max_entries && bytes <= self.max_logical_bytes =>
            {
                self.scanned_entries = entries;
                self.scanned_logical_bytes = bytes;
                Ok(())
            }
            _ => Err(ComponentPropertyError::ReadBudgetExceeded),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPropertyError {
    ReadBudgetExceeded,
    PathDepthLimit,
    PathByteLimit,
    /// Invalid caller IDs are not retained: they may be arbitrarily large unadmitted candidates.
    InvalidFieldId,
    VersionMismatch {
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    InvalidSchema(ComponentShapeError),
    ValueBudget(ComponentShapeError),
    InvalidValue(ComponentCodecError),
    UnknownRoot(ComponentFieldId),
    RetiredField(ComponentFieldId),
    RetiredMember(ComponentMemberPath),
    ValueUnavailable(ComponentFieldPathError),
    OptionalAbsent {
        step: usize,
    },
    ExpectedRecord {
        step: usize,
    },
    /// No immediate identified member matches. Anonymous ancestors cannot be skipped, and
    /// anonymous members are never inferred from a name that happens to match the requested ID.
    MemberNotFound {
        step: usize,
        id: ComponentFieldId,
    },
    ExpectedList {
        step: usize,
    },
    UnkeyedList {
        step: usize,
    },
    ListIdentityMismatch {
        step: usize,
        expected: ComponentFieldId,
        actual: ComponentFieldId,
    },
    KeyTypeMismatch {
        step: usize,
        expected: ComponentValueKind,
        actual: ComponentValueKind,
    },
    InvalidListItemKey {
        step: usize,
        index: usize,
    },
    ListItemNotFound {
        step: usize,
        key: ComponentPropertyKey,
    },
    AmbiguousListItem {
        step: usize,
        key: ComponentPropertyKey,
    },
    ExpectedMap {
        step: usize,
    },
    MapEntryNotFound {
        step: usize,
        key: String,
    },
    InvalidReplacementShape(ComponentShapeError),
    InvalidReplacement(ComponentCodecError),
    StorageIndexOverflow,
}

impl Display for ComponentPropertyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "component property resolution failed: {self:?}")
    }
}

impl Error for ComponentPropertyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSchema(error)
            | Self::ValueBudget(error)
            | Self::InvalidReplacementShape(error) => Some(error),
            Self::InvalidValue(error) | Self::InvalidReplacement(error) => Some(error),
            Self::ValueUnavailable(error) => Some(error),
            _ => None,
        }
    }
}

/// Separates selector interpretation failure from the owning caller's work allowance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPropertyAdmissionError<E> {
    Property(ComponentPropertyError),
    Admission(E),
}

type PropertyAdmission<'a> = dyn FnMut(usize, usize, usize) -> bool + 'a;

/// Resolves against the caller's exact admitted current schema and current-version value.
///
/// The schema owner supplies admission/lineage authority; this function does not select a
/// catalog, migrate data or authorize an edit. The supplied value and selector are bounded by
/// Reflect's schema/value admission limits. Missing fields/defaults are never synthesized.
pub fn resolve_component_property<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: &'value ComponentValue,
    path: &ComponentPropertyPath,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    let (resolved, _) = resolve_location(schema, version, value, path, None)?;
    validate_component_value(schema, value).map_err(ComponentPropertyError::InvalidValue)?;
    Ok(resolved)
}

/// Resolves a property using the caller's explicit logical value-admission limits.
///
/// The caller supplies an admitted current schema. This checks the whole value's node, byte
/// and depth cost using `limits`, then shares the ordinary selector and semantic validation.
/// It neither selects a Schema authority nor materializes a missing property. Arbitrary-value
/// callers that need Reflect's fixed default admission should use [`resolve_component_property`].
pub fn resolve_component_property_with_limits<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: &'value ComponentValue,
    path: &ComponentPropertyPath,
    limits: ComponentValueMaterializationLimits,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    let (resolved, _) = resolve_location(schema, version, value, path, Some(limits))?;
    validate_component_value_with_limits(schema, value, limits)
        .map_err(ComponentPropertyError::InvalidValue)?;
    Ok(resolved)
}

/// Resolves only a selector's schema claim, retaining its owning field and selected shape.
///
/// The caller supplies an admitted schema. Current version, durable IDs, tombstones and keyed
/// list identity policy are checked through the same walker as value resolution. Selector size,
/// selected declaration depth and declaration scans are bounded; the budget includes selector
/// bytes and examined root declarations/tombstones as well as nested lookup work.
///
/// No actual component, list item, map entry or optional value is required or inspected. This
/// does not validate a target, replacement value, default, binding, edit or native authority.
pub fn resolve_component_property_schema_claim<'schema>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyError> {
    resolve_component_property_schema_claim_impl(schema, version, path, budget, None)
}

/// Resolves a selector through the same walker with pre-scan and pre-growth work admission.
///
/// The caller supplies an admitted schema and retains the shared read budget across lookups.
/// Callback deltas count examined entries, logical identity bytes and modeled declaration
/// scratch, excluding shared catalog storage. Refusal returns the exact caller error before
/// the next declaration comparison or scratch allocation. Selector/version/depth guards and
/// interpretation errors remain the same as the ordinary schema-claim entry point.
pub fn resolve_component_property_schema_claim_with_admission<'schema, E>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyAdmissionError<E>> {
    let mut refusal = None;
    let result = {
        let mut callback = |nodes, bytes, allocated| match admit(nodes, bytes, allocated) {
            Ok(()) => true,
            Err(error) => {
                refusal = Some(error);
                false
            }
        };
        resolve_component_property_schema_claim_impl(
            schema,
            version,
            path,
            budget,
            Some(&mut callback),
        )
    };
    match refusal {
        Some(error) => Err(ComponentPropertyAdmissionError::Admission(error)),
        None => result.map_err(ComponentPropertyAdmissionError::Property),
    }
}

fn resolve_component_property_schema_claim_impl<'schema>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
    mut admission: Option<&mut PropertyAdmission<'_>>,
) -> Result<ComponentPropertySchemaClaim<'schema>, ComponentPropertyError> {
    if version != schema.version {
        return Err(ComponentPropertyError::VersionMismatch {
            expected: schema.version,
            actual: version,
        });
    }
    observe_read(
        &mut Some(&mut *budget),
        &mut admission,
        path.logical_bytes()?,
    )?;
    let field = resolve_root_field(schema, &path.root, Some(&mut *budget), &mut admission)?;
    let selected = resolve_root_location(field, None, path, Some(budget), None, admission)?;
    Ok(ComponentPropertySchemaClaim {
        field,
        shape: selected.shape,
    })
}

/// Borrows a descendant of an already admitted root, without validating/materializing siblings.
/// The caller owns current Schema/version admission and explicit/default source interpretation.
/// This is neither arbitrary input validation nor permission to edit the observed value.
pub fn resolve_component_property_in_root<'schema, 'value>(
    field: &'schema ComponentFieldSchema,
    value: &'value ComponentValue,
    path: &ComponentPropertyPath,
    budget: &mut ComponentPropertyReadBudget,
) -> Result<ResolvedComponentProperty<'schema, 'value>, ComponentPropertyError> {
    path.validate()?;
    if field.id != path.root {
        return Err(ComponentPropertyError::UnknownRoot(path.root.clone()));
    }
    let selected = resolve_root_location(field, Some(value), path, Some(budget), None, None)?;
    Ok(ResolvedComponentProperty {
        shape: selected.shape,
        value: selected
            .value
            .expect("value resolution retains its observation"),
    })
}

/// Replaces exactly one existing target, returning its previous value.
///
/// All resolution, shape and complete candidate validation precede publication. Failure leaves
/// the supplied value unchanged, including when a key edit would create a duplicate elsewhere.
/// Native codecs and document owners still validate business invariants, references and revision
/// authority before their own publication. Changing an item's key changes its logical identity.
pub fn replace_component_property(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &mut ComponentValue,
    path: &ComponentPropertyPath,
    replacement: ComponentValue,
) -> Result<ComponentValue, ComponentPropertyError> {
    let (resolved, storage) = resolve_location(schema, version, value, path, None)?;
    validate_component_value(schema, value).map_err(ComponentPropertyError::InvalidValue)?;
    validate_property_replacement(resolved.shape, path, &replacement, None)?;
    let mut candidate = value.clone();
    let previous = replace_prepared_property(&mut candidate, &storage, replacement, None)?;
    validate_component_value(schema, &candidate)
        .map_err(ComponentPropertyError::InvalidReplacement)?;
    *value = candidate;
    Ok(previous)
}

fn validate_property_replacement(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    replacement: &ComponentValue,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<(), ComponentPropertyError> {
    admit_property_value(replacement, limits)?;
    shape.validate_value(replacement).map_err(|error| {
        if error.kind == crate::ComponentShapeErrorKind::NumericInterval {
            ComponentPropertyError::InvalidReplacement(crate::registry::numeric_range_error(
                &path.root,
            ))
        } else {
            ComponentPropertyError::InvalidReplacementShape(error)
        }
    })
}

fn replace_prepared_property(
    candidate: &mut ComponentValue,
    storage: &ComponentFieldPath,
    replacement: ComponentValue,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<ComponentValue, ComponentPropertyError> {
    let previous = candidate
        .replace_path(storage, replacement)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    admit_property_value(candidate, limits)?;
    Ok(previous)
}

fn admit_property_value(
    value: &ComponentValue,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<(), ComponentPropertyError> {
    match limits {
        Some(limits) => validate_value_materialization_budget(value, limits)
            .map_err(ComponentPropertyError::InvalidValue),
        None => validate_component_value_budget(value).map_err(ComponentPropertyError::ValueBudget),
    }
}

fn resolve_location<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: &'value ComponentValue,
    path: &ComponentPropertyPath,
    limits: Option<ComponentValueMaterializationLimits>,
) -> Result<
    (
        ResolvedComponentProperty<'schema, 'value>,
        ComponentFieldPath,
    ),
    ComponentPropertyError,
> {
    if version != schema.version {
        return Err(ComponentPropertyError::VersionMismatch {
            expected: schema.version,
            actual: version,
        });
    }
    path.validate()?;
    validate_component_shape_budget(schema)
        .map_err(|(_, error)| ComponentPropertyError::InvalidSchema(error))?;
    admit_property_value(value, limits)?;
    let root = resolve_root_field(schema, &path.root, None, &mut None)?;
    let current = value
        .get_path(&root.path)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    let selected = resolve_root_location(
        root,
        Some(current),
        path,
        None,
        Some(root.path.segments().to_vec()),
        None,
    )?;
    Ok((
        ResolvedComponentProperty {
            shape: selected.shape,
            value: selected
                .value
                .expect("value resolution retains its observation"),
        },
        selected
            .storage
            .expect("complete resolution records its storage path"),
    ))
}

struct PropertyLocation<'schema, 'value> {
    shape: &'schema ComponentValueShape,
    value: Option<&'value ComponentValue>,
    storage: Option<ComponentFieldPath>,
}

fn resolve_root_location<'schema, 'value>(
    root: &'schema ComponentFieldSchema,
    mut current: Option<&'value ComponentValue>,
    path: &ComponentPropertyPath,
    mut budget: Option<&mut ComponentPropertyReadBudget>,
    mut storage: Option<Vec<ComponentFieldPathSegment>>,
    mut admission: Option<&mut PropertyAdmission<'_>>,
) -> Result<PropertyLocation<'schema, 'value>, ComponentPropertyError> {
    let mut shape = &root.shape;
    let mut declaration = Vec::new();
    let mut depth = 0;
    for (step, selector) in path.steps.iter().enumerate() {
        while let ComponentValueShape::Optional { value } = shape {
            observe_read(&mut budget, &mut admission, 0)?;
            if matches!(current, Some(ComponentValue::Null)) {
                return Err(ComponentPropertyError::OptionalAbsent { step });
            }
            advance_declaration_depth(&mut depth)?;
            shape = value;
        }
        advance_declaration_depth(&mut depth)?;
        match selector {
            ComponentPropertyStep::Member(id) => {
                reserve_declaration(&mut declaration, &mut admission)?;
                admit_property_work(&mut admission, 0, 0, id.as_str().len())?;
                declaration.push(ComponentMemberPathSegment::Member(id.clone()));
                for tombstone in &root.member_tombstones {
                    if budget.is_some() {
                        for segment in tombstone.segments() {
                            let bytes = match segment {
                                ComponentMemberPathSegment::Member(id) => id.as_str().len(),
                                _ => 0,
                            };
                            observe_read(&mut budget, &mut admission, bytes)?;
                        }
                    }
                    if declaration.starts_with(tombstone.segments()) {
                        return Err(ComponentPropertyError::RetiredMember(
                            ComponentMemberPath::new(declaration),
                        ));
                    }
                }
                let ComponentValueShape::Record { members } = shape else {
                    return Err(ComponentPropertyError::ExpectedRecord { step });
                };
                let mut selected = None;
                for member in members {
                    observe_read(
                        &mut budget,
                        &mut admission,
                        member
                            .name
                            .len()
                            .saturating_add(member.id.as_ref().map_or(0, |id| id.as_str().len())),
                    )?;
                    if member.id.as_ref() == Some(id) {
                        selected = Some(member);
                        break;
                    }
                }
                let member = selected.ok_or_else(|| ComponentPropertyError::MemberNotFound {
                    step,
                    id: id.clone(),
                })?;
                current = current
                    .map(|value| value.field(&member.name))
                    .transpose()
                    .map_err(ComponentPropertyError::InvalidValue)?;
                if let Some(storage) = &mut storage {
                    storage.push(ComponentFieldPathSegment::field(&member.name));
                }
                shape = &member.shape;
            }
            ComponentPropertyStep::ListItem { by, key } => {
                if matches!(shape, ComponentValueShape::List { .. }) {
                    return Err(ComponentPropertyError::UnkeyedList { step });
                }
                let ComponentValueShape::KeyedList {
                    item,
                    key: identity,
                } = shape
                else {
                    return Err(ComponentPropertyError::ExpectedList { step });
                };
                if by != identity {
                    return Err(ComponentPropertyError::ListIdentityMismatch {
                        step,
                        expected: identity.clone(),
                        actual: by.clone(),
                    });
                }
                // Admit the helper's finite declaration scan before it visits the members.
                if budget.is_some()
                    && let ComponentValueShape::Record { members } = item.as_ref()
                {
                    for member in members {
                        observe_read(
                            &mut budget,
                            &mut admission,
                            member.name.len().saturating_add(
                                member.id.as_ref().map_or(0, |id| id.as_str().len()),
                            ),
                        )?;
                    }
                }
                let member = keyed_list_member(item, identity).map_err(|kind| {
                    ComponentPropertyError::InvalidSchema(ComponentShapeError {
                        path: ComponentFieldPath::new(
                            storage.as_deref().unwrap_or_default().iter().cloned(),
                        ),
                        kind,
                    })
                })?;
                let expected = member.shape.value_kind();
                if key.value_kind() != expected {
                    return Err(ComponentPropertyError::KeyTypeMismatch {
                        step,
                        expected,
                        actual: key.value_kind(),
                    });
                }
                shape = item;
                reserve_declaration(&mut declaration, &mut admission)?;
                declaration.push(ComponentMemberPathSegment::ListItem);
                let Some(value) = current else {
                    continue;
                };
                let ComponentValue::List(items) = value else {
                    return Err(ComponentPropertyError::InvalidValue(
                        ComponentCodecError::invalid_field("keyed list", "list"),
                    ));
                };
                let mut found = None;
                for (index, value) in items.iter().enumerate() {
                    observe_read(&mut budget, &mut admission, member.name.len())?;
                    let identity = value
                        .get(&member.name)
                        .filter(|value| value.kind() == expected)
                        .ok_or(ComponentPropertyError::InvalidListItemKey { step, index })?;
                    let bytes = match identity {
                        ComponentValue::String(value) => value.len(),
                        _ => 8,
                    };
                    observe_read(&mut budget, &mut admission, bytes)?;
                    if key.matches(identity) && found.replace(index).is_some() {
                        return Err(ComponentPropertyError::AmbiguousListItem {
                            step,
                            key: key.clone(),
                        });
                    }
                }
                let index = found.ok_or_else(|| ComponentPropertyError::ListItemNotFound {
                    step,
                    key: key.clone(),
                })?;
                current = Some(&items[index]);
                if let Some(storage) = &mut storage {
                    storage.push(ComponentFieldPathSegment::index(
                        u32::try_from(index)
                            .map_err(|_| ComponentPropertyError::StorageIndexOverflow)?,
                    ));
                }
            }
            ComponentPropertyStep::MapEntry(key) => {
                let ComponentValueShape::Map { value: entry } = shape else {
                    return Err(ComponentPropertyError::ExpectedMap { step });
                };
                observe_read(&mut budget, &mut admission, key.len())?;
                current = current
                    .map(|value| {
                        value
                            .get(key)
                            .ok_or_else(|| ComponentPropertyError::MapEntryNotFound {
                                step,
                                key: key.clone(),
                            })
                    })
                    .transpose()?;
                if let Some(storage) = &mut storage {
                    storage.push(ComponentFieldPathSegment::field(key));
                }
                shape = entry;
                reserve_declaration(&mut declaration, &mut admission)?;
                declaration.push(ComponentMemberPathSegment::MapEntry);
            }
        }
    }
    Ok(PropertyLocation {
        shape,
        value: current,
        storage: storage.map(ComponentFieldPath::new),
    })
}

fn advance_declaration_depth(depth: &mut usize) -> Result<(), ComponentPropertyError> {
    *depth += 1;
    if *depth > MAX_SHAPE_DEPTH {
        return Err(ComponentPropertyError::PathDepthLimit);
    }
    Ok(())
}

fn observe_read(
    budget: &mut Option<&mut ComponentPropertyReadBudget>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
    bytes: usize,
) -> Result<(), ComponentPropertyError> {
    if let Some(admit) = admission.as_deref_mut() {
        // Validate the shared read ceiling without consuming an observation denied by the
        // caller. The ordinary path below retains its existing single budget update.
        let next = budget
            .as_deref()
            .map(|budget| {
                let mut next = *budget;
                next.observe(bytes)?;
                Ok::<_, ComponentPropertyError>(next)
            })
            .transpose()?;
        if !admit(1, bytes, 0) {
            return Err(ComponentPropertyError::ReadBudgetExceeded);
        }
        if let (Some(budget), Some(next)) = (budget.as_deref_mut(), next) {
            *budget = next;
        }
        Ok(())
    } else {
        match budget {
            Some(budget) => budget.observe(bytes),
            None => Ok(()),
        }
    }
}

fn admit_property_work(
    admission: &mut Option<&mut PropertyAdmission<'_>>,
    nodes: usize,
    bytes: usize,
    allocated: usize,
) -> Result<(), ComponentPropertyError> {
    if let Some(admit) = admission.as_deref_mut()
        && !admit(nodes, bytes, allocated)
    {
        // The public admission entry point retains the exact caller error outside this
        // internal carrier. No diagnostic or cloned selector is needed while unwinding.
        return Err(ComponentPropertyError::ReadBudgetExceeded);
    }
    Ok(())
}

fn reserve_declaration(
    declaration: &mut Vec<ComponentMemberPathSegment>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<(), ComponentPropertyError> {
    if admission.is_none() || declaration.len() < declaration.capacity() {
        return Ok(());
    }
    let capacity = declaration
        .len()
        .saturating_add(1)
        .max(declaration.capacity().saturating_mul(2))
        .max(4);
    admit_property_work(
        admission,
        0,
        0,
        capacity.saturating_mul(size_of::<ComponentMemberPathSegment>()),
    )?;
    declaration.reserve_exact(capacity - declaration.len());
    Ok(())
}

fn resolve_root_field<'schema>(
    schema: &'schema ComponentSchema,
    id: &ComponentFieldId,
    mut budget: Option<&mut ComponentPropertyReadBudget>,
    admission: &mut Option<&mut PropertyAdmission<'_>>,
) -> Result<&'schema ComponentFieldSchema, ComponentPropertyError> {
    for tombstone in &schema.field_tombstones {
        observe_read(&mut budget, admission, tombstone.as_str().len())?;
        if tombstone == id {
            return Err(ComponentPropertyError::RetiredField(id.clone()));
        }
    }
    for field in &schema.fields {
        observe_read(&mut budget, admission, field.id.as_str().len())?;
        if &field.id == id {
            return Ok(field);
        }
    }
    Err(ComponentPropertyError::UnknownRoot(id.clone()))
}

#[cfg(test)]
mod admission_tests;
