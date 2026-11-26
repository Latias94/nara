use std::collections::BTreeSet;

use crate::{
    AliasError, ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema, ComponentMemberPath,
    ComponentSchema, ComponentSchemaCatalog, ComponentShapeError, ComponentTypeId,
    ComponentValueKind, ComponentValueRef,
    schema::{validate_alias, validate_component_shape_budget, validate_component_value_budget},
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComponentRegistryError {
    #[error("component ID '{0}' is already registered")]
    DuplicateComponentId(ComponentTypeId),
    #[error("invalid component ID '{0}'")]
    InvalidComponentTypeId(ComponentTypeId),
    #[error("component '{component_id}' has a zero schema version")]
    InvalidComponentSchemaVersion { component_id: ComponentTypeId },
    #[error("invalid field ID '{field_id}' in component '{component_id}'")]
    InvalidComponentFieldId {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    #[error("invalid alias in component '{component_id}', field {field_id:?}: {source}")]
    InvalidAlias {
        component_id: ComponentTypeId,
        field_id: Option<ComponentFieldId>,
        source: AliasError,
    },
    #[error("duplicate alias '{alias}' in component '{component_id}', field {field_id:?}")]
    DuplicateAlias {
        component_id: ComponentTypeId,
        field_id: Option<ComponentFieldId>,
        alias: String,
    },
    #[error("invalid capability {capability:?} on component '{component_id}'")]
    InvalidComponentCapability {
        component_id: ComponentTypeId,
        capability: ComponentCapability,
    },
    #[error("invalid capability {capability:?} on field '{field_id}' of '{component_id}'")]
    InvalidFieldCapability {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        capability: ComponentCapability,
    },
    #[error("duplicate field '{field_id}' in component '{component_id}'")]
    DuplicateComponentFieldId {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    #[error("duplicate field path '{path}' in component '{component_id}'")]
    DuplicateComponentFieldPath {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
    },
    #[error("overlapping field paths '{first}' and '{second}' in component '{component_id}'")]
    OverlappingComponentFieldPaths {
        component_id: ComponentTypeId,
        first: ComponentFieldPath,
        second: ComponentFieldPath,
    },
    #[error("duplicate field tombstone '{field_id}' in component '{component_id}'")]
    DuplicateFieldTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    #[error("active field '{field_id}' is tombstoned in component '{component_id}'")]
    ActiveFieldIsTombstoned {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
    },
    #[error("duplicate member tombstone {path:?} on field '{field_id}' of '{component_id}'")]
    DuplicateMemberTombstone {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    #[error("active member {path:?} is tombstoned on field '{field_id}' of '{component_id}'")]
    ActiveMemberIsTombstoned {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        path: ComponentMemberPath,
    },
    #[error("invalid shape for field '{field_id}' of '{component_id}': {source}")]
    InvalidComponentFieldShape {
        component_id: ComponentTypeId,
        field_id: ComponentFieldId,
        source: ComponentShapeError,
    },
    #[error("invalid default at '{path}' in component '{component_id}': {source}")]
    InvalidComponentFieldDefault {
        component_id: ComponentTypeId,
        path: ComponentFieldPath,
        source: ComponentShapeError,
    },
    #[error("invalid initial value in component '{component_id}': {source}")]
    InvalidComponentAuthoringInitialValue {
        component_id: ComponentTypeId,
        source: ComponentCodecError,
    },
}

impl ComponentSchemaCatalog {
    /// Builds a catalog with unique component IDs and validated field metadata.
    pub fn from_components(
        components: impl IntoIterator<Item = ComponentSchema>,
    ) -> Result<Self, ComponentRegistryError> {
        Self {
            components: components.into_iter().collect(),
        }
        .validated()
    }

    pub(crate) fn validated(mut self) -> Result<Self, ComponentRegistryError> {
        self.validate()?;
        self.canonicalize();
        Ok(self)
    }

    pub(crate) fn validate(&self) -> Result<(), ComponentRegistryError> {
        let mut ids = BTreeSet::new();
        for schema in &self.components {
            if !ids.insert(schema.id()) {
                return Err(ComponentRegistryError::DuplicateComponentId(
                    schema.id.clone(),
                ));
            }
            validate_schema(schema)?;
        }
        Ok(())
    }

    pub(crate) fn insert(
        &mut self,
        mut schema: ComponentSchema,
    ) -> Result<(), ComponentRegistryError> {
        let index = match self
            .components
            .binary_search_by(|item| item.id.cmp(&schema.id))
        {
            Ok(_) => return Err(ComponentRegistryError::DuplicateComponentId(schema.id)),
            Err(index) => index,
        };
        validate_schema(&schema)?;
        schema.canonicalize();
        self.components.insert(index, schema);
        Ok(())
    }
}

fn validate_schema(schema: &ComponentSchema) -> Result<(), ComponentRegistryError> {
    schema
        .id
        .validate()
        .map_err(|_| ComponentRegistryError::InvalidComponentTypeId(schema.id.clone()))?;
    if schema.version.get() == 0 {
        return Err(ComponentRegistryError::InvalidComponentSchemaVersion {
            component_id: schema.id.clone(),
        });
    }
    validate_aliases(&schema.id, None, &schema.aliases)?;
    for capability in [
        ComponentCapability::AssetRef,
        ComponentCapability::EntityRef,
    ] {
        if schema.has_capability(capability) {
            return Err(ComponentRegistryError::InvalidComponentCapability {
                component_id: schema.id.clone(),
                capability,
            });
        }
    }
    validate_component_shape_budget(schema).map_err(|(field_id, source)| {
        ComponentRegistryError::InvalidComponentFieldShape {
            component_id: schema.id.clone(),
            field_id,
            source,
        }
    })?;

    let mut field_ids = BTreeSet::new();
    for field in &schema.fields {
        validate_field_id(schema, &field.id)?;
        if !field_ids.insert(&field.id) {
            return Err(ComponentRegistryError::DuplicateComponentFieldId {
                component_id: schema.id.clone(),
                field_id: field.id.clone(),
            });
        }
        validate_field(schema, field)?;
    }
    let mut tombstones = BTreeSet::new();
    for field_id in &schema.field_tombstones {
        validate_field_id(schema, field_id)?;
        if !tombstones.insert(field_id) {
            return Err(ComponentRegistryError::DuplicateFieldTombstone {
                component_id: schema.id.clone(),
                field_id: field_id.clone(),
            });
        }
        if field_ids.contains(field_id) {
            return Err(ComponentRegistryError::ActiveFieldIsTombstoned {
                component_id: schema.id.clone(),
                field_id: field_id.clone(),
            });
        }
    }
    let paths = sorted_paths(schema);
    for pair in paths.windows(2) {
        let [first, second] = pair else { continue };
        if first == second {
            return Err(ComponentRegistryError::DuplicateComponentFieldPath {
                component_id: schema.id.clone(),
                path: (*first).clone(),
            });
        }
        if second.segments().starts_with(first.segments()) {
            return Err(ComponentRegistryError::OverlappingComponentFieldPaths {
                component_id: schema.id.clone(),
                first: (*first).clone(),
                second: (*second).clone(),
            });
        }
    }
    if let Some(value) = &schema.authoring_initial_value {
        let invalid = |source| ComponentRegistryError::InvalidComponentAuthoringInitialValue {
            component_id: schema.id.clone(),
            source,
        };
        validate_component_value_budget(value)
            .map_err(|error| invalid(ComponentCodecError::Message(error.to_string())))?;
        validate_value(schema, &paths, value.into(), true).map_err(invalid)?;
    }
    Ok(())
}

fn validate_field_id(
    schema: &ComponentSchema,
    field_id: &ComponentFieldId,
) -> Result<(), ComponentRegistryError> {
    field_id
        .validate()
        .map_err(|_| ComponentRegistryError::InvalidComponentFieldId {
            component_id: schema.id.clone(),
            field_id: field_id.clone(),
        })
}

fn validate_aliases(
    component_id: &ComponentTypeId,
    field_id: Option<&ComponentFieldId>,
    aliases: &[String],
) -> Result<(), ComponentRegistryError> {
    let invalid = |source| ComponentRegistryError::InvalidAlias {
        component_id: component_id.clone(),
        field_id: field_id.cloned(),
        source,
    };
    if aliases.is_empty() {
        return Err(invalid(AliasError::Empty));
    }
    let mut unique = BTreeSet::new();
    for alias in aliases {
        validate_alias(alias).map_err(invalid)?;
        if !unique.insert(alias) {
            return Err(ComponentRegistryError::DuplicateAlias {
                component_id: component_id.clone(),
                field_id: field_id.cloned(),
                alias: alias.clone(),
            });
        }
    }
    Ok(())
}

fn validate_field(
    schema: &ComponentSchema,
    field: &ComponentFieldSchema,
) -> Result<(), ComponentRegistryError> {
    validate_aliases(&schema.id, Some(&field.id), &field.aliases)?;
    let invalid_capability = |capability| ComponentRegistryError::InvalidFieldCapability {
        component_id: schema.id.clone(),
        field_id: field.id.clone(),
        capability,
    };
    for capability in [
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::Edit,
    ] {
        if field.has_capability(capability) && !schema.has_capability(capability) {
            return Err(invalid_capability(capability));
        }
    }
    for (kind, capability) in [
        (ComponentValueKind::AssetRef, ComponentCapability::AssetRef),
        (
            ComponentValueKind::EntityRef,
            ComponentCapability::EntityRef,
        ),
    ] {
        if field.shape.declares_kind(kind) != field.has_capability(capability) {
            return Err(invalid_capability(capability));
        }
    }
    if let Some(default) = &field.default_value {
        validate_component_value_budget(default)
            .and_then(|()| field.shape.validate_value(default))
            .map_err(
                |source| ComponentRegistryError::InvalidComponentFieldDefault {
                    component_id: schema.id.clone(),
                    path: field.path.clone(),
                    source,
                },
            )?;
    }
    if !field.member_tombstones.is_empty() {
        let active = field.active_member_paths();
        let mut unique = BTreeSet::new();
        for path in &field.member_tombstones {
            if !unique.insert(path) {
                return Err(ComponentRegistryError::DuplicateMemberTombstone {
                    component_id: schema.id.clone(),
                    field_id: field.id.clone(),
                    path: path.clone(),
                });
            }
            if active.contains(path) {
                return Err(ComponentRegistryError::ActiveMemberIsTombstoned {
                    component_id: schema.id.clone(),
                    field_id: field.id.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(())
}

fn sorted_paths(schema: &ComponentSchema) -> Vec<&ComponentFieldPath> {
    let mut paths: Vec<_> = schema.fields.iter().map(|field| &field.path).collect();
    paths.sort_unstable_by(|left, right| left.segments().cmp(right.segments()));
    paths
}

/// Checks a value against its declared paths and field shapes. Missing optional fields and fields
/// with defaults are accepted.
pub fn validate_component_value<'a>(
    schema: &ComponentSchema,
    value: impl Into<ComponentValueRef<'a>>,
) -> Result<(), ComponentCodecError> {
    validate_value(schema, &sorted_paths(schema), value.into(), false)
}

fn validate_value(
    schema: &ComponentSchema,
    paths: &[&ComponentFieldPath],
    value: ComponentValueRef<'_>,
    require_all_fields: bool,
) -> Result<(), ComponentCodecError> {
    if paths.is_empty() && value.as_map().is_some_and(|fields| fields.is_empty()) {
        return Ok(());
    }
    for field in &schema.fields {
        match value.get_path(&field.path) {
            Ok(value) => field.shape.validate_value(value).map_err(|error| {
                let path = ComponentFieldPath::new(
                    field
                        .path
                        .segments()
                        .iter()
                        .chain(error.path.segments())
                        .cloned(),
                );
                ComponentCodecError::invalid_field(path.to_string(), error.to_string())
            })?,
            Err(
                ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. },
            ) if !require_all_fields && (!field.required || field.default_value.is_some()) => {}
            Err(
                ComponentFieldPathError::MissingField { .. }
                | ComponentFieldPathError::IndexOutOfBounds { .. },
            ) => {
                return Err(ComponentCodecError::missing_field(field.path.to_string()));
            }
            Err(error) => return Err(ComponentCodecError::Message(error.to_string())),
        }
    }
    validate_coverage(paths, value, &mut Vec::new())
}

fn validate_coverage(
    paths: &[&ComponentFieldPath],
    value: ComponentValueRef<'_>,
    path: &mut Vec<ComponentFieldPathSegment>,
) -> Result<(), ComponentCodecError> {
    let next = paths.partition_point(|candidate| candidate.segments() < path.as_slice());
    if let Some(declared) = paths.get(next) {
        if declared.segments() == path.as_slice() {
            return Ok(());
        }
        if declared.segments().starts_with(path) {
            if let Some(fields) = value.as_map() {
                for (name, child) in fields.iter() {
                    path.push(ComponentFieldPathSegment::field(name));
                    validate_coverage(paths, child, path)?;
                    path.pop();
                }
                return Ok(());
            }
            if let Some(items) = value.as_list() {
                for (index, child) in items.iter().enumerate() {
                    let index = u32::try_from(index).map_err(|_| undeclared_path(path))?;
                    path.push(ComponentFieldPathSegment::index(index));
                    validate_coverage(paths, child, path)?;
                    path.pop();
                }
                return Ok(());
            }
        }
    }
    Err(undeclared_path(path))
}

fn undeclared_path(path: &[ComponentFieldPathSegment]) -> ComponentCodecError {
    let path = ComponentFieldPath::new(path.iter().cloned());
    ComponentCodecError::Message(format!(
        "component value path '{path}' is not declared by schema"
    ))
}
