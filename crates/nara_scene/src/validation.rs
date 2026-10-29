use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::__private::plan_declared_entity_references;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentDecodeContext,
    ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentMigrationError,
    ComponentRegistry, ComponentSchema, ComponentTypeId, ComponentValue,
    ComponentValueMaterializationLimits, DeclaredAssetReference, EntityReference,
    EntityReferenceTraversalLimits, PreparedComponent,
    collect_declared_asset_references_from_schema_with_limits,
    validate_component_value_with_limits,
};

use crate::{
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, push_with_operation_index, with_asset_ref, with_codec_error,
        with_entity_reference_rewrite_error, with_migration_error, with_public_locator,
    },
    schema_authority::SchemaAuthority,
};

pub(crate) struct PreparedScene {
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<PreparedSceneEntity>,
    pub(crate) diagnostics: DiagnosticReport,
}

pub(crate) struct PreparedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) components: Vec<PreparedSceneComponent>,
    pub(crate) data_components: Vec<PreparedSceneDataComponent>,
}

pub(crate) struct PreparedSceneComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) prepared: PreparedComponent,
}

pub(crate) struct PreparedSceneDataComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) owner: nara_reflect::ComponentSchemaOwnerSnapshot,
    pub(crate) record: SceneComponentRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedScenePlacement {
    parents: BTreeMap<SceneEntityId, Option<SceneEntityId>>,
}

impl ValidatedScenePlacement {
    pub(crate) fn parent_of(&self, entity: &SceneEntityId) -> Option<&SceneEntityId> {
        self.parents.get(entity).and_then(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlacementOwner {
    Root,
    Parent(SceneEntityId),
}

#[derive(Clone, Copy)]
enum SceneValidationMode {
    Native,
    Shape,
}

struct ScenePreflightOptions {
    allow_prefab_instances: bool,
    operation_index: Option<usize>,
    mode: SceneValidationMode,
    validate_local_references: bool,
}

pub(crate) struct EffectiveSceneValidation {
    pub(crate) diagnostics: DiagnosticReport,
    #[cfg(feature = "serde")]
    pub(crate) asset_references: Vec<DeclaredAssetReference>,
}

/// Admits a complete expanded value graph before asset realization or native decoding.
/// Runtime preparation still crosses the ordinary native path after the Host captures assets.
pub(crate) fn validate_expanded_values<E>(
    document: &SceneDocument,
    authority: SchemaAuthority<'_>,
    context: &mut ComponentDecodeContext<'_>,
    mut admit_asset_work: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<EffectiveSceneValidation, E> {
    let (placement, mut diagnostics) = validate_scene_structure_with_operation(document, None);
    let mut asset_references = Vec::new();
    if let Some(registry) = authority.native_registry() {
        if registry.snapshot().is_err() {
            diagnostics.push(diagnostic_error(
                "scene.component-registry-not-frozen",
                "Scene validation requires a frozen component registry",
            ));
            return Ok(EffectiveSceneValidation {
                diagnostics,
                #[cfg(feature = "serde")]
                asset_references,
            });
        }
        if let Some(entity) = placement.as_ref().and_then(|placement| {
            first_unavailable_hierarchy_projection_entity(document, placement, registry)
        }) {
            diagnostics.push(with_public_locator(
                diagnostic_error(
                    "scene.hierarchy-projection-unavailable",
                    "Scene hierarchy requires a runtime projection that is not implemented",
                ),
                "entity-id",
                entity.as_str(),
            ));
        }
    }
    let namespace = document
        .entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    let limits = context.materialization_limits();
    let traversal = reference_traversal_limits(limits);
    for entity in sorted_entities(document) {
        if let Some(instance) = &entity.prefab {
            diagnostics.push(with_asset_ref(
                with_public_locator(
                    diagnostic_error(
                        "scene.prefab-instance-unsupported",
                        "Prefab instances require expansion before scene spawn",
                    ),
                    "entity-id",
                    entity.id.as_str(),
                ),
                "asset-ref",
                &instance.source,
            ));
        }
        for (component, record) in &entity.components {
            let locate = |diagnostic| {
                with_public_locator(
                    with_public_locator(diagnostic, "entity-id", entity.id.as_str()),
                    "component-id",
                    component.as_str(),
                )
            };
            let Some(schema) = authority.schema(component) else {
                diagnostics.push(locate(diagnostic_error(
                    "scene.unknown-component",
                    "Component type is not registered",
                )));
                continue;
            };
            if !authority.owner_matches(component, record) {
                diagnostics.push(locate(diagnostic_error(
                    "scene.component-owner-mismatch",
                    "Persistent component owner does not match the admitted schema owner",
                )));
                continue;
            }
            if record.version != schema.version() {
                diagnostics.push(component_migration_diagnostic(
                    entity.id.as_str(),
                    component.as_str(),
                    &ComponentMigrationError::UnsupportedVersion {
                        component_id: component.clone(),
                        from_version: record.version,
                        target_version: schema.version(),
                    },
                ));
                continue;
            }
            if !schema.has_capability(ComponentCapability::Scene) {
                diagnostics.push(locate(diagnostic_error(
                    "scene.component-not-scene-capable",
                    "Component is not scene-capable",
                )));
                continue;
            }
            let invalid = |error: &ComponentCodecError| {
                locate(with_codec_error(
                    diagnostic_error(
                        "scene.invalid-component-payload",
                        "Component payload is invalid",
                    ),
                    error,
                    Some(schema),
                ))
            };
            if let Err(error) = nara_reflect::__private::validate_schema_capabilities(
                schema,
                [ComponentCapability::Scene],
            ) {
                diagnostics.push(invalid(&ComponentCodecError::Message(error.to_string())));
                continue;
            }
            if let Err(error) =
                nara_reflect::validate_component_value_with_limits(schema, &record.value, limits)
            {
                diagnostics.push(invalid(&error));
                continue;
            }
            let references = match nara_reflect::__private::collect_declared_effective_asset_references_with_admission(
                schema, record.version, &record.value, traversal, &mut admit_asset_work,
            ) {
                Ok(references) => references,
                Err(nara_reflect::__private::AssetReferenceCollectionError::Admission(error)) =>
                    return Err(error),
                Err(nara_reflect::__private::AssetReferenceCollectionError::Reference(_)) => {
                    diagnostics.push(invalid(&ComponentCodecError::Message("Schema-declared asset reference traversal failed".into())));
                    continue;
                }
            };
            // Scene checks the enclosing source record only. The selected product's actual
            // existence and typed domain admission belong to the root content owner.
            if let Some(database) = context.project_asset_database()
                && let Some(reference) = references
                    .iter()
                    .find(|reference| database.resolve_ref(reference.asset_ref()).is_err())
            {
                diagnostics.push(invalid(&ComponentCodecError::invalid_asset_ref(
                    reference.path().to_string(),
                    reference.asset_ref().to_string(),
                    "Asset reference is not declared in the project asset database",
                )));
                continue;
            }
            if let Err(error) = validate_effective_local_references(
                schema,
                &record.value,
                |target| namespace.contains(target),
                traversal,
            ) {
                diagnostics.push(with_entity_reference_rewrite_error(
                    locate(diagnostic_error(
                        "scene.local-reference-invalid",
                        "Scene component references do not close over the document namespace",
                    )),
                    &error,
                    |_| "scene-local-target-missing",
                ));
                continue;
            }
            // Admit the complete new allocation when growing the retained inventory.
            // Doubling amortizes component-by-component collection without hidden reallocations.
            let required = asset_references.len().saturating_add(references.len());
            if required > asset_references.capacity() {
                let capacity = required.max(asset_references.capacity().saturating_mul(2));
                admit_asset_work(
                    0,
                    0,
                    capacity.saturating_mul(size_of::<DeclaredAssetReference>()),
                )?;
                asset_references.reserve_exact(capacity - asset_references.len());
            }
            asset_references.extend(references);
        }
    }
    Ok(EffectiveSceneValidation {
        diagnostics,
        #[cfg(feature = "serde")]
        asset_references,
    })
}

pub(crate) fn preflight_scene(
    document: &SceneDocument,
    registry: &ComponentRegistry,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene(
    document: &SceneDocument,
    registry: &ComponentRegistry,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_authoring_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    operation_index: usize,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context_options(
        document,
        registry,
        None,
        &mut context,
        ScenePreflightOptions {
            allow_prefab_instances: true,
            operation_index: Some(operation_index),
            mode: SceneValidationMode::Native,
            validate_local_references: true,
        },
    )
}

pub(crate) fn validate_patch_intermediate_scene_with_limits(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    operation_index: usize,
    limits: ComponentValueMaterializationLimits,
) -> DiagnosticReport {
    let mut context = ComponentDecodeContext::new().with_materialization_limits(limits);
    preflight_scene_with_context_options(
        document,
        registry,
        None,
        &mut context,
        ScenePreflightOptions {
            allow_prefab_instances: true,
            operation_index: Some(operation_index),
            mode: SceneValidationMode::Shape,
            validate_local_references: false,
        },
    )
    .diagnostics
}

pub(crate) fn preflight_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_scene_with_context_options(
        document,
        registry,
        None,
        context,
        ScenePreflightOptions {
            allow_prefab_instances: false,
            operation_index: None,
            mode: SceneValidationMode::Native,
            validate_local_references: true,
        },
    )
}

/// Preflights both representations through the same admission and reference-validation path.
pub(crate) fn preflight_scene_with_context_and_bindings(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    bindings: &SceneComponentBindings,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_scene_with_context_options(
        document,
        registry,
        Some(bindings),
        context,
        ScenePreflightOptions {
            allow_prefab_instances: false,
            operation_index: None,
            mode: SceneValidationMode::Native,
            validate_local_references: true,
        },
    )
}

pub(crate) fn preflight_authoring_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_authoring_scene_with_context_at_operation(document, registry, context, None)
}

pub(crate) fn preflight_authoring_scene_with_context_at_operation(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
    operation_index: Option<usize>,
) -> PreparedScene {
    preflight_scene_with_context_options(
        document,
        registry,
        None,
        context,
        ScenePreflightOptions {
            allow_prefab_instances: true,
            operation_index,
            mode: SceneValidationMode::Native,
            validate_local_references: true,
        },
    )
}

pub(crate) fn preflight_authoring_scene_with_context_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
    operation_index: usize,
) -> PreparedScene {
    preflight_scene_with_context_options(
        document,
        registry,
        None,
        context,
        ScenePreflightOptions {
            allow_prefab_instances: true,
            operation_index: Some(operation_index),
            mode: SceneValidationMode::Native,
            validate_local_references: true,
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SceneLocalTargetMissing;

pub(crate) fn reference_traversal_limits(
    limits: ComponentValueMaterializationLimits,
) -> EntityReferenceTraversalLimits {
    let overhead = ComponentFieldId::MAX_BYTES
        .saturating_add(limits.depth().get().saturating_mul(size_of::<u32>()))
        .saturating_add(20);
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(limits.nodes().get().saturating_mul(2))
            .expect("non-zero value limits produce non-zero reference work limits"),
        ByteLimit::new(
            limits
                .bytes()
                .get()
                .saturating_mul(2)
                .saturating_add(limits.nodes().get().saturating_mul(overhead)),
        )
        .expect("non-zero value limits produce non-zero reference byte limits"),
        limits.depth(),
    )
}

pub(crate) fn validate_effective_local_references(
    schema: &ComponentSchema,
    effective: &ComponentValue,
    contains_entity: impl Fn(&SceneEntityId) -> bool,
    limits: EntityReferenceTraversalLimits,
) -> Result<(), ComponentEntityReferenceRewriteError<SceneLocalTargetMissing>> {
    let plan = plan_declared_entity_references(schema, effective, limits)?;
    plan.visit(effective, |_, reference| match reference {
        EntityReference::SceneLocal { entity } if !contains_entity(entity) => {
            Err(SceneLocalTargetMissing)
        }
        _ => Ok(()),
    })
}

pub(crate) fn validate_scene_structure(document: &SceneDocument) -> DiagnosticReport {
    validate_scene_structure_with_operation(document, None).1
}

fn validate_scene_structure_with_operation(
    document: &SceneDocument,
    operation_index: Option<usize>,
) -> (Option<ValidatedScenePlacement>, DiagnosticReport) {
    let mut diagnostics = DiagnosticReport::default();
    let placement = validate_scene_placement(document, &mut diagnostics, operation_index);
    (placement, diagnostics)
}

fn preflight_scene_with_context_options(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    bindings: Option<&SceneComponentBindings>,
    context: &mut ComponentDecodeContext<'_>,
    options: ScenePreflightOptions,
) -> PreparedScene {
    let ScenePreflightOptions {
        allow_prefab_instances,
        operation_index,
        mode,
        validate_local_references,
    } = options;
    let authority = bindings.map_or(SchemaAuthority::Native(registry), SchemaAuthority::Mixed);
    let (placement, mut diagnostics) =
        validate_scene_structure_with_operation(document, operation_index);
    let execution = match registry.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            diagnostics.push(diagnostic_error(
                "scene.component-registry-not-frozen",
                "Scene validation requires a frozen component registry",
            ));
            return PreparedScene {
                roots: Vec::new(),
                entities: Vec::new(),
                diagnostics,
            };
        }
    };
    if bindings.is_some_and(|bindings| !execution.ptr_eq(bindings.native_snapshot())) {
        diagnostics.push(diagnostic_error(
            "scene.component-bindings-mismatch",
            "Scene bindings do not select the supplied native component registry",
        ));
        return PreparedScene {
            roots: Vec::new(),
            entities: Vec::new(),
            diagnostics,
        };
    }

    for entity in &document.entities {
        if let Some(prefab) = &entity.prefab
            && !allow_prefab_instances
        {
            push_with_operation_index(
                &mut diagnostics,
                with_asset_ref(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.prefab-instance-unsupported",
                                "Prefab instances require expansion before scene spawn",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "field-path",
                        "prefab.source",
                    ),
                    "asset-ref",
                    &prefab.source,
                ),
                operation_index,
            );
        }
    }

    if let Some(entity_id) = placement.as_ref().and_then(|placement| {
        first_unavailable_hierarchy_projection_entity(document, placement, registry)
    }) {
        push_with_operation_index(
            &mut diagnostics,
            with_public_locator(
                diagnostic_error(
                    "scene.hierarchy-projection-unavailable",
                    "Scene hierarchy requires a runtime projection that is not implemented",
                ),
                "entity-id",
                entity_id.as_str(),
            ),
            operation_index,
        );
    }

    let mut prepared_entities = Vec::new();
    let namespace = document
        .entities
        .iter()
        .map(|entity| &entity.id)
        .collect::<BTreeSet<_>>();
    let value_limits = context.materialization_limits();
    let mut value_budget = SceneValueBudget::new(value_limits);
    let traversal_limits = reference_traversal_limits(value_limits);
    for entity in sorted_entities(document) {
        let mut prepared_components = Vec::new();
        let mut prepared_data_components = Vec::new();
        for (component_id, component) in &entity.components {
            let data_owner = bindings.and_then(|bindings| bindings.data_owner(component_id));
            let Some(schema) = authority.schema(component_id) else {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.unknown-component",
                                "Component type is not registered",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            };
            if !authority.owner_matches(component_id, component) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-owner-mismatch",
                                if data_owner.is_some() {
                                    "Persistent component owner does not match the admitted schema owner"
                                } else {
                                    "Persistent component owner does not match the executable schema owner"
                                },
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            }
            if !schema.has_capability(nara_reflect::ComponentCapability::Scene) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-not-scene-capable",
                                "Component is not scene-capable",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            }
            if component.version > schema.version()
                || (data_owner.is_some() && component.version != schema.version())
            {
                push_with_operation_index(
                    &mut diagnostics,
                    component_migration_diagnostic(
                        entity.id.as_str(),
                        component_id.as_str(),
                        &ComponentMigrationError::UnsupportedVersion {
                            component_id: component_id.clone(),
                            from_version: component.version,
                            target_version: schema.version(),
                        },
                    ),
                    operation_index,
                );
                continue;
            }

            // Current-version values remain borrowed until complete effective admission. Only
            // native migration requires an owned input; admit that input before its clone.
            let migrated = if component.version == schema.version() {
                Cow::Borrowed(&component.value)
            } else {
                if let Err(error) = value_budget.admit_input(&component.value) {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
                match registry.migrate_component_value(
                    component_id,
                    component.version,
                    &component.value,
                ) {
                    Ok(migrated) if migrated.version == schema.version() => {
                        Cow::Owned(migrated.value)
                    }
                    result => {
                        let error = match result {
                            Ok(migrated) => ComponentMigrationError::UnsupportedVersion {
                                component_id: component_id.clone(),
                                from_version: migrated.version,
                                target_version: schema.version(),
                            },
                            Err(error) => error,
                        };
                        push_with_operation_index(
                            &mut diagnostics,
                            component_migration_diagnostic(
                                entity.id.as_str(),
                                component_id.as_str(),
                                &error,
                            ),
                            operation_index,
                        );
                        continue;
                    }
                }
            };
            let effective = match value_budget.materialize(schema, &migrated) {
                Ok(effective) => effective,
                Err(error) => {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };

            if matches!(mode, SceneValidationMode::Shape) || data_owner.is_some() {
                let validation = nara_reflect::__private::validate_schema_capabilities(
                    schema,
                    [ComponentCapability::Scene],
                )
                .map_err(|error| ComponentCodecError::Message(error.to_string()))
                .and_then(|()| {
                    validate_component_value_with_limits(schema, &effective, value_limits)
                });
                if let Err(error) = validation {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &error,
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
                if matches!(mode, SceneValidationMode::Shape) {
                    continue;
                }
            }

            let asset_references = match collect_declared_asset_references_from_schema_with_limits(
                schema,
                schema.version(),
                &effective,
                value_limits,
                traversal_limits,
            ) {
                Ok(references) => references,
                Err(_) => {
                    push_with_operation_index(
                        &mut diagnostics,
                        component_payload_diagnostic(
                            entity.id.as_str(),
                            component_id,
                            &ComponentCodecError::Message(
                                "Schema-declared asset reference traversal failed".to_owned(),
                            ),
                            authority,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };
            // This strict database gate proves source identity, not domain product existence.
            if let Some(database) = context.project_asset_database()
                && let Some(reference) = asset_references
                    .iter()
                    .find(|reference| database.resolve_ref(reference.asset_ref()).is_err())
            {
                push_with_operation_index(
                    &mut diagnostics,
                    component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &ComponentCodecError::invalid_asset_ref(
                            reference.path().to_string(),
                            reference.asset_ref().to_string(),
                            "Asset reference is not declared in the project asset database",
                        ),
                        authority,
                    ),
                    operation_index,
                );
                continue;
            }

            if validate_local_references
                && let Err(error) = validate_effective_local_references(
                    schema,
                    &effective,
                    |target| namespace.contains(target),
                    traversal_limits,
                )
            {
                push_with_operation_index(
                    &mut diagnostics,
                    with_entity_reference_rewrite_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.local-reference-invalid",
                                    "Scene component references do not close over the document namespace",
                                ),
                                "entity-id",
                                entity.id.as_str(),
                            ),
                            "component-id",
                            component_id.as_str(),
                        ),
                        &error,
                        |_| "scene-local-target-missing",
                    ),
                    operation_index,
                );
                continue;
            }

            if let Some(owner) = data_owner {
                prepared_data_components.push(PreparedSceneDataComponent {
                    id: component_id.clone(),
                    owner: owner.clone(),
                    record: SceneComponentRecord::new(schema.version(), effective.into_owned())
                        .with_owner(owner.owner_id().clone()),
                });
                continue;
            }

            match execution.preflight_component_with_context(component_id, &effective, context) {
                Ok(prepared) => prepared_components.push(PreparedSceneComponent {
                    id: component_id.clone(),
                    prepared,
                }),
                Err(ComponentCodecError::MissingNativeBinding { .. }) => push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.missing-component-codec",
                                "Component has no scene codec",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                ),
                Err(error) => {
                    let diagnostic = component_payload_diagnostic(
                        entity.id.as_str(),
                        component_id,
                        &error,
                        authority,
                    );
                    push_with_operation_index(&mut diagnostics, diagnostic, operation_index);
                }
            }
        }

        prepared_entities.push(PreparedSceneEntity {
            id: entity.id.clone(),
            children: entity.children.clone(),
            components: prepared_components,
            data_components: prepared_data_components,
        });
    }

    PreparedScene {
        roots: document.roots.clone(),
        entities: prepared_entities,
        diagnostics,
    }
}

/// Logical effective-value admission is shared across entities and representations. Admission
/// remains charged after later validation fails, so a rejected record cannot recycle allowance.
struct SceneValueBudget {
    nodes: usize,
    bytes: usize,
    depth: DepthLimit,
}

impl SceneValueBudget {
    fn new(limits: ComponentValueMaterializationLimits) -> Self {
        Self {
            nodes: limits.nodes().get(),
            bytes: limits.bytes().get(),
            depth: limits.depth(),
        }
    }

    fn admit_input(&self, value: &ComponentValue) -> Result<(), ComponentCodecError> {
        if value.cost_up_to(self.nodes, self.bytes).is_none() {
            return Err(if value.cost_up_to(self.nodes, usize::MAX).is_none() {
                ComponentCodecError::MaterializationNodeLimit {
                    observed: self.nodes.saturating_add(1),
                    maximum: self.nodes,
                }
            } else {
                ComponentCodecError::MaterializationByteLimit {
                    observed: self.bytes.saturating_add(1),
                    maximum: self.bytes,
                }
            });
        }
        validate_input_depth(value, self.depth)
    }

    fn materialize<'value>(
        &mut self,
        schema: &ComponentSchema,
        value: &'value ComponentValue,
    ) -> Result<Cow<'value, ComponentValue>, ComponentCodecError> {
        // Reflect owns the default plan and checks the complete resulting cost before cloning.
        // Raw usize ceilings intentionally preserve a zero residual byte allowance.
        let (effective, cost) = nara_reflect::__private::effective_component_value_with_budget(
            schema, value, self.nodes, self.bytes, self.depth,
        )?;
        self.nodes -= cost.nodes();
        self.bytes -= cost.logical_bytes();
        Ok(effective)
    }
}

fn validate_input_depth(
    value: &ComponentValue,
    maximum: DepthLimit,
) -> Result<(), ComponentCodecError> {
    enum Frame<'value> {
        List(std::slice::Iter<'value, ComponentValue>, usize),
        Map(
            std::collections::btree_map::Iter<'value, String, ComponentValue>,
            usize,
        ),
    }

    let mut pending = Vec::new();
    let mut current = Some((value, 1_usize));
    while let Some((value, depth)) = current {
        if depth > maximum.get() {
            return Err(ComponentCodecError::MaterializationDepthLimit {
                observed: depth,
                maximum: maximum.get(),
            });
        }
        match value {
            ComponentValue::List(values) if !values.is_empty() => {
                pending.push(Frame::List(values.iter(), depth.saturating_add(1)));
            }
            ComponentValue::Map(values) if !values.is_empty() => {
                pending.push(Frame::Map(values.iter(), depth.saturating_add(1)));
            }
            _ => {}
        }
        current = loop {
            let next = match pending.last_mut() {
                Some(Frame::List(values, depth)) => values.next().map(|value| (value, *depth)),
                Some(Frame::Map(values, depth)) => values.next().map(|(_, value)| (value, *depth)),
                None => break None,
            };
            if next.is_some() {
                break next;
            }
            pending.pop();
        };
    }
    Ok(())
}

fn component_payload_diagnostic(
    entity_id: &str,
    component_id: &ComponentTypeId,
    error: &ComponentCodecError,
    authority: SchemaAuthority<'_>,
) -> Diagnostic {
    let diagnostic = with_codec_error(
        with_public_locator(
            with_public_locator(
                diagnostic_error(
                    "scene.invalid-component-payload",
                    "Component payload is invalid",
                ),
                "entity-id",
                entity_id,
            ),
            "component-id",
            component_id.as_str(),
        ),
        error,
        authority.schema(component_id),
    );
    let owner = match authority {
        SchemaAuthority::Mixed(bindings) if bindings.data_owner(component_id).is_some() => bindings
            .data_owner(component_id)
            .map(|owner| owner.owner_id().as_str()),
        _ => authority
            .native_registry()
            .and_then(|registry| registry.component_schema_owner(component_id))
            .map(|owner| owner.as_str()),
    };
    if let Some(owner) = owner {
        with_public_locator(diagnostic, "schema-owner", owner)
    } else {
        diagnostic
    }
}

pub(crate) fn first_unavailable_hierarchy_projection_entity<'document>(
    document: &'document SceneDocument,
    placement: &ValidatedScenePlacement,
    registry: &ComponentRegistry,
) -> Option<&'document SceneEntityId> {
    let requires_unavailable_projection = |component_id: &ComponentTypeId| {
        component_id.as_str() == "nara.scene.Visibility" && registry.schema(component_id).is_some()
    };
    let by_id = document
        .entities
        .iter()
        .map(|entity| (&entity.id, entity))
        .collect::<BTreeMap<_, _>>();

    document.entities.iter().find_map(|entity| {
        let parent_id = placement.parent_of(&entity.id)?;
        let parent = by_id.get(parent_id).copied();
        let requires_unavailable_projection = entity
            .components
            .keys()
            .any(requires_unavailable_projection)
            || parent.is_some_and(|parent| {
                parent
                    .components
                    .keys()
                    .any(requires_unavailable_projection)
            });
        requires_unavailable_projection.then_some(&entity.id)
    })
}

fn component_migration_diagnostic(
    entity_id: &str,
    component_id: &str,
    error: &ComponentMigrationError,
) -> Diagnostic {
    match error {
        ComponentMigrationError::MigrationFailed { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.component-migration-failed",
                        "Component migration failed",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
        ComponentMigrationError::UnknownComponentId { .. }
        | ComponentMigrationError::UnsupportedVersion { .. }
        | ComponentMigrationError::MissingMigration { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.unsupported-component-version",
                        "Component version is unsupported",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
    }
}

fn sorted_entities(document: &SceneDocument) -> Vec<&SceneEntityRecord> {
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities
}

fn validate_scene_placement(
    document: &SceneDocument,
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
) -> Option<ValidatedScenePlacement> {
    let errors_before = diagnostics.stats().observed_errors();
    let mut ids = BTreeSet::<SceneEntityId>::new();
    for entity in &document.entities {
        if !ids.insert(entity.id.clone()) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-entity-id",
                "Scene entity ID is duplicated",
                &entity.id,
                None,
            );
        }
    }

    let mut owners = BTreeMap::<SceneEntityId, PlacementOwner>::new();
    for root in &document.roots {
        if !ids.contains(root) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.dangling-root",
                "Scene root does not identify an entity record",
                root,
                None,
            );
            continue;
        }
        match owners.get(root) {
            None => {
                owners.insert(root.clone(), PlacementOwner::Root);
            }
            Some(PlacementOwner::Root) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-root",
                "Scene root is listed more than once",
                root,
                None,
            ),
            Some(PlacementOwner::Parent(parent)) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.root-child-overlap",
                "Scene entity cannot be both a root and a child",
                root,
                Some(parent),
            ),
        }
    }

    for parent in &document.entities {
        let mut local_children = BTreeSet::<SceneEntityId>::new();
        for child in &parent.children {
            if !local_children.insert(child.clone()) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.duplicate-child",
                    "Scene parent lists the same child more than once",
                    child,
                    Some(&parent.id),
                );
                continue;
            }
            if !ids.contains(child) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.dangling-child",
                    "Scene child does not identify an entity record",
                    child,
                    Some(&parent.id),
                );
                continue;
            }
            match owners.get(child) {
                None => {
                    owners.insert(child.clone(), PlacementOwner::Parent(parent.id.clone()));
                }
                Some(PlacementOwner::Root) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.root-child-overlap",
                    "Scene entity cannot be both a root and a child",
                    child,
                    Some(&parent.id),
                ),
                Some(PlacementOwner::Parent(existing)) if existing == &parent.id => {
                    push_placement_diagnostic(
                        diagnostics,
                        operation_index,
                        "scene.duplicate-child",
                        "Scene parent lists the same child more than once",
                        child,
                        Some(&parent.id),
                    );
                }
                Some(PlacementOwner::Parent(existing)) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.multiple-parents",
                    "Scene entity is owned by more than one parent",
                    child,
                    Some(existing),
                ),
            }
        }
    }

    for id in &ids {
        if !owners.contains_key(id) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.missing-placement",
                "Scene entity is absent from roots and every parent child list",
                id,
                None,
            );
        }
    }

    let parents = owners
        .iter()
        .map(|(entity, owner)| {
            let parent = match owner {
                PlacementOwner::Root => None,
                PlacementOwner::Parent(parent) => Some(parent.clone()),
            };
            (entity.clone(), parent)
        })
        .collect::<BTreeMap<_, _>>();

    let mut completed = BTreeSet::<SceneEntityId>::new();
    for entity in &document.entities {
        if completed.contains(&entity.id) {
            continue;
        }

        let mut path = Vec::<SceneEntityId>::new();
        let mut positions = BTreeMap::<SceneEntityId, usize>::new();
        let mut current = Some(entity.id.clone());
        while let Some(id) = current {
            if completed.contains(&id) {
                break;
            }
            if let Some(cycle_start) = positions.get(&id).copied() {
                let witness = path[cycle_start..]
                    .iter()
                    .min()
                    .expect("cycle path contains the repeated entity")
                    .clone();
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.placement-cycle",
                    "Scene placement contains a cycle",
                    &witness,
                    parents.get(&witness).and_then(Option::as_ref),
                );
                break;
            }
            positions.insert(id.clone(), path.len());
            path.push(id.clone());
            current = parents.get(&id).and_then(Clone::clone);
        }
        completed.extend(path);
    }

    (diagnostics.stats().observed_errors() == errors_before)
        .then_some(ValidatedScenePlacement { parents })
}

fn push_placement_diagnostic(
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
    parent: Option<&SceneEntityId>,
) {
    let diagnostic = with_public_locator(
        diagnostic_error(code, summary),
        "entity-id",
        entity.as_str(),
    );
    let diagnostic = match parent {
        Some(parent) => with_public_locator(diagnostic, "parent-id", parent.as_str()),
        None => diagnostic,
    };
    push_with_operation_index(diagnostics, diagnostic, operation_index);
}

#[cfg(test)]
mod tests {
    use nara_ecs::Component;
    use nara_reflect::{
        ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;

    #[derive(Component)]
    struct NativeFlag(bool);

    fn native_id() -> ComponentTypeId {
        ComponentTypeId::new("a.NativeFlag")
    }

    fn data_id() -> ComponentTypeId {
        ComponentTypeId::new("z.Data")
    }

    fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        )
    }

    fn data_schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
        ComponentSchema::new(data_id(), "Data", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields(fields)
    }

    fn definitions(schema: ComponentSchema) -> (ComponentRegistry, SceneComponentBindings) {
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_with_codec::<NativeFlag, _, _>(
                ComponentSchema::new(native_id(), "Flag", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("x"),
                        "Flag",
                        ComponentFieldPath::from_fields(["x"]),
                        ComponentValueKind::Bool,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                |value| Ok(NativeFlag(value.field_bool("x")?)),
                |flag| Ok(ComponentValue::map([("x", ComponentValue::Bool(flag.0))])),
            )
            .unwrap();
        registry.freeze().unwrap();
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("game.data").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let bindings = SceneComponentBindings::new(registry.snapshot().unwrap(), [owner]).unwrap();
        (registry, bindings)
    }

    fn data_record(value: ComponentValue) -> SceneComponentRecord {
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
            .with_owner(PersistentComponentSchemaOwnerId::try_new("game.data").unwrap())
    }

    fn mixed_document(value: ComponentValue, separate_entities: bool) -> SceneDocument {
        let mut native = SceneEntityRecord::new(SceneEntityId::new("a").unwrap()).with_component(
            native_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("x", ComponentValue::Bool(true))]),
            ),
        );
        if separate_entities {
            SceneDocument::from_ordered_roots([
                native,
                SceneEntityRecord::new(SceneEntityId::new("z").unwrap())
                    .with_component(data_id(), data_record(value)),
            ])
        } else {
            native.components.insert(data_id(), data_record(value));
            SceneDocument::from_ordered_roots([native])
        }
    }

    fn prepare(
        document: &SceneDocument,
        registry: &ComponentRegistry,
        bindings: &SceneComponentBindings,
        limits: ComponentValueMaterializationLimits,
    ) -> PreparedScene {
        preflight_scene_with_context_and_bindings(
            document,
            registry,
            bindings,
            &mut ComponentDecodeContext::new().with_materialization_limits(limits),
        )
    }

    #[test]
    fn native_and_data_share_aggregate_allowance_including_zero_byte_residual() {
        let (registry, bindings) = definitions(data_schema([]));
        for separate_entities in [false, true] {
            let document = mixed_document(ComponentValue::Map(BTreeMap::new()), separate_entities);
            for (nodes, bytes, accepted) in [(3, 2, true), (2, 2, false), (3, 1, false)] {
                let prepared = prepare(&document, &registry, &bindings, limits(nodes, bytes, 2));
                assert_eq!(
                    !prepared.diagnostics.has_errors(),
                    accepted,
                    "{:?}",
                    prepared.diagnostics,
                );
                if accepted {
                    assert_eq!(prepared.entities[0].components.len(), 1);
                    assert_eq!(prepared.entities.last().unwrap().data_components.len(), 1);
                }
            }
        }
    }

    #[test]
    fn defaults_consume_complete_effective_cost_without_changing_source() {
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("amount"),
            "Amount",
            ComponentFieldPath::from_fields(["amount"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        let (registry, bindings) = definitions(schema);
        let document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
        let before = document.clone();
        for (nodes, bytes, accepted) in [(4, 16, true), (3, 16, false), (4, 15, false)] {
            let prepared = prepare(&document, &registry, &bindings, limits(nodes, bytes, 2));
            assert_eq!(
                !prepared.diagnostics.has_errors(),
                accepted,
                "{:?}",
                prepared.diagnostics
            );
            if accepted {
                let data = &prepared.entities[0].data_components[0];
                assert_eq!(data.record.value.field_u64("amount").unwrap(), 7);
                assert!(data.owner.ptr_eq(bindings.data_owner(&data_id()).unwrap()));
            }
        }
        assert_eq!(document, before);
    }

    #[test]
    fn data_requires_explicit_matching_owner_and_current_version() {
        let (registry, bindings) = definitions(data_schema([]));
        for (owner, version, code) in [
            (
                None,
                ComponentSchemaVersion::ONE,
                "scene.component-owner-mismatch",
            ),
            (
                Some(PersistentComponentSchemaOwnerId::try_new("game.other").unwrap()),
                ComponentSchemaVersion::ONE,
                "scene.component-owner-mismatch",
            ),
            (
                Some(PersistentComponentSchemaOwnerId::try_new("game.data").unwrap()),
                ComponentSchemaVersion(2),
                "scene.unsupported-component-version",
            ),
        ] {
            let mut document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
            let record = document.entities[0].components.get_mut(&data_id()).unwrap();
            record.owner = owner;
            record.version = version;
            let prepared = prepare(&document, &registry, &bindings, limits(8, 32, 3));
            assert!(prepared.entities[0].data_components.is_empty());
            assert!(
                prepared
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code)
            );
        }
    }

    #[test]
    fn data_default_local_references_close_over_the_shared_namespace() {
        let target = SceneEntityId::new("target").unwrap();
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: target.clone(),
            }),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef)]);
        let (registry, bindings) = definitions(schema);
        for include_target in [false, true] {
            let mut document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
            if include_target {
                document.roots.push(target.clone());
                document
                    .entities
                    .push(SceneEntityRecord::new(target.clone()));
            }
            let prepared = prepare(&document, &registry, &bindings, limits(8, 64, 3));
            assert_eq!(
                !prepared.diagnostics.has_errors(),
                include_target,
                "{:?}",
                prepared.diagnostics
            );
            if !include_target {
                assert!(prepared.entities[0].data_components.is_empty());
                assert!(prepared.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.local-reference-invalid"
                }));
            }
        }
    }

    #[test]
    fn data_default_assets_require_selected_project_metadata() {
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueKind::AssetRef,
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                (
                    "value",
                    ComponentValue::String("textures/missing.png".into()),
                ),
            ]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]);
        let (registry, bindings) = definitions(schema);
        let document = mixed_document(ComponentValue::Map(BTreeMap::new()), false);
        let database = nara_asset::ProjectAssetDatabase::default();
        let mut context = ComponentDecodeContext::new().with_project_asset_database(&database);
        let prepared = preflight_scene_with_context_and_bindings(
            &document,
            &registry,
            &bindings,
            &mut context,
        );
        assert!(prepared.diagnostics.has_errors());
        assert!(prepared.entities[0].data_components.is_empty());
    }

    #[test]
    fn bounded_input_and_no_default_effective_values_enforce_depth() {
        let value = ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]);
        let mut budget = SceneValueBudget::new(limits(3, 1, 2));
        assert!(matches!(
            budget.admit_input(&value),
            Err(ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2
            })
        ));
        assert!(matches!(
            budget.materialize(&data_schema([]), &value),
            Err(ComponentCodecError::MaterializationDepthLimit {
                observed: 3,
                maximum: 2
            })
        ));
    }

    #[test]
    fn zero_byte_residual_accepts_zero_cost_values_and_rejects_positive_defaults() {
        let mut budget = SceneValueBudget {
            nodes: 4,
            bytes: 0,
            depth: DepthLimit::new(2).unwrap(),
        };
        let empty_schema = data_schema([]);
        assert!(
            budget
                .materialize(&empty_schema, &ComponentValue::Null)
                .is_ok()
        );
        let schema = data_schema([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("x"),
            "Flag",
            ComponentFieldPath::from_fields(["x"]),
            ComponentValueKind::Bool,
            ComponentValue::Null,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        assert!(matches!(
            budget.materialize(&schema, &ComponentValue::Map(BTreeMap::new())),
            Err(ComponentCodecError::MaterializationByteLimit { maximum: 0, .. })
        ));
        assert_eq!(budget.nodes, 3);
        assert_eq!(budget.bytes, 0);
    }

    #[test]
    fn native_migration_input_is_admitted_before_invoking_the_hook() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_with_codec::<NativeFlag, _, _>(
                ComponentSchema::new(native_id(), "Flag", ComponentSchemaVersion(2))
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("x"),
                        "Flag",
                        ComponentFieldPath::from_fields(["x"]),
                        ComponentValueKind::Bool,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                |value| Ok(NativeFlag(value.field_bool("x")?)),
                |flag| Ok(ComponentValue::map([("x", ComponentValue::Bool(flag.0))])),
            )
            .unwrap();
        let hook_calls = Arc::clone(&calls);
        registry
            .register_component_migration(
                &native_id(),
                ComponentSchemaVersion::ONE,
                ComponentSchemaVersion(2),
                move |_| {
                    hook_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(ComponentValue::map([("x", ComponentValue::Bool(true))]))
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        for (value, limits) in [
            (
                ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]),
                limits(8, 8, 2),
            ),
            (ComponentValue::String("too long".into()), limits(8, 1, 8)),
            (
                ComponentValue::List(vec![ComponentValue::Null; 2]),
                limits(2, 8, 8),
            ),
        ] {
            let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("source").unwrap(),
            )
            .with_component(
                native_id(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            )]);
            let prepared = preflight_scene_with_context(
                &document,
                &registry,
                &mut ComponentDecodeContext::new().with_materialization_limits(limits),
            );
            assert!(prepared.diagnostics.has_errors());
            assert!(prepared.entities[0].components.is_empty());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("source").unwrap(),
        )
        .with_component(
            native_id(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
        let prepared = preflight_scene_with_context(
            &document,
            &registry,
            &mut ComponentDecodeContext::new().with_materialization_limits(limits(2, 2, 2)),
        );
        assert!(
            !prepared.diagnostics.has_errors(),
            "{:?}",
            prepared.diagnostics
        );
        assert_eq!(prepared.entities[0].components.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn patch_shape_mode_retains_operation_diagnostics_without_native_decode() {
        let (registry, _) = definitions(data_schema([]));
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("source").unwrap(),
        )
        .with_component(
            native_id(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("x", ComponentValue::U64(1))]),
            ),
        )]);
        let diagnostics = validate_patch_intermediate_scene_with_limits(
            &document,
            &registry,
            7,
            ComponentValueMaterializationLimits::default(),
        );
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.invalid-component-payload"
                && diagnostic
                    .fields()
                    .iter()
                    .any(|field| field.key().as_str() == "operation-index")
        }));
    }
}
