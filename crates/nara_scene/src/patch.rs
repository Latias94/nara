use std::collections::BTreeSet;

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::{
    ComponentCapability, ComponentFieldPath, ComponentFieldSchema, ComponentRegistry,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, with_capability, with_component_field_path,
        with_component_field_path_error, with_public_locator, with_public_u64,
    },
};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ScenePatchDocument {
    pub format_version: u32,
    pub operations: Vec<ScenePatchOperation>,
}

impl ScenePatchDocument {
    pub const CURRENT_FORMAT_VERSION: u32 = 1;

    #[must_use]
    pub fn new(operations: impl IntoIterator<Item = ScenePatchOperation>) -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            operations: operations.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn apply_to_scene(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        self.apply_to_scene_with_validator(
            document,
            registry,
            |document, registry, operation_index| match operation_index {
                Some(operation_index) => {
                    document.validate_authoring_for_patch(registry, operation_index)
                }
                None => document.validate_authoring(registry),
            },
        )
    }

    pub fn apply_to_scene_with_asset_database(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> ScenePatchReport {
        self.apply_to_scene_with_validator(
            document,
            registry,
            |document, registry, operation_index| match operation_index {
                Some(operation_index) => document.validate_authoring_for_patch_with_asset_database(
                    registry,
                    database,
                    operation_index,
                ),
                None => document.validate_authoring_with_asset_database(registry, database),
            },
        )
    }

    fn apply_to_scene_with_validator(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        mut validate: impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> ScenePatchReport {
        if self.format_version != Self::CURRENT_FORMAT_VERSION {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(with_public_u64(
                with_public_u64(
                    diagnostic_error(
                        "scene.patch-unsupported-format-version",
                        "Scene patch format version is unsupported",
                    ),
                    "actual-version",
                    u64::from(self.format_version),
                ),
                "expected-version",
                u64::from(Self::CURRENT_FORMAT_VERSION),
            ));
            return ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            };
        }

        let source_validation = validate(document, registry, None);
        if source_validation.has_errors() {
            return ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics: source_validation,
            };
        }

        let mut scratch = document.clone();
        let mut inverse_groups = Vec::<Vec<ScenePatchOperation>>::new();
        for (operation_index, operation) in self.operations.iter().enumerate() {
            let inverse = match apply_operation(&mut scratch, registry, operation, operation_index)
            {
                Ok(inverse) => inverse,
                Err(diagnostic) => {
                    let mut diagnostics = DiagnosticReport::default();
                    diagnostics.push(diagnostic);
                    return ScenePatchReport {
                        applied: false,
                        inverse: None,
                        diagnostics,
                    };
                }
            };

            scratch.canonicalize();
            let validation = validate(&scratch, registry, Some(operation_index));
            if validation.has_errors() {
                return ScenePatchReport {
                    applied: false,
                    inverse: None,
                    diagnostics: validation,
                };
            }

            inverse_groups.push(inverse);
        }

        scratch.canonicalize();
        *document = scratch;

        let inverse_operations: Vec<ScenePatchOperation> =
            inverse_groups.into_iter().rev().flatten().collect();

        ScenePatchReport {
            applied: true,
            inverse: Some(ScenePatchDocument::new(inverse_operations)),
            diagnostics: DiagnosticReport::default(),
        }
    }
}

impl Default for ScenePatchDocument {
    fn default() -> Self {
        Self {
            format_version: Self::CURRENT_FORMAT_VERSION,
            operations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "op",
        content = "args",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ScenePatchOperation {
    AddEntity {
        entity: SceneEntityRecord,
    },
    RemoveEntity {
        entity: SceneEntityId,
    },
    AddComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
        value: SceneComponentRecord,
    },
    RemoveComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    ReplaceComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
        value: SceneComponentRecord,
    },
    SetField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
        value: ComponentValue,
    },
    RemoveField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
    },
    Reparent {
        entity: SceneEntityId,
        parent: Option<SceneEntityId>,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
        asset_ref: AssetRef,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePatchReport {
    pub applied: bool,
    pub inverse: Option<ScenePatchDocument>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Clone, Copy)]
struct FieldPatchTarget<'a> {
    registry: &'a ComponentRegistry,
    entity: &'a SceneEntityId,
    component: &'a ComponentTypeId,
    component_version: &'a ComponentSchemaVersion,
    path: &'a ComponentFieldPath,
    operation_index: usize,
}

impl<'a> FieldPatchTarget<'a> {
    fn new(
        registry: &'a ComponentRegistry,
        entity: &'a SceneEntityId,
        component: &'a ComponentTypeId,
        component_version: &'a ComponentSchemaVersion,
        path: &'a ComponentFieldPath,
        operation_index: usize,
    ) -> Self {
        Self {
            registry,
            entity,
            component,
            component_version,
            path,
            operation_index,
        }
    }
}

fn apply_operation(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    match operation {
        ScenePatchOperation::AddEntity { entity } => add_entity(document, entity, operation_index),
        ScenePatchOperation::RemoveEntity { entity } => {
            remove_entity_subtree(document, entity, operation_index)
        }
        ScenePatchOperation::AddComponent {
            entity,
            component,
            value,
        } => add_component(document, entity, component, value, operation_index),
        ScenePatchOperation::RemoveComponent { entity, component } => {
            remove_component(document, entity, component, operation_index)
        }
        ScenePatchOperation::ReplaceComponent {
            entity,
            component,
            value,
        } => replace_component(document, entity, component, value, operation_index),
        ScenePatchOperation::SetField {
            entity,
            component,
            component_version,
            path,
            value,
        } => set_field(
            document,
            FieldPatchTarget::new(
                registry,
                entity,
                component,
                component_version,
                path,
                operation_index,
            ),
            value.clone(),
        ),
        ScenePatchOperation::RemoveField {
            entity,
            component,
            component_version,
            path,
        } => remove_field(
            document,
            registry,
            entity,
            component,
            component_version,
            path,
            operation_index,
        ),
        ScenePatchOperation::Reparent { entity, parent } => {
            reparent_entity(document, entity, parent.as_ref(), operation_index)
        }
        ScenePatchOperation::SetAssetRefField {
            entity,
            component,
            component_version,
            path,
            asset_ref,
        } => set_asset_ref_field(
            document,
            FieldPatchTarget::new(
                registry,
                entity,
                component,
                component_version,
                path,
                operation_index,
            ),
            asset_ref,
        ),
    }
}

fn add_entity(
    document: &mut SceneDocument,
    entity: &SceneEntityRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    if find_entity(document, &entity.id).is_some() {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-duplicate-entity",
            "patch adds an entity that already exists",
            Some(&entity.id),
            None,
            None,
        ));
    }

    document.entities.push(entity.clone());
    Ok(vec![ScenePatchOperation::RemoveEntity {
        entity: entity.id.clone(),
    }])
}

fn remove_entity_subtree(
    document: &mut SceneDocument,
    entity: &SceneEntityId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    if find_entity(document, entity).is_none() {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-missing-entity",
            "patch removes an entity that does not exist",
            Some(entity),
            None,
            None,
        ));
    }

    let subtree = subtree_ids(document, entity);
    let mut removed = Vec::new();
    document.entities.retain(|record| {
        if subtree.contains(&record.id) {
            removed.push(record.clone());
            false
        } else {
            true
        }
    });
    let removed_for_depth = removed.clone();
    removed.sort_by_key(|record| subtree_depth(&removed_for_depth, &record.id));

    Ok(removed
        .into_iter()
        .map(|entity| ScenePatchOperation::AddEntity { entity })
        .collect())
}

fn add_component(
    document: &mut SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    value: &SceneComponentRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let record = entity_mut(document, entity, operation_index)?;
    if record.components.contains_key(component) {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-duplicate-component",
            "patch adds a component that already exists",
            Some(entity),
            Some(component),
            None,
        ));
    }
    record.components.insert(component.clone(), value.clone());
    Ok(vec![ScenePatchOperation::RemoveComponent {
        entity: entity.clone(),
        component: component.clone(),
    }])
}

fn remove_component(
    document: &mut SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let record = entity_mut(document, entity, operation_index)?;
    let Some(previous) = record.components.remove(component) else {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-missing-component",
            "patch removes a component that does not exist",
            Some(entity),
            Some(component),
            None,
        ));
    };
    Ok(vec![ScenePatchOperation::AddComponent {
        entity: entity.clone(),
        component: component.clone(),
        value: previous,
    }])
}

fn replace_component(
    document: &mut SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    value: &SceneComponentRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let record = entity_mut(document, entity, operation_index)?;
    let Some(previous) = record.components.insert(component.clone(), value.clone()) else {
        record.components.remove(component);
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-missing-component",
            "patch replaces a component that does not exist",
            Some(entity),
            Some(component),
            None,
        ));
    };
    Ok(vec![ScenePatchOperation::ReplaceComponent {
        entity: entity.clone(),
        component: component.clone(),
        value: previous,
    }])
}

fn set_field(
    document: &mut SceneDocument,
    target: FieldPatchTarget<'_>,
    value: ComponentValue,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let FieldPatchTarget {
        registry,
        entity,
        component,
        component_version,
        path,
        operation_index,
    } = target;
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        path,
        operation_index,
        entity,
    )?;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        path,
        operation_index,
        entity,
    )?;
    if !field_value_matches(field_schema, &value) {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-invalid-field-kind",
            "patch field value kind does not match registered schema",
            Some(entity),
            Some(component),
            Some(path),
        ));
    }

    let component_record = component_mut(document, entity, component, operation_index)?;
    let previous = component_record
        .value
        .set_path(path, value)
        .map_err(|error| {
            with_component_field_path_error(
                patch_diagnostic(
                    operation_index,
                    "scene.patch-invalid-field-path",
                    "Patch field path cannot be applied",
                    Some(entity),
                    Some(component),
                    Some(path),
                ),
                &error,
            )
        })?;

    Ok(vec![match previous {
        Some(value) => ScenePatchOperation::SetField {
            entity: entity.clone(),
            component: component.clone(),
            component_version: *component_version,
            path: path.clone(),
            value,
        },
        None => ScenePatchOperation::RemoveField {
            entity: entity.clone(),
            component: component.clone(),
            component_version: *component_version,
            path: path.clone(),
        },
    }])
}

fn remove_field(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    component_version: &ComponentSchemaVersion,
    path: &ComponentFieldPath,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        path,
        operation_index,
        entity,
    )?;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        path,
        operation_index,
        entity,
    )?;
    if field_schema.required && field_schema.default_value.is_none() {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-required-field-removal",
            "patch removes a required component field without a registered default",
            Some(entity),
            Some(component),
            Some(path),
        ));
    }

    let component_record = component_mut(document, entity, component, operation_index)?;
    let previous = component_record.value.remove_path(path).map_err(|error| {
        with_component_field_path_error(
            patch_diagnostic(
                operation_index,
                "scene.patch-invalid-field-path",
                "Patch field path cannot be removed",
                Some(entity),
                Some(component),
                Some(path),
            ),
            &error,
        )
    })?;

    Ok(vec![ScenePatchOperation::SetField {
        entity: entity.clone(),
        component: component.clone(),
        component_version: *component_version,
        path: path.clone(),
        value: previous,
    }])
}

fn reparent_entity(
    document: &mut SceneDocument,
    entity: &SceneEntityId,
    parent: Option<&SceneEntityId>,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    if let Some(parent) = parent
        && find_entity(document, parent).is_none()
    {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-missing-parent",
            "patch reparents to an entity that does not exist",
            Some(entity),
            None,
            None,
        ));
    }
    let record = entity_mut(document, entity, operation_index)?;
    let previous = record.parent.clone();
    record.parent = parent.cloned();
    Ok(vec![ScenePatchOperation::Reparent {
        entity: entity.clone(),
        parent: previous,
    }])
}

fn set_asset_ref_field(
    document: &mut SceneDocument,
    target: FieldPatchTarget<'_>,
    asset_ref: &AssetRef,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let FieldPatchTarget {
        registry,
        entity,
        component,
        component_version,
        path,
        operation_index,
    } = target;
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        path,
        operation_index,
        entity,
    )?;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        path,
        operation_index,
        entity,
    )?;
    require_field_capability(
        field_schema,
        ComponentCapability::AssetRef,
        component,
        path,
        operation_index,
        entity,
    )?;
    if field_schema.value_kind != ComponentValueKind::AssetRef {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-invalid-field-kind",
            "patch sets an asset reference into a field that is not registered as an asset reference",
            Some(entity),
            Some(component),
            Some(path),
        ));
    }
    set_field(document, target, asset_ref_value(asset_ref))
}

fn find_entity(document: &SceneDocument, id: &SceneEntityId) -> Option<usize> {
    document.entities.iter().position(|record| record.id == *id)
}

fn entity_mut<'a>(
    document: &'a mut SceneDocument,
    id: &SceneEntityId,
    operation_index: usize,
) -> Result<&'a mut SceneEntityRecord, Diagnostic> {
    let Some(index) = find_entity(document, id) else {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-missing-entity",
            "patch targets an entity that does not exist",
            Some(id),
            None,
            None,
        ));
    };
    Ok(&mut document.entities[index])
}

fn component_mut<'a>(
    document: &'a mut SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    operation_index: usize,
) -> Result<&'a mut SceneComponentRecord, Diagnostic> {
    let entity = entity_mut(document, entity, operation_index)?;
    entity.components.get_mut(component).ok_or_else(|| {
        patch_diagnostic(
            operation_index,
            "scene.patch-missing-component",
            "patch targets a component that does not exist",
            Some(&entity.id),
            Some(component),
            None,
        )
    })
}

fn field_schema<'a>(
    registry: &'a ComponentRegistry,
    component: &ComponentTypeId,
    component_version: &ComponentSchemaVersion,
    path: &ComponentFieldPath,
    operation_index: usize,
    entity: &SceneEntityId,
) -> Result<&'a ComponentFieldSchema, Diagnostic> {
    let Some(component_schema) = registry.schema(component) else {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-unknown-component",
            "patch targets a component type that is not registered",
            Some(entity),
            Some(component),
            Some(path),
        ));
    };

    if component_schema.version != *component_version {
        return Err(with_public_u64(
            with_public_u64(
                patch_diagnostic(
                    operation_index,
                    "scene.patch-stale-component-schema-version",
                    "Patch component schema version is stale",
                    Some(entity),
                    Some(component),
                    Some(path),
                ),
                "actual-version",
                u64::from(component_version.0),
            ),
            "expected-version",
            u64::from(component_schema.version.0),
        ));
    }

    component_schema
        .fields
        .iter()
        .find(|field| field.path == *path)
        .ok_or_else(|| {
            patch_diagnostic(
                operation_index,
                "scene.patch-unknown-field",
                "patch targets a field path that is not registered in the component schema",
                Some(entity),
                Some(component),
                Some(path),
            )
        })
}

fn require_field_capability(
    field_schema: &ComponentFieldSchema,
    capability: ComponentCapability,
    component: &ComponentTypeId,
    path: &ComponentFieldPath,
    operation_index: usize,
    entity: &SceneEntityId,
) -> Result<(), Diagnostic> {
    if field_schema.has_capability(capability) {
        return Ok(());
    }

    Err(with_capability(
        patch_diagnostic(
            operation_index,
            "scene.patch-field-capability-missing",
            "Patch field operation requires a missing capability",
            Some(entity),
            Some(component),
            Some(path),
        ),
        capability,
    ))
}

fn field_value_matches(field: &ComponentFieldSchema, value: &ComponentValue) -> bool {
    if !field.required && matches!(value, ComponentValue::Null) {
        return true;
    }
    match field.value_kind {
        ComponentValueKind::AssetRef => is_asset_ref_value(value),
        expected => value.kind() == expected,
    }
}

fn is_asset_ref_value(value: &ComponentValue) -> bool {
    let ComponentValue::Map(fields) = value else {
        return false;
    };
    matches!(
        (fields.get("kind"), fields.get("value")),
        (Some(ComponentValue::String(kind)), Some(ComponentValue::String(_)))
            if kind == "path" || kind == "stable_id"
    )
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn subtree_ids(document: &SceneDocument, root: &SceneEntityId) -> BTreeSet<SceneEntityId> {
    let mut subtree = BTreeSet::from([root.clone()]);
    let mut changed = true;
    while changed {
        changed = false;
        for entity in &document.entities {
            if let Some(parent) = &entity.parent
                && subtree.contains(parent)
                && subtree.insert(entity.id.clone())
            {
                changed = true;
            }
        }
    }
    subtree
}

fn subtree_depth(records: &[SceneEntityRecord], id: &SceneEntityId) -> usize {
    let mut depth = 0;
    let mut current = records
        .iter()
        .find(|record| record.id == *id)
        .and_then(|record| record.parent.as_ref());
    while let Some(parent) = current {
        let Some(parent_record) = records.iter().find(|record| record.id == *parent) else {
            break;
        };
        depth += 1;
        current = parent_record.parent.as_ref();
    }
    depth
}

fn patch_diagnostic(
    operation_index: usize,
    code: &'static str,
    summary: &'static str,
    entity: Option<&SceneEntityId>,
    component: Option<&ComponentTypeId>,
    path: Option<&ComponentFieldPath>,
) -> Diagnostic {
    let mut diagnostic = with_public_u64(
        diagnostic_error(code, summary),
        "operation-index",
        u64::try_from(operation_index).unwrap_or(u64::MAX),
    );
    if let Some(entity) = entity {
        diagnostic = with_public_locator(diagnostic, "entity-id", entity.as_str());
    }
    if let Some(component) = component {
        diagnostic = with_public_locator(diagnostic, "component-id", component.as_str());
    }
    if let Some(path) = path {
        diagnostic = with_component_field_path(diagnostic, "field-path", "field-path-depth", path);
    }
    diagnostic
}
