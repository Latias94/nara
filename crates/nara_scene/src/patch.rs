use std::collections::BTreeMap;

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentProjectionError, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueCost, ComponentValueKind,
};

use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, with_capability, with_component_field_path,
        with_component_field_path_error, with_public_locator, with_public_u64,
    },
};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ScenePatchDocument {
    pub format_version: u32,
    pub operations: Vec<ScenePatchOperation>,
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenePatchDocumentWire {
    pub(crate) format_version: u32,
    pub(crate) operations: Vec<ScenePatchOperation>,
}

#[cfg(feature = "serde")]
impl Default for ScenePatchDocumentWire {
    fn default() -> Self {
        Self {
            format_version: ScenePatchDocument::CURRENT_FORMAT_VERSION,
            operations: Vec::new(),
        }
    }
}

#[cfg(feature = "serde")]
impl ScenePatchDocumentWire {
    pub(crate) fn into_document(self) -> ScenePatchDocument {
        ScenePatchDocument {
            format_version: self.format_version,
            operations: self.operations,
        }
    }
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

    pub(crate) fn unsupported_format_version(&self) -> Option<u32> {
        unsupported_patch_tree_format_version(self)
    }

    pub fn apply_to_scene(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
    ) -> ScenePatchReport {
        self.apply_to_scene_with_limits(document, registry, ScenePatchApplyLimits::default())
    }

    pub fn apply_to_scene_with_limits(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        limits: ScenePatchApplyLimits,
    ) -> ScenePatchReport {
        self.apply_to_scene_with_validator(
            document,
            registry,
            limits,
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
        self.apply_to_scene_with_asset_database_and_limits(
            document,
            registry,
            database,
            ScenePatchApplyLimits::default(),
        )
    }

    pub fn apply_to_scene_with_asset_database_and_limits(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        limits: ScenePatchApplyLimits,
    ) -> ScenePatchReport {
        self.apply_to_scene_with_validator(
            document,
            registry,
            limits,
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
        limits: ScenePatchApplyLimits,
        mut validate: impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> ScenePatchReport {
        if let Some(format_version) = self.unsupported_format_version() {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(with_public_u64(
                with_public_u64(
                    diagnostic_error(
                        "scene.patch-unsupported-format-version",
                        "Scene patch format version is unsupported",
                    ),
                    "actual-version",
                    u64::from(format_version),
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

        let source_work = scene_validation_work(document);
        if let Some(diagnostic) = validation_work_budget_diagnostic(source_work, limits, None) {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
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
        let mut validation_work = source_work;
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

            let operation_work = scene_validation_work(&scratch);
            let observed = validation_work.saturating_add(operation_work);
            if let Some(diagnostic) =
                validation_work_budget_diagnostic(observed, limits, Some(operation_index))
            {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(diagnostic);
                return ScenePatchReport {
                    applied: false,
                    inverse: None,
                    diagnostics,
                };
            }
            validation_work = observed;
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
        field: ComponentFieldId,
        value: ComponentValue,
    },
    RemoveField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
    },
    Reparent {
        entity: SceneEntityId,
        parent: Option<SceneEntityId>,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        asset_ref: AssetRef,
    },
}

enum PatchTreeNode<'a> {
    Entity(&'a SceneEntityRecord),
    Patch(&'a ScenePatchDocument),
}

fn unsupported_patch_tree_format_version(patch: &ScenePatchDocument) -> Option<u32> {
    let mut pending = vec![PatchTreeNode::Patch(patch)];
    while let Some(node) = pending.pop() {
        match node {
            PatchTreeNode::Entity(entity) => {
                if let Some(prefab) = &entity.prefab {
                    pending.push(PatchTreeNode::Patch(&prefab.overrides));
                }
            }
            PatchTreeNode::Patch(patch) => {
                if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION {
                    return Some(patch.format_version);
                }
                pending.extend(
                    patch
                        .operations
                        .iter()
                        .filter_map(|operation| match operation {
                            ScenePatchOperation::AddEntity { entity } => {
                                Some(PatchTreeNode::Entity(entity))
                            }
                            _ => None,
                        }),
                );
            }
        }
    }
    None
}

enum ValueCostNode<'a> {
    Entity(&'a SceneEntityRecord),
    Patch(&'a ScenePatchDocument),
    Value(&'a ComponentValue),
}

pub(crate) fn scene_entity_component_value_cost(entity: &SceneEntityRecord) -> ComponentValueCost {
    entity
        .components
        .values()
        .fold(ComponentValueCost::ZERO, |cost, component| {
            cost.saturating_add(component.value.cost())
        })
}

pub(crate) fn scene_entities_source_value_cost(
    entities: &[SceneEntityRecord],
) -> ComponentValueCost {
    value_cost(entities.iter().map(ValueCostNode::Entity).collect())
}

pub(crate) fn scene_patch_value_cost(patch: &ScenePatchDocument) -> ComponentValueCost {
    value_cost(vec![ValueCostNode::Patch(patch)])
}

fn value_cost(mut pending: Vec<ValueCostNode<'_>>) -> ComponentValueCost {
    let mut cost = ComponentValueCost::ZERO;
    while let Some(node) = pending.pop() {
        match node {
            ValueCostNode::Entity(entity) => {
                cost = cost.saturating_add(scene_entity_component_value_cost(entity));
                if let Some(prefab) = &entity.prefab {
                    pending.push(ValueCostNode::Patch(&prefab.overrides));
                }
            }
            ValueCostNode::Patch(patch) => {
                for operation in &patch.operations {
                    match operation {
                        ScenePatchOperation::AddEntity { entity } => {
                            pending.push(ValueCostNode::Entity(entity));
                        }
                        ScenePatchOperation::AddComponent { value, .. }
                        | ScenePatchOperation::ReplaceComponent { value, .. } => {
                            pending.push(ValueCostNode::Value(&value.value));
                        }
                        ScenePatchOperation::SetField { value, .. } => {
                            pending.push(ValueCostNode::Value(value));
                        }
                        ScenePatchOperation::SetAssetRefField { asset_ref, .. } => {
                            cost = cost.saturating_add(asset_ref_value(asset_ref).cost());
                        }
                        ScenePatchOperation::RemoveEntity { .. }
                        | ScenePatchOperation::RemoveComponent { .. }
                        | ScenePatchOperation::RemoveField { .. }
                        | ScenePatchOperation::Reparent { .. } => {}
                    }
                }
            }
            ValueCostNode::Value(value) => {
                cost = cost.saturating_add(value.cost());
            }
        }
    }
    cost
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePatchReport {
    pub applied: bool,
    pub inverse: Option<ScenePatchDocument>,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenePatchApplyLimits {
    validation_work: ItemLimit,
    validation_value_nodes: ItemLimit,
    validation_value_bytes: ByteLimit,
}

impl Default for ScenePatchApplyLimits {
    fn default() -> Self {
        Self {
            validation_work: ItemLimit::new(5_000_000)
                .expect("default scene patch validation work limit is non-zero"),
            validation_value_nodes: ItemLimit::new(5_000_000)
                .expect("default scene patch value node work limit is non-zero"),
            validation_value_bytes: ByteLimit::new(64 * 1024 * 1024)
                .expect("default scene patch value byte work limit is non-zero"),
        }
    }
}

impl ScenePatchApplyLimits {
    #[must_use]
    pub const fn validation_work(self) -> ItemLimit {
        self.validation_work
    }

    #[must_use]
    pub const fn validation_value_nodes(self) -> ItemLimit {
        self.validation_value_nodes
    }

    #[must_use]
    pub const fn validation_value_bytes(self) -> ByteLimit {
        self.validation_value_bytes
    }

    #[must_use]
    pub const fn with_validation_work(mut self, limit: ItemLimit) -> Self {
        self.validation_work = limit;
        self
    }

    #[must_use]
    pub const fn with_validation_value_nodes(mut self, limit: ItemLimit) -> Self {
        self.validation_value_nodes = limit;
        self
    }

    #[must_use]
    pub const fn with_validation_value_bytes(mut self, limit: ByteLimit) -> Self {
        self.validation_value_bytes = limit;
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SceneValidationWork {
    structural_items: usize,
    value_cost: ComponentValueCost,
}

impl SceneValidationWork {
    fn saturating_add(self, other: Self) -> Self {
        Self {
            structural_items: self.structural_items.saturating_add(other.structural_items),
            value_cost: self.value_cost.saturating_add(other.value_cost),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenePatchValidationBudgetKind {
    StructuralItems,
    ValueNodes,
    ValueBytes,
}

impl ScenePatchValidationBudgetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::StructuralItems => "structural-items",
            Self::ValueNodes => "value-nodes",
            Self::ValueBytes => "value-bytes",
        }
    }
}

fn scene_validation_work(document: &SceneDocument) -> SceneValidationWork {
    SceneValidationWork {
        structural_items: document.entities.iter().fold(1_usize, |work, entity| {
            work.saturating_add(1)
                .saturating_add(entity.components.len())
        }),
        value_cost: scene_entities_source_value_cost(&document.entities),
    }
}

fn validation_work_budget_diagnostic(
    work: SceneValidationWork,
    limits: ScenePatchApplyLimits,
    operation_index: Option<usize>,
) -> Option<Diagnostic> {
    let (kind, observed, maximum) = [
        (
            ScenePatchValidationBudgetKind::StructuralItems,
            work.structural_items,
            limits.validation_work().get(),
        ),
        (
            ScenePatchValidationBudgetKind::ValueNodes,
            work.value_cost.nodes(),
            limits.validation_value_nodes().get(),
        ),
        (
            ScenePatchValidationBudgetKind::ValueBytes,
            work.value_cost.logical_bytes(),
            limits.validation_value_bytes().get(),
        ),
    ]
    .into_iter()
    .find(|(_, observed, maximum)| observed > maximum)?;
    let diagnostic = match operation_index {
        Some(operation_index) => patch_diagnostic(
            operation_index,
            "scene.patch-validation-work-budget-exceeded",
            "Patch validation work budget was exceeded",
            None,
            None,
            None,
        ),
        None => diagnostic_error(
            "scene.patch-validation-work-budget-exceeded",
            "Patch validation work budget was exceeded",
        ),
    };
    Some(with_public_locator(
        with_public_u64(
            with_public_u64(
                diagnostic,
                "observed",
                u64::try_from(observed).unwrap_or(u64::MAX),
            ),
            "maximum",
            u64::try_from(maximum).unwrap_or(u64::MAX),
        ),
        "budget-kind",
        kind.as_str(),
    ))
}

#[derive(Clone, Copy)]
struct FieldPatchTarget<'a> {
    registry: &'a ComponentRegistry,
    entity: &'a SceneEntityId,
    component: &'a ComponentTypeId,
    component_version: &'a ComponentSchemaVersion,
    field: &'a ComponentFieldId,
    operation_index: usize,
}

impl<'a> FieldPatchTarget<'a> {
    fn new(
        registry: &'a ComponentRegistry,
        entity: &'a SceneEntityId,
        component: &'a ComponentTypeId,
        component_version: &'a ComponentSchemaVersion,
        field: &'a ComponentFieldId,
        operation_index: usize,
    ) -> Self {
        Self {
            registry,
            entity,
            component,
            component_version,
            field,
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
        ScenePatchOperation::AddEntity { entity } => {
            add_entity(document, registry, entity, operation_index)
        }
        ScenePatchOperation::RemoveEntity { entity } => {
            remove_entity_subtree(document, registry, entity, operation_index)
        }
        ScenePatchOperation::AddComponent {
            entity,
            component,
            value,
        } => add_component(
            document,
            registry,
            entity,
            component,
            value,
            operation_index,
        ),
        ScenePatchOperation::RemoveComponent { entity, component } => {
            remove_component(document, registry, entity, component, operation_index)
        }
        ScenePatchOperation::ReplaceComponent {
            entity,
            component,
            value,
        } => replace_component(
            document,
            registry,
            entity,
            component,
            value,
            operation_index,
        ),
        ScenePatchOperation::SetField {
            entity,
            component,
            component_version,
            field,
            value,
        } => set_field(
            document,
            FieldPatchTarget::new(
                registry,
                entity,
                component,
                component_version,
                field,
                operation_index,
            ),
            value.clone(),
        ),
        ScenePatchOperation::RemoveField {
            entity,
            component,
            component_version,
            field,
        } => remove_field(
            document,
            registry,
            entity,
            component,
            component_version,
            field,
            operation_index,
        ),
        ScenePatchOperation::Reparent { entity, parent } => {
            reparent_entity(document, entity, parent.as_ref(), operation_index)
        }
        ScenePatchOperation::SetAssetRefField {
            entity,
            component,
            component_version,
            field,
            asset_ref,
        } => set_asset_ref_field(
            document,
            FieldPatchTarget::new(
                registry,
                entity,
                component,
                component_version,
                field,
                operation_index,
            ),
            asset_ref,
        ),
    }
}

fn add_entity(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
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
    for component in entity.components.keys() {
        require_whole_component_edit(registry, &entity.id, component, operation_index)?;
    }

    document.entities.push(entity.clone());
    Ok(vec![ScenePatchOperation::RemoveEntity {
        entity: entity.id.clone(),
    }])
}

fn remove_entity_subtree(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
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

    let subtree = subtree_depths(document, entity);
    for record in document
        .entities
        .iter()
        .filter(|record| subtree.contains_key(&record.id))
    {
        for component in record.components.keys() {
            require_whole_component_edit(registry, &record.id, component, operation_index)?;
        }
    }
    let mut removed = Vec::new();
    document.entities.retain(|record| {
        if subtree.contains_key(&record.id) {
            removed.push(record.clone());
            false
        } else {
            true
        }
    });
    removed.sort_by_key(|record| subtree.get(&record.id).copied().unwrap_or(usize::MAX));

    Ok(removed
        .into_iter()
        .map(|entity| ScenePatchOperation::AddEntity { entity })
        .collect())
}

fn add_component(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    value: &SceneComponentRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    require_whole_component_edit(registry, entity, component, operation_index)?;
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
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    require_whole_component_edit(registry, entity, component, operation_index)?;
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
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    value: &SceneComponentRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    require_whole_component_edit(registry, entity, component, operation_index)?;
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
        field,
        operation_index,
    } = target;
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        field,
        operation_index,
        entity,
    )?;
    let path = &field_schema.path;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        field,
        path,
        operation_index,
        entity,
    )?;
    if !field_value_matches(field_schema, &value) {
        return Err(with_component_field_id(
            patch_diagnostic(
                operation_index,
                "scene.patch-invalid-field-kind",
                "patch field value kind does not match registered schema",
                Some(entity),
                Some(component),
                Some(path),
            ),
            field,
        ));
    }

    let component_record = component_mut_for_field(
        document,
        entity,
        component,
        *component_version,
        field,
        path,
        operation_index,
    )?;
    let previous = component_record
        .value
        .set_path(path, value)
        .map_err(|error| {
            with_component_field_id(
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
                ),
                field,
            )
        })?;

    Ok(vec![match previous {
        Some(value) => ScenePatchOperation::SetField {
            entity: entity.clone(),
            component: component.clone(),
            component_version: *component_version,
            field: field.clone(),
            value,
        },
        None => ScenePatchOperation::RemoveField {
            entity: entity.clone(),
            component: component.clone(),
            component_version: *component_version,
            field: field.clone(),
        },
    }])
}

fn remove_field(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    component_version: &ComponentSchemaVersion,
    field: &ComponentFieldId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        field,
        operation_index,
        entity,
    )?;
    let path = &field_schema.path;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        field,
        path,
        operation_index,
        entity,
    )?;
    if field_schema.required && field_schema.default_value.is_none() {
        return Err(with_component_field_id(
            patch_diagnostic(
                operation_index,
                "scene.patch-required-field-removal",
                "patch removes a required component field without a registered default",
                Some(entity),
                Some(component),
                Some(path),
            ),
            field,
        ));
    }

    let component_record = component_mut_for_field(
        document,
        entity,
        component,
        *component_version,
        field,
        path,
        operation_index,
    )?;
    let previous = component_record.value.remove_path(path).map_err(|error| {
        with_component_field_id(
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
            ),
            field,
        )
    })?;

    Ok(vec![ScenePatchOperation::SetField {
        entity: entity.clone(),
        component: component.clone(),
        component_version: *component_version,
        field: field.clone(),
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
        field,
        operation_index,
    } = target;
    let field_schema = field_schema(
        registry,
        component,
        component_version,
        field,
        operation_index,
        entity,
    )?;
    let path = &field_schema.path;
    require_field_capability(
        field_schema,
        ComponentCapability::Edit,
        component,
        field,
        path,
        operation_index,
        entity,
    )?;
    require_field_capability(
        field_schema,
        ComponentCapability::AssetRef,
        component,
        field,
        path,
        operation_index,
        entity,
    )?;
    if field_schema.value_kind != ComponentValueKind::AssetRef {
        return Err(with_component_field_id(
            patch_diagnostic(
                operation_index,
                "scene.patch-invalid-field-kind",
                "patch sets an asset reference into a field that is not registered as an asset reference",
                Some(entity),
                Some(component),
                Some(path),
            ),
            field,
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

fn component_mut_for_field<'a>(
    document: &'a mut SceneDocument,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    expected_version: ComponentSchemaVersion,
    field: &ComponentFieldId,
    path: &ComponentFieldPath,
    operation_index: usize,
) -> Result<&'a mut SceneComponentRecord, Diagnostic> {
    let record = component_mut(document, entity, component, operation_index)?;
    if record.version == expected_version {
        return Ok(record);
    }

    Err(with_component_field_id(
        with_public_u64(
            with_public_u64(
                patch_diagnostic(
                    operation_index,
                    "scene.patch-target-component-version-mismatch",
                    "Patch field operation requires a target record at the current schema version",
                    Some(entity),
                    Some(component),
                    Some(path),
                ),
                "actual-version",
                u64::from(record.version.get()),
            ),
            "expected-version",
            u64::from(expected_version.get()),
        ),
        field,
    ))
}

fn require_whole_component_edit(
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    operation_index: usize,
) -> Result<(), Diagnostic> {
    match registry.validate_whole_value_capabilities(
        component,
        [ComponentCapability::Scene, ComponentCapability::Edit],
    ) {
        Ok(()) => Ok(()),
        // Let the normal document validator aggregate every unknown component instead of
        // truncating a multi-component failure to the first structural operation target.
        Err(ComponentProjectionError::UnknownComponentId(_)) => Ok(()),
        Err(ComponentProjectionError::RegistryNotFrozen) => Err(patch_diagnostic(
            operation_index,
            "scene.patch-registry-not-frozen",
            "Patch application requires a frozen component registry",
            Some(entity),
            Some(component),
            None,
        )),
        Err(ComponentProjectionError::MissingComponentCapability { capability, .. }) => {
            Err(with_capability(
                patch_diagnostic(
                    operation_index,
                    "scene.patch-component-capability-missing",
                    "Whole-component patch operation requires a missing capability",
                    Some(entity),
                    Some(component),
                    None,
                ),
                capability,
            ))
        }
        Err(ComponentProjectionError::ProjectionRequired {
            field_id,
            capability,
            ..
        }) => Err(with_component_field_id(
            with_capability(
                patch_diagnostic(
                    operation_index,
                    "scene.patch-component-capability-missing",
                    "Whole-component patch operation requires every field to carry the capability",
                    Some(entity),
                    Some(component),
                    None,
                ),
                capability,
            ),
            &field_id,
        )),
    }
}

fn field_schema<'a>(
    registry: &'a ComponentRegistry,
    component: &ComponentTypeId,
    component_version: &ComponentSchemaVersion,
    field: &ComponentFieldId,
    operation_index: usize,
    entity: &SceneEntityId,
) -> Result<&'a ComponentFieldSchema, Diagnostic> {
    let Some(component_schema) = registry.schema(component) else {
        return Err(with_component_field_id(
            patch_diagnostic(
                operation_index,
                "scene.patch-unknown-component",
                "patch targets a component type that is not registered",
                Some(entity),
                Some(component),
                None,
            ),
            field,
        ));
    };
    let field_schema = registry.resolve_field(component, field);
    let path = field_schema.map(|field| &field.path);

    if component_schema.version != *component_version {
        return Err(with_component_field_id(
            with_public_u64(
                with_public_u64(
                    patch_diagnostic(
                        operation_index,
                        "scene.patch-stale-component-schema-version",
                        "Patch component schema version is stale",
                        Some(entity),
                        Some(component),
                        path,
                    ),
                    "actual-version",
                    u64::from(component_version.0),
                ),
                "expected-version",
                u64::from(component_schema.version.0),
            ),
            field,
        ));
    }

    field_schema.ok_or_else(|| {
        with_component_field_id(
            patch_diagnostic(
                operation_index,
                "scene.patch-unknown-field",
                "patch targets a field ID that is not registered in the component schema",
                Some(entity),
                Some(component),
                None,
            ),
            field,
        )
    })
}

fn require_field_capability(
    field_schema: &ComponentFieldSchema,
    capability: ComponentCapability,
    component: &ComponentTypeId,
    field: &ComponentFieldId,
    path: &ComponentFieldPath,
    operation_index: usize,
    entity: &SceneEntityId,
) -> Result<(), Diagnostic> {
    if field_schema.has_capability(capability) {
        return Ok(());
    }

    Err(with_component_field_id(
        with_capability(
            patch_diagnostic(
                operation_index,
                "scene.patch-field-capability-missing",
                "Patch field operation requires a missing capability",
                Some(entity),
                Some(component),
                Some(path),
            ),
            capability,
        ),
        field,
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

fn subtree_depths(
    document: &SceneDocument,
    root: &SceneEntityId,
) -> BTreeMap<SceneEntityId, usize> {
    let mut children = BTreeMap::<SceneEntityId, Vec<SceneEntityId>>::new();
    for entity in &document.entities {
        if let Some(parent) = &entity.parent {
            children
                .entry(parent.clone())
                .or_default()
                .push(entity.id.clone());
        }
    }

    let mut depths = BTreeMap::from([(root.clone(), 0_usize)]);
    let mut pending = vec![root.clone()];
    while let Some(parent) = pending.pop() {
        let depth = depths.get(&parent).copied().unwrap_or(0);
        if let Some(child_ids) = children.get(&parent) {
            for child in child_ids {
                if depths.contains_key(child) {
                    continue;
                }
                depths.insert(child.clone(), depth.saturating_add(1));
                pending.push(child.clone());
            }
        }
    }
    depths
}

fn with_component_field_id(diagnostic: Diagnostic, field: &ComponentFieldId) -> Diagnostic {
    with_public_locator(diagnostic, "field-id", field.as_str())
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
