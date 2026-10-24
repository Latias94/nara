//! Schema-relative property identity, independent of documents and runtime incarnations.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentMemberPath, ComponentMemberPathSegment, ComponentSchema,
    ComponentSchemaVersion, ComponentShapeError, ComponentValue, ComponentValueKind,
    ComponentValueShape,
    schema::{
        MAX_SHAPE_BYTES, MAX_SHAPE_DEPTH, keyed_list_member, validate_component_shape_budget,
        validate_component_value_budget,
    },
    validate_component_value,
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
        if self.steps.len() > MAX_SHAPE_DEPTH {
            return Err(ComponentPropertyError::PathDepthLimit);
        }
        let mut bytes = self.root.as_str().len();
        self.root
            .validate()
            .map_err(|_| ComponentPropertyError::InvalidFieldId(self.root.clone()))?;
        for step in &self.steps {
            bytes = bytes.saturating_add(match step {
                ComponentPropertyStep::Member(id) => {
                    id.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId(id.clone()))?;
                    id.as_str().len()
                }
                ComponentPropertyStep::ListItem { by, key } => {
                    by.validate()
                        .map_err(|_| ComponentPropertyError::InvalidFieldId(by.clone()))?;
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
}

/// A borrowed observation tied to the exact supplied schema and value. No reusable storage
/// locator escapes resolution, and replacing the value requires resolving the selector again.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedComponentProperty<'schema, 'value> {
    pub shape: &'schema ComponentValueShape,
    pub value: &'value ComponentValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentPropertyError {
    PathDepthLimit,
    PathByteLimit,
    InvalidFieldId(ComponentFieldId),
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
    let (resolved, _) = resolve_location(schema, version, value, path)?;
    validate_component_value(schema, value).map_err(ComponentPropertyError::InvalidValue)?;
    Ok(resolved)
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
    let (resolved, storage) = resolve_location(schema, version, value, path)?;
    validate_component_value(schema, value).map_err(ComponentPropertyError::InvalidValue)?;
    validate_component_value_budget(&replacement).map_err(ComponentPropertyError::ValueBudget)?;
    resolved
        .shape
        .validate_value(&replacement)
        .map_err(|error| {
            if error.kind == crate::ComponentShapeErrorKind::NumericInterval {
                ComponentPropertyError::InvalidReplacement(crate::registry::numeric_range_error(
                    &path.root,
                ))
            } else {
                ComponentPropertyError::InvalidReplacementShape(error)
            }
        })?;
    let mut candidate = value.clone();
    let previous = candidate
        .replace_path(&storage, replacement)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    validate_component_value_budget(&candidate).map_err(ComponentPropertyError::ValueBudget)?;
    validate_component_value(schema, &candidate)
        .map_err(ComponentPropertyError::InvalidReplacement)?;
    *value = candidate;
    Ok(previous)
}

fn resolve_location<'schema, 'value>(
    schema: &'schema ComponentSchema,
    version: ComponentSchemaVersion,
    value: &'value ComponentValue,
    path: &ComponentPropertyPath,
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
    validate_component_value_budget(value).map_err(ComponentPropertyError::ValueBudget)?;
    refuse_retired(schema, &path.root)?;
    let root = schema
        .fields
        .iter()
        .find(|field| field.id == path.root)
        .ok_or_else(|| ComponentPropertyError::UnknownRoot(path.root.clone()))?;
    let mut current = value
        .get_path(&root.path)
        .map_err(ComponentPropertyError::ValueUnavailable)?;
    let mut shape = &root.shape;
    let mut storage = root.path.segments().to_vec();
    let mut declaration = Vec::new();
    for (step, selector) in path.steps.iter().enumerate() {
        while let ComponentValueShape::Optional { value } = shape {
            if matches!(current, ComponentValue::Null) {
                return Err(ComponentPropertyError::OptionalAbsent { step });
            }
            shape = value;
        }
        match selector {
            ComponentPropertyStep::Member(id) => {
                declaration.push(ComponentMemberPathSegment::Member(id.clone()));
                if root
                    .member_tombstones
                    .iter()
                    .any(|tombstone| declaration.starts_with(tombstone.segments()))
                {
                    return Err(ComponentPropertyError::RetiredMember(
                        ComponentMemberPath::new(declaration),
                    ));
                }
                let ComponentValueShape::Record { members } = shape else {
                    return Err(ComponentPropertyError::ExpectedRecord { step });
                };
                let member = members
                    .iter()
                    .find(|member| member.id.as_ref() == Some(id))
                    .ok_or_else(|| ComponentPropertyError::MemberNotFound {
                        step,
                        id: id.clone(),
                    })?;
                current = current
                    .field(&member.name)
                    .map_err(ComponentPropertyError::InvalidValue)?;
                storage.push(ComponentFieldPathSegment::field(&member.name));
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
                let member = keyed_list_member(item, identity).map_err(|kind| {
                    ComponentPropertyError::InvalidSchema(ComponentShapeError {
                        path: ComponentFieldPath::new(storage.iter().cloned()),
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
                let ComponentValue::List(items) = current else {
                    return Err(ComponentPropertyError::InvalidValue(
                        ComponentCodecError::invalid_field("keyed list", "list"),
                    ));
                };
                let mut found = None;
                for (index, value) in items.iter().enumerate() {
                    let identity = value
                        .get(&member.name)
                        .filter(|value| value.kind() == expected)
                        .ok_or(ComponentPropertyError::InvalidListItemKey { step, index })?;
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
                current = &items[index];
                storage.push(ComponentFieldPathSegment::index(
                    u32::try_from(index)
                        .map_err(|_| ComponentPropertyError::StorageIndexOverflow)?,
                ));
                shape = item;
                declaration.push(ComponentMemberPathSegment::ListItem);
            }
            ComponentPropertyStep::MapEntry(key) => {
                let ComponentValueShape::Map { value: entry } = shape else {
                    return Err(ComponentPropertyError::ExpectedMap { step });
                };
                current =
                    current
                        .get(key)
                        .ok_or_else(|| ComponentPropertyError::MapEntryNotFound {
                            step,
                            key: key.clone(),
                        })?;
                storage.push(ComponentFieldPathSegment::field(key));
                shape = entry;
                declaration.push(ComponentMemberPathSegment::MapEntry);
            }
        }
    }
    Ok((
        ResolvedComponentProperty {
            shape,
            value: current,
        },
        ComponentFieldPath::new(storage),
    ))
}

fn refuse_retired(
    schema: &ComponentSchema,
    id: &ComponentFieldId,
) -> Result<(), ComponentPropertyError> {
    if schema.field_tombstones.contains(id) {
        Err(ComponentPropertyError::RetiredField(id.clone()))
    } else {
        Ok(())
    }
}
