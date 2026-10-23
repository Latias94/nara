use std::collections::{BTreeMap, BTreeSet};

use nara_asset::AssetRefExportPolicy;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Entity, World};
use nara_hierarchy::{Children, HierarchyRoots, Parent, validate_hierarchy};
use nara_identity::{
    EntityLookup, EntityReference, RuntimeEntityReference, SceneInstanceId, WorldEntityLocator,
    WorldIdentityDomain, WorldIdentityDomainId,
};
use nara_reflect::{
    ComponentCapability, ComponentEncodeContext, ComponentRegistry, ComponentValue,
    ComponentValueMaterializationLimits, effective_component_value_owned,
    rewrite_declared_entity_references,
};

use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, with_codec_error, with_entity_reference_rewrite_error,
        with_public_locator, with_public_u64,
    },
    validation::reference_traversal_limits,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneExportRemap {
    source_domain: WorldIdentityDomainId,
    entries: BTreeMap<WorldEntityLocator, SceneEntityId>,
}

impl SceneExportRemap {
    /// Returns the document-local ID for one active source identity axis.
    ///
    /// An exported entity with both scene and persistent axes contributes two locators that map to
    /// the same document ID, so the remap length can exceed the number of document records.
    #[must_use]
    pub const fn source_domain_id(&self) -> WorldIdentityDomainId {
        self.source_domain
    }

    #[must_use]
    pub fn get(&self, locator: &WorldEntityLocator) -> Option<&SceneEntityId> {
        self.entries.get(locator)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&WorldEntityLocator, &SceneEntityId)> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneExportOutput {
    pub document: SceneDocument,
    pub remap: SceneExportRemap,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SceneExportReport {
    output: Option<SceneExportOutput>,
    pub diagnostics: DiagnosticReport,
}

impl SceneExportReport {
    #[must_use]
    pub const fn output(&self) -> Option<&SceneExportOutput> {
        self.output.as_ref()
    }

    #[must_use]
    pub fn into_output(self) -> Option<SceneExportOutput> {
        self.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneExportOptions {
    pub asset_ref_export_policy: AssetRefExportPolicy,
    /// Bounds the aggregate logical value retained by one export and each value's depth.
    pub value_limits: ComponentValueMaterializationLimits,
}

impl Default for SceneExportOptions {
    fn default() -> Self {
        Self {
            asset_ref_export_policy: AssetRefExportPolicy::Path,
            value_limits: ComponentValueMaterializationLimits::default(),
        }
    }
}

#[derive(Debug)]
struct ExportEntity {
    locator: WorldEntityLocator,
    entity: Entity,
    instance: SceneInstanceId,
    authored_id: SceneEntityId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportReferenceResolutionError {
    SceneLocalTargetMissing,
    SceneLocalTargetTombstoned,
    SceneLocalTargetStale,
    PersistentTargetMissing,
    PersistentTargetTombstoned,
    PersistentTargetUnloaded,
    PersistentTargetDeleted,
    PersistentTargetStale,
    InvalidIdentityState,
}

#[must_use]
pub fn export_scene(world: &World, registry: &ComponentRegistry) -> SceneExportReport {
    export_scene_with_options(world, registry, SceneExportOptions::default())
}

#[must_use]
pub fn export_scene_with_options(
    world: &World,
    registry: &ComponentRegistry,
    options: SceneExportOptions,
) -> SceneExportReport {
    let mut diagnostics = DiagnosticReport::default();
    let mut exported_value_nodes = 0_usize;
    let mut exported_value_bytes = 0_usize;
    let execution = match registry.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => {
            diagnostics.push(diagnostic_error(
                "scene.component-registry-not-frozen",
                "Component registry must be frozen before scene publication",
            ));
            return SceneExportReport {
                output: None,
                diagnostics,
            };
        }
    };
    let schemas = execution
        .catalog()
        .components
        .iter()
        .filter(|schema| schema.has_capability(ComponentCapability::Scene))
        .collect::<Vec<_>>();
    let Some(domain) = world.get_resource::<WorldIdentityDomain>() else {
        diagnostics.push(diagnostic_error(
            "scene.export-identity-domain-missing",
            "Scene export requires a world identity domain",
        ));
        return SceneExportReport {
            output: None,
            diagnostics,
        };
    };
    if validate_hierarchy(world).is_err() {
        diagnostics.push(diagnostic_error(
            "scene.export-runtime-hierarchy-invalid",
            "Scene export requires a valid ordered runtime hierarchy",
        ));
        return SceneExportReport {
            output: None,
            diagnostics,
        };
    }

    let registered_locators = match domain.registered_locators(world) {
        Ok(locators) => {
            let mut locators = locators.collect::<Vec<_>>();
            locators.sort();
            locators
        }
        Err(_) => {
            diagnostics.push(diagnostic_error(
                "scene.export-identity-domain-invalid",
                "Scene export identity domain is not valid for this world",
            ));
            return SceneExportReport {
                output: None,
                diagnostics,
            };
        }
    };

    let mut export_entities = Vec::with_capacity(registered_locators.len());
    for locator in registered_locators
        .iter()
        .filter(|locator| matches!(locator.entity(), RuntimeEntityReference::Scene { .. }))
        .cloned()
    {
        let (instance, authored_id) = match locator.entity() {
            RuntimeEntityReference::Scene {
                instance, entity, ..
            } => (*instance, entity.clone()),
            RuntimeEntityReference::Persistent { .. } => continue,
        };
        match domain.lookup(world, locator.entity()) {
            EntityLookup::Resolved(entity) => export_entities.push(ExportEntity {
                locator,
                entity,
                instance,
                authored_id,
            }),
            _ => {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.export-identity-unresolved",
                        "Scene export identity did not resolve to a live entity",
                    ),
                    "entity-id",
                    authored_id.as_str(),
                ));
            }
        }
    }

    let assigned_ids = assign_export_ids(&export_entities, &mut diagnostics);
    let mut remap_entries = BTreeMap::new();
    let mut id_by_entity = BTreeMap::<Entity, SceneEntityId>::new();
    for (export_entity, assigned_id) in export_entities.iter().zip(&assigned_ids) {
        if remap_entries
            .insert(export_entity.locator.clone(), assigned_id.clone())
            .is_some()
            || id_by_entity
                .insert(export_entity.entity, assigned_id.clone())
                .is_some()
        {
            diagnostics.push(diagnostic_error(
                "scene.export-identity-collision",
                "Scene export identity mapping was not injective",
            ));
        }
    }
    for locator in registered_locators
        .iter()
        .filter(|locator| matches!(locator.entity(), RuntimeEntityReference::Persistent { .. }))
    {
        let EntityLookup::Resolved(entity) = domain.lookup(world, locator.entity()) else {
            continue;
        };
        let Some(assigned_id) = id_by_entity.get(&entity) else {
            continue;
        };
        if remap_entries
            .insert(locator.clone(), assigned_id.clone())
            .is_some()
        {
            diagnostics.push(diagnostic_error(
                "scene.export-identity-collision",
                "Scene export identity mapping was not injective",
            ));
        }
    }

    let roots = world
        .get_resource::<HierarchyRoots>()
        .into_iter()
        .flat_map(HierarchyRoots::iter)
        .filter_map(|entity| id_by_entity.get(&entity).cloned())
        .collect::<Vec<_>>();
    let root_entities = world
        .get_resource::<HierarchyRoots>()
        .into_iter()
        .flat_map(HierarchyRoots::iter)
        .filter(|entity| id_by_entity.contains_key(entity))
        .collect::<BTreeSet<_>>();

    let encode_context =
        ComponentEncodeContext::new().with_asset_ref_export_policy(options.asset_ref_export_policy);
    let mut records = Vec::with_capacity(export_entities.len());
    for (export_entity, id) in export_entities.iter().zip(assigned_ids) {
        let parent = world
            .get::<Parent>(export_entity.entity)
            .map(Parent::parent);
        if let Some(parent) = parent
            && !id_by_entity.contains_key(&parent)
        {
            diagnostics.push(with_public_locator(
                diagnostic_error(
                    "scene.export-foreign-parent",
                    "Scene export entity is owned by a non-scene runtime parent",
                ),
                "entity-id",
                id.as_str(),
            ));
        } else if parent.is_none() && !root_entities.contains(&export_entity.entity) {
            diagnostics.push(with_public_locator(
                diagnostic_error(
                    "scene.export-runtime-placement-missing",
                    "Scene export entity has no registered runtime placement",
                ),
                "entity-id",
                id.as_str(),
            ));
        }

        let mut components = BTreeMap::new();
        for schema in &schemas {
            let encoded = execution.encode_component_with_context(
                &schema.id,
                world,
                export_entity.entity,
                &encode_context,
            );
            match encoded {
                Ok(Some(value)) => {
                    let value = match effective_component_value_owned(
                        schema,
                        value,
                        options.value_limits,
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            diagnostics.push(with_codec_error(
                                with_public_locator(
                                    with_public_locator(
                                        diagnostic_error(
                                            "scene.export-value-budget-exceeded",
                                            "Scene export component value exceeded its admitted interpretation budget",
                                        ),
                                        "entity-id",
                                        id.as_str(),
                                    ),
                                    "component-id",
                                    schema.id.as_str(),
                                ),
                                &error,
                                Some(schema),
                            ));
                            continue;
                        }
                    };
                    let rewritten = rewrite_declared_entity_references(
                        schema,
                        &value,
                        reference_traversal_limits(options.value_limits),
                        |_path, reference| {
                            rewrite_export_reference(
                                world,
                                domain,
                                export_entity.instance,
                                reference,
                                &remap_entries,
                            )
                        },
                    );
                    match rewritten {
                        Ok(value) => {
                            let output_cost = value.cost();
                            let output_depth = value_depth(&value);
                            let Some(output_nodes) =
                                exported_value_nodes.checked_add(output_cost.nodes())
                            else {
                                diagnostics.push(export_value_budget_diagnostic(
                                    usize::MAX,
                                    options.value_limits.nodes().get(),
                                    id.as_str(),
                                    schema.id.as_str(),
                                ));
                                continue;
                            };
                            let Some(output_bytes) =
                                exported_value_bytes.checked_add(output_cost.logical_bytes())
                            else {
                                diagnostics.push(export_value_budget_diagnostic(
                                    usize::MAX,
                                    options.value_limits.bytes().get(),
                                    id.as_str(),
                                    schema.id.as_str(),
                                ));
                                continue;
                            };
                            if output_nodes > options.value_limits.nodes().get()
                                || output_bytes > options.value_limits.bytes().get()
                                || output_depth > options.value_limits.depth().get()
                            {
                                let (observed, maximum) =
                                    if output_nodes > options.value_limits.nodes().get() {
                                        (output_nodes, options.value_limits.nodes().get())
                                    } else if output_bytes > options.value_limits.bytes().get() {
                                        (output_bytes, options.value_limits.bytes().get())
                                    } else {
                                        (output_depth, options.value_limits.depth().get())
                                    };
                                diagnostics.push(export_value_budget_diagnostic(
                                    observed,
                                    maximum,
                                    id.as_str(),
                                    schema.id.as_str(),
                                ));
                                continue;
                            }
                            exported_value_nodes = output_nodes;
                            exported_value_bytes = output_bytes;
                            components.insert(
                                schema.id.clone(),
                                SceneComponentRecord::new(schema.version, value),
                            );
                        }
                        Err(error) => {
                            diagnostics.push(with_entity_reference_rewrite_error(
                                with_public_locator(
                                    with_public_locator(
                                        diagnostic_error(
                                            "scene.export-entity-reference-rewrite-failed",
                                            "Scene entity reference export failed",
                                        ),
                                        "entity-id",
                                        id.as_str(),
                                    ),
                                    "component-id",
                                    schema.id.as_str(),
                                ),
                                &error,
                                |error| export_reference_resolution_error_name(*error),
                            ));
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    diagnostics.push(with_codec_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.export-component-failed",
                                    "Component export failed",
                                ),
                                "entity-id",
                                id.as_str(),
                            ),
                            "component-id",
                            schema.id.as_str(),
                        ),
                        &error,
                        Some(schema),
                    ));
                }
            }
        }

        records.push(SceneEntityRecord {
            id,
            children: world
                .get::<Children>(export_entity.entity)
                .into_iter()
                .flat_map(Children::iter)
                .filter_map(|child| id_by_entity.get(&child).cloned())
                .collect(),
            components,
            prefab: None,
        });
    }

    let output = (!diagnostics.has_errors()).then(|| SceneExportOutput {
        document: SceneDocument::new(roots, records),
        remap: SceneExportRemap {
            source_domain: domain.id(),
            entries: remap_entries,
        },
    });
    SceneExportReport {
        output,
        diagnostics,
    }
}

fn export_value_budget_diagnostic(
    observed: usize,
    maximum: usize,
    entity: &str,
    component: &str,
) -> nara_diagnostic::Diagnostic {
    with_public_u64(
        with_public_u64(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.export-value-budget-exceeded",
                        "Scene export value budget was exceeded",
                    ),
                    "entity-id",
                    entity,
                ),
                "component-id",
                component,
            ),
            "observed",
            observed as u64,
        ),
        "maximum",
        maximum as u64,
    )
}

fn value_depth(value: &ComponentValue) -> usize {
    let mut maximum = 1_usize;
    let mut pending = vec![(value, 1_usize)];
    while let Some((value, depth)) = pending.pop() {
        maximum = maximum.max(depth);
        match value {
            ComponentValue::List(values) => {
                pending.extend(values.iter().map(|value| (value, depth.saturating_add(1))));
            }
            ComponentValue::Map(values) => {
                pending.extend(
                    values
                        .values()
                        .map(|value| (value, depth.saturating_add(1))),
                );
            }
            _ => {}
        }
    }
    maximum
}

const fn export_reference_resolution_error_name(
    error: ExportReferenceResolutionError,
) -> &'static str {
    match error {
        ExportReferenceResolutionError::SceneLocalTargetMissing => "scene-local-target-missing",
        ExportReferenceResolutionError::SceneLocalTargetTombstoned => {
            "scene-local-target-tombstoned"
        }
        ExportReferenceResolutionError::SceneLocalTargetStale => "scene-local-target-stale",
        ExportReferenceResolutionError::PersistentTargetMissing => "persistent-target-missing",
        ExportReferenceResolutionError::PersistentTargetTombstoned => {
            "persistent-target-tombstoned"
        }
        ExportReferenceResolutionError::PersistentTargetUnloaded => "persistent-target-unloaded",
        ExportReferenceResolutionError::PersistentTargetDeleted => "persistent-target-deleted",
        ExportReferenceResolutionError::PersistentTargetStale => "persistent-target-stale",
        ExportReferenceResolutionError::InvalidIdentityState => "invalid-identity-state",
    }
}

fn assign_export_ids(
    entities: &[ExportEntity],
    diagnostics: &mut DiagnosticReport,
) -> Vec<SceneEntityId> {
    let mut candidate_counts = BTreeMap::<SceneEntityId, usize>::new();
    let mut authored_claims = BTreeSet::new();
    for entity in entities {
        *candidate_counts
            .entry(entity.authored_id.clone())
            .or_default() += 1;
        authored_claims.insert(entity.authored_id.clone());
    }

    let mut assigned_claims = BTreeSet::new();
    let mut generated_ordinal = 1_u64;
    entities
        .iter()
        .map(|entity| {
            if candidate_counts.get(&entity.authored_id) == Some(&1) {
                assigned_claims.insert(entity.authored_id.clone());
                return entity.authored_id.clone();
            }

            loop {
                let Some(next_ordinal) = generated_ordinal.checked_add(1) else {
                    diagnostics.push(diagnostic_error(
                        "scene.export-id-exhausted",
                        "Scene export generated identity space is exhausted",
                    ));
                    return SceneEntityId::new("entity_exhausted")
                        .expect("fallback export identity literal is valid");
                };
                let candidate = SceneEntityId::new(format!("entity_{generated_ordinal}"))
                    .expect("generated export identities are valid");
                generated_ordinal = next_ordinal;
                if !authored_claims.contains(&candidate)
                    && assigned_claims.insert(candidate.clone())
                {
                    return candidate;
                }
            }
        })
        .collect()
}

fn rewrite_export_reference(
    world: &World,
    domain: &WorldIdentityDomain,
    owner_instance: SceneInstanceId,
    reference: &EntityReference,
    remap: &BTreeMap<WorldEntityLocator, SceneEntityId>,
) -> Result<EntityReference, ExportReferenceResolutionError> {
    match (
        reference,
        domain.resolve_entity_reference(world, reference, Some(owner_instance)),
    ) {
        (EntityReference::SceneLocal { .. }, EntityLookup::Resolved(target)) => {
            let locators = domain
                .locators_for_entity(world, target)
                .map_err(|_| ExportReferenceResolutionError::SceneLocalTargetStale)?
                .ok_or(ExportReferenceResolutionError::SceneLocalTargetStale)?;
            let locator = locators
                .scene()
                .ok_or(ExportReferenceResolutionError::SceneLocalTargetMissing)?;
            let entity = remap
                .get(locator)
                .cloned()
                .ok_or(ExportReferenceResolutionError::SceneLocalTargetMissing)?;
            Ok(EntityReference::SceneLocal { entity })
        }
        (EntityReference::SceneLocal { .. }, EntityLookup::Tombstoned(_)) => {
            Err(ExportReferenceResolutionError::SceneLocalTargetTombstoned)
        }
        (EntityReference::SceneLocal { .. }, EntityLookup::Missing) => {
            Err(ExportReferenceResolutionError::SceneLocalTargetMissing)
        }
        (EntityReference::SceneLocal { .. }, EntityLookup::StaleRegistration) => {
            Err(ExportReferenceResolutionError::SceneLocalTargetStale)
        }
        (EntityReference::Persistent { entity }, EntityLookup::Resolved(target)) => {
            let locators = domain
                .locators_for_entity(world, target)
                .map_err(|_| ExportReferenceResolutionError::PersistentTargetStale)?
                .ok_or(ExportReferenceResolutionError::PersistentTargetStale)?;
            let locator = locators
                .persistent()
                .ok_or(ExportReferenceResolutionError::PersistentTargetStale)?;
            if let Some(entity) = remap.get(locator) {
                return Ok(EntityReference::SceneLocal {
                    entity: entity.clone(),
                });
            }
            Ok(EntityReference::Persistent {
                entity: entity.clone(),
            })
        }
        (EntityReference::Persistent { .. }, EntityLookup::Tombstoned(_)) => {
            Err(ExportReferenceResolutionError::PersistentTargetTombstoned)
        }
        (EntityReference::Persistent { .. }, EntityLookup::Missing) => {
            Err(ExportReferenceResolutionError::PersistentTargetMissing)
        }
        (EntityReference::Persistent { .. }, EntityLookup::StaleRegistration) => {
            Err(ExportReferenceResolutionError::PersistentTargetStale)
        }
        (EntityReference::Persistent { .. }, EntityLookup::Unloaded) => {
            Err(ExportReferenceResolutionError::PersistentTargetUnloaded)
        }
        (EntityReference::Persistent { .. }, EntityLookup::Deleted) => {
            Err(ExportReferenceResolutionError::PersistentTargetDeleted)
        }
        (
            _,
            EntityLookup::ContextRequired
            | EntityLookup::DomainUnavailable
            | EntityLookup::WrongWorldBinding
            | EntityLookup::WrongDomain { .. }
            | EntityLookup::StaleRuntime
            | EntityLookup::Unloaded
            | EntityLookup::Deleted,
        ) => Err(ExportReferenceResolutionError::InvalidIdentityState),
    }
}
