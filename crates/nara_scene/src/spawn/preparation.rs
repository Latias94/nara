use nara_ecs::bevy_ecs::world::WorldId;
use nara_reflect::{
    ComponentRegistryAuthorityError, ComponentRegistrySnapshot, component_registry,
    validate_component_registry_authority,
};

use super::*;

pub(super) enum ScenePublicationTarget {
    Register,
    Replace(SpawnedSceneInstance),
    Projection(Option<SpawnedSceneInstance>),
}

impl ScenePublicationTarget {
    pub(super) fn from_borrowed(target: SceneIdentityCommit<'_>) -> Self {
        match target {
            SceneIdentityCommit::Register => Self::Register,
            SceneIdentityCommit::Replace(current) => Self::Replace(current.clone()),
            SceneIdentityCommit::Projection(current) => Self::Projection(current.cloned()),
        }
    }

    fn as_borrowed(&self) -> SceneIdentityCommit<'_> {
        match self {
            Self::Register => SceneIdentityCommit::Register,
            Self::Replace(current) => SceneIdentityCommit::Replace(current),
            Self::Projection(current) => SceneIdentityCommit::Projection(current.as_ref()),
        }
    }

    fn predecessor(&self) -> Option<&SpawnedSceneInstance> {
        match self {
            Self::Register => None,
            Self::Replace(current) => Some(current),
            Self::Projection(current) => current.as_ref(),
        }
    }
}

pub(super) struct UnboundScenePublication {
    pub(super) definition: ScenePreparationDefinition,
    pub(super) preflight: PreflightScene,
    pub(super) component_locations: Vec<(usize, ComponentTypeId)>,
    pub(super) prepared_values: UnboundComponentApplyBatch,
    pub(super) prepared_product: Option<PreparedSceneProductTransaction>,
    pub(super) prepared_occurrence: PreparedSceneOccurrence,
    pub(super) provenance: SceneEntityProvenanceMap,
    pub(super) target: ScenePublicationTarget,
    pub(super) claims: SceneNativeRetirementClaims,
    pub(super) diagnostics: DiagnosticReport,
}

pub(super) struct ScenePreparationDefinition {
    world: WorldId,
    snapshot: ComponentRegistrySnapshot,
    host: SceneHostCompositionBasis,
}

enum SceneHostCompositionBasis {
    Standalone,
    Managed(ComponentRegistrySnapshot),
}

impl ScenePreparationDefinition {
    pub(super) fn capture(
        world: &World,
        registry: &ComponentRegistry,
    ) -> Result<Self, SceneSpawnReport> {
        let snapshot = registry.snapshot().map_err(|_| {
            preparation_rejection(
                "scene.component-registry-not-frozen",
                "Scene validation requires a frozen component registry",
            )
        })?;
        let host = match (
            component_registry(world),
            validate_component_registry_authority(world),
        ) {
            (Some(registry), Ok(())) => SceneHostCompositionBasis::Managed(
                registry.snapshot().map_err(|_| definition_rejection())?,
            ),
            (None, Err(ComponentRegistryAuthorityError::MissingAuthority)) => {
                SceneHostCompositionBasis::Standalone
            }
            _ => return Err(definition_rejection()),
        };
        Ok(Self {
            world: world.id(),
            snapshot,
            host,
        })
    }

    fn require_managed_definition(&self) -> Result<(), SceneSpawnReport> {
        match &self.host {
            SceneHostCompositionBasis::Managed(snapshot) if !snapshot.ptr_eq(&self.snapshot) => {
                Err(definition_rejection())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn validate(&self, world: &World) -> Result<(), SceneSpawnReport> {
        if world.id() != self.world {
            return Err(preparation_rejection(
                "scene.prepared-wrong-world",
                "Prepared Scene belongs to another World",
            ));
        }
        match (
            &self.host,
            component_registry(world),
            validate_component_registry_authority(world),
        ) {
            (SceneHostCompositionBasis::Managed(snapshot), Some(registry), Ok(()))
                if registry.shares_snapshot(snapshot) =>
            {
                Ok(())
            }
            (
                SceneHostCompositionBasis::Standalone,
                None,
                Err(ComponentRegistryAuthorityError::MissingAuthority),
            ) => Ok(()),
            _ => Err(definition_rejection()),
        }
    }
}

/// Actual native values prepared for one receiving World, before Scene target allocation.
///
/// Dropping this move-only token cancels private preparation and requires no World. Preparation
/// may register metadata and flush ordinary deferred World work; it does not reserve a logical
/// slot or runtime instance number. Commit revalidates current eligibility without rerunning
/// codecs. Native destructors and checked process-local ID allocation retain their normal effects.
///
/// ```compile_fail
/// use nara_ecs::World;
/// use nara_scene::PreparedScene;
/// fn commit_twice(prepared: PreparedScene, world: &mut World) {
///     prepared.commit(world);
///     prepared.commit(world); // Preparation is consumed even when publication rejects.
/// }
/// ```
#[must_use]
pub struct PreparedScene {
    unbound: UnboundScenePublication,
}

impl PreparedScene {
    /// Diagnostics produced while preparing the candidate, before any publication.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.unbound.diagnostics
    }

    /// Consumes preparation once, publishing only after current World facts are admitted.
    ///
    /// A failure consumes and disposes private preparation. It neither ends the predecessor
    /// activation nor publishes the candidate. Pending ordinary World work may still be flushed.
    pub fn commit(self, world: &mut World) -> SceneSpawnReport {
        match self.unbound.bind(world) {
            Ok(prepared) => finish_scene_publication(world, prepared),
            Err(report) => report,
        }
    }
}

impl fmt::Debug for PreparedScene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedScene")
            .field("entities", &self.unbound.preflight.entities.len())
            .finish_non_exhaustive()
    }
}

/// Prepares a source-bound Scene for its explicitly declared first logical placement.
pub fn prepare_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    occurrence: &SceneOccurrenceKey,
    database: Option<&ProjectAssetDatabase>,
) -> Result<PreparedScene, SceneSpawnReport> {
    prepare_ordinary_scene(
        world,
        registry,
        candidate,
        occurrence,
        database,
        SceneIdentityCommit::Register,
    )
}

/// Prepares replacement of one exact activation, reusing its declared logical occurrence.
pub fn prepare_scene_replacement(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    current: &SpawnedSceneInstance,
    database: Option<&ProjectAssetDatabase>,
) -> Result<PreparedScene, SceneSpawnReport> {
    let occurrence = occurrence_for_instance(world, current)
        .map_err(|error| occurrence_rejection(DiagnosticReport::default(), error))?
        .clone();
    prepare_ordinary_scene(
        world,
        registry,
        candidate,
        &occurrence,
        database,
        SceneIdentityCommit::Replace(current),
    )
}

pub(super) fn prepare_ordinary_scene(
    world: &mut World,
    registry: &ComponentRegistry,
    candidate: &ExpandedSceneCandidate,
    occurrence: &SceneOccurrenceKey,
    database: Option<&ProjectAssetDatabase>,
    target: SceneIdentityCommit<'_>,
) -> Result<PreparedScene, SceneSpawnReport> {
    let definition = ScenePreparationDefinition::capture(world, registry)?;
    definition.require_managed_definition()?;
    let bindings = SceneAssetBindings::from_world(world);
    let unbound = SceneSpawner::new().prepare_unbound_with_asset_context_and_product_bindings(
        world,
        registry,
        definition,
        candidate,
        occurrence,
        database,
        target,
        None,
        bindings,
        &SceneNativeRetirementClaims::default(),
    )?;
    unbound.definition.validate(world)?;
    Ok(PreparedScene { unbound })
}

fn preparation_rejection(code: &'static str, summary: &'static str) -> SceneSpawnReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(code, summary));
    SceneSpawnReport {
        diagnostics,
        ..Default::default()
    }
}

fn definition_rejection() -> SceneSpawnReport {
    preparation_rejection(
        "scene.prepared-definition-changed",
        "Prepared Scene definition or managed authority is unavailable",
    )
}

pub(super) fn finish_scene_publication(
    world: &mut World,
    prepared: PreparedScenePublication,
) -> SceneSpawnReport {
    match prepared.commit(world) {
        Ok(report) => report,
        Err((prepared, mut report)) => {
            if prepared.discard(world).is_err() {
                report.diagnostics.push(diagnostic_error(
                    "scene.candidate-cleanup-incomplete",
                    "Rejected Scene candidate could not be safely discarded",
                ));
            }
            report
        }
    }
}

impl UnboundScenePublication {
    fn revalidate(
        &mut self,
        world: &mut World,
    ) -> Result<Option<WorldIdentityDomain>, SceneSpawnReport> {
        self.definition.validate(world)?;
        let has_targets = !self.preflight.entities.is_empty();
        self.prepared_values
            .validate_fresh_targets(world, has_targets)
            .map_err(|error| persistent_apply_rejection(self.diagnostics.clone(), &error))?;
        prepare_mutation_baseline(world);
        validate_scene_identity_support(world, &[]).map_err(|error| {
            let mut diagnostics = self.diagnostics.clone();
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            SceneSpawnReport {
                diagnostics,
                ..Default::default()
            }
        })?;
        if has_targets {
            world.register_component::<SceneEntitySource>();
        }
        self.prepared_values
            .validate_fresh_targets(world, has_targets)
            .map_err(|error| persistent_apply_rejection(self.diagnostics.clone(), &error))?;
        let world = &*world;
        self.definition.validate(world)?;
        if validate_hierarchy(world).is_err() {
            return Err(preparation_rejection(
                "scene.hierarchy-invalid",
                "The existing runtime hierarchy is invalid",
            ));
        }
        if let Some(product) = &self.prepared_product {
            validate_scene_product_current(
                world,
                &product.overlay,
                &product.additional_retirements,
            )
            .map_err(|error| scene_product_rejection(self.diagnostics.clone(), error))?;
        }
        self.diagnostics = validate_scene_predecessor(
            world,
            self.target.as_borrowed(),
            self.prepared_product.as_ref(),
            &self.claims,
            std::mem::take(&mut self.diagnostics),
        )?;
        nara_identity::__private::validate_registered_identity_support_topology(world, [])
            .map_err(|_| {
                preparation_rejection(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                )
            })?;
        if has_targets {
            validate_registered_persistent_component_apply::<SceneEntitySource>(world, None)
                .map_err(|_| {
                    preparation_rejection(
                        "scene.runtime-projection-ineligible",
                        "Scene runtime projections would run unsupported lifecycle work",
                    )
                })?;
            nara_hierarchy::__private::validate_fresh_construction_topology(
                world,
                self.preflight
                    .entities
                    .iter()
                    .any(|entity| !entity.children.is_empty()),
            )
            .map_err(|_| {
                preparation_rejection(
                    "scene.runtime-projection-ineligible",
                    "Scene hierarchy would run unsupported lifecycle work",
                )
            })?;
        }
        self.prepared_occurrence
            .validate(world, self.target.predecessor())
            .map_err(|error| occurrence_rejection(self.diagnostics.clone(), error))?;
        prepare_scene_identity(
            world,
            self.preflight.entities.len(),
            self.target.as_borrowed(),
        )
        .map_err(|error| {
            let mut diagnostics = self.diagnostics.clone();
            diagnostics.push(crate::diagnostics::with_identity_error(
                diagnostic_error(
                    "scene.prepared-baseline-changed",
                    "Prepared Scene Identity admission changed before target binding",
                ),
                &error,
            ));
            SceneSpawnReport {
                diagnostics,
                ..Default::default()
            }
        })
    }

    pub(super) fn bind(
        mut self,
        world: &mut World,
    ) -> Result<PreparedScenePublication, SceneSpawnReport> {
        let new_identity_domain = self.revalidate(world)?;
        let Self {
            definition,
            preflight,
            component_locations,
            prepared_values,
            prepared_product,
            prepared_occurrence,
            provenance,
            target,
            claims,
            mut diagnostics,
        } = self;
        let commit = target.as_borrowed();
        let mut spawned_by_id = BTreeMap::new();
        let mut spawned_entities = Vec::new();
        for entity in &preflight.entities {
            let runtime_entity = world.spawn_empty().id();
            spawned_entities.push(runtime_entity);
            if spawned_by_id
                .insert(entity.id.clone(), runtime_entity)
                .is_some()
            {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.internal-duplicate-entity",
                        "Scene spawn produced a duplicate entity identity",
                    ),
                    "entity-id",
                    entity.id.as_str(),
                ));
                break;
            }
        }

        if diagnostics.has_errors() {
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        let new_hierarchy_operations =
            match build_scene_hierarchy_operations(&preflight, &spawned_by_id) {
                Ok(operations) => operations,
                Err(entity_id) => {
                    diagnostics.push(with_public_locator(
                        diagnostic_error(
                            "scene.internal-missing-placement-remap",
                            "Validated scene placement was not mapped to a runtime entity",
                        ),
                        "entity-id",
                        entity_id.as_str(),
                    ));
                    rollback_spawn_transaction(world, &spawned_entities);
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            };
        let (product_insertion, product_resources, additional_retirements) = match prepared_product
        {
            Some(product) => {
                let (insertion, resources) = product.overlay.lower_components(&spawned_by_id);
                (insertion, Some(resources), product.additional_retirements)
            }
            None => (LifecycleFreeInsertionPlan::new(), None, Vec::new()),
        };
        let additional_retirement_entities = additional_retirements
            .iter()
            .map(|token| token.entity())
            .collect::<Vec<_>>();

        let mut component_targets = Vec::with_capacity(component_locations.len());
        for (entity_index, _) in &component_locations {
            let entity_id = &preflight.entities[*entity_index].id;
            let Some(runtime_entity) = spawned_by_id.get(entity_id).copied() else {
                diagnostics.push(with_public_locator(
                    diagnostic_error(
                        "scene.internal-missing-entity",
                        "Spawned scene entity is missing",
                    ),
                    "entity-id",
                    entity_id.as_str(),
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            };
            component_targets.push(runtime_entity);
        }
        let component_batch = match prepared_values.bind_targets(&component_targets) {
            Ok(batch) => batch,
            Err(_) => {
                diagnostics.push(diagnostic_error(
                    "scene.internal-component-target-count",
                    "Prepared component values do not match runtime target bindings",
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        if let Err(error) = component_batch.validate_commit(world) {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
                None,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        if let Err(error) =
            declare_persistent_apply_targets(world, spawned_entities.iter().copied())
        {
            diagnostics.push(with_codec_error(
                diagnostic_error(
                    "scene.component-apply-commit-failed",
                    "Prepared scene components could not be committed",
                ),
                &error,
                None,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        let identity_tokens = match prepare_scene_identity_tokens(
            world,
            &spawned_by_id,
            new_identity_domain.as_ref(),
        ) {
            Ok(tokens) => tokens,
            Err(error) => {
                diagnostics.push(crate::diagnostics::with_identity_error(
                    match commit {
                        SceneIdentityCommit::Register => diagnostic_error(
                            "scene.identity-registration-failed",
                            "Scene identity registration failed",
                        ),
                        SceneIdentityCommit::Replace(_) => diagnostic_error(
                            "scene.identity-replacement-failed",
                            "Scene identity replacement failed",
                        ),
                        SceneIdentityCommit::Projection(_) => diagnostic_error(
                            "scene.projection-epoch-ineligible",
                            "Scene projection epoch could not be prepared",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let prepared_identity = match prepare_scene_identity_commit(
            world,
            &identity_tokens,
            commit,
            new_identity_domain,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.push(crate::diagnostics::with_identity_error(
                    match commit {
                        SceneIdentityCommit::Register => diagnostic_error(
                            "scene.identity-registration-failed",
                            "Scene identity registration failed",
                        ),
                        SceneIdentityCommit::Replace(_) => diagnostic_error(
                            "scene.identity-replacement-failed",
                            "Scene identity replacement failed",
                        ),
                        SceneIdentityCommit::Projection(_) => diagnostic_error(
                            "scene.projection-epoch-ineligible",
                            "Scene projection epoch could not be prepared",
                        ),
                    },
                    &error,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let scene_retirements = prepared_identity.retiring_entities().to_vec();
        let mut retired_entities = scene_retirements.clone();
        retired_entities.extend(additional_retirements.iter().map(|token| token.entity()));
        let prepared_hierarchy_retirement = if prepared_identity.replaces_instance() {
            match prepare_scene_hierarchy_retirement(world, &retired_entities) {
                Ok(retirement) => Some(retirement),
                Err(_) => {
                    diagnostics.push(diagnostic_error(
                        "scene.hierarchy-retirement-ineligible",
                        "Scene hierarchy could not be prepared for exact retirement",
                    ));
                    rollback_spawn_transaction(world, &spawned_entities);
                    return Err(SceneSpawnReport {
                        instance: None,
                        diagnostics,
                        retired_entities: 0,
                    });
                }
            }
        } else {
            None
        };
        let hierarchy_operations = if prepared_identity.replaces_instance() {
            combine_scene_hierarchy_operations(
                world,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("prepared scene replacement requires hierarchy retirement"),
                &new_hierarchy_operations,
                &scene_retirements,
            )
        } else {
            new_hierarchy_operations.clone()
        };
        if !additional_retirements.is_empty()
            && PreparedSceneEntityRetirement::prepare(
                world,
                &additional_retirement_entities,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("product replacement requires hierarchy retirement"),
            )
            .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.product-retirement-lifecycle-active",
                "Scene product retirement would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        if let Err(error) = validate_scene_identity_support(world, &retired_entities) {
            diagnostics.push(crate::diagnostics::with_identity_support_error(
                diagnostic_error(
                    "scene.identity-support-ineligible",
                    "Scene identity support is ineligible for target-World apply",
                ),
                &error,
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        if let Err(error) = validate_existing_scene_persistent_apply(world, &scene_retirements) {
            let report = persistent_apply_rejection(diagnostics, &error);
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(report);
        }

        if validate_scene_source_support(world, &spawned_entities).is_err() {
            diagnostics.push(diagnostic_error(
                "scene.runtime-projection-ineligible",
                "Scene runtime projections would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        if prepared_identity.replaces_instance()
            && validate_scene_entity_retirement_admission(
                world,
                &retired_entities,
                prepared_hierarchy_retirement
                    .as_ref()
                    .expect("prepared scene replacement requires hierarchy retirement"),
                claims.component_ids(),
            )
            .is_err()
        {
            diagnostics.push(diagnostic_error(
                "scene.identity-replacement-ineligible",
                "Scene replacement would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }

        let prepared_components = match component_batch.prepare(world) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.push(with_codec_error(
                    diagnostic_error(
                        "scene.component-apply-commit-failed",
                        "Prepared scene components could not be committed",
                    ),
                    &error,
                    None,
                ));
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        let _prepared_hierarchy = match prepare_construction_batch(world, &hierarchy_operations) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.push(if error == nara_hierarchy::HierarchyError::LifecycleWork {
                    diagnostic_error(
                        "scene.runtime-projection-ineligible",
                        "Scene runtime projections would run unsupported lifecycle work",
                    )
                } else {
                    diagnostic_error(
                        "scene.hierarchy-publication-failed",
                        "Validated scene hierarchy could not be prepared",
                    )
                });
                rollback_spawn_transaction(world, &spawned_entities);
                return Err(SceneSpawnReport {
                    instance: None,
                    diagnostics,
                    retired_entities: 0,
                });
            }
        };

        if product_insertion.validate(world).is_err() {
            diagnostics.push(diagnostic_error(
                "scene.product-overlay-lifecycle-ineligible",
                "Scene product overlay would run unsupported lifecycle work",
            ));
            rollback_spawn_transaction(world, &spawned_entities);
            return Err(SceneSpawnReport {
                instance: None,
                diagnostics,
                retired_entities: 0,
            });
        }
        Ok(PreparedScenePublication {
            definition,
            prepared_identity,
            identity_tokens,
            expected: match target {
                ScenePublicationTarget::Register => None,
                ScenePublicationTarget::Replace(current) => Some(current),
                ScenePublicationTarget::Projection(current) => current,
            },
            prepared_components,
            product_insertion,
            product_resources,
            hierarchy_operations,
            scene_retirements,
            additional_retirement_tokens: additional_retirements,
            retired_entities,
            spawned_by_id,
            provenance,
            occurrence: prepared_occurrence,
            claims,
            diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use nara_asset::AssetServer;
    use nara_ecs::{
        Commands,
        bevy_ecs::{lifecycle::Remove, observer::On},
    };
    use nara_hierarchy::__private::CompletedHierarchyProjection;

    use super::*;

    #[test]
    fn late_hierarchy_completion_observer_rejects_before_targets_or_publication() {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let source = SourceDocumentKey::named(
            "4be07aef-1441-4fcb-9d89-2157bb2796b1".parse().unwrap(),
            "completion",
            crate::SourceDocumentKind::Scene,
        )
        .unwrap();
        let candidate = ExpandedSceneCandidate::plain(
            source,
            Arc::new(crate::SceneDocument::from_ordered_roots([
                crate::SceneEntityRecord::new(SceneEntityId::new("root").unwrap()),
            ])),
        );
        let occurrence = SceneOccurrenceKey::new("scene.tests", "completion").unwrap();
        let mut server = AssetServer::new();
        server.reserve::<u32>("textures/retained.png").unwrap();
        let mut world = World::new();
        let mut control = World::new();
        for world in [&mut world, &mut control] {
            world.init_resource::<CompletedHierarchyProjection>();
            world.insert_resource(server.clone());
        }
        let pending = prepare_scene(&mut world, &registry, &candidate, &occurrence, None).unwrap();
        drop(prepare_scene(&mut control, &registry, &candidate, &occurrence, None).unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        for world in [&mut world, &mut control] {
            let hits = Arc::clone(&hits);
            world.add_observer(
                move |_: On<Remove<CompletedHierarchyProjection>>, mut commands: Commands| {
                    hits.fetch_add(1, Ordering::SeqCst);
                    commands.queue(|world: &mut World| {
                        *world.resource_mut::<AssetServer>() = AssetServer::new();
                    });
                },
            );
            world.flush();
        }
        let report = pending.commit(&mut world);
        assert!(report.instance.is_none());
        assert!(report.diagnostics.has_errors());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert_eq!(world.resource::<AssetServer>(), &server);
        assert!(!world.contains_resource::<WorldIdentityDomain>());
        assert_eq!(world.spawn_empty().id(), control.spawn_empty().id());
    }
}
