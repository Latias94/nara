use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use nara_asset::{
    AssetProductRef, AssetRef, AssetRefExportPolicy, ProjectAssetDatabase, StableAssetId,
};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Entity, World};
use nara_hierarchy::{Children, HierarchyRoots, Parent, validate_hierarchy};
use nara_identity::{
    EntityLookup, EntityReference, PersistentRuntimeReference, RuntimeEntityReference,
    SceneInstanceId, WorldEntityLocator, WorldIdentityDomain, WorldIdentityDomainId,
};
use nara_reflect::__private::{
    component_snapshot_matches_runtime_type, effective_component_value_with_budget,
    validate_persistent_export_coverage,
};
use nara_reflect::{
    ComponentCapability, ComponentEncodeContext, ComponentEntityReferenceRewriteError,
    ComponentRegistry, ComponentRegistryAuthorityError, ComponentSchema, ComponentValue,
    ComponentValueMaterializationLimits, collect_declared_asset_references_from_schema_with_limits,
    component_registry, validate_component_registry_authority,
    validate_component_value_with_limits,
};

use crate::{
    SceneComponentBindings, SceneComponentRecord, SceneDataComponents, SceneDocument,
    SceneEntityId, SceneEntityRecord,
    component_binding_plugin::{
        SceneComponentBindingsAuthorityError, scene_component_bindings,
        validate_scene_component_bindings_authority,
    },
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
    ValueBudgetExceeded { observed: usize },
}

#[must_use]
/// Exports all current live Scene identities as a flattened native-only document.
///
/// This is distinct from source Save and requires the exact installed native definition in a
/// managed World. Independent preview publication does not make another registry the managed
/// export authority. Any encountered data-family member refuses the complete output.
pub fn export_scene(world: &World, registry: &ComponentRegistry) -> SceneExportReport {
    export_scene_with_options(world, registry, SceneExportOptions::default())
}

#[must_use]
pub fn export_scene_with_options(
    world: &World,
    registry: &ComponentRegistry,
    options: SceneExportOptions,
) -> SceneExportReport {
    export_scene_selected(world, registry, None, options, None)
}

/// Exports native values using explicit immutable project metadata for asset policy lowering,
/// including references introduced by omitted-field defaults after native encoding.
#[must_use]
pub fn export_scene_with_asset_database(
    world: &World,
    registry: &ComponentRegistry,
    options: SceneExportOptions,
    database: &ProjectAssetDatabase,
) -> SceneExportReport {
    export_scene_selected(world, registry, None, options, Some(database))
}

impl SceneComponentBindings {
    /// Exports the current live Scene identities with this exact representation selection.
    #[must_use]
    pub fn export_scene(&self, world: &World) -> SceneExportReport {
        self.export_scene_with_options(world, SceneExportOptions::default(), None)
    }

    /// Exports native and data records without realizing data-family assets or invoking codecs
    /// for them. Path export of a stable asset ID requires the selected project metadata.
    #[must_use]
    pub fn export_scene_with_options(
        &self,
        world: &World,
        options: SceneExportOptions,
        database: Option<&ProjectAssetDatabase>,
    ) -> SceneExportReport {
        export_scene_selected(world, self.native_registry(), Some(self), options, database)
    }
}

fn export_scene_selected(
    world: &World,
    registry: &ComponentRegistry,
    bindings: Option<&SceneComponentBindings>,
    options: SceneExportOptions,
    database: Option<&ProjectAssetDatabase>,
) -> SceneExportReport {
    let mut diagnostics = DiagnosticReport::default();
    let mut exported_value_nodes = 0_usize;
    let mut exported_value_bytes = 0_usize;
    if bindings.is_none() {
        let native_authority = match (
            component_registry(world),
            validate_component_registry_authority(world),
        ) {
            (None, Err(ComponentRegistryAuthorityError::MissingAuthority)) => true,
            (Some(installed), Ok(())) => registry
                .snapshot()
                .is_ok_and(|selected| installed.shares_snapshot(&selected)),
            _ => false,
        };
        if !native_authority {
            diagnostics.push(diagnostic_error(
                "scene.export-native-authority-invalid",
                "Scene export requires the exact admitted managed native definition",
            ));
            return SceneExportReport {
                output: None,
                diagnostics,
            };
        }
    }
    if let Some(bindings) = bindings {
        let standalone = component_registry(world).is_none()
            && validate_component_registry_authority(world)
                == Err(ComponentRegistryAuthorityError::MissingAuthority);
        let native_matches = standalone
            || (validate_component_registry_authority(world).is_ok()
                && component_registry(world)
                    .is_some_and(|registry| registry.shares_snapshot(bindings.native_snapshot())));
        let selection_matches = match (
            scene_component_bindings(world),
            validate_scene_component_bindings_authority(world),
        ) {
            (Some(installed), Ok(())) => installed.same_definitions(bindings),
            (None, Err(SceneComponentBindingsAuthorityError::MissingAuthority)) => standalone,
            _ => false,
        };
        if !native_matches || !selection_matches {
            diagnostics.push(diagnostic_error(
                "scene.export-bindings-authority-invalid",
                "Scene export requires the exact admitted managed component bindings",
            ));
            return SceneExportReport {
                output: None,
                diagnostics,
            };
        }
    }
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

    // Complete native coverage admission before any encoder runs. A refusal on the last entity
    // must not execute callbacks for an earlier member under an incomplete selection.
    for entity in &export_entities {
        if let Err(error) =
            validate_persistent_export_coverage(world, entity.entity, |component, runtime| {
                execution
                    .schema(component)
                    .is_some_and(|schema| schema.has_capability(ComponentCapability::Scene))
                    && component_snapshot_matches_runtime_type(
                        &execution, world, component, runtime,
                    )
            })
        {
            diagnostics.push(with_codec_error(
                with_public_locator(
                    diagnostic_error(
                        "scene.export-native-binding-unavailable",
                        "Scene export selection does not cover the actual persistent native components",
                    ),
                    "entity-id", entity.authored_id.as_str(),
                ), &error, None,
            ));
        }
    }
    if diagnostics.has_errors() {
        return SceneExportReport {
            output: None,
            diagnostics,
        };
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

    let mut encode_context =
        ComponentEncodeContext::new().with_asset_ref_export_policy(options.asset_ref_export_policy);
    if let Some(database) = database {
        encode_context = encode_context.with_project_asset_database(database);
    }
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
                    let (effective, cost) = match effective_component_value_with_budget(
                        schema,
                        &value,
                        options
                            .value_limits
                            .nodes()
                            .get()
                            .saturating_sub(exported_value_nodes),
                        options
                            .value_limits
                            .bytes()
                            .get()
                            .saturating_sub(exported_value_bytes),
                        options.value_limits.depth(),
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
                    // Reuse the codec's owning value when no defaults were missing. Default
                    // copies above were admitted against the complete aggregate remainder.
                    let mut value = match effective {
                        Cow::Borrowed(_) => value,
                        Cow::Owned(value) => value,
                    };
                    let context = ExportValueContext {
                        world,
                        domain,
                        instance: export_entity.instance,
                        remap: &remap_entries,
                        options,
                        database,
                        retained_nodes: exported_value_nodes,
                        retained_bytes: exported_value_bytes,
                    };
                    match context.lower_references(schema, &mut value, cost.logical_bytes(), &id) {
                        Ok(output_bytes) => {
                            exported_value_nodes += cost.nodes();
                            exported_value_bytes += output_bytes;
                            components.insert(
                                schema.id.clone(),
                                SceneComponentRecord::new(schema.version, value),
                            );
                        }
                        Err(diagnostic) => {
                            diagnostics.push(diagnostic);
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

        if let Some(carrier) = world.get::<SceneDataComponents>(export_entity.entity) {
            let location_matches = carrier.matches_location(world, export_entity.entity);
            if !location_matches {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.export-data-location-mismatch",
                        "Scene data carrier does not belong to this live entity location",
                    ),
                    "entity-id",
                    id.as_str(),
                ));
            }
            for (component, entry) in carrier.entries() {
                let locate = |diagnostic| {
                    with_public_locator(
                        with_public_locator(diagnostic, "entity-id", id.as_str()),
                        "component-id",
                        component.as_str(),
                    )
                };
                if !location_matches {
                    continue;
                }
                if components.contains_key(component) {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-component-collision",
                        "Scene export contains duplicate native and data component IDs",
                    )));
                    continue;
                }
                let Some(owner) = bindings.and_then(|bindings| bindings.data_owner(component))
                else {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-data-binding-unavailable",
                        "Scene export requires the selected live data component binding",
                    )));
                    continue;
                };
                let record = entry.record();
                if !owner.ptr_eq(entry.owner()) || record.owner() != Some(owner.owner_id()) {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-data-owner-mismatch",
                        "Scene data component does not match the exact selected owner admission",
                    )));
                    continue;
                }
                let Some(schema) = owner.schema(component) else {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-data-binding-unavailable",
                        "Scene data component has no current selected Schema",
                    )));
                    continue;
                };
                if record.version != schema.version() {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-data-version-mismatch",
                        "Scene data export requires the current selected Schema version",
                    )));
                    continue;
                }
                if nara_reflect::__private::validate_schema_capabilities(
                    schema,
                    [ComponentCapability::Scene],
                )
                .is_err()
                {
                    diagnostics.push(locate(diagnostic_error(
                        "scene.export-data-schema-invalid",
                        "Scene data component Schema does not support complete Scene export",
                    )));
                    continue;
                }
                let context = ExportValueContext {
                    world,
                    domain,
                    instance: export_entity.instance,
                    remap: &remap_entries,
                    options,
                    database,
                    retained_nodes: exported_value_nodes,
                    retained_bytes: exported_value_bytes,
                };
                match context.export_data_value(schema, record, &id) {
                    Ok(value) => {
                        let cost = value.cost();
                        exported_value_nodes += cost.nodes();
                        exported_value_bytes += cost.logical_bytes();
                        components.insert(
                            component.clone(),
                            SceneComponentRecord::new(record.version, value)
                                .with_owner(owner.owner_id().clone()),
                        );
                    }
                    Err(diagnostic) => {
                        diagnostics.push(diagnostic);
                    }
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

struct ExportValueContext<'a> {
    world: &'a World,
    domain: &'a WorldIdentityDomain,
    instance: SceneInstanceId,
    remap: &'a BTreeMap<WorldEntityLocator, SceneEntityId>,
    options: SceneExportOptions,
    database: Option<&'a ProjectAssetDatabase>,
    retained_nodes: usize,
    retained_bytes: usize,
}

impl ExportValueContext<'_> {
    fn export_data_value(
        &self,
        schema: &ComponentSchema,
        record: &SceneComponentRecord,
        entity: &SceneEntityId,
    ) -> Result<ComponentValue, Diagnostic> {
        let locate = |diagnostic| {
            with_public_locator(
                with_public_locator(diagnostic, "entity-id", entity.as_str()),
                "component-id",
                schema.id().as_str(),
            )
        };
        let limits = self.options.value_limits;
        let nodes = limits.nodes().get().saturating_sub(self.retained_nodes);
        let bytes = limits.bytes().get().saturating_sub(self.retained_bytes);
        // Raw ceilings preserve an exact zero remainder. The helper admits all source nodes,
        // default containers and keys before any owning materialization or output copy.
        let (effective, cost) = effective_component_value_with_budget(
            schema,
            &record.value,
            nodes,
            bytes,
            limits.depth(),
        )
        .map_err(|error| {
            with_codec_error(
                locate(diagnostic_error(
                    "scene.export-value-budget-exceeded",
                    "Scene data export default materialization was refused",
                )),
                &error,
                Some(schema),
            )
        })?;
        // Defaults are complete, so these existing validators and walkers borrow the admitted
        // effective tree; they do not materialize another value using the typed limits.
        validate_component_value_with_limits(schema, effective.as_ref(), limits).map_err(
            |error| {
                with_codec_error(
                    locate(diagnostic_error(
                        "scene.export-data-value-invalid",
                        "Scene data component payload is invalid",
                    )),
                    &error,
                    Some(schema),
                )
            },
        )?;
        let mut value = effective.into_owned();
        self.lower_references(schema, &mut value, cost.logical_bytes(), entity)?;
        Ok(value)
    }

    fn lower_references(
        &self,
        schema: &ComponentSchema,
        value: &mut ComponentValue,
        mut value_bytes: usize,
        entity: &SceneEntityId,
    ) -> Result<usize, Diagnostic> {
        let locate = |diagnostic| {
            with_public_locator(
                with_public_locator(diagnostic, "entity-id", entity.as_str()),
                "component-id",
                schema.id().as_str(),
            )
        };
        let budget = |observed, maximum| {
            export_value_budget_diagnostic(observed, maximum, entity.as_str(), schema.id().as_str())
        };
        let limits = self.options.value_limits;
        let bytes = limits.bytes().get().saturating_sub(self.retained_bytes);
        let traversal = reference_traversal_limits(limits);
        let assets = collect_declared_asset_references_from_schema_with_limits(
            schema,
            schema.version(),
            value,
            limits,
            traversal,
        )
        .map_err(|_| {
            locate(diagnostic_error(
                "scene.export-data-asset-reference-invalid",
                "Scene export requires valid Schema-declared asset references",
            ))
        })?;
        // Asset policy is metadata lowering, independent of entity identity rewriting. Both
        // operations admit their replacement bytes before constructing any owning payload.
        for reference in assets {
            let lowered = lower_data_asset_reference(
                reference.asset_ref(),
                self.options.asset_ref_export_policy,
                self.database,
            )
            .ok_or_else(|| {
                locate(diagnostic_error(
                    "scene.export-data-asset-metadata-unavailable",
                    "Scene asset export requires admitted project metadata for this conversion",
                ))
            })?;
            let invalid_path = || {
                locate(diagnostic_error(
                    "scene.export-data-asset-reference-invalid",
                    "Scene asset export could not resolve its declared property path",
                ))
            };
            let target = value
                .get_path_mut(reference.path())
                .map_err(|_| invalid_path())?;
            value_bytes = value_bytes
                .checked_sub(target.cost().logical_bytes())
                .and_then(|bytes| bytes.checked_add(lowered.logical_bytes()))
                .ok_or_else(|| budget(usize::MAX, limits.bytes().get()))?;
            if value_bytes > bytes {
                return Err(budget(
                    self.retained_bytes.saturating_add(value_bytes),
                    limits.bytes().get(),
                ));
            }
            *target = lowered.into_value();
        }
        self.rewrite_references(schema, value, value_bytes, entity)
    }

    fn rewrite_references(
        &self,
        schema: &ComponentSchema,
        value: &mut ComponentValue,
        mut value_bytes: usize,
        entity: &SceneEntityId,
    ) -> Result<usize, Diagnostic> {
        let invalid =
            |error: &ComponentEntityReferenceRewriteError<ExportReferenceResolutionError>| {
                with_entity_reference_rewrite_error(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.export-entity-reference-rewrite-failed",
                                "Scene entity reference export failed",
                            ),
                            "entity-id",
                            entity.as_str(),
                        ),
                        "component-id",
                        schema.id().as_str(),
                    ),
                    error,
                    |error| export_reference_resolution_error_name(*error),
                )
            };
        let references = nara_reflect::__private::plan_declared_entity_references::<
            ExportReferenceResolutionError,
        >(
            schema,
            value,
            reference_traversal_limits(self.options.value_limits),
        )
        .map_err(|error| invalid(&error))?;
        let bytes = self
            .options
            .value_limits
            .bytes()
            .get()
            .saturating_sub(self.retained_bytes);
        let rewritten = references.rewrite_in_place(value, |_path, reference| {
            let target = resolve_export_reference(
                self.world,
                self.domain,
                self.instance,
                reference,
                self.remap,
            )?;
            let projected = value_bytes
                .checked_sub(export_reference_bytes(reference))
                .and_then(|bytes| bytes.checked_add(target.logical_bytes()));
            let projected = projected.unwrap_or(usize::MAX);
            if projected > bytes {
                return Err(ExportReferenceResolutionError::ValueBudgetExceeded {
                    observed: self.retained_bytes.saturating_add(projected),
                });
            }
            value_bytes = projected;
            Ok(target.into_reference())
        });
        if let Err(error) = rewritten {
            return Err(
                if let ComponentEntityReferenceRewriteError::Rewrite {
                    error: ExportReferenceResolutionError::ValueBudgetExceeded { observed },
                    ..
                } = &error
                {
                    export_value_budget_diagnostic(
                        *observed,
                        self.options.value_limits.bytes().get(),
                        entity.as_str(),
                        schema.id().as_str(),
                    )
                } else {
                    invalid(&error)
                },
            );
        }
        Ok(value_bytes)
    }
}

enum LoweredDataAssetReference<'a> {
    Path(&'a str),
    StableId(StableAssetId),
    Product(AssetProductRef),
}

impl LoweredDataAssetReference<'_> {
    fn logical_bytes(&self) -> usize {
        match self {
            Self::Path(path) => 13_usize.saturating_add(path.len()),
            Self::StableId(_) => 54,
            Self::Product(reference) => {
                crate::patch::asset_ref_value_size(&AssetRef::Product(*reference)).1
            }
        }
    }

    fn into_value(self) -> ComponentValue {
        let (kind, value) = match self {
            Self::Path(path) => ("path", path.to_owned()),
            Self::StableId(id) => ("stable_id", id.to_string()),
            Self::Product(reference) => {
                return crate::patch::asset_ref_value(&AssetRef::Product(reference));
            }
        };
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("value", ComponentValue::String(value)),
        ])
    }
}

fn lower_data_asset_reference<'a>(
    reference: &'a AssetRef,
    policy: AssetRefExportPolicy,
    database: Option<&'a ProjectAssetDatabase>,
) -> Option<LoweredDataAssetReference<'a>> {
    match (policy, reference) {
        // Export policy chooses the spelling of primary source selectors only. An explicit
        // product selection, including PRIMARY, remains complete under either policy.
        (_, AssetRef::Product(reference)) => Some(LoweredDataAssetReference::Product(*reference)),
        (AssetRefExportPolicy::Path, AssetRef::Path(path)) => {
            Some(LoweredDataAssetReference::Path(path.as_str()))
        }
        (AssetRefExportPolicy::Path, AssetRef::StableId(id)) => database
            .and_then(|database| database.record_for_stable_id(*id))
            .map(|record| LoweredDataAssetReference::Path(record.path().as_str())),
        (AssetRefExportPolicy::StableIdWhenKnown, AssetRef::StableId(id)) => {
            Some(LoweredDataAssetReference::StableId(*id))
        }
        (AssetRefExportPolicy::StableIdWhenKnown, AssetRef::Path(path)) => Some(
            database
                .and_then(|database| database.record_for_path(path))
                .map_or(LoweredDataAssetReference::Path(path.as_str()), |record| {
                    LoweredDataAssetReference::StableId(record.stable_id())
                }),
        ),
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
        ExportReferenceResolutionError::ValueBudgetExceeded { .. } => "value-budget-exceeded",
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

enum ExportReferenceTarget<'a> {
    SceneLocal(&'a SceneEntityId),
    Persistent(&'a PersistentRuntimeReference),
}

impl ExportReferenceTarget<'_> {
    fn logical_bytes(&self) -> usize {
        match self {
            Self::SceneLocal(entity) => entity.as_str().len(),
            Self::Persistent(entity) => entity.namespace.as_str().len().saturating_add(16),
        }
    }

    fn into_reference(self) -> EntityReference {
        match self {
            Self::SceneLocal(entity) => EntityReference::SceneLocal {
                entity: entity.clone(),
            },
            Self::Persistent(entity) => EntityReference::Persistent {
                entity: entity.clone(),
            },
        }
    }
}

fn export_reference_bytes(reference: &EntityReference) -> usize {
    match reference {
        EntityReference::SceneLocal { entity } => entity.as_str().len(),
        EntityReference::Persistent { entity } => {
            entity.namespace.as_str().len().saturating_add(16)
        }
    }
}

fn resolve_export_reference<'a>(
    world: &World,
    domain: &WorldIdentityDomain,
    owner_instance: SceneInstanceId,
    reference: &'a EntityReference,
    remap: &'a BTreeMap<WorldEntityLocator, SceneEntityId>,
) -> Result<ExportReferenceTarget<'a>, ExportReferenceResolutionError> {
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
                .ok_or(ExportReferenceResolutionError::SceneLocalTargetMissing)?;
            Ok(ExportReferenceTarget::SceneLocal(entity))
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
                return Ok(ExportReferenceTarget::SceneLocal(entity));
            }
            Ok(ExportReferenceTarget::Persistent(entity))
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
