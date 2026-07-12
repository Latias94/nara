use std::collections::{BTreeMap, BTreeSet};

use nara_asset::AssetRefExportPolicy;
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_identity::{
    EntityLookup, EntityReference, RuntimeEntityReference, SceneInstanceId, WorldEntityLocator,
    WorldIdentityDomain, WorldIdentityDomainId,
};
use nara_reflect::{
    ComponentCapability, ComponentEncodeContext, ComponentEntityReferenceRewriteError,
    ComponentFieldPath, ComponentRegistry, ComponentValueKind, EntityReferenceTraversalLimits,
    rewrite_declared_entity_references,
};

use crate::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, usize_to_u64, warning as diagnostic_warning, with_codec_error,
        with_component_field_path, with_component_field_path_error, with_public_identifier,
        with_public_locator, with_public_u64,
    },
    hierarchy::Parent,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SceneExportOptions {
    pub asset_ref_export_policy: AssetRefExportPolicy,
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
    let schemas = match registry.schemas() {
        Ok(schemas) => schemas
            .filter(|schema| schema.has_capability(ComponentCapability::Scene))
            .collect::<Vec<_>>(),
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
            RuntimeEntityReference::Scene { instance, entity } => (*instance, entity.clone()),
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

    let encode_context =
        ComponentEncodeContext::new().with_asset_ref_export_policy(options.asset_ref_export_policy);
    let mut records = Vec::with_capacity(export_entities.len());
    for (export_entity, id) in export_entities.iter().zip(assigned_ids) {
        let parent = world
            .get::<Parent>(export_entity.entity)
            .and_then(|parent| id_by_entity.get(&parent.0).cloned());
        if world.get::<Parent>(export_entity.entity).is_some() && parent.is_none() {
            diagnostics.push(with_public_locator(
                diagnostic_warning(
                    "scene.export-parent-skipped",
                    "Parent entity is not exported with this scene",
                ),
                "entity-id",
                id.as_str(),
            ));
        }

        let mut components = BTreeMap::new();
        for schema in &schemas {
            let Some(encoded) = registry.encode_component_with_context(
                &schema.id,
                world,
                export_entity.entity,
                &encode_context,
            ) else {
                continue;
            };
            match encoded {
                Ok(Some(value)) => {
                    let rewritten = rewrite_declared_entity_references(
                        schema,
                        &value,
                        EntityReferenceTraversalLimits::default(),
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
                            components.insert(
                                schema.id.clone(),
                                SceneComponentRecord::new(schema.version, value),
                            );
                        }
                        Err(error) => {
                            diagnostics.push(with_export_reference_rewrite_error(
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
                    ));
                }
            }
        }

        records.push(SceneEntityRecord {
            id,
            parent,
            components,
            prefab: None,
        });
    }

    let output = (!diagnostics.has_errors()).then(|| SceneExportOutput {
        document: SceneDocument::new(records),
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

fn with_export_reference_rewrite_error(
    diagnostic: Diagnostic,
    error: &ComponentEntityReferenceRewriteError<ExportReferenceResolutionError>,
) -> Diagnostic {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "node-limit"),
            "maximum-nodes",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::ByteLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "byte-limit"),
            "maximum-bytes",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::DepthLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "depth-limit"),
            "maximum-depth",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            with_public_identifier(diagnostic, "rewrite-error-kind", "path-index-overflow")
        }
        ComponentEntityReferenceRewriteError::DuplicateDeclaredPath { path } => with_rewrite_path(
            with_public_identifier(diagnostic, "rewrite-error-kind", "duplicate-declared-path"),
            path,
        ),
        ComponentEntityReferenceRewriteError::UndeclaredReference { path } => with_rewrite_path(
            with_public_identifier(diagnostic, "rewrite-error-kind", "undeclared-reference"),
            path,
        ),
        ComponentEntityReferenceRewriteError::MissingEntityRefCapability { path } => {
            with_rewrite_path(
                with_public_identifier(
                    diagnostic,
                    "rewrite-error-kind",
                    "missing-entity-ref-capability",
                ),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::RequiredReferenceMissing { path } => {
            with_rewrite_path(
                with_public_identifier(
                    diagnostic,
                    "rewrite-error-kind",
                    "required-reference-missing",
                ),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::InvalidReferenceValue { path, actual } => {
            with_public_identifier(
                with_rewrite_path(
                    with_public_identifier(
                        diagnostic,
                        "rewrite-error-kind",
                        "invalid-reference-value",
                    ),
                    path,
                ),
                "actual-value-kind",
                component_value_kind_name(*actual),
            )
        }
        ComponentEntityReferenceRewriteError::InvalidPath { path, error } => {
            with_component_field_path_error(
                with_rewrite_path(
                    with_public_identifier(diagnostic, "rewrite-error-kind", "invalid-path"),
                    path,
                ),
                error,
            )
        }
        ComponentEntityReferenceRewriteError::Rewrite { path, error } => with_rewrite_path(
            with_public_identifier(
                diagnostic,
                "rewrite-error-kind",
                export_reference_resolution_error_name(*error),
            ),
            path,
        ),
    }
}

fn with_rewrite_path(diagnostic: Diagnostic, path: &ComponentFieldPath) -> Diagnostic {
    with_component_field_path(
        diagnostic,
        "reference-field-path",
        "reference-field-path-depth",
        path,
    )
}

const fn component_value_kind_name(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Null => "null",
        ComponentValueKind::Bool => "bool",
        ComponentValueKind::I64 => "i64",
        ComponentValueKind::U64 => "u64",
        ComponentValueKind::F64 => "f64",
        ComponentValueKind::String => "string",
        ComponentValueKind::List => "list",
        ComponentValueKind::Map => "map",
        ComponentValueKind::AssetRef => "asset-ref",
        ComponentValueKind::EntityRef => "entity-ref",
    }
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
        (EntityReference::SceneLocal { entity }, EntityLookup::Resolved(_)) => {
            let locator = WorldEntityLocator::new(
                domain.id(),
                RuntimeEntityReference::scene(owner_instance, entity.clone()),
            );
            let entity = remap
                .get(&locator)
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
        (EntityReference::Persistent { entity }, EntityLookup::Resolved(_)) => {
            let runtime_reference = RuntimeEntityReference::persistent(entity.clone());
            let locator = WorldEntityLocator::new(domain.id(), runtime_reference);
            if let Some(entity) = remap.get(&locator) {
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
        (
            _,
            EntityLookup::ContextRequired
            | EntityLookup::DomainUnavailable
            | EntityLookup::WrongWorldBinding
            | EntityLookup::WrongDomain { .. },
        ) => Err(ExportReferenceResolutionError::InvalidIdentityState),
    }
}
