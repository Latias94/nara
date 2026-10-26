use nara_reflect::{
    ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueCost,
    EntityReference, validate_component_value_with_limits,
};

use crate::{
    PrefabComponentIntent, PrefabDocument, PrefabInstance, PrefabPropertyIntent,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SceneFileLimits,
    SceneFilePublicationError, SceneFormatError, ScenePatchDocument, ScenePatchOperation,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, with_migration_error, with_public_identifier,
        with_public_locator, with_public_u64,
    },
};

pub(crate) fn canonicalize_scene_document(
    mut document: SceneDocument,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) -> Result<(SceneDocument, bool), SceneFilePublicationError> {
    require_frozen_registry(registry)?;
    let mut budget = PublicationValueBudget::new(limits);
    let changed = canonicalize_entities(&mut document.entities, registry, &mut budget)?;
    document.canonicalize();
    if changed {
        crate::format::validate_scene_publication_shape(&document, limits)
            .map_err(post_migration_format_error)?;
    }
    Ok((document, changed))
}

pub(crate) fn canonicalize_prefab_document(
    mut document: PrefabDocument,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) -> Result<(PrefabDocument, bool), SceneFilePublicationError> {
    require_frozen_registry(registry)?;
    let mut budget = PublicationValueBudget::new(limits);
    let changed = canonicalize_entities(&mut document.entities, registry, &mut budget)?;
    document.canonicalize();
    if changed {
        crate::format::validate_prefab_publication_shape(&document, limits)
            .map_err(post_migration_format_error)?;
    }
    Ok((document, changed))
}

pub(crate) fn canonicalize_scene_patch(
    mut patch: ScenePatchDocument,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) -> Result<(ScenePatchDocument, bool), SceneFilePublicationError> {
    require_frozen_registry(registry)?;
    let mut budget = PublicationValueBudget::new(limits);
    let changed = canonicalize_patch(&mut patch, registry, &mut budget)?;
    if changed {
        crate::format::validate_patch_publication_shape(&patch, limits)
            .map_err(post_migration_format_error)?;
    }
    Ok((patch, changed))
}

fn require_frozen_registry(registry: &ComponentRegistry) -> Result<(), SceneFilePublicationError> {
    if registry.is_frozen() {
        return Ok(());
    }
    Err(publication_error(diagnostic_error(
        "scene.component-registry-not-frozen",
        "Component registry must be frozen before scene publication",
    )))
}

fn canonicalize_entities(
    entities: &mut [SceneEntityRecord],
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<bool, SceneFilePublicationError> {
    let mut changed = false;
    for entity in entities {
        changed |= canonicalize_entity(entity, registry, budget)?;
    }
    Ok(changed)
}

fn canonicalize_entity(
    entity: &mut SceneEntityRecord,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<bool, SceneFilePublicationError> {
    let mut changed = false;
    for (component_id, component) in &mut entity.components {
        changed |= migrate_component_record(&entity.id, component_id, component, registry, budget)?;
    }
    // Overlay claims are source declarations. Their Schema/selector/base predicates belong
    // to the surviving selection, so a replaced obsolete declaration must not reject here.
    // Raw admission already charged the complete tree, including inactive declarations.
    Ok(changed)
}

fn canonicalize_patch(
    patch: &mut ScenePatchDocument,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<bool, SceneFilePublicationError> {
    let mut changed = false;
    for operation in &mut patch.operations {
        match operation {
            ScenePatchOperation::AddEntity { entity, .. } => {
                changed |= canonicalize_entity(entity, registry, budget)?;
            }
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                ..
            } => {
                changed |= canonicalize_entity(entity, registry, budget)?;
                for descendant in descendants {
                    changed |= canonicalize_entity(descendant, registry, budget)?;
                }
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
                changed |= migrate_component_record(entity, component, value, registry, budget)?;
            }
            ScenePatchOperation::SetField {
                entity,
                component,
                component_version,
                field,
                value,
            } => {
                changed |= canonicalize_field_target(
                    entity,
                    component,
                    component_version,
                    field.as_str(),
                    registry,
                    FieldVersionPolicy::RequireCurrentValueSemantics,
                )?;
                budget.observe(value)?;
            }
            ScenePatchOperation::RemoveField {
                entity,
                component,
                component_version,
                field,
            } => {
                changed |= canonicalize_field_target(
                    entity,
                    component,
                    component_version,
                    field.as_str(),
                    registry,
                    FieldVersionPolicy::StableIdentityOnly,
                )?;
            }
            ScenePatchOperation::SetAssetRefField {
                entity,
                component,
                component_version,
                field,
                ..
            } => {
                changed |= canonicalize_field_target(
                    entity,
                    component,
                    component_version,
                    field.as_str(),
                    registry,
                    FieldVersionPolicy::RequireCurrentValueSemantics,
                )?;
            }
            ScenePatchOperation::SetPrefabInstance {
                expected, value, ..
            } => {
                // Both sides are equality evidence. Check exact record versions without
                // rewriting claims or resolving activation-only targets and selectors.
                for instance in expected.iter().chain(value.iter()) {
                    validate_exact_instance_snapshot(instance, registry, budget)?;
                }
            }
            ScenePatchOperation::RemoveEntity { .. }
            | ScenePatchOperation::RemoveComponent { .. }
            | ScenePatchOperation::MoveEntity { .. } => {}
        }
    }
    Ok(changed)
}

fn validate_exact_instance_snapshot(
    instance: &PrefabInstance,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<(), SceneFilePublicationError> {
    let Some(overlay) = &instance.overrides else {
        return Ok(());
    };
    for addition in &overlay.additions {
        for (component, record) in &addition.components {
            validate_exact_component_record(&addition.id, component, record, registry, budget)?;
        }
        if let Some(instance) = &addition.prefab {
            validate_exact_instance_snapshot(instance, registry, budget)?;
        }
    }
    for claim in &overlay.instances {
        if let Some(instance) = &claim.value {
            validate_exact_instance_snapshot(instance, registry, budget)?;
        }
    }
    for intent in &overlay.components {
        match intent {
            PrefabComponentIntent::Set {
                target,
                component,
                record,
                ..
            } => {
                validate_exact_component_record(
                    target.entity.entity(),
                    component,
                    record,
                    registry,
                    budget,
                )?;
            }
            PrefabComponentIntent::Properties {
                target,
                component,
                version,
                claims,
            } => {
                let entity = target.entity.entity();
                let schema = require_component_schema(entity, component, registry)?;
                require_exact_snapshot_version(entity, component, *version, schema.version)?;
                for claim in claims {
                    if let PrefabPropertyIntent::Set(value) = &claim.intent {
                        budget.observe(value)?;
                    }
                }
            }
            PrefabComponentIntent::Suppress { .. } => {}
        }
    }
    Ok(())
}

fn require_component_schema<'a>(
    entity: &SceneEntityId,
    component_id: &ComponentTypeId,
    registry: &'a ComponentRegistry,
) -> Result<&'a nara_reflect::ComponentSchema, SceneFilePublicationError> {
    registry.schema(component_id).ok_or_else(|| {
        publication_error(with_public_locator(
            with_public_locator(
                diagnostic_error(
                    "scene.unknown-component",
                    "Component type is not registered",
                ),
                "entity-id",
                entity.as_str(),
            ),
            "component-id",
            component_id.as_str(),
        ))
    })
}

fn validate_exact_component_record(
    entity: &SceneEntityId,
    component_id: &ComponentTypeId,
    record: &SceneComponentRecord,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<(), SceneFilePublicationError> {
    let schema = require_component_schema(entity, component_id, registry)?;
    require_exact_snapshot_version(entity, component_id, record.version, schema.version)?;
    budget.observe(&record.value)?;
    validate_component_value_with_limits(
        schema,
        &record.value,
        budget.limits.component_materialization_limits(),
    )
    .map_err(|error| {
        publication_error(crate::diagnostics::with_codec_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.file-exact-snapshot-value-invalid",
                        "Exact Prefab snapshot component value does not match its current schema",
                    ),
                    "entity-id",
                    entity.as_str(),
                ),
                "component-id",
                component_id.as_str(),
            ),
            &error,
            Some(schema),
        ))
    })
}

fn migrate_component_record(
    entity: &SceneEntityId,
    component_id: &ComponentTypeId,
    record: &mut SceneComponentRecord,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<bool, SceneFilePublicationError> {
    require_component_schema(entity, component_id, registry)?;
    let migrated = registry
        .migrate_component_value(component_id, record.version, &record.value)
        .map_err(|error| {
            publication_error(with_migration_error(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.file-component-migration-failed",
                            "Persistent scene component migration failed",
                        ),
                        "entity-id",
                        entity.as_str(),
                    ),
                    "component-id",
                    component_id.as_str(),
                ),
                &error,
            ))
        })?;
    budget.observe(&migrated.value)?;
    let changed = migrated.version != record.version || migrated.value != record.value;
    if changed {
        record.version = migrated.version;
        record.value = migrated.value;
    }
    Ok(changed)
}

fn canonicalize_field_target(
    entity: &SceneEntityId,
    component_id: &ComponentTypeId,
    component_version: &mut ComponentSchemaVersion,
    field_id: &str,
    registry: &ComponentRegistry,
    version_policy: FieldVersionPolicy,
) -> Result<bool, SceneFilePublicationError> {
    let Some(schema) = registry.schema(component_id) else {
        return Err(publication_error(with_public_locator(
            with_public_locator(
                diagnostic_error(
                    "scene.file-unknown-component",
                    "Persistent scene patch targets an unknown component",
                ),
                "entity-id",
                entity.as_str(),
            ),
            "component-id",
            component_id.as_str(),
        )));
    };
    if *component_version > schema.version {
        return Err(publication_error(with_public_u64(
            with_public_u64(
                with_public_locator(
                    with_public_locator(
                        diagnostic_error(
                            "scene.file-component-version-unsupported",
                            "Persistent scene patch targets a future component version",
                        ),
                        "entity-id",
                        entity.as_str(),
                    ),
                    "component-id",
                    component_id.as_str(),
                ),
                "source-version",
                u64::from(component_version.get()),
            ),
            "target-version",
            u64::from(schema.version.get()),
        )));
    }
    let field = nara_reflect::ComponentFieldId::new(field_id);
    if registry.resolve_field(component_id, &field).is_none() {
        return Err(publication_error(with_public_locator(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.file-unknown-component-field",
                        "Persistent scene patch targets an unknown or removed component field",
                    ),
                    "entity-id",
                    entity.as_str(),
                ),
                "component-id",
                component_id.as_str(),
            ),
            "field-id",
            field_id,
        )));
    }

    if *component_version < schema.version
        && version_policy == FieldVersionPolicy::RequireCurrentValueSemantics
    {
        return Err(publication_error(with_public_locator(
            with_public_u64(
                with_public_u64(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.file-field-value-migration-required",
                                "Persistent field writes require an explicit value migration",
                            ),
                            "entity-id",
                            entity.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    "source-version",
                    u64::from(component_version.get()),
                ),
                "target-version",
                u64::from(schema.version.get()),
            ),
            "field-id",
            field_id,
        )));
    }

    let changed = *component_version != schema.version;
    *component_version = schema.version;
    Ok(changed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldVersionPolicy {
    StableIdentityOnly,
    RequireCurrentValueSemantics,
}

fn require_exact_snapshot_version(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    current: ComponentSchemaVersion,
) -> Result<(), SceneFilePublicationError> {
    if version == current {
        return Ok(());
    }
    Err(publication_error(with_public_u64(
        with_public_u64(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.file-exact-snapshot-version-mismatch",
                        "Exact Prefab snapshots require current component versions without migration",
                    ),
                    "entity-id",
                    entity.as_str(),
                ),
                "component-id",
                component.as_str(),
            ),
            "source-version",
            u64::from(version.get()),
        ),
        "target-version",
        u64::from(current.get()),
    )))
}

struct PublicationValueBudget {
    cost: ComponentValueCost,
    shape_nodes: usize,
    total_string_bytes: usize,
    limits: SceneFileLimits,
}

impl PublicationValueBudget {
    fn new(limits: SceneFileLimits) -> Self {
        Self {
            cost: ComponentValueCost::ZERO,
            shape_nodes: 0,
            total_string_bytes: 0,
            limits,
        }
    }

    fn observe(&mut self, value: &ComponentValue) -> Result<(), SceneFilePublicationError> {
        let (shape_nodes, total_string_bytes) = self.validate_shape(value)?;
        let cost = value.cost();
        let observed = self.cost.saturating_add(cost);
        for (kind, observed, maximum) in [
            (
                "nodes",
                observed.nodes(),
                self.limits.component_value_nodes().get(),
            ),
            (
                "logical-bytes",
                observed.logical_bytes(),
                self.limits.component_value_bytes().get(),
            ),
        ] {
            if observed > maximum {
                return Err(publication_error(with_public_identifier(
                    with_public_u64(
                        with_public_u64(
                            diagnostic_error(
                                "scene.file-component-value-budget-exceeded",
                                "Migrated persistent component values exceed the publication budget",
                            ),
                            "observed",
                            usize_to_u64(observed),
                        ),
                        "maximum",
                        usize_to_u64(maximum),
                    ),
                    "budget-kind",
                    kind,
                )));
            }
        }
        self.cost = observed;
        self.shape_nodes = shape_nodes;
        self.total_string_bytes = total_string_bytes;
        Ok(())
    }

    fn validate_shape(
        &self,
        value: &ComponentValue,
    ) -> Result<(usize, usize), SceneFilePublicationError> {
        let shape = self.limits.shape();
        let mut shape_nodes = self.shape_nodes;
        let mut total_string_bytes = self.total_string_bytes;
        let mut pending = vec![(value, 1_usize)];

        while let Some((value, depth)) = pending.pop() {
            require_shape_limit("depth", depth, shape.depth().get())?;
            shape_nodes = shape_nodes.saturating_add(1);
            require_shape_limit("nodes", shape_nodes, shape.nodes().get())?;

            match value {
                ComponentValue::Null
                | ComponentValue::Bool(_)
                | ComponentValue::I64(_)
                | ComponentValue::U64(_)
                | ComponentValue::F64(_) => {}
                ComponentValue::String(value) => {
                    observe_string(value, &mut total_string_bytes, self.limits)?
                }
                ComponentValue::List(values) => {
                    require_shape_limit(
                        "container-items",
                        values.len(),
                        shape.container_items().get(),
                    )?;
                    let child_depth = depth.saturating_add(1);
                    pending.extend(values.iter().rev().map(|value| (value, child_depth)));
                }
                ComponentValue::Map(values) => {
                    require_shape_limit(
                        "container-items",
                        values.len(),
                        shape.container_items().get(),
                    )?;
                    let child_depth = depth.saturating_add(1);
                    for (key, value) in values.iter().rev() {
                        require_shape_limit("depth", child_depth, shape.depth().get())?;
                        shape_nodes = shape_nodes.saturating_add(1);
                        require_shape_limit("nodes", shape_nodes, shape.nodes().get())?;
                        observe_string(key, &mut total_string_bytes, self.limits)?;
                        pending.push((value, child_depth));
                    }
                }
                ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) => {
                    observe_string(entity.as_str(), &mut total_string_bytes, self.limits)?;
                }
                ComponentValue::EntityReference(EntityReference::Persistent { entity }) => {
                    observe_string(
                        entity.namespace.as_str(),
                        &mut total_string_bytes,
                        self.limits,
                    )?;
                }
            }
        }

        Ok((shape_nodes, total_string_bytes))
    }
}

fn observe_string(
    value: &str,
    total_string_bytes: &mut usize,
    limits: SceneFileLimits,
) -> Result<(), SceneFilePublicationError> {
    let shape = limits.shape();
    require_shape_limit("string-bytes", value.len(), shape.string_bytes().get())?;
    *total_string_bytes = total_string_bytes.saturating_add(value.len());
    require_shape_limit(
        "total-string-bytes",
        *total_string_bytes,
        shape.total_string_bytes().get(),
    )
}

fn require_shape_limit(
    kind: &'static str,
    observed: usize,
    maximum: usize,
) -> Result<(), SceneFilePublicationError> {
    if observed <= maximum {
        return Ok(());
    }
    Err(publication_error(with_public_identifier(
        with_public_u64(
            with_public_u64(
                diagnostic_error(
                    "scene.file-component-value-shape-budget-exceeded",
                    "Migrated persistent component values exceed the structural publication budget",
                ),
                "observed",
                usize_to_u64(observed),
            ),
            "maximum",
            usize_to_u64(maximum),
        ),
        "budget-kind",
        kind,
    )))
}

fn publication_error(diagnostic: nara_diagnostic::Diagnostic) -> SceneFilePublicationError {
    let mut diagnostics = nara_diagnostic::DiagnosticReport::default();
    diagnostics.push(diagnostic);
    SceneFilePublicationError::new(diagnostics)
}

fn post_migration_format_error(error: SceneFormatError) -> SceneFilePublicationError {
    let kind = match error {
        SceneFormatError::EncodedBytesExceeded { .. } => "encoded-bytes",
        SceneFormatError::Shape { .. } => "shape",
        SceneFormatError::Header { .. } => "header",
        SceneFormatError::Contract(_) => "contract",
        SceneFormatError::PatchFormatVersion { .. } => "patch-version",
        SceneFormatError::LegacyPlacementShape { .. } => "legacy-placement-shape",
        SceneFormatError::Payload { .. } => "payload",
        SceneFormatError::Budget(_) => "domain-budget",
        SceneFormatError::Encode { .. } => "encode",
        SceneFormatError::Metadata(_) => "metadata",
    };
    publication_error(with_public_identifier(
        diagnostic_error(
            "scene.file-post-migration-format-invalid",
            "Migrated persistent scene data does not satisfy the file publication contract",
        ),
        "format-error-kind",
        kind,
    ))
}

#[cfg(test)]
mod overlay_snapshot_tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use nara_asset::{AssetPath, AssetRef};
    use nara_ecs::Component;
    use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentPropertyPath, ComponentSchema, ComponentValueKind,
    };

    use super::*;
    use crate::{
        PrefabAddition, PrefabComponentSetMode, PrefabEntityTarget, PrefabInstanceClaim,
        PrefabOverlay, PrefabPropertyClaim, SourceDocumentKey, SourceDocumentKind,
    };

    fn component() -> ComponentTypeId {
        ComponentTypeId::new("test.snapshot")
    }

    #[derive(Component)]
    struct SnapshotComponent {
        number: u64,
    }

    fn registry(calls: Arc<AtomicUsize>) -> ComponentRegistry {
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_with_codec::<SnapshotComponent, _, _>(
                ComponentSchema::new(component(), "Snapshot", ComponentSchemaVersion(2))
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("number"),
                        "Number",
                        ComponentFieldPath::from_fields(["number"]),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                |value| {
                    Ok(SnapshotComponent {
                        number: value.field_u64("number")?,
                    })
                },
                |value| {
                    Ok(ComponentValue::map([(
                        "number",
                        ComponentValue::U64(value.number),
                    )]))
                },
            )
            .unwrap();
        registry
            .register_component_migration(
                &component(),
                ComponentSchemaVersion::ONE,
                ComponentSchemaVersion(2),
                move |value| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(value)
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        registry
    }

    fn overlay() -> PrefabOverlay {
        PrefabOverlay::new(
            SourceDocumentKey::named(
                "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
                "base",
                SourceDocumentKind::Prefab,
            )
            .unwrap(),
        )
    }

    fn instance(overlay: PrefabOverlay) -> PrefabInstance {
        PrefabInstance {
            source: AssetRef::Path(AssetPath::new("p.prefab.json").unwrap()),
            overrides: Some(overlay),
        }
    }

    fn old_record() -> SceneComponentRecord {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("number", ComponentValue::U64(7))]),
        )
    }

    #[test]
    fn direct_records_migrate_while_overlay_records_remain_original_source_data() {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = registry(Arc::clone(&calls));
        let mut declaration = overlay();
        let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        addition.components.insert(component(), old_record());
        declaration.additions.push(addition);
        let declaration = instance(declaration);
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        entity.components.insert(component(), old_record());
        entity.prefab = Some(declaration.clone());
        let document = SceneDocument::from_ordered_roots([entity]);
        let (canonical, changed) =
            canonicalize_scene_document(document, &registry, SceneFileLimits::default()).unwrap();
        assert!(changed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            canonical.entities[0].components[&component()].version,
            ComponentSchemaVersion(2)
        );
        assert_eq!(canonical.entities[0].prefab.as_ref(), Some(&declaration));
    }

    #[test]
    fn exact_snapshot_sides_never_invoke_migrations_in_nested_additions_or_claims() {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = registry(Arc::clone(&calls));
        let mut old = overlay();
        old.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(SceneEntityId::new("target").unwrap()),
            component: component(),
            record: old_record(),
            bindings: Vec::new(),
        });
        let mut through_addition = overlay();
        let mut addition = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        addition.prefab = Some(instance(old.clone()));
        through_addition.additions.push(addition);
        let mut through_claim = overlay();
        through_claim.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(SceneEntityId::new("nested").unwrap()),
            value: Some(instance(old)),
        });
        for declaration in [through_addition, through_claim] {
            for expected_side in [true, false] {
                let declaration = instance(declaration.clone());
                let (expected, value) = if expected_side {
                    (Some(declaration), None)
                } else {
                    (None, Some(declaration))
                };
                let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                    entity: SceneEntityId::new("anchor").unwrap(),
                    expected,
                    value,
                }]);
                let error = canonicalize_scene_patch(patch, &registry, SceneFileLimits::default())
                    .unwrap_err();
                assert!(error.diagnostics().iter().any(|entry| {
                    entry.code().as_str() == "scene.file-exact-snapshot-version-mismatch"
                }));
                assert_eq!(calls.load(Ordering::SeqCst), 0);
            }
        }
    }

    #[test]
    fn exact_property_snapshot_preserves_unresolved_selectors_without_rewriting() {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = registry(Arc::clone(&calls));
        let mut declaration = overlay();
        declaration
            .components
            .push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(SceneEntityId::new("missing").unwrap()),
                component: component(),
                version: ComponentSchemaVersion(2),
                claims: vec![PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("removed")),
                    intent: PrefabPropertyIntent::Set(ComponentValue::String(
                        "unresolved".to_owned(),
                    )),
                    bindings: Vec::new(),
                }],
            });
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected: Some(instance(declaration)),
            value: None,
        }]);
        let (canonical, changed) =
            canonicalize_scene_patch(patch.clone(), &registry, SceneFileLimits::default()).unwrap();
        assert!(!changed);
        assert_eq!(canonical, patch);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn exact_property_version_and_complete_record_shape_still_require_exact_admission() {
        let registry = registry(Arc::new(AtomicUsize::new(0)));
        let mut old_property = overlay();
        old_property
            .components
            .push(PrefabComponentIntent::Properties {
                target: PrefabEntityTarget::source(SceneEntityId::new("missing").unwrap()),
                component: component(),
                version: ComponentSchemaVersion::ONE,
                claims: Vec::new(),
            });
        let mut invalid_record = overlay();
        invalid_record.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: PrefabEntityTarget::source(SceneEntityId::new("missing").unwrap()),
            component: component(),
            record: SceneComponentRecord::new(
                ComponentSchemaVersion(2),
                ComponentValue::map([("number", ComponentValue::String("invalid".to_owned()))]),
            ),
            bindings: Vec::new(),
        });
        for (declaration, code) in [
            (old_property, "scene.file-exact-snapshot-version-mismatch"),
            (invalid_record, "scene.file-exact-snapshot-value-invalid"),
        ] {
            let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: SceneEntityId::new("anchor").unwrap(),
                expected: Some(instance(declaration)),
                value: None,
            }]);
            let error =
                canonicalize_scene_patch(patch, &registry, SceneFileLimits::default()).unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|entry| entry.code().as_str() == code)
            );
        }
    }
}
