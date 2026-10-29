#[cfg(feature = "serde")]
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::{
    ComponentCapability, ComponentDecodeContext, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentProjectionError, ComponentRegistry, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueCost, ComponentValueKind,
    ComponentValueMaterializationLimits,
};

use crate::{
    PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, with_capability, with_component_field_path,
        with_component_field_path_error, with_public_locator, with_public_u64,
    },
    provenance::{CostScratchAdmission, reserve_cost_scratch},
    validation::{
        preflight_authoring_scene_with_context_at_operation,
        validate_patch_intermediate_scene_with_limits,
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
        (self.format_version != Self::CURRENT_FORMAT_VERSION).then_some(self.format_version)
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
            |document, registry, operation_index| {
                validate_patch_candidate(document, registry, None, operation_index, limits)
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
            |document, registry, operation_index| {
                validate_patch_candidate(
                    document,
                    registry,
                    Some(database),
                    operation_index,
                    limits,
                )
            },
        )
    }

    pub(crate) fn prepare_to_scene(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
    ) -> Result<PreparedScenePatch, ScenePatchReport> {
        self.prepare_to_scene_with_validator(
            document,
            registry,
            ScenePatchApplyLimits::default(),
            |document, registry, operation_index| match operation_index {
                Some(operation_index) => {
                    document.validate_authoring_for_patch(registry, operation_index)
                }
                None => document.validate_authoring(registry),
            },
        )
    }

    pub(crate) fn prepare_to_scene_with_asset_database(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<PreparedScenePatch, ScenePatchReport> {
        self.prepare_to_scene_with_validator(
            document,
            registry,
            ScenePatchApplyLimits::default(),
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

    pub(crate) fn prepare_to_prefab(
        &self,
        document: &PrefabDocument,
        registry: &ComponentRegistry,
    ) -> Result<PreparedPrefabPatch, ScenePatchReport> {
        self.prepare_to_prefab_with_validator(document, registry, |document, registry, index| {
            match index {
                Some(index) => document
                    .instantiate()
                    .validate_authoring_for_patch(registry, index),
                None => document.instantiate().validate_authoring(registry),
            }
        })
    }

    pub(crate) fn prepare_to_prefab_with_asset_database(
        &self,
        document: &PrefabDocument,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
    ) -> Result<PreparedPrefabPatch, ScenePatchReport> {
        self.prepare_to_prefab_with_validator(document, registry, |document, registry, index| {
            match index {
                Some(index) => document
                    .instantiate()
                    .validate_authoring_for_patch_with_asset_database(registry, database, index),
                None => document
                    .instantiate()
                    .validate_authoring_with_asset_database(registry, database),
            }
        })
    }

    fn prepare_to_prefab_with_validator(
        &self,
        document: &PrefabDocument,
        registry: &ComponentRegistry,
        mut validate: impl FnMut(&PrefabDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> Result<PreparedPrefabPatch, ScenePatchReport> {
        let limits = ScenePatchApplyLimits::default();
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
            return Err(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            });
        }

        let mut validation_work = prefab_validation_work(document);
        if let Some(diagnostic) = validation_work_budget_diagnostic(validation_work, limits, None) {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
            return Err(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            });
        }
        self.admit_input_cost(limits)?;
        let source_validation = validate(document, registry, None);
        if source_validation.has_errors() {
            return Err(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics: source_validation,
            });
        }

        let mut scratch = document.clone();
        let mut inverse_groups = Vec::<Vec<ScenePatchOperation>>::new();
        for (operation_index, operation) in self.operations.iter().enumerate() {
            let inverse =
                apply_prefab_operation(&mut scratch, registry, operation, operation_index);
            let inverse = match inverse {
                Ok(inverse) => inverse,
                Err(diagnostic) => {
                    let mut diagnostics = DiagnosticReport::default();
                    diagnostics.push(diagnostic);
                    return Err(ScenePatchReport {
                        applied: false,
                        inverse: None,
                        diagnostics,
                    });
                }
            };

            let operation_work = prefab_validation_work(&scratch);
            let observed = validation_work.saturating_add(operation_work);
            if let Some(diagnostic) =
                validation_work_budget_diagnostic(observed, limits, Some(operation_index))
            {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(diagnostic);
                return Err(ScenePatchReport {
                    applied: false,
                    inverse: None,
                    diagnostics,
                });
            }
            validation_work = observed;
            let validation = if operation_index + 1 == self.operations.len() {
                scratch.canonicalize();
                validate(&scratch, registry, Some(operation_index))
            } else {
                validate_patch_intermediate_scene_with_limits(
                    &scratch.instantiate(),
                    registry,
                    operation_index,
                    limits.materialization_limits(),
                )
            };
            if validation.has_errors() {
                return Err(ScenePatchReport {
                    applied: false,
                    inverse: None,
                    diagnostics: validation,
                });
            }
            inverse_groups.push(inverse);
        }

        scratch.canonicalize();
        Ok(PreparedPrefabPatch {
            document: scratch,
            report: ScenePatchReport {
                applied: true,
                inverse: Some(ScenePatchDocument::new(
                    inverse_groups.into_iter().rev().flatten(),
                )),
                diagnostics: DiagnosticReport::default(),
            },
        })
    }

    fn apply_to_scene_with_validator(
        &self,
        document: &mut SceneDocument,
        registry: &ComponentRegistry,
        limits: ScenePatchApplyLimits,
        mut validate: impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> ScenePatchReport {
        match self.prepare_to_scene_with_validator(document, registry, limits, &mut validate) {
            Ok(prepared) => prepared.publish_into(document),
            Err(report) => report,
        }
    }

    fn prepare_to_scene_with_validator(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
        limits: ScenePatchApplyLimits,
        mut validate: impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> Result<PreparedScenePatch, ScenePatchReport> {
        let source_work =
            match self.validate_patch_source(document, registry, limits, &mut validate) {
                Ok(source_work) => source_work,
                Err(report) => return Err(*report),
            };

        let mut scratch = document.clone();
        let mut inverse_groups = Vec::<Vec<ScenePatchOperation>>::new();
        let mut validation_work = source_work;
        for (operation_index, operation) in self.operations.iter().enumerate() {
            let inverse =
                match apply_operation_for_patch(&mut scratch, registry, operation, operation_index)
                {
                    Ok(inverse) => inverse,
                    Err(report) => return Err(*report),
                };
            if let Err(report) = validate_applied_operation(
                &mut scratch,
                registry,
                operation_index,
                &mut validation_work,
                limits,
                operation_index + 1 == self.operations.len(),
                &mut validate,
            ) {
                return Err(*report);
            }

            inverse_groups.push(inverse);
        }

        scratch.canonicalize();

        let inverse_operations: Vec<ScenePatchOperation> =
            inverse_groups.into_iter().rev().flatten().collect();

        Ok(PreparedScenePatch {
            document: scratch,
            report: ScenePatchReport {
                applied: true,
                inverse: Some(ScenePatchDocument::new(inverse_operations)),
                diagnostics: DiagnosticReport::default(),
            },
        })
    }

    fn validate_patch_source(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
        limits: ScenePatchApplyLimits,
        validate: &mut impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
    ) -> Result<SceneValidationWork, Box<ScenePatchReport>> {
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
            return Err(Box::new(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            }));
        }

        let source_work = scene_validation_work(document);
        if let Some(diagnostic) = validation_work_budget_diagnostic(source_work, limits, None) {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
            return Err(Box::new(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            }));
        }

        self.admit_input_cost(limits).map_err(Box::new)?;
        let source_validation = validate(document, registry, None);
        if source_validation.has_errors() {
            return Err(Box::new(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics: source_validation,
            }));
        }

        Ok(source_work)
    }

    fn admit_input_cost(&self, limits: ScenePatchApplyLimits) -> Result<(), ScenePatchReport> {
        if bounded_scene_patch_cost(self, limits).is_some() {
            return Ok(());
        }
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic_error(
            "scene.patch-validation-work-budget-exceeded",
            "Patch input exceeds the replay data or depth budget",
        ));
        Err(ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics,
        })
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
        placement: ScenePlacement,
    },
    RemoveEntity {
        entity: SceneEntityId,
        from: ScenePlacement,
    },
    RestoreEntity {
        entity: SceneEntityRecord,
        descendants: Vec<SceneEntityRecord>,
        placement: ScenePlacement,
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
    MoveEntity {
        entity: SceneEntityId,
        from: ScenePlacement,
        to: ScenePlacement,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        asset_ref: AssetRef,
    },
    /// Replaces source-owned instance intent after comparing its complete prior declaration.
    /// This does not resolve the Prefab or convert its generated projection into source data.
    SetPrefabInstance {
        entity: SceneEntityId,
        expected: Option<PrefabInstance>,
        value: Option<PrefabInstance>,
    },
}

/// The ordered collection that owns one entity's placement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ScenePlacementOwner {
    Root,
    Parent { parent: SceneEntityId },
}

/// A stable insertion point within one ordered placement owner.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ScenePlacementAnchor {
    First,
    Last,
    Before { sibling: SceneEntityId },
    After { sibling: SceneEntityId },
}

/// A stable root-or-parent owner plus an insertion anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ScenePlacement {
    pub owner: ScenePlacementOwner,
    pub anchor: ScenePlacementAnchor,
}

impl ScenePlacement {
    #[must_use]
    pub const fn first(owner: ScenePlacementOwner) -> Self {
        Self {
            owner,
            anchor: ScenePlacementAnchor::First,
        }
    }

    #[must_use]
    pub const fn last(owner: ScenePlacementOwner) -> Self {
        Self {
            owner,
            anchor: ScenePlacementAnchor::Last,
        }
    }

    #[must_use]
    pub fn before(owner: ScenePlacementOwner, sibling: SceneEntityId) -> Self {
        Self {
            owner,
            anchor: ScenePlacementAnchor::Before { sibling },
        }
    }

    #[must_use]
    pub fn after(owner: ScenePlacementOwner, sibling: SceneEntityId) -> Self {
        Self {
            owner,
            anchor: ScenePlacementAnchor::After { sibling },
        }
    }

    /// Returns the entity's current stable placement in a validated document.
    #[must_use]
    pub fn current(document: &SceneDocument, entity: &SceneEntityId) -> Option<Self> {
        placement_for_entity(document, entity)
    }

    /// Returns the entity's current stable placement in a validated Prefab source.
    #[must_use]
    pub fn current_prefab(document: &PrefabDocument, entity: &SceneEntityId) -> Option<Self> {
        placement_for_entity(document, entity)
    }

    /// Reports whether this placement names the entity's exact current Prefab position.
    #[must_use]
    pub fn matches_current_prefab(
        &self,
        document: &PrefabDocument,
        entity: &SceneEntityId,
    ) -> bool {
        placement_matches(document, entity, self)
    }
}

#[cfg(feature = "serde")]
pub(crate) fn authoring_entity_placements<'a>(
    roots: &[SceneEntityId],
    entities: impl IntoIterator<Item = &'a SceneEntityRecord>,
) -> BTreeMap<SceneEntityId, ScenePlacement> {
    let owners = std::iter::once((ScenePlacementOwner::Root, roots)).chain(
        entities.into_iter().map(|entity| {
            (
                ScenePlacementOwner::Parent {
                    parent: entity.id.clone(),
                },
                entity.children.as_slice(),
            )
        }),
    );
    let mut placements = BTreeMap::new();
    for (owner, siblings) in owners {
        for (index, entity) in siblings.iter().enumerate() {
            let placement = if index == 0 {
                ScenePlacement::first(owner.clone())
            } else {
                ScenePlacement::after(owner.clone(), siblings[index - 1].clone())
            };
            placements.insert(entity.clone(), placement);
        }
    }
    placements
}

enum ValueCostNode<'a> {
    Entity(&'a SceneEntityRecord, usize),
    Instance(&'a PrefabInstance, usize),
    Entities(std::slice::Iter<'a, SceneEntityRecord>, usize),
    Patch(&'a ScenePatchDocument, usize),
    Operations(std::slice::Iter<'a, ScenePatchOperation>, usize),
    Value(&'a ComponentValue, usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScenePatchCost {
    value_cost: ComponentValueCost,
    operation_count: usize,
    structural_logical_bytes: usize,
}

impl ScenePatchCost {
    pub(crate) const fn value_cost(self) -> ComponentValueCost {
        self.value_cost
    }

    pub(crate) const fn operation_count(self) -> usize {
        self.operation_count
    }

    pub(crate) const fn logical_bytes(self) -> usize {
        self.value_cost
            .logical_bytes()
            .saturating_add(self.structural_logical_bytes)
    }
}

pub(crate) fn scene_entities_source_value_cost(
    entities: &[SceneEntityRecord],
) -> ComponentValueCost {
    tree_cost(ValueCostNode::Entities(entities.iter(), 1), None, None)
        .expect("unbounded cost traversal always completes")
        .value_cost()
}

/// Admits raw source records and embedded declarations before recursive copying.
pub(crate) fn bounded_scene_entities_source_cost(
    entities: &[SceneEntityRecord],
    limits: ScenePatchApplyLimits,
) -> Option<ScenePatchCost> {
    tree_cost(
        ValueCostNode::Entities(entities.iter(), 1),
        Some(limits),
        None,
    )
}

/// Reuses the raw source walker, admitting every scratch growth before allocation.
#[cfg(feature = "serde")]
pub(crate) fn bounded_scene_entities_source_cost_with_admission<E>(
    entities: &[SceneEntityRecord],
    limits: ScenePatchApplyLimits,
    admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<Option<ScenePatchCost>, E> {
    admitted_tree_cost(ValueCostNode::Entities(entities.iter(), 1), limits, admit)
}

pub(crate) fn scene_patch_cost(patch: &ScenePatchDocument) -> ScenePatchCost {
    tree_cost(ValueCostNode::Patch(patch, 1), None, None)
        .expect("unbounded cost traversal always completes")
}

pub(crate) fn bounded_scene_patch_cost(
    patch: &ScenePatchDocument,
    limits: ScenePatchApplyLimits,
) -> Option<ScenePatchCost> {
    tree_cost(ValueCostNode::Patch(patch, 1), Some(limits), None)
}

pub(crate) fn bounded_scene_patch_cost_with_admission<E>(
    patch: &ScenePatchDocument,
    limits: ScenePatchApplyLimits,
    admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<Option<ScenePatchCost>, E> {
    admitted_tree_cost(ValueCostNode::Patch(patch, 1), limits, admit)
}

fn admitted_tree_cost<E>(
    first: ValueCostNode<'_>,
    limits: ScenePatchApplyLimits,
    mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<Option<ScenePatchCost>, E> {
    let mut refusal = None;
    let result = {
        let mut callback = |bytes| match admit(bytes) {
            Ok(()) => true,
            Err(error) => {
                refusal = Some(error);
                false
            }
        };
        tree_cost(first, Some(limits), Some(&mut callback))
    };
    match refusal {
        Some(error) => Err(error),
        None => Ok(result),
    }
}

fn tree_cost(
    first: ValueCostNode<'_>,
    limits: Option<ScenePatchApplyLimits>,
    mut admission: Option<&mut CostScratchAdmission<'_>>,
) -> Option<ScenePatchCost> {
    let mut pending = if admission.is_some() {
        let mut pending = Vec::new();
        reserve_cost_scratch(&mut pending, 1, &mut admission)?;
        pending.push(first);
        pending
    } else {
        vec![first]
    };
    let mut cost = ScenePatchCost::default();
    while let Some(node) = pending.pop() {
        match node {
            ValueCostNode::Entity(entity, depth) => {
                admit_data_depth(depth, limits)?;
                cost.operation_count = cost
                    .operation_count
                    .saturating_add(1)
                    .saturating_add(entity.children.len())
                    .saturating_add(entity.components.len());
                if limits
                    .is_some_and(|limits| cost.operation_count > limits.validation_work().get())
                {
                    return None;
                }
                cost.structural_logical_bytes = cost
                    .structural_logical_bytes
                    .saturating_add(entity.id.as_str().len())
                    .saturating_add(entity.children.iter().fold(0usize, |bytes, child| {
                        bytes.saturating_add(child.as_str().len())
                    }))
                    .saturating_add(entity.components.keys().fold(0usize, |bytes, component| {
                        bytes.saturating_add(component.as_str().len())
                    }));
                for component in entity.components.values() {
                    cost.structural_logical_bytes = cost
                        .structural_logical_bytes
                        .saturating_add(component.owner().map_or(0, |owner| owner.as_str().len()));
                    add_patch_value_cost(
                        &mut cost,
                        &component.value,
                        depth + 1,
                        limits,
                        &mut admission,
                    )?;
                }
                if let Some(prefab) = &entity.prefab {
                    reserve_cost_scratch(&mut pending, 1, &mut admission)?;
                    pending.push(ValueCostNode::Instance(prefab, depth + 1));
                }
            }
            ValueCostNode::Instance(instance, depth) => {
                admit_data_depth(depth, limits)?;
                let mut allowance = crate::prefab::PrefabOverlayCostLimits::UNBOUNDED;
                if let Some(limits) = limits {
                    allowance.depth = ComponentValueMaterializationLimits::default()
                        .depth()
                        .get()
                        .checked_sub(depth)?
                        .checked_add(1)?;
                    allowance.nodes = limits
                        .validation_work()
                        .get()
                        .checked_sub(cost.operation_count)?;
                    allowance.value_nodes = limits
                        .validation_value_nodes()
                        .get()
                        .checked_sub(cost.value_cost.nodes())?;
                    allowance.value_bytes = limits
                        .validation_value_bytes()
                        .get()
                        .checked_sub(cost.value_cost.logical_bytes())?;
                }
                let instance_cost = if let Some(admit) = admission.as_deref_mut() {
                    crate::prefab::prefab_instance_cost_with_admission(instance, allowance, admit)
                        .ok()??
                } else {
                    crate::prefab::prefab_instance_cost(instance, allowance).ok()?
                };
                cost.operation_count = cost.operation_count.saturating_add(instance_cost.nodes);
                cost.value_cost = cost.value_cost.saturating_add(instance_cost.value_cost);
                cost.structural_logical_bytes = cost
                    .structural_logical_bytes
                    .saturating_add(instance_cost.structural_logical_bytes);
            }
            ValueCostNode::Entities(mut entities, depth) => {
                if let Some(entity) = entities.next() {
                    reserve_cost_scratch(&mut pending, 2, &mut admission)?;
                    pending.push(ValueCostNode::Entities(entities, depth));
                    pending.push(ValueCostNode::Entity(entity, depth));
                }
            }
            ValueCostNode::Patch(patch, depth) => {
                admit_data_depth(depth, limits)?;
                cost.operation_count = cost.operation_count.saturating_add(patch.operations.len());
                if limits
                    .is_some_and(|limits| cost.operation_count > limits.validation_work().get())
                {
                    return None;
                }
                reserve_cost_scratch(&mut pending, 1, &mut admission)?;
                pending.push(ValueCostNode::Operations(patch.operations.iter(), depth));
            }
            ValueCostNode::Operations(mut operations, depth) => {
                if let Some(operation) = operations.next() {
                    // One continuation plus at most two owned payload frames (RestoreEntity
                    // or the expected/value sides of SetPrefabInstance).
                    reserve_cost_scratch(&mut pending, 3, &mut admission)?;
                    pending.push(ValueCostNode::Operations(operations, depth));
                    match operation {
                        ScenePatchOperation::AddEntity { entity, placement } => {
                            add_placement_cost(&mut cost, placement);
                            pending.push(ValueCostNode::Entity(entity, depth + 1));
                        }
                        ScenePatchOperation::RestoreEntity {
                            entity,
                            descendants,
                            placement,
                        } => {
                            add_placement_cost(&mut cost, placement);
                            pending.push(ValueCostNode::Entity(entity, depth + 1));
                            pending.push(ValueCostNode::Entities(descendants.iter(), depth + 1));
                        }
                        ScenePatchOperation::RemoveEntity { entity, from } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_placement_cost(&mut cost, from);
                        }
                        ScenePatchOperation::AddComponent {
                            entity,
                            component,
                            value,
                        }
                        | ScenePatchOperation::ReplaceComponent {
                            entity,
                            component,
                            value,
                        } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_component_type_id_cost(&mut cost, component);
                            cost.structural_logical_bytes =
                                cost.structural_logical_bytes.saturating_add(
                                    value.owner().map_or(0, |owner| owner.as_str().len()),
                                );
                            pending.push(ValueCostNode::Value(&value.value, depth + 1));
                        }
                        ScenePatchOperation::RemoveComponent { entity, component } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_component_type_id_cost(&mut cost, component);
                        }
                        ScenePatchOperation::SetField {
                            entity,
                            component,
                            field,
                            value,
                            ..
                        } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_component_type_id_cost(&mut cost, component);
                            add_component_field_id_cost(&mut cost, field);
                            pending.push(ValueCostNode::Value(value, depth + 1));
                        }
                        ScenePatchOperation::RemoveField {
                            entity,
                            component,
                            field,
                            ..
                        } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_component_type_id_cost(&mut cost, component);
                            add_component_field_id_cost(&mut cost, field);
                        }
                        ScenePatchOperation::MoveEntity { entity, from, to } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_placement_cost(&mut cost, from);
                            add_placement_cost(&mut cost, to);
                        }
                        ScenePatchOperation::SetAssetRefField {
                            entity,
                            component,
                            field,
                            asset_ref,
                            ..
                        } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            add_component_type_id_cost(&mut cost, component);
                            add_component_field_id_cost(&mut cost, field);
                            // Admit the complete canonical value before encoding, including
                            // the nested source/product maps of an explicit product selection.
                            // The bounded value walker separately admits its iterator scratch.
                            if let Some(admit) = admission.as_deref_mut() {
                                let (nodes, logical_bytes) = asset_ref_value_size(asset_ref);
                                if !admit(logical_bytes.checked_add(nodes * 96)) {
                                    return None;
                                }
                            }
                            add_patch_value_cost(
                                &mut cost,
                                &asset_ref_value(asset_ref),
                                depth + 1,
                                limits,
                                &mut admission,
                            )?;
                        }
                        ScenePatchOperation::SetPrefabInstance {
                            entity,
                            expected,
                            value,
                        } => {
                            add_scene_entity_id_cost(&mut cost, entity);
                            pending.extend(
                                expected
                                    .iter()
                                    .chain(value)
                                    .map(|instance| ValueCostNode::Instance(instance, depth + 1)),
                            );
                        }
                    }
                }
            }
            ValueCostNode::Value(value, depth) => {
                add_patch_value_cost(&mut cost, value, depth, limits, &mut admission)?;
            }
        }
    }
    Some(cost)
}

fn add_patch_value_cost(
    cost: &mut ScenePatchCost,
    value: &ComponentValue,
    depth: usize,
    limits: Option<ScenePatchApplyLimits>,
    admission: &mut Option<&mut CostScratchAdmission<'_>>,
) -> Option<()> {
    let value_cost = match limits {
        Some(limits) if admission.is_some() => value
            .cost_up_to_with_admission(
                limits
                    .validation_value_nodes()
                    .get()
                    .saturating_sub(cost.value_cost.nodes()),
                limits
                    .validation_value_bytes()
                    .get()
                    .saturating_sub(cost.value_cost.logical_bytes()),
                |bytes| {
                    if admission.as_deref_mut().is_some_and(|admit| admit(bytes)) {
                        Ok(())
                    } else {
                        Err(())
                    }
                },
            )
            .ok()??,
        Some(limits) => value.cost_up_to(
            limits
                .validation_value_nodes()
                .get()
                .saturating_sub(cost.value_cost.nodes()),
            limits
                .validation_value_bytes()
                .get()
                .saturating_sub(cost.value_cost.logical_bytes()),
        )?,
        None => value.cost(),
    };
    if limits.is_some() {
        admit_value_depth(value, depth)?;
    }
    cost.value_cost = cost.value_cost.saturating_add(value_cost);
    Some(())
}

// This semantic depth is no stricter than the serialized grammar's depth. It bounds
// programmatically supplied data before derived equality, Clone, or serialization recurse.
fn admit_data_depth(depth: usize, limits: Option<ScenePatchApplyLimits>) -> Option<()> {
    if limits.is_some() && depth > ComponentValueMaterializationLimits::default().depth().get() {
        return None;
    }
    Some(())
}

fn admit_value_depth(value: &ComponentValue, depth: usize) -> Option<()> {
    if depth > ComponentValueMaterializationLimits::default().depth().get() {
        return None;
    }
    match value {
        ComponentValue::List(values) => {
            for value in values {
                admit_value_depth(value, depth + 1)?;
            }
        }
        ComponentValue::Map(values) => {
            for value in values.values() {
                admit_value_depth(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Some(())
}

#[cfg(all(test, feature = "serde"))]
mod bounded_cost_tests {
    use super::*;
    use crate::{
        PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
        PrefabInstanceClaim, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
        PrefabPlacementSlot, SourceDocumentKey, SourceDocumentKind,
    };

    fn source_key() -> SourceDocumentKey {
        SourceDocumentKey::named(
            "6b691227-a68a-4b11-8d1c-a9d5d8c7e952".parse().unwrap(),
            "p",
            SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn deep_declaration(depth: usize, through_entities: bool) -> PrefabInstance {
        let mut declaration = PrefabInstance {
            source: AssetRef::path("p").unwrap(),
            overrides: None,
        };
        for _ in 0..depth {
            let id = SceneEntityId::new("e").unwrap();
            let mut overlay = PrefabOverlay::new(source_key());
            if through_entities {
                let mut addition = PrefabAddition::new(id.clone());
                addition.prefab = Some(declaration);
                overlay.additions.push(addition);
                overlay.placements.push(PrefabPlacement {
                    target: PrefabEntityTarget::addition(id),
                    parent: PrefabPlacementParent::Root,
                    slot: PrefabPlacementSlot::Last,
                });
            } else {
                overlay.instances.push(PrefabInstanceClaim {
                    target: PrefabEntityTarget::source(id),
                    value: Some(declaration),
                });
            }
            declaration = PrefabInstance {
                source: AssetRef::path("p").unwrap(),
                overrides: Some(overlay),
            };
        }
        declaration
    }

    fn set_instance(declaration: PrefabInstance, expected_side: bool) -> ScenePatchDocument {
        let (expected, value) = if expected_side {
            (Some(declaration), None)
        } else {
            (None, Some(declaration))
        };
        ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("a").unwrap(),
            expected,
            value,
        }])
    }

    #[test]
    fn borrowed_cost_rejects_deep_snapshots_on_both_sides_and_entity_paths() {
        for expected_side in [false, true] {
            for through_entities in [false, true] {
                let patch = set_instance(deep_declaration(128, through_entities), expected_side);
                // This input fits every count/byte ceiling; semantic depth alone rejects it.
                let cost = scene_patch_cost(&patch);
                assert!(cost.operation_count() < 1024);
                assert!(cost.logical_bytes() < 64 * 1024);
                assert!(
                    bounded_scene_patch_cost(&patch, ScenePatchApplyLimits::default()).is_none()
                );
            }
            let mut value = ComponentValue::Null;
            for _ in 0..128 {
                value = ComponentValue::List(vec![ComponentValue::map([("v", value)])]);
            }
            let root = SceneEntityId::new("e").unwrap();
            let child = SceneEntityId::new("child").unwrap();
            let mut addition = PrefabAddition::new(child.clone());
            addition.components.insert(
                ComponentTypeId::new("c"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            );
            let mut overlay = PrefabOverlay::new(source_key());
            overlay.additions = vec![PrefabAddition::new(root.clone()), addition];
            overlay.placements = vec![
                PrefabPlacement {
                    target: PrefabEntityTarget::addition(root.clone()),
                    parent: PrefabPlacementParent::Root,
                    slot: PrefabPlacementSlot::Last,
                },
                PrefabPlacement {
                    target: PrefabEntityTarget::addition(child),
                    parent: PrefabPlacementParent::Entity(PrefabEntityTarget::addition(root)),
                    slot: PrefabPlacementSlot::Last,
                },
            ];
            let declaration = PrefabInstance {
                source: AssetRef::path("p").unwrap(),
                overrides: Some(overlay),
            };
            assert!(
                bounded_scene_patch_cost(
                    &set_instance(declaration, expected_side),
                    ScenePatchApplyLimits::default(),
                )
                .is_none()
            );
        }
    }

    #[test]
    fn borrowed_value_depth_has_an_inclusive_default_boundary() {
        let payload = |depth: usize, maps: bool| {
            let mut value = ComponentValue::Null;
            for _ in 1..depth {
                value = if maps {
                    ComponentValue::map([("v", value)])
                } else {
                    ComponentValue::List(vec![value])
                };
            }
            value
        };
        let patch = |value| {
            ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                entity: SceneEntityId::new("a").unwrap(),
                component: ComponentTypeId::new("c"),
                value: SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            }])
        };
        let maximum = ComponentValueMaterializationLimits::default().depth().get();
        for maps in [false, true] {
            // Patch is depth one; its component payload begins at depth two.
            let admitted = patch(payload(maximum - 1, maps));
            assert_eq!(
                bounded_scene_patch_cost(&admitted, ScenePatchApplyLimits::default()),
                Some(scene_patch_cost(&admitted)),
            );
            assert!(
                bounded_scene_patch_cost(
                    &patch(payload(maximum, maps)),
                    ScenePatchApplyLimits::default(),
                )
                .is_none()
            );

            // A source entity is also depth one, without a synthetic patch wrapper.
            let entity = |value| {
                SceneEntityRecord::new(SceneEntityId::new("a").unwrap()).with_component(
                    ComponentTypeId::new("c"),
                    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
                )
            };
            let admitted = [entity(payload(maximum - 1, maps))];
            let cost =
                bounded_scene_entities_source_cost(&admitted, ScenePatchApplyLimits::default())
                    .expect("source payload at the exact depth boundary is admitted");
            assert_eq!(
                cost.value_cost(),
                scene_entities_source_value_cost(&admitted)
            );
            assert!(
                bounded_scene_entities_source_cost(
                    &[entity(payload(maximum, maps))],
                    ScenePatchApplyLimits::default(),
                )
                .is_none()
            );
        }
    }

    #[test]
    fn borrowed_source_cost_rejects_deep_embedded_overlays_before_copying() {
        for through_entities in [false, true] {
            let mut entity = SceneEntityRecord::new(SceneEntityId::new("a").unwrap());
            entity.prefab = Some(deep_declaration(128, through_entities));
            let entities = std::slice::from_ref(&entity);
            let unbounded = tree_cost(ValueCostNode::Entities(entities.iter(), 1), None, None)
                .expect("unbounded source accounting completes");
            assert!(unbounded.operation_count() < 1024);
            assert!(unbounded.logical_bytes() < 64 * 1024);
            assert_eq!(unbounded.value_cost(), ComponentValueCost::ZERO);
            assert!(
                bounded_scene_entities_source_cost(entities, ScenePatchApplyLimits::default())
                    .is_none()
            );
        }
    }

    #[test]
    fn borrowed_source_cost_has_exact_independent_inclusive_ceilings() {
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("a").unwrap()).with_component(
            ComponentTypeId::new("c"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("v", ComponentValue::String("abc".into()))]),
            ),
        );
        entity.prefab = Some(deep_declaration(1, true));
        let entities = [entity];
        let limits = ScenePatchApplyLimits::default()
            .with_validation_work(ItemLimit::new(8).unwrap())
            .with_validation_value_nodes(ItemLimit::new(2).unwrap())
            .with_validation_value_bytes(ByteLimit::new(4).unwrap());
        let cost = bounded_scene_entities_source_cost(&entities, limits).unwrap();
        assert_eq!(cost.operation_count(), 8);
        assert_eq!(cost.value_cost().nodes(), 2);
        assert_eq!(cost.value_cost().logical_bytes(), 4);
        // Entity/component IDs (2), payload (4), canonical base (19), addition ID (1),
        // placement target/tags (4), and two AssetRef wire payloads (14 each).
        assert_eq!(cost.logical_bytes(), 2 + 4 + 19 + 1 + 4 + 2 * 14);
        assert_eq!(
            cost.value_cost(),
            scene_entities_source_value_cost(&entities)
        );
        for insufficient in [
            limits.with_validation_work(ItemLimit::new(7).unwrap()),
            limits.with_validation_value_nodes(ItemLimit::new(1).unwrap()),
            limits.with_validation_value_bytes(ByteLimit::new(3).unwrap()),
        ] {
            assert!(bounded_scene_entities_source_cost(&entities, insufficient).is_none());
        }
    }

    #[test]
    fn admitted_raw_walk_refuses_before_its_first_scratch_allocation() {
        let entities = [SceneEntityRecord::new(SceneEntityId::new("a").unwrap())];
        let mut calls = 0;
        let result = bounded_scene_entities_source_cost_with_admission(
            &entities,
            ScenePatchApplyLimits::default(),
            |bytes| {
                calls += 1;
                assert_eq!(bytes, Some(4 * std::mem::size_of::<ValueCostNode<'_>>()));
                Err("scratch refused")
            },
        );
        assert_eq!(result, Err("scratch refused"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn admitted_raw_walk_accounts_repeated_value_scratch_without_file_limit_reservation() {
        let record = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("v", ComponentValue::List(vec![ComponentValue::U64(7)]))]),
        );
        let entities = (0..1000)
            .map(|index| {
                SceneEntityRecord::new(SceneEntityId::new(format!("e{index}")).unwrap())
                    .with_component(ComponentTypeId::new("c"), record.clone())
            })
            .collect::<Vec<_>>();
        let cost_and_scratch = |entities: &[SceneEntityRecord]| {
            let mut scratch = 0usize;
            let cost = bounded_scene_entities_source_cost_with_admission(
                entities,
                ScenePatchApplyLimits::default(),
                |bytes| {
                    scratch = scratch.checked_add(bytes.unwrap()).unwrap();
                    Ok::<_, ()>(())
                },
            )
            .unwrap();
            assert_eq!(
                cost,
                bounded_scene_entities_source_cost(entities, ScenePatchApplyLimits::default())
            );
            (cost, scratch)
        };
        let (_, empty) = cost_and_scratch(&[]);
        let (_, one) = cost_and_scratch(&entities[..1]);
        let (_, many) = cost_and_scratch(&entities);
        assert!(one > empty);
        assert_eq!(many - empty, 1000 * (one - empty));
        assert!(one < 16 * 1024);
        for through_entities in [false, true] {
            let mut entity = SceneEntityRecord::new(SceneEntityId::new("a").unwrap());
            entity.prefab = Some(deep_declaration(8, through_entities));
            cost_and_scratch(&[entity]);
        }
    }

    #[test]
    fn admitted_asset_field_cost_refuses_its_wire_copy_before_materialization() {
        let path = format!("textures/{}.png", "asset".repeat(200));
        let wire_bytes = 3 * 96 + 13 + path.len();
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetAssetRefField {
            entity: SceneEntityId::new("a").unwrap(),
            component: ComponentTypeId::new("c"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("image"),
            asset_ref: AssetRef::path(&path).unwrap(),
        }]);
        let mut wire_seen = false;
        let refused = bounded_scene_patch_cost_with_admission(
            &patch,
            ScenePatchApplyLimits::default(),
            |bytes| {
                if bytes == Some(wire_bytes) {
                    wire_seen = true;
                    Err("wire copy refused")
                } else {
                    Ok(())
                }
            },
        );
        assert!(wire_seen);
        assert_eq!(refused, Err("wire copy refused"));
        assert_eq!(
            bounded_scene_patch_cost_with_admission(
                &patch,
                ScenePatchApplyLimits::default(),
                |_| Ok::<_, ()>(()),
            )
            .unwrap(),
            bounded_scene_patch_cost(&patch, ScenePatchApplyLimits::default())
        );
    }

    #[test]
    fn admitted_product_field_cost_charges_nested_canonical_wire_before_materialization() {
        for product in [
            nara_asset::ImportedProductId::PRIMARY,
            nara_asset::ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000051")
                .unwrap(),
        ] {
            let reference = AssetRef::Product(
                nara_asset::AssetProductRef::new(
                    "00000000-0000-4000-8000-000000000041".parse().unwrap(),
                    product,
                )
                .unwrap(),
            );
            let value = asset_ref_value(&reference);
            let cost = value.cost();
            assert_eq!(
                asset_ref_value_size(&reference),
                (cost.nodes(), cost.logical_bytes())
            );
            let wire_bytes = cost.logical_bytes() + 96 * cost.nodes();
            let patch = ScenePatchDocument::new([ScenePatchOperation::SetAssetRefField {
                entity: SceneEntityId::new("a").unwrap(),
                component: ComponentTypeId::new("c"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("product"),
                asset_ref: reference,
            }]);
            let refused = bounded_scene_patch_cost_with_admission(
                &patch,
                ScenePatchApplyLimits::default(),
                |bytes| {
                    if bytes == Some(wire_bytes) {
                        Err("product wire copy refused")
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(refused, Err("product wire copy refused"));
            assert_eq!(
                bounded_scene_patch_cost(&patch, ScenePatchApplyLimits::default())
                    .unwrap()
                    .value_cost(),
                cost
            );
        }
    }

    #[test]
    fn source_transactions_reject_deep_input_before_validation_or_mutation() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        for expected_side in [false, true] {
            let patch = set_instance(deep_declaration(128, false), expected_side);
            let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("a").unwrap(),
            )]);
            let mut document = original.clone();
            let report = patch.apply_to_scene(&mut document, &registry);
            assert!(!report.applied);
            assert!(report.inverse.is_none());
            assert_eq!(document, original);
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            }));
            let prefab = PrefabDocument::new(original.roots.clone(), original.entities.clone());
            let rejected = patch.prepare_to_prefab_with_validator(&prefab, &registry, |_, _, _| {
                panic!("input must be admitted before source validation")
            });
            let report = match rejected {
                Ok(_) => panic!("deep declaration must be rejected before source validation"),
                Err(report) => report,
            };
            assert!(!report.applied);
            assert!(report.inverse.is_none());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.patch-validation-work-budget-exceeded"
            }));
            assert_eq!(prefab.instantiate(), original);
        }
    }

    #[test]
    fn authoring_depth_rejection_keeps_document_revision_and_history() {
        use crate::{AuthoringDocumentSource, SceneAuthoringSession, SceneDocumentCandidate};
        use nara_reflect::AuthoringSchemaBundle;

        let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("a").unwrap(),
        )]);
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let published = SceneDocumentCandidate::decode_json_bytes(
            original.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("s").unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
        let session = SceneAuthoringSession::from_published_authoring_document(published);
        let revision = session.revision();
        let content = session.content_state();
        let history = session.history_status();
        let patch = set_instance(deep_declaration(128, false), false);
        let report = session
            .prepare_authoring_edit(revision, &patch)
            .unwrap_err();
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-history-entry-limit-exceeded"
        }));
        assert_eq!(session.document(), &original);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.history_status(), history);
    }

    #[test]
    fn exact_instance_snapshot_history_cost_counts_each_retained_record_owner() {
        let owner = nara_reflect::PersistentComponentSchemaOwnerId::try_new("test.owner").unwrap();
        let id = SceneEntityId::new("e").unwrap();
        let component = ComponentTypeId::new("c");
        let value = SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null);
        for whole_entity in [false, true] {
            let snapshot = |value: SceneComponentRecord| {
                let mut overlay = PrefabOverlay::new(source_key());
                if whole_entity {
                    let mut addition = PrefabAddition::new(id.clone());
                    addition.components.insert(component.clone(), value);
                    overlay.additions.push(addition);
                    overlay.placements.push(PrefabPlacement {
                        target: PrefabEntityTarget::addition(id.clone()),
                        parent: PrefabPlacementParent::Root,
                        slot: PrefabPlacementSlot::Last,
                    });
                } else {
                    overlay.components.push(PrefabComponentIntent::Set {
                        mode: PrefabComponentSetMode::Add,
                        target: PrefabEntityTarget::source(id.clone()),
                        component: component.clone(),
                        record: value,
                        bindings: Vec::new(),
                    });
                }
                PrefabInstance {
                    source: AssetRef::path("p").unwrap(),
                    overrides: Some(overlay),
                }
            };
            for (expected_side, value_side) in [(true, false), (false, true), (true, true)] {
                let patch = |value: SceneComponentRecord| {
                    ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                        entity: SceneEntityId::new("a").unwrap(),
                        expected: expected_side.then(|| snapshot(value.clone())),
                        value: value_side.then(|| snapshot(value)),
                    }])
                };
                let snapshots = usize::from(expected_side) + usize::from(value_side);
                let unclaimed = scene_patch_cost(&patch(value.clone()));
                let claimed_patch = patch(value.clone().with_owner(owner.clone()));
                let claimed = scene_patch_cost(&claimed_patch);
                assert_eq!(
                    claimed.logical_bytes() - unclaimed.logical_bytes(),
                    snapshots * owner.as_str().len(),
                );
                assert_eq!(
                    claimed.logical_bytes(),
                    // Both forms retain the 14-byte source claim and 19-byte base claim.
                    // Added entity/placement uses one more structural byte than component Set.
                    1 + snapshots * if whole_entity { 54 } else { 53 },
                );
                assert_eq!(claimed.value_cost().nodes(), snapshots);
                assert_eq!(claimed.value_cost().logical_bytes(), 0);
                assert_eq!(
                    claimed.operation_count(),
                    1 + snapshots * if whole_entity { 7 } else { 6 },
                );
                let limits = ScenePatchApplyLimits::default()
                    .with_validation_work(ItemLimit::new(claimed.operation_count()).unwrap())
                    .with_validation_value_nodes(ItemLimit::new(snapshots).unwrap());
                assert_eq!(
                    bounded_scene_patch_cost(&claimed_patch, limits),
                    Some(claimed)
                );
                assert!(
                    bounded_scene_patch_cost(
                        &claimed_patch,
                        limits.with_validation_work(
                            ItemLimit::new(claimed.operation_count() - 1).unwrap(),
                        ),
                    )
                    .is_none()
                );
            }
        }
    }

    #[test]
    fn structural_replay_cost_has_exact_independent_inclusive_ceilings() {
        let root = SceneEntityId::new("r").unwrap();
        let child = SceneEntityId::new("c").unwrap();
        let patch = ScenePatchDocument::new([ScenePatchOperation::RestoreEntity {
            entity: SceneEntityRecord::new(root)
                .with_children([child.clone()])
                .with_component(
                    ComponentTypeId::new("game.Note"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([("v", ComponentValue::String("abc".into()))]),
                    ),
                ),
            descendants: vec![SceneEntityRecord::new(child)],
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]);
        let limits = ScenePatchApplyLimits::default()
            .with_validation_work(ItemLimit::new(5).unwrap())
            .with_validation_value_nodes(ItemLimit::new(2).unwrap())
            .with_validation_value_bytes(ByteLimit::new(4).unwrap());
        let cost = bounded_scene_patch_cost(&patch, limits).unwrap();
        assert_eq!(cost.operation_count(), 5);
        assert_eq!(cost.value_cost().nodes(), 2);
        assert_eq!(cost.value_cost().logical_bytes(), 4);
        assert_eq!(cost.logical_bytes(), 16);
        assert_eq!(cost, scene_patch_cost(&patch));
        for insufficient in [
            limits.with_validation_work(ItemLimit::new(4).unwrap()),
            limits.with_validation_value_nodes(ItemLimit::new(1).unwrap()),
            limits.with_validation_value_bytes(ByteLimit::new(3).unwrap()),
        ] {
            assert!(bounded_scene_patch_cost(&patch, insufficient).is_none());
        }
    }
}

fn add_scene_entity_id_cost(cost: &mut ScenePatchCost, entity: &SceneEntityId) {
    cost.structural_logical_bytes = cost
        .structural_logical_bytes
        .saturating_add(entity.as_str().len());
}

fn add_component_type_id_cost(cost: &mut ScenePatchCost, component: &ComponentTypeId) {
    cost.structural_logical_bytes = cost
        .structural_logical_bytes
        .saturating_add(component.as_str().len());
}

fn add_component_field_id_cost(cost: &mut ScenePatchCost, field: &ComponentFieldId) {
    cost.structural_logical_bytes = cost
        .structural_logical_bytes
        .saturating_add(field.as_str().len());
}

fn add_placement_cost(cost: &mut ScenePatchCost, placement: &ScenePlacement) {
    if let ScenePlacementOwner::Parent { parent } = &placement.owner {
        add_scene_entity_id_cost(cost, parent);
    }
    match &placement.anchor {
        ScenePlacementAnchor::Before { sibling } | ScenePlacementAnchor::After { sibling } => {
            add_scene_entity_id_cost(cost, sibling)
        }
        ScenePlacementAnchor::First | ScenePlacementAnchor::Last => {}
    }
}

pub(crate) struct PreparedScenePatch {
    document: SceneDocument,
    report: ScenePatchReport,
}

pub(crate) struct PreparedPrefabPatch {
    document: PrefabDocument,
    report: ScenePatchReport,
}

impl PreparedPrefabPatch {
    pub(crate) fn into_parts(self) -> (PrefabDocument, ScenePatchReport) {
        (self.document, self.report)
    }
}

impl PreparedScenePatch {
    pub(crate) fn into_parts(self) -> (SceneDocument, ScenePatchReport) {
        (self.document, self.report)
    }

    fn publish_into(self, document: &mut SceneDocument) -> ScenePatchReport {
        let (candidate, report) = self.into_parts();
        *document = candidate;
        report
    }
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
    fn materialization_limits(self) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            self.validation_value_nodes(),
            self.validation_value_bytes(),
            ComponentValueMaterializationLimits::default().depth(),
        )
    }

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
        structural_items: document.entities.iter().fold(
            1_usize.saturating_add(document.roots.len()),
            |work, entity| {
                work.saturating_add(1)
                    .saturating_add(entity.children.len())
                    .saturating_add(entity.components.len())
            },
        ),
        value_cost: scene_entities_source_value_cost(&document.entities),
    }
}

fn prefab_validation_work(document: &PrefabDocument) -> SceneValidationWork {
    SceneValidationWork {
        structural_items: document.entities.iter().fold(
            1_usize.saturating_add(document.roots.len()),
            |work, entity| {
                work.saturating_add(1)
                    .saturating_add(entity.children.len())
                    .saturating_add(entity.components.len())
            },
        ),
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

fn apply_operation_for_patch(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Box<ScenePatchReport>> {
    match apply_operation(document, registry, operation, operation_index) {
        Ok(inverse) => Ok(inverse),
        Err(diagnostic) => {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
            Err(Box::new(ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            }))
        }
    }
}

fn validate_patch_candidate(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    database: Option<&ProjectAssetDatabase>,
    operation_index: Option<usize>,
    limits: ScenePatchApplyLimits,
) -> DiagnosticReport {
    let mut context =
        ComponentDecodeContext::new().with_materialization_limits(limits.materialization_limits());
    if let Some(database) = database {
        context = context.with_project_asset_database(database);
    }
    preflight_authoring_scene_with_context_at_operation(
        document,
        registry,
        &mut context,
        operation_index,
    )
    .diagnostics
}

fn validate_applied_operation(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    operation_index: usize,
    validation_work: &mut SceneValidationWork,
    limits: ScenePatchApplyLimits,
    final_operation: bool,
    validate: &mut impl FnMut(&SceneDocument, &ComponentRegistry, Option<usize>) -> DiagnosticReport,
) -> Result<(), Box<ScenePatchReport>> {
    let operation_work = scene_validation_work(document);
    let observed = validation_work.saturating_add(operation_work);
    if let Some(diagnostic) =
        validation_work_budget_diagnostic(observed, limits, Some(operation_index))
    {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        return Err(Box::new(ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics,
        }));
    }
    *validation_work = observed;

    let validation = if final_operation {
        document.canonicalize();
        validate(document, registry, Some(operation_index))
    } else {
        validate_patch_intermediate_scene_with_limits(
            document,
            registry,
            operation_index,
            limits.materialization_limits(),
        )
    };
    if validation.has_errors() {
        return Err(Box::new(ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics: validation,
        }));
    }

    Ok(())
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

fn apply_prefab_operation(
    document: &mut PrefabDocument,
    registry: &ComponentRegistry,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    match operation {
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
        _ => apply_non_component_operation(document, registry, operation, operation_index),
    }
}

/// Applies topology after the authoring owner has admitted the operation and component values.
#[cfg(feature = "serde")]
pub(crate) fn apply_authoring_structure_operation(
    document: &mut SceneDocument,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    match operation {
        ScenePatchOperation::AddEntity { entity, placement } => {
            prepare_entity_placement(document, entity, placement, operation_index)?;
            let insertion =
                resolve_insertion(document, &entity.id, placement, false, operation_index)?;
            document.entities.push(entity.clone());
            insert_placed_entity(document, &entity.id, insertion);
            Ok(vec![ScenePatchOperation::RemoveEntity {
                entity: entity.id.clone(),
                from: placement.clone(),
            }])
        }
        ScenePatchOperation::RemoveEntity { entity, from } => {
            let removed =
                prepare_subtree_removal_with(document, entity, from, operation_index, |_| Ok(()))?;
            remove_subtree_records(document, &removed);
            let mut removed = removed.into_iter();
            let entity = removed.next().expect("prepared subtree contains its root");
            let descendants = removed.collect::<Vec<_>>();
            if entity.components.is_empty() && entity.children.is_empty() && entity.prefab.is_none()
            {
                Ok(vec![ScenePatchOperation::AddEntity {
                    entity,
                    placement: from.clone(),
                }])
            } else {
                Ok(vec![ScenePatchOperation::RestoreEntity {
                    entity,
                    descendants,
                    placement: from.clone(),
                }])
            }
        }
        ScenePatchOperation::RestoreEntity {
            entity,
            descendants,
            placement,
        } => {
            prepare_entity_restoration_with(
                document,
                entity,
                descendants,
                placement,
                operation_index,
                |_| Ok(()),
            )?;
            let insertion =
                resolve_insertion(document, &entity.id, placement, false, operation_index)?;
            document.entities.push(entity.clone());
            document.entities.extend(descendants.iter().cloned());
            insert_placed_entity(document, &entity.id, insertion);
            Ok(vec![ScenePatchOperation::RemoveEntity {
                entity: entity.id.clone(),
                from: placement.clone(),
            }])
        }
        ScenePatchOperation::MoveEntity { entity, from, to } => {
            move_entity(document, entity, from, to, operation_index)
        }
        ScenePatchOperation::AddComponent {
            entity,
            component,
            value,
        } => add_component_record(document, entity, component, value, operation_index),
        ScenePatchOperation::RemoveComponent { entity, component } => {
            remove_component_record(document, entity, component, operation_index)
        }
        ScenePatchOperation::SetPrefabInstance {
            entity,
            expected,
            value,
        } => set_prefab_instance(document, entity, expected, value, operation_index),
        _ => unreachable!("the authoring owner admits only its finite structure vocabulary"),
    }
}

fn apply_operation(
    document: &mut SceneDocument,
    registry: &ComponentRegistry,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    match operation {
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
        _ => apply_non_component_operation(document, registry, operation, operation_index),
    }
}

fn apply_non_component_operation<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    operation: &ScenePatchOperation,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    match operation {
        ScenePatchOperation::AddEntity { entity, placement } => {
            add_entity(document, registry, entity, placement, operation_index)
        }
        ScenePatchOperation::RemoveEntity { entity, from } => {
            remove_entity(document, registry, entity, from, operation_index)
        }
        ScenePatchOperation::RestoreEntity {
            entity,
            descendants,
            placement,
        } => restore_entity(
            document,
            registry,
            entity,
            descendants,
            placement,
            operation_index,
        ),
        ScenePatchOperation::AddComponent { .. }
        | ScenePatchOperation::RemoveComponent { .. }
        | ScenePatchOperation::ReplaceComponent { .. } => Err(patch_diagnostic(
            operation_index,
            "scene.patch-component-operation-routing-failed",
            "Component structure operation did not pass through its document policy",
            None,
            None,
            None,
        )),
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
        ScenePatchOperation::MoveEntity { entity, from, to } => {
            move_entity(document, entity, from, to, operation_index)
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
        ScenePatchOperation::SetPrefabInstance {
            entity,
            expected,
            value,
        } => set_prefab_instance(document, entity, expected, value, operation_index),
    }
}

fn set_prefab_instance<D: EntityDocument + ?Sized>(
    document: &mut D,
    entity: &SceneEntityId,
    expected: &Option<PrefabInstance>,
    value: &Option<PrefabInstance>,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    prefab_instance_record(document, entity, expected, operation_index)?.prefab = value.clone();
    Ok(vec![ScenePatchOperation::SetPrefabInstance {
        entity: entity.clone(),
        expected: value.clone(),
        value: expected.clone(),
    }])
}

fn prefab_instance_record<'document, D: EntityDocument + ?Sized>(
    document: &'document mut D,
    entity: &SceneEntityId,
    expected: &Option<PrefabInstance>,
    operation_index: usize,
) -> Result<&'document mut SceneEntityRecord, Diagnostic> {
    let record = entity_mut(document, entity, operation_index)?;
    if &record.prefab != expected {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-prefab-instance-context-mismatch",
            "Instance mutation requires its exact prior source declaration",
            Some(entity),
            None,
            None,
        ));
    }
    Ok(record)
}

fn add_entity<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityRecord,
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    prepare_entity_insertion(document, registry, entity, placement, operation_index)?;
    let insertion = resolve_insertion(document, &entity.id, placement, false, operation_index)?;
    document.entities_mut().push(entity.clone());
    insert_placed_entity(document, &entity.id, insertion);
    Ok(vec![ScenePatchOperation::RemoveEntity {
        entity: entity.id.clone(),
        from: placement.clone(),
    }])
}

fn remove_entity<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    from: &ScenePlacement,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let removed = prepare_subtree_removal(document, registry, entity, from, operation_index)?;
    remove_subtree_records(document, &removed);
    let mut removed = removed.into_iter();
    let entity = removed
        .next()
        .expect("a prepared subtree removal always contains its root");
    Ok(vec![ScenePatchOperation::RestoreEntity {
        entity,
        descendants: removed.collect(),
        placement: from.clone(),
    }])
}

fn restore_entity<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityRecord,
    descendants: &[SceneEntityRecord],
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    prepare_entity_restoration(
        document,
        registry,
        entity,
        descendants,
        placement,
        operation_index,
    )?;
    let insertion = resolve_insertion(document, &entity.id, placement, false, operation_index)?;
    document.entities_mut().push(entity.clone());
    document.entities_mut().extend(descendants.iter().cloned());
    insert_placed_entity(document, &entity.id, insertion);
    Ok(vec![ScenePatchOperation::RemoveEntity {
        entity: entity.id.clone(),
        from: placement.clone(),
    }])
}

fn add_component<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    value: &SceneComponentRecord,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    require_whole_component_edit(registry, entity, component, operation_index)?;
    add_component_record(document, entity, component, value, operation_index)
}

fn add_component_record<D: EntityDocument>(
    document: &mut D,
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

fn remove_component<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    require_whole_component_edit(registry, entity, component, operation_index)?;
    remove_component_record(document, entity, component, operation_index)
}

fn remove_component_record<D: EntityDocument>(
    document: &mut D,
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

fn replace_component<D: EntityDocument>(
    document: &mut D,
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

fn set_field<D: EntityDocument>(
    document: &mut D,
    target: FieldPatchTarget<'_>,
    value: ComponentValue,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let previous = set_field_value(document, target, value)?;
    let FieldPatchTarget {
        entity,
        component,
        component_version,
        field,
        ..
    } = target;
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

fn set_field_value<D: EntityDocument>(
    document: &mut D,
    target: FieldPatchTarget<'_>,
    value: ComponentValue,
) -> Result<Option<ComponentValue>, Diagnostic> {
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

    Ok(previous)
}

fn remove_field<D: EntityDocument>(
    document: &mut D,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    component_version: &ComponentSchemaVersion,
    field: &ComponentFieldId,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    let previous = remove_field_value(
        document,
        FieldPatchTarget::new(
            registry,
            entity,
            component,
            component_version,
            field,
            operation_index,
        ),
    )?;
    Ok(vec![ScenePatchOperation::SetField {
        entity: entity.clone(),
        component: component.clone(),
        component_version: *component_version,
        field: field.clone(),
        value: previous,
    }])
}

fn remove_field_value<D: EntityDocument>(
    document: &mut D,
    target: FieldPatchTarget<'_>,
) -> Result<ComponentValue, Diagnostic> {
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
    Ok(previous)
}

fn move_entity<D: EntityDocument>(
    document: &mut D,
    entity: &SceneEntityId,
    from: &ScenePlacement,
    to: &ScenePlacement,
    operation_index: usize,
) -> Result<Vec<ScenePatchOperation>, Diagnostic> {
    entity_mut(document, entity, operation_index)?;
    if !placement_matches(document, entity, from) {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-placement-context-mismatch",
            "patch source placement does not match the current entity context",
            Some(entity),
            None,
            None,
        ));
    }
    let insertion = resolve_insertion(document, entity, to, true, operation_index)?;
    detach_entity(document, entity);
    insert_placed_entity(document, entity, insertion);
    Ok(vec![ScenePatchOperation::MoveEntity {
        entity: entity.clone(),
        from: to.clone(),
        to: from.clone(),
    }])
}

#[derive(Clone)]
struct ResolvedInsertion {
    owner: ScenePlacementOwner,
    index: usize,
}

fn prepare_entity_insertion<D: EntityDocument + ?Sized>(
    document: &D,
    registry: &ComponentRegistry,
    entity: &SceneEntityRecord,
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<(), Diagnostic> {
    prepare_entity_placement(document, entity, placement, operation_index)?;
    for component in entity.components.keys() {
        require_whole_component_edit(registry, &entity.id, component, operation_index)?;
    }
    Ok(())
}

fn prepare_entity_placement<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityRecord,
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<(), Diagnostic> {
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
    if !entity.children.is_empty() {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-add-entity-has-children",
            "patch adds an entity with children; use explicit placement operations",
            Some(&entity.id),
            None,
            None,
        ));
    }
    validate_placement_owner(document, &entity.id, placement, operation_index)
}

fn prepare_subtree_removal<D: EntityDocument + ?Sized>(
    document: &D,
    registry: &ComponentRegistry,
    entity: &SceneEntityId,
    from: &ScenePlacement,
    operation_index: usize,
) -> Result<Vec<SceneEntityRecord>, Diagnostic> {
    prepare_subtree_removal_with(document, entity, from, operation_index, |record| {
        for component in record.components.keys() {
            require_whole_component_edit(registry, &record.id, component, operation_index)?;
        }
        Ok(())
    })
}

fn prepare_subtree_removal_with<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityId,
    from: &ScenePlacement,
    operation_index: usize,
    mut admit_record: impl FnMut(&SceneEntityRecord) -> Result<(), Diagnostic>,
) -> Result<Vec<SceneEntityRecord>, Diagnostic> {
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
    if !placement_matches(document, entity, from) {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-placement-context-mismatch",
            "patch removal placement does not match the current entity context",
            Some(entity),
            None,
            None,
        ));
    }

    let mut removed = Vec::new();
    let mut pending = vec![entity.clone()];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let record_index = find_entity(document, &id)
            .expect("validated subtree entity must remain in the source document");
        let record = &document.entities()[record_index];
        admit_record(record)?;
        pending.extend(record.children.iter().rev().cloned());
        removed.push(record.clone());
    }
    Ok(removed)
}

fn prepare_entity_restoration<D: EntityDocument + ?Sized>(
    document: &D,
    registry: &ComponentRegistry,
    entity: &SceneEntityRecord,
    descendants: &[SceneEntityRecord],
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<(), Diagnostic> {
    prepare_entity_restoration_with(
        document,
        entity,
        descendants,
        placement,
        operation_index,
        |record| {
            for component in record.components.keys() {
                require_whole_component_edit(registry, &record.id, component, operation_index)?;
            }
            Ok(())
        },
    )
}

fn prepare_entity_restoration_with<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityRecord,
    descendants: &[SceneEntityRecord],
    placement: &ScenePlacement,
    operation_index: usize,
    mut admit_record: impl FnMut(&SceneEntityRecord) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let mut restored_ids = BTreeSet::new();
    for record in std::iter::once(entity).chain(descendants) {
        if !restored_ids.insert(record.id.clone()) {
            return Err(patch_diagnostic(
                operation_index,
                "scene.patch-restore-duplicate-entity",
                "patch restores the same entity more than once",
                Some(&record.id),
                None,
                None,
            ));
        }
        if find_entity(document, &record.id).is_some() {
            return Err(patch_diagnostic(
                operation_index,
                "scene.patch-duplicate-entity",
                "patch restores an entity that already exists",
                Some(&record.id),
                None,
                None,
            ));
        }
        admit_record(record)?;
    }
    validate_placement_owner(document, &entity.id, placement, operation_index)
}

fn remove_subtree_records<D: EntityDocument + ?Sized>(
    document: &mut D,
    removed: &[SceneEntityRecord],
) {
    let removed_ids = removed
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    document
        .entities_mut()
        .retain(|record| !removed_ids.contains(&record.id));
    document
        .roots_mut()
        .retain(|root| !removed_ids.contains(root));
    for record in document.entities_mut() {
        record.children.retain(|child| !removed_ids.contains(child));
    }
}

fn validate_placement_owner<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityId,
    placement: &ScenePlacement,
    operation_index: usize,
) -> Result<(), Diagnostic> {
    let ScenePlacementOwner::Parent { parent } = &placement.owner else {
        return Ok(());
    };
    if parent == entity {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-placement-self-parent",
            "patch places an entity beneath itself",
            Some(entity),
            None,
            None,
        ));
    }
    if find_entity(document, parent).is_none() {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-placement-owner-missing",
            "patch placement owner does not exist",
            Some(entity),
            None,
            None,
        ));
    }
    if descendant_of(document, parent, entity) {
        return Err(patch_diagnostic(
            operation_index,
            "scene.patch-placement-cycle",
            "patch placement would create a cycle",
            Some(entity),
            None,
            None,
        ));
    }
    Ok(())
}

fn resolve_insertion<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityId,
    placement: &ScenePlacement,
    remove_entity_first: bool,
    operation_index: usize,
) -> Result<ResolvedInsertion, Diagnostic> {
    validate_placement_owner(document, entity, placement, operation_index)?;
    let mut siblings = siblings_for_owner(document, &placement.owner).ok_or_else(|| {
        patch_diagnostic(
            operation_index,
            "scene.patch-placement-owner-missing",
            "patch placement owner does not exist",
            Some(entity),
            None,
            None,
        )
    })?;
    if remove_entity_first {
        siblings.retain(|sibling| sibling != entity);
    }

    let index = match &placement.anchor {
        ScenePlacementAnchor::First => 0,
        ScenePlacementAnchor::Last => siblings.len(),
        ScenePlacementAnchor::Before { sibling } | ScenePlacementAnchor::After { sibling }
            if sibling == entity =>
        {
            return Err(patch_diagnostic(
                operation_index,
                "scene.patch-placement-self-anchor",
                "patch placement cannot use the moving entity as its anchor",
                Some(entity),
                None,
                None,
            ));
        }
        ScenePlacementAnchor::Before { sibling } | ScenePlacementAnchor::After { sibling } => {
            let Some(anchor_index) = siblings.iter().position(|candidate| candidate == sibling)
            else {
                return Err(patch_diagnostic(
                    operation_index,
                    "scene.patch-placement-anchor-missing",
                    "patch placement anchor is missing, retired, or belongs to another owner",
                    Some(entity),
                    None,
                    None,
                ));
            };
            if matches!(placement.anchor, ScenePlacementAnchor::Before { .. }) {
                anchor_index
            } else {
                anchor_index.saturating_add(1)
            }
        }
    };
    Ok(ResolvedInsertion {
        owner: placement.owner.clone(),
        index,
    })
}

fn placement_for_entity<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityId,
) -> Option<ScenePlacement> {
    let owner = placement_owner(document.roots(), document.entities(), entity)?;
    let siblings = siblings_for_owner(document, &owner)?;
    let index = siblings.iter().position(|candidate| candidate == entity)?;
    let anchor = if index == 0 {
        ScenePlacementAnchor::First
    } else {
        ScenePlacementAnchor::After {
            sibling: siblings[index - 1].clone(),
        }
    };
    Some(ScenePlacement { owner, anchor })
}

fn placement_matches<D: EntityDocument + ?Sized>(
    document: &D,
    entity: &SceneEntityId,
    placement: &ScenePlacement,
) -> bool {
    let Some(owner) = placement_owner(document.roots(), document.entities(), entity) else {
        return false;
    };
    if owner != placement.owner {
        return false;
    }
    let Some(siblings) = siblings_for_owner(document, &owner) else {
        return false;
    };
    let Some(index) = siblings.iter().position(|candidate| candidate == entity) else {
        return false;
    };
    match &placement.anchor {
        ScenePlacementAnchor::First => index == 0,
        ScenePlacementAnchor::Last => index + 1 == siblings.len(),
        ScenePlacementAnchor::Before { sibling } => siblings.get(index + 1) == Some(sibling),
        ScenePlacementAnchor::After { sibling } => {
            index > 0 && siblings.get(index - 1) == Some(sibling)
        }
    }
}

pub(crate) fn placement_owner(
    roots: &[SceneEntityId],
    entities: &[SceneEntityRecord],
    entity: &SceneEntityId,
) -> Option<ScenePlacementOwner> {
    if roots.contains(entity) {
        return Some(ScenePlacementOwner::Root);
    }
    entities.iter().find_map(|record| {
        record
            .children
            .contains(entity)
            .then(|| ScenePlacementOwner::Parent {
                parent: record.id.clone(),
            })
    })
}

fn siblings_for_owner<D: EntityDocument + ?Sized>(
    document: &D,
    owner: &ScenePlacementOwner,
) -> Option<Vec<SceneEntityId>> {
    match owner {
        ScenePlacementOwner::Root => Some(document.roots().to_vec()),
        ScenePlacementOwner::Parent { parent } => document
            .entities()
            .iter()
            .find(|record| &record.id == parent)
            .map(|record| record.children.clone()),
    }
}

fn descendant_of<D: EntityDocument + ?Sized>(
    document: &D,
    candidate: &SceneEntityId,
    ancestor: &SceneEntityId,
) -> bool {
    let mut pending = vec![ancestor.clone()];
    while let Some(parent) = pending.pop() {
        let Some(record) = document
            .entities()
            .iter()
            .find(|record| record.id == parent)
        else {
            continue;
        };
        if record.children.contains(candidate) {
            return true;
        }
        pending.extend(record.children.iter().cloned());
    }
    false
}

fn detach_entity<D: EntityDocument + ?Sized>(document: &mut D, entity: &SceneEntityId) {
    document.roots_mut().retain(|root| root != entity);
    for record in document.entities_mut() {
        record.children.retain(|child| child != entity);
    }
}

fn insert_placed_entity<D: EntityDocument + ?Sized>(
    document: &mut D,
    entity: &SceneEntityId,
    insertion: ResolvedInsertion,
) {
    match insertion.owner {
        ScenePlacementOwner::Root => document.roots_mut().insert(insertion.index, entity.clone()),
        ScenePlacementOwner::Parent { parent } => {
            let record = document
                .entities_mut()
                .iter_mut()
                .find(|record| record.id == parent)
                .expect("resolved placement owner must remain in the unpublished document");
            record.children.insert(insertion.index, entity.clone());
        }
    }
}

fn set_asset_ref_field<D: EntityDocument>(
    document: &mut D,
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
    if field_schema.value_kind() != ComponentValueKind::AssetRef {
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

trait EntityDocument {
    fn roots(&self) -> &[SceneEntityId];
    fn roots_mut(&mut self) -> &mut Vec<SceneEntityId>;
    fn entities(&self) -> &[SceneEntityRecord];
    fn entities_mut(&mut self) -> &mut Vec<SceneEntityRecord>;
}

impl EntityDocument for SceneDocument {
    fn roots(&self) -> &[SceneEntityId] {
        &self.roots
    }

    fn roots_mut(&mut self) -> &mut Vec<SceneEntityId> {
        &mut self.roots
    }

    fn entities(&self) -> &[SceneEntityRecord] {
        &self.entities
    }

    fn entities_mut(&mut self) -> &mut Vec<SceneEntityRecord> {
        &mut self.entities
    }
}

impl EntityDocument for PrefabDocument {
    fn roots(&self) -> &[SceneEntityId] {
        &self.roots
    }

    fn roots_mut(&mut self) -> &mut Vec<SceneEntityId> {
        &mut self.roots
    }

    fn entities(&self) -> &[SceneEntityRecord] {
        &self.entities
    }

    fn entities_mut(&mut self) -> &mut Vec<SceneEntityRecord> {
        &mut self.entities
    }
}

fn find_entity<D: EntityDocument + ?Sized>(document: &D, id: &SceneEntityId) -> Option<usize> {
    document
        .entities()
        .iter()
        .position(|record| record.id == *id)
}

fn entity_mut<'a, D: EntityDocument + ?Sized>(
    document: &'a mut D,
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
    Ok(&mut document.entities_mut()[index])
}

fn component_mut<'a, D: EntityDocument + ?Sized>(
    document: &'a mut D,
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

fn component_mut_for_field<'a, D: EntityDocument + ?Sized>(
    document: &'a mut D,
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

pub(crate) fn field_value_matches(field: &ComponentFieldSchema, value: &ComponentValue) -> bool {
    field.accepts_value(value)
}

pub(crate) fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    nara_reflect::encode_asset_reference_value(asset_ref)
}

/// Nodes and logical bytes of the canonical field value, without allocating its wire strings.
/// This models encoding work; an in-memory product reference itself owns no strings.
pub(crate) fn asset_ref_value_size(asset_ref: &AssetRef) -> (usize, usize) {
    match asset_ref {
        AssetRef::Path(path) => (3, 13_usize.saturating_add(path.as_str().len())),
        AssetRef::StableId(_) => (3, 54),
        // kind=product, value={source, product={kind=primary|subproduct, id?}}.
        AssetRef::Product(reference) => match reference.product().as_subproduct() {
            None => (6, 76),
            Some(_) => (7, 117),
        },
    }
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
