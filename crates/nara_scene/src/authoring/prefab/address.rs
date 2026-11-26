use std::{
    borrow::Cow,
    collections::BTreeMap,
    fmt::{self, Display, Formatter, Write},
};

use nara_core::EntityUuid;
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathParseError,
    ComponentFieldPathSegment, ComponentNumericDomain, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape,
};
use thiserror::Error;

use super::{
    super::{CompactValueError, RawRon, SceneAsset, SceneEntity, compact},
    MAX_PREFAB_DEPTH,
};

/// A component field on an instance root or one of its generated descendants.
///
/// Ordinary keys use `game.Stats.health` and `child:<uuid>/game.Stats.health`. Nested
/// instances add UUID segments. Use [`Self::canonical_key`] when writing into an override
/// map. It disambiguates component IDs containing dots and quotes field keys when needed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrefabOverridePath {
    child_path: Vec<EntityUuid>,
    component: ComponentTypeId,
    field: ComponentFieldPath,
}

impl PrefabOverridePath {
    pub fn new(
        child_path: impl IntoIterator<Item = EntityUuid>,
        component: ComponentTypeId,
        field: ComponentFieldPath,
    ) -> Result<Self, PrefabOverrideError> {
        let child_path: Vec<_> = child_path.into_iter().take(MAX_PREFAB_DEPTH + 1).collect();
        if child_path.len() > MAX_PREFAB_DEPTH
            || !matches!(
                field.segments().first(),
                Some(ComponentFieldPathSegment::Field(_))
            )
        {
            return Err(PrefabOverrideError::InvalidPath);
        }
        Ok(Self {
            child_path,
            component,
            field,
        })
    }

    /// Parse a field path against a schema catalog without a target entity.
    pub fn parse(raw: &str, schemas: &ComponentSchemaCatalog) -> Result<Self, PrefabOverrideError> {
        let (child_path, rest) = split_target(raw)?;
        let component_ids = schemas.components().iter().map(ComponentSchema::id);
        let path = Self::parse_field(child_path, rest, component_ids)?;
        path.shape(schemas)?;
        Ok(path)
    }

    #[must_use]
    pub fn child_path(&self) -> &[EntityUuid] {
        &self.child_path
    }

    #[must_use]
    pub fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub fn field(&self) -> &ComponentFieldPath {
        &self.field
    }

    /// Canonical map key for the current schema catalog. `::` marks the component boundary
    /// if a dot could belong to either a component ID or a field path.
    #[must_use]
    pub fn canonical_key(&self, schemas: &ComponentSchemaCatalog) -> String {
        let mut key = self.to_string();
        if Self::parse(&key, schemas)
            .as_ref()
            .is_ok_and(|path| path == self)
        {
            return key;
        }
        key.clear();
        let _ = self.write_key(&mut key, "::");
        key
    }

    pub(super) fn parse_target(
        child_path: Vec<EntityUuid>,
        rest: &str,
        schemas: &ComponentSchemaCatalog,
        target: &SceneEntity,
    ) -> Result<Self, PrefabOverrideError> {
        match Self::parse_field(child_path, rest, target.components.keys()) {
            Err(PrefabOverrideError::UnknownComponent) => {
                let component_ids = schemas.components().iter().map(ComponentSchema::id);
                let (component, _) = split_component(rest, component_ids)?;
                Err(PrefabOverrideError::MissingComponent(component.clone()))
            }
            result => result,
        }
    }

    fn parse_field<'a>(
        child_path: Vec<EntityUuid>,
        rest: &str,
        component_ids: impl Iterator<Item = &'a ComponentTypeId>,
    ) -> Result<Self, PrefabOverrideError> {
        let (component, tail) = split_component(rest, component_ids)?;
        Self::new(child_path, component.clone(), tail.parse()?)
    }

    fn write_key(&self, output: &mut impl Write, delimiter: &str) -> fmt::Result {
        if !self.child_path.is_empty() {
            output.write_str("child:")?;
            for child in &self.child_path {
                write!(output, "{child}/")?;
            }
        }
        write!(output, "{}{delimiter}{}", self.component, self.field)
    }

    fn shape<'a>(
        &self,
        schemas: &'a ComponentSchemaCatalog,
    ) -> Result<Cow<'a, ComponentValueShape>, PrefabOverrideError> {
        let schema = find_schema(schemas, &self.component)
            .ok_or_else(|| PrefabOverrideError::MissingSchema(self.component.clone()))?;
        field_shape(schema, &self.field).ok_or(PrefabOverrideError::UnknownField)
    }
}

impl Display for PrefabOverridePath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.write_key(formatter, ".")
    }
}

/// Encode one validated field value into its compact override representation.
pub fn encode_override_value(
    path: &PrefabOverridePath,
    value: &ComponentValue,
    schemas: &ComponentSchemaCatalog,
    assets: &BTreeMap<String, SceneAsset>,
) -> Result<RawRon, PrefabOverrideError> {
    Ok(compact::encode_value(
        value,
        path.shape(schemas)?.as_ref(),
        assets,
    )?)
}

/// Decode one override using the addressed field shape and the declaring document's aliases.
pub fn decode_override_value(
    path: &PrefabOverridePath,
    value: &RawRon,
    schemas: &ComponentSchemaCatalog,
    assets: &BTreeMap<String, SceneAsset>,
) -> Result<ComponentValue, PrefabOverrideError> {
    Ok(compact::decode_value(
        value,
        path.shape(schemas)?.as_ref(),
        assets,
    )?)
}

#[derive(Debug, Error)]
pub enum PrefabOverrideError {
    #[error("invalid field or child path")]
    InvalidPath,
    #[error("no component ID matches the override path")]
    UnknownComponent,
    #[error("ambiguous component ID, use 'component::field' to mark the boundary")]
    AmbiguousComponent,
    #[error("field is not declared by the component schema")]
    UnknownField,
    #[error("child path does not select an entity in this instance")]
    MissingTarget,
    #[error("target does not have component '{0}'")]
    MissingComponent(ComponentTypeId),
    #[error("schema unavailable for component '{0}'")]
    MissingSchema(ComponentTypeId),
    #[error("component '{0}' is opaque")]
    OpaqueComponent(ComponentTypeId),
    #[error("component schema version {} does not match current version {}", .stored.get(), .current.get())]
    VersionMismatch {
        stored: ComponentSchemaVersion,
        current: ComponentSchemaVersion,
    },
    #[error("field overlaps another override at '{0}'")]
    OverlappingField(String),
    #[error(transparent)]
    FieldSyntax(#[from] ComponentFieldPathParseError),
    #[error(transparent)]
    Value(#[from] CompactValueError),
    #[error(transparent)]
    FieldPath(#[from] ComponentFieldPathError),
    #[error(transparent)]
    ComponentValue(#[from] ComponentCodecError),
}

pub(super) fn find_schema<'a>(
    schemas: &'a ComponentSchemaCatalog,
    component: &ComponentTypeId,
) -> Option<&'a ComponentSchema> {
    let schemas = schemas.components();
    let index = schemas
        .binary_search_by(|schema| schema.id().cmp(component))
        .ok()?;
    schemas.get(index)
}

pub(super) fn split_target(raw: &str) -> Result<(Vec<EntityUuid>, &str), PrefabOverrideError> {
    let Some(mut rest) = raw.strip_prefix("child:") else {
        return Ok((Vec::new(), raw));
    };
    let mut children = Vec::new();
    while let Some((segment, tail)) = rest.split_once('/') {
        let Ok(child) = segment.parse::<EntityUuid>() else {
            break;
        };
        if children.len() == MAX_PREFAB_DEPTH {
            return Err(PrefabOverrideError::InvalidPath);
        }
        children.push(child);
        rest = tail;
    }
    if children.is_empty() || rest.is_empty() {
        return Err(PrefabOverrideError::InvalidPath);
    }
    Ok((children, rest))
}

fn split_component<'a, 'ids>(
    text: &'a str,
    component_ids: impl Iterator<Item = &'ids ComponentTypeId>,
) -> Result<(&'ids ComponentTypeId, &'a str), PrefabOverrideError> {
    if let Some((explicit, tail)) = text
        .split_once("::")
        .filter(|(explicit, _)| ComponentTypeId::try_new(*explicit).is_ok())
    {
        return component_ids
            .filter(|id| id.as_str() == explicit)
            .map(|id| (id, tail))
            .next()
            .ok_or(PrefabOverrideError::UnknownComponent);
    }
    let mut found = None;
    for id in component_ids {
        if let Some(tail) = text
            .strip_prefix(id.as_str())
            .and_then(|tail| tail.strip_prefix('.'))
        {
            if found.is_some_and(|(other, _)| other != id) {
                return Err(PrefabOverrideError::AmbiguousComponent);
            }
            found = Some((id, tail));
        }
    }
    found.ok_or(PrefabOverrideError::UnknownComponent)
}

fn field_shape<'a>(
    schema: &'a ComponentSchema,
    path: &ComponentFieldPath,
) -> Option<Cow<'a, ComponentValueShape>> {
    let (mut shape, tail) = schema.fields().iter().find_map(|field| {
        let tail = path.segments().strip_prefix(field.path().segments())?;
        Some((field.shape(), tail))
    })?;
    for (index, segment) in tail.iter().enumerate() {
        while let ComponentValueShape::Optional { value } = shape {
            shape = value;
        }
        shape = match (shape, segment) {
            (ComponentValueShape::Record { members }, ComponentFieldPathSegment::Field(name)) => {
                &members.iter().find(|member| &member.name == name)?.shape
            }
            (ComponentValueShape::Map { value }, ComponentFieldPathSegment::Field(_)) => value,
            (
                ComponentValueShape::List { item } | ComponentValueShape::KeyedList { item, .. },
                ComponentFieldPathSegment::Index(_),
            ) => item,
            (ComponentValueShape::Scalar { kind, .. }, ComponentFieldPathSegment::Index(_))
                if index + 1 == tail.len() =>
            {
                let domain = match kind {
                    ComponentValueKind::PackedU16 => ComponentNumericDomain::U16,
                    ComponentValueKind::PackedU32 => ComponentNumericDomain::U32,
                    ComponentValueKind::PackedI32 => ComponentNumericDomain::I32,
                    ComponentValueKind::PackedF32 => ComponentNumericDomain::F32,
                    ComponentValueKind::Bytes => ComponentNumericDomain::U8,
                    _ => return None,
                };
                return Some(Cow::Owned(
                    ComponentValueShape::scalar(domain.value_kind()).with_numeric_domain(domain),
                ));
            }
            _ => return None,
        };
    }
    Some(Cow::Borrowed(shape))
}
