use nara_reflect::{
    ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueCost,
    EntityReference,
};

use crate::{
    PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneFileLimits, SceneFilePublicationError, SceneFormatError, ScenePatchDocument,
    ScenePatchOperation,
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
    if let Some(prefab) = &mut entity.prefab {
        changed |= canonicalize_patch(&mut prefab.overrides, registry, budget)?;
    }
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
            ScenePatchOperation::AddEntity { entity } => {
                changed |= canonicalize_entity(entity, registry, budget)?;
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
            ScenePatchOperation::RemoveEntity { .. }
            | ScenePatchOperation::RemoveComponent { .. }
            | ScenePatchOperation::Reparent { .. } => {}
        }
    }
    Ok(changed)
}

fn migrate_component_record(
    entity: &SceneEntityId,
    component_id: &ComponentTypeId,
    record: &mut SceneComponentRecord,
    registry: &ComponentRegistry,
    budget: &mut PublicationValueBudget,
) -> Result<bool, SceneFilePublicationError> {
    if registry.schema(component_id).is_none() {
        return Err(publication_error(with_public_locator(
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
        )));
    }
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
        *record = SceneComponentRecord::new(migrated.version, migrated.value);
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
        SceneFormatError::EmbeddedPatchFormatVersion { .. } => "embedded-patch-version",
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
