//! Owned Scene preparation and the final exclusive publication tail.

use nara_hierarchy::{__private::PreparedHierarchyBatch, HierarchyOperation};
use nara_reflect::__private::PreparedComponentApplyBatch;

use super::*;
use crate::product_transaction::PreparedSceneProductResources;

/// Fully decoded replacement candidate retained across native-domain retirement.
///
/// The owner must retain this token after native effects begin. A failed commit returns the same
/// candidate; retry never reruns codecs or the product configuration callback. Candidate entities
/// remain unpublished and must be discarded explicitly if preparation is abandoned before native
/// retirement. Runtime Stop may instead destroy the owning World after domain shutdown.
#[doc(hidden)]
#[must_use]
pub struct PreparedSceneReplacement(pub(super) PreparedScenePublication);

pub(super) struct PreparedScenePublication {
    pub(super) definition: preparation::ScenePreparationDefinition,
    pub(super) prepared_identity: PreparedSceneIdentityCommit,
    pub(super) identity_tokens: BTreeMap<SceneEntityId, WorldEntityToken>,
    pub(super) expected: Option<SpawnedSceneInstance>,
    pub(super) prepared_components: PreparedComponentApplyBatch,
    pub(super) component_insertion: LifecycleFreeInsertionPlan,
    pub(super) product_resources: Option<PreparedSceneProductResources>,
    pub(super) hierarchy_operations: Vec<HierarchyOperation>,
    pub(super) scene_retirements: Vec<Entity>,
    pub(super) additional_retirement_tokens: Vec<WorldEntityToken>,
    pub(super) retired_entities: Vec<Entity>,
    pub(super) spawned_by_id: BTreeMap<SceneEntityId, Entity>,
    pub(super) provenance: SceneEntityProvenanceMap,
    pub(super) occurrence: PreparedSceneOccurrence,
    pub(super) claims: SceneNativeRetirementClaims,
    pub(super) diagnostics: DiagnosticReport,
}

struct FinalScenePublication {
    identity: FinalSceneIdentityCommit,
    hierarchy: PreparedHierarchyBatch,
    retirement: Option<PreparedSceneEntityRetirement>,
}

enum FinalSceneIdentityCommit {
    Register(PreparedSceneInstanceRegistration),
    Replace(PreparedSceneInstanceReplacement),
    Projection {
        registration: PreparedSceneInstanceRegistration,
        retirement: Option<PreparedSceneInstanceRetirement>,
    },
}

impl PreparedSceneReplacement {
    /// Revalidates all captured World facts before the first native effect.
    pub fn validate_baseline(&self, world: &mut World) -> Result<(), SceneSpawnReport> {
        self.0.validate(world, false).map(|_| ())
    }

    /// Publishes after every native owner has removed its explicitly claimed components.
    pub fn commit(self, world: &mut World) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        self.0
            .commit(world)
            .map_err(|(prepared, report)| (Self(prepared), report))
    }

    /// Abandons an unpublished candidate before native retirement has begun.
    pub fn discard(self, world: &mut World) -> Result<(), Self> {
        self.0.discard(world).map_err(Self)
    }
}

impl PreparedScenePublication {
    fn rejection(&self, code: &'static str, message: &'static str) -> SceneSpawnReport {
        let mut diagnostics = self.diagnostics.clone();
        diagnostics.push(diagnostic_error(code, message));
        SceneSpawnReport {
            instance: None,
            diagnostics,
            retired_entities: 0,
        }
    }

    fn validate(
        &self,
        world: &mut World,
        finalize: bool,
    ) -> Result<FinalScenePublication, SceneSpawnReport> {
        self.definition.validate(world)?;
        prepare_mutation_baseline(world);
        let world = &*world;
        let changed = || {
            self.rejection(
                "scene.prepared-baseline-changed",
                "Prepared Scene World facts changed before publication",
            )
        };
        self.occurrence
            .validate(world, self.expected.as_ref())
            .map_err(|error| occurrence_rejection(self.diagnostics.clone(), error))?;
        self.prepared_components
            .validate_commit(world)
            .map_err(|_| changed())?;
        self.component_insertion
            .validate(world)
            .map_err(|_| changed())?;
        if let Some(resources) = &self.product_resources {
            resources.validate(world).map_err(|error| {
                scene_product_rejection(
                    self.diagnostics.clone(),
                    PrepareSceneProductTransactionError::Overlay(error),
                )
            })?;
        }
        validate_hierarchy(world).map_err(|_| changed())?;
        nara_identity::__private::validate_registered_identity_support_topology(
            world,
            self.retired_entities.iter().copied(),
        )
        .map_err(|_| changed())?;
        validate_registered_persistent_apply_targets(world, self.scene_retirements.iter().copied())
            .map_err(|_| changed())?;
        if !self.additional_retirement_tokens.is_empty() {
            validate_additional_retirement_identity_axes(
                world,
                self.additional_retirement_tokens.iter().copied(),
            )
            .map_err(|_| changed())?;
            if self
                .additional_retirement_tokens
                .iter()
                .any(|token| world.get::<SceneEntitySource>(token.entity()).is_some())
            {
                return Err(changed());
            }
        }
        for candidate in self.spawned_by_id.values().copied() {
            validate_registered_persistent_component_apply::<SceneEntitySource>(
                world,
                Some(candidate),
            )
            .map_err(|_| changed())?;
        }
        let entries = self
            .identity_tokens
            .iter()
            .map(|(id, token)| (id.clone(), *token))
            .collect::<Vec<_>>();
        let identity = match &self.prepared_identity {
            PreparedSceneIdentityCommit::Register {
                domain,
                bound_domain,
                ..
            } => {
                let registration = match domain {
                    Some(domain) if !world.contains_resource::<WorldIdentityDomain>() => {
                        prepare_exact_scene_instance_registration(domain, world, &entries)
                    }
                    None => {
                        let installed = world
                            .get_resource::<WorldIdentityDomain>()
                            .ok_or_else(changed)?;
                        if installed.id() != *bound_domain {
                            return Err(changed());
                        }
                        prepare_exact_scene_instance_registration(installed, world, &entries)
                    }
                    _ => return Err(changed()),
                }
                .map_err(|_| changed())?;
                if registration.instance_id() != self.prepared_identity.instance_id() {
                    return Err(changed());
                }
                FinalSceneIdentityCommit::Register(registration)
            }
            PreparedSceneIdentityCommit::Replace { .. } => {
                let expected = self
                    .expected
                    .as_ref()
                    .expect("replacement retains its predecessor receipt");
                let replacement = prepare_registered_scene_instance_replacement(
                    world,
                    expected,
                    &entries,
                    TombstoneCause::Replaced,
                )
                .map_err(|_| changed())?;
                if replacement.instance_id() != self.prepared_identity.instance_id()
                    || replacement.retiring_entities() != self.scene_retirements
                {
                    return Err(changed());
                }
                FinalSceneIdentityCommit::Replace(replacement)
            }
            PreparedSceneIdentityCommit::Projection {
                domain,
                predecessor,
                ..
            } => {
                if world
                    .get_resource::<WorldIdentityDomain>()
                    .map(WorldIdentityDomain::id)
                    != *predecessor
                {
                    return Err(changed());
                }
                let retirement = prepare_registered_scene_projection_epoch_retirement(
                    world,
                    self.expected.as_ref(),
                )
                .map_err(|_| changed())?;
                let registration =
                    prepare_exact_scene_instance_registration(domain, world, &entries)
                        .map_err(|_| changed())?;
                if registration.instance_id() != self.prepared_identity.instance_id()
                    || retirement
                        .as_ref()
                        .map_or(&[][..], |retirement| retirement.retiring_entities())
                        != self.scene_retirements
                {
                    return Err(changed());
                }
                FinalSceneIdentityCommit::Projection {
                    registration,
                    retirement,
                }
            }
        };
        let mut retirement = None;
        if self.expected.is_some() {
            let hierarchy = prepare_scene_hierarchy_retirement(world, &self.retired_entities)
                .map_err(|_| changed())?;
            if finalize {
                if self.scene_retirements.iter().any(|entity| {
                    world.get_entity(*entity).is_ok_and(|entity| {
                        self.claims
                            .component_ids()
                            .iter()
                            .any(|component| entity.contains_id(*component))
                    })
                }) {
                    return Err(self.rejection(
                        "scene.native-retirement-incomplete",
                        "A native domain retained a claimed component after retirement",
                    ));
                }
                retirement = Some(
                    PreparedSceneEntityRetirement::prepare(
                        world,
                        &self.retired_entities,
                        &hierarchy,
                    )
                    .map_err(|_| changed())?,
                );
            } else {
                validate_scene_entity_retirement_admission(
                    world,
                    &self.scene_retirements,
                    &hierarchy,
                    self.claims.component_ids(),
                )
                .map_err(|_| changed())?;
                let additional_entities = self
                    .additional_retirement_tokens
                    .iter()
                    .map(|token| token.entity())
                    .collect::<Vec<_>>();
                PreparedSceneEntityRetirement::prepare(world, &additional_entities, &hierarchy)
                    .map_err(|_| changed())?;
            }
        }
        let hierarchy =
            prepare_construction_batch(world, &self.hierarchy_operations).map_err(|_| changed())?;
        self.definition.validate(world)?;
        Ok(FinalScenePublication {
            identity,
            hierarchy,
            retirement,
        })
    }

    pub(super) fn commit(
        self,
        world: &mut World,
    ) -> Result<SceneSpawnReport, (Self, SceneSpawnReport)> {
        let finalization = match self.validate(world, true) {
            Ok(finalization) => finalization,
            Err(report) => return Err((self, report)),
        };
        let Self {
            prepared_identity,
            prepared_components,
            component_insertion,
            product_resources,
            scene_retirements,
            retired_entities,
            spawned_by_id,
            provenance,
            occurrence,
            diagnostics,
            ..
        } = self;
        // All callbacks have already run. The remaining tail holds exclusive World access and
        // applies only freshly validated, lifecycle-free publications.
        let world = component_insertion
            .prepare(world)
            .expect("Scene overlay was validated in the same exclusive publication tail")
            .commit();
        prepared_components.commit(world);
        let instance_id = prepared_identity.instance_id();
        for (entity_id, runtime_entity) in &spawned_by_id {
            let source = provenance
                .get(entity_id)
                .expect("preflight admits complete source coverage");
            world.entity_mut(*runtime_entity).insert(SceneEntitySource {
                context: Arc::clone(&occurrence.context),
                instance_id,
                entity_id: entity_id.clone(),
                provenance: source.clone(),
            });
        }
        finalization.hierarchy.commit(world);
        world.flush();
        let instance = match (prepared_identity, finalization.identity) {
            (
                PreparedSceneIdentityCommit::Register { domain, .. },
                FinalSceneIdentityCommit::Register(registration),
            ) => match domain {
                Some(mut domain) => {
                    let instance = registration.commit(&mut domain);
                    world.insert_resource(domain);
                    instance
                }
                None => registration.commit(&mut world.resource_mut::<WorldIdentityDomain>()),
            },
            (
                PreparedSceneIdentityCommit::Replace { .. },
                FinalSceneIdentityCommit::Replace(replacement),
            ) => {
                let mut domain = world
                    .remove_resource::<WorldIdentityDomain>()
                    .expect("prepared Scene replacement retains its identity domain");
                let (instance, retired_scene_entities) = replacement.commit(&mut domain);
                debug_assert_eq!(retired_scene_entities, scene_retirements);
                finalization
                    .retirement
                    .expect("replacement prepared exact retirement")
                    .commit(world);
                world.insert_resource(domain);
                instance
            }
            (
                PreparedSceneIdentityCommit::Projection { mut domain, .. },
                FinalSceneIdentityCommit::Projection {
                    registration,
                    retirement,
                },
            ) => {
                if let Some(retirement) = retirement {
                    let mut predecessor = world
                        .remove_resource::<WorldIdentityDomain>()
                        .expect("projection retains its predecessor until exact retirement");
                    let retired = retirement.commit(&mut predecessor);
                    debug_assert_eq!(retired, scene_retirements);
                    finalization
                        .retirement
                        .expect("projection prepared exact retirement")
                        .commit(world);
                }
                let instance = registration.commit(&mut domain);
                world.insert_resource(domain);
                instance
            }
            _ => unreachable!("final identity admission matches the prepared publication mode"),
        };
        occurrence.commit(world, &instance);
        if let Some(resources) = product_resources {
            resources.commit(world);
        }
        Ok(SceneSpawnReport {
            instance: Some(instance),
            diagnostics,
            retired_entities: retired_entities.len(),
        })
    }

    pub(super) fn discard(self, world: &mut World) -> Result<(), Self> {
        prepare_mutation_baseline(world);
        if self.identity_tokens.is_empty() {
            return Ok(());
        }
        // Missing generations are already disposed. A surviving entity must still be this
        // candidate's unregistered incarnation before cleanup may take custody of it.
        let domain = match &self.prepared_identity {
            PreparedSceneIdentityCommit::Register {
                domain: Some(domain),
                ..
            }
            | PreparedSceneIdentityCommit::Projection { domain, .. } => domain,
            _ => match world.get_resource::<WorldIdentityDomain>() {
                Some(domain) => domain,
                None => return Err(self),
            },
        };
        let mut candidates = Vec::with_capacity(self.identity_tokens.len());
        let identity_valid = self.identity_tokens.values().copied().all(|token| {
            match domain.locators_for_token(world, token) {
                Ok(None) if world.get::<SceneEntitySource>(token.entity()).is_none() => {
                    candidates.push(token.entity());
                    true
                }
                Err(IdentityDomainError::EntityTokenNotAlive) => true,
                _ => false,
            }
        });
        if !identity_valid {
            return Err(self);
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let has_relationship = candidates.iter().copied().any(|entity| {
            world.get::<Parent>(entity).is_some()
                || world.get::<Children>(entity).is_some()
                || world
                    .get_resource::<HierarchyRoots>()
                    .is_some_and(|roots| roots.contains(entity))
        });
        if has_relationship {
            let hierarchy = match prepare_scene_hierarchy_retirement(world, &candidates) {
                Ok(hierarchy) => hierarchy,
                Err(_) => return Err(self),
            };
            let construction = match prepare_construction_batch(world, hierarchy.operations()) {
                Ok(construction) => construction,
                Err(_) => return Err(self),
            };
            let retirement =
                match PreparedSceneEntityRetirement::prepare(world, &candidates, &hierarchy) {
                    Ok(retirement) => retirement,
                    Err(_) => return Err(self),
                };
            construction.commit(world);
            retirement.commit(world);
        } else {
            let retirement = match prepare_lifecycle_free_despawn(world, &candidates) {
                Ok(retirement) => retirement,
                Err(_) => return Err(self),
            };
            let _ = retirement.commit();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SceneDocument, SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey,
        SourceDocumentKind, advanced::SceneProductResource, spawn_scene,
    };
    use nara_asset::{AssetRef, AssetServer, Handle};
    use nara_core::ItemLimit;
    use nara_ecs::{
        Resource,
        bevy_ecs::lifecycle::{Despawn, HookContext},
        bevy_ecs::observer::On,
        bevy_ecs::world::DeferredWorld,
    };
    use nara_identity::{
        EntityLookup, PersistentRuntimeId, PersistentRuntimeNamespaceId,
        PersistentRuntimeReference, WorldIdentityDomain, spawn_identity_entity,
    };
    use nara_reflect::{
        __macro_support::{decode_persistent_field, encode_persistent_field},
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, PreparedComponentCandidate,
    };
    use std::{
        collections::BTreeSet,
        sync::{
            Arc, OnceLock,
            atomic::{AtomicUsize, Ordering},
        },
    };

    #[derive(Component)]
    #[component(on_remove = retired)]
    struct NativeState;

    #[derive(Resource, Default)]
    struct Removed(usize);

    fn retired(mut world: DeferredWorld<'_>, _context: HookContext) {
        world.resource_mut::<Removed>().0 += 1;
    }

    #[derive(Resource)]
    struct ProductValue(u32, Arc<()>);

    impl ProductValue {
        fn new(value: u32) -> Self {
            Self(value, Arc::new(()))
        }

        fn set(&mut self, value: u32) {
            self.0 = value;
            self.1 = Arc::new(());
        }
    }

    impl SceneProductResource for ProductValue {
        type Baseline = (u32, Arc<()>);

        fn capture_baseline(&self) -> Self::Baseline {
            (self.0, Arc::clone(&self.1))
        }

        fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
            self.0 == baseline.0 && Arc::ptr_eq(&self.1, &baseline.1)
        }
    }

    struct ProbeAsset;

    #[derive(Default)]
    struct NativePreparationProbe {
        runs: AtomicUsize,
        drops: AtomicUsize,
        handle: OnceLock<Handle<ProbeAsset>>,
    }

    #[derive(Component)]
    struct PreparedNativeValue {
        handle: Handle<ProbeAsset>,
        probe: Arc<NativePreparationProbe>,
    }

    impl Drop for PreparedNativeValue {
        fn drop(&mut self) {
            self.probe.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn source_key() -> SourceDocumentKey {
        SourceDocumentKey::named(
            "dce16c36-187f-4a1f-bf61-72c0f41b361b".parse().unwrap(),
            "publication.fixture",
            SourceDocumentKind::Scene,
        )
        .unwrap()
    }

    fn occurrence() -> SceneOccurrenceKey {
        SceneOccurrenceKey::new("publication-tests", "resident").unwrap()
    }

    fn native_value_candidate(
        id: SceneEntityId,
        probe: &Arc<NativePreparationProbe>,
    ) -> (ComponentRegistry, ExpandedSceneCandidate) {
        let component_id = ComponentTypeId::new("test.PreparedNativeValue");
        let schema = ComponentSchema::new(
            component_id.clone(),
            "Prepared value",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("asset"),
            "Asset",
            ComponentFieldPath::from_fields(["asset"]),
            ComponentValueKind::AssetRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]);
        let observed = Arc::clone(probe);
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component_codec_with_context::<PreparedNativeValue, _, _>(
                schema,
                move |value, _| {
                    let asset = decode_persistent_field::<AssetRef>(value, "asset")?;
                    let probe = Arc::clone(&observed);
                    Ok(PreparedComponentCandidate::with_asset_server(
                        move |context| {
                            probe.runs.fetch_add(1, Ordering::Relaxed);
                            let handle = context
                                .resolve_asset_ref::<ProbeAsset>(&asset)
                                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                            assert!(
                                probe.handle.set(handle).is_ok(),
                                "native preparation must run once"
                            );
                            Ok(PreparedNativeValue { handle, probe })
                        },
                    ))
                },
                |value, world, context| {
                    let asset = AssetRef::from_handle_with_policy(
                        world.resource::<AssetServer>(),
                        value.handle,
                        context.asset_path_scope(),
                        context.asset_ref_export_policy(),
                    )
                    .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                    Ok(ComponentValue::map([(
                        "asset",
                        encode_persistent_field(&asset)?,
                    )]))
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        let asset = AssetRef::path("textures/retained-native.png").unwrap();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id)
            .with_component(
                component_id,
                crate::SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("asset", encode_persistent_field(&asset).unwrap())]),
                ),
            )]);
        (
            registry,
            ExpandedSceneCandidate::plain(source_key(), Arc::new(document)),
        )
    }

    fn fixture() -> (
        World,
        ComponentRegistry,
        ExpandedSceneCandidate,
        SpawnedSceneInstance,
        Entity,
        SceneEntityId,
    ) {
        let mut world = World::new();
        world.insert_resource(Removed::default());
        world.insert_resource(ProductValue::new(1));
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let id = SceneEntityId::new("root").unwrap();
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id.clone())]);
        let candidate = ExpandedSceneCandidate::plain(source_key(), Arc::new(document));
        let report = spawn_scene(&mut world, &registry, &candidate, &occurrence(), None);
        let receipt = report.instance.unwrap();
        let EntityLookup::Resolved(entity) = receipt.resolve(&world, &id) else {
            panic!("root must resolve")
        };
        world.entity_mut(entity).insert(NativeState);
        (world, registry, candidate, receipt, entity, id)
    }

    fn limits() -> SceneProductTransactionLimits {
        SceneProductTransactionLimits::new(ItemLimit::new(1).unwrap(), ItemLimit::new(1).unwrap())
    }

    fn prepare_fixture_replacement(
        world: &mut World,
        registry: &ComponentRegistry,
        candidate: &ExpandedSceneCandidate,
        current: &SpawnedSceneInstance,
    ) -> PreparedSceneReplacement {
        let claims = SceneNativeRetirementClaims::from_component_ids([world
            .component_id::<NativeState>()
            .unwrap()]);
        let bindings = SceneAssetBindings::from_world(world);
        prepare_expanded_scene_replacement_with_claims(
            world,
            registry,
            candidate,
            None,
            current,
            bindings,
            &claims,
            limits(),
            &[],
            |overlay| {
                overlay.replace_resource(ProductValue::new(2));
            },
        )
        .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics))
    }

    #[test]
    fn resource_changes_reject_before_native_retirement_and_remain_discardable() {
        for remove in [false, true] {
            let (mut world, registry, candidate, current, old, id) = fixture();
            let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
            prepared.validate_baseline(&mut world).unwrap();
            if remove {
                world.remove_resource::<ProductValue>().unwrap();
            } else {
                world.resource_mut::<ProductValue>().set(7);
            }
            let report = prepared.validate_baseline(&mut world).unwrap_err();
            let expected_code = if remove {
                "scene.product-resource-missing"
            } else {
                "scene.product-resource-changed"
            };
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == expected_code),
                "{:?}",
                report.diagnostics
            );
            assert_eq!(world.resource::<Removed>().0, 0);
            assert!(world.get::<NativeState>(old).is_some());
            assert!(prepared.discard(&mut world).is_ok());
            assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
            if remove {
                assert!(!world.contains_resource::<ProductValue>());
            } else {
                assert_eq!(world.resource::<ProductValue>().0, 7);
            }
        }
    }

    #[test]
    fn resource_changes_during_native_wait_reject_without_any_publication() {
        for mutation in [
            "same-tick",
            "content-aba",
            "equal-write",
            "reinstalled",
            "missing",
        ] {
            let (mut world, _, _, current, old, id) = fixture();
            world.insert_resource(AssetServer::new());
            world.resource_mut::<ProductValue>().set(1);
            let original = world.resource::<ProductValue>().capture_baseline();
            let old_source = world.get::<SceneEntitySource>(old).unwrap().clone();
            let probe = Arc::new(NativePreparationProbe::default());
            let (registry, candidate) = native_value_candidate(id.clone(), &probe);
            let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
            prepared.validate_baseline(&mut world).unwrap();
            // Model the native owner's completed retirement after the original admission.
            // The final commit must still compare the original product-resource baseline.
            world.entity_mut(old).remove::<NativeState>();
            let resource_ticks = world.get_resource_change_ticks::<ProductValue>().unwrap();
            match mutation {
                "same-tick" => world.resource_mut::<ProductValue>().set(7),
                "content-aba" => {
                    let mut resource = world.resource_mut::<ProductValue>();
                    resource.set(7);
                    resource.set(1);
                }
                "equal-write" => world.resource_mut::<ProductValue>().set(1),
                "reinstalled" => {
                    let removed = world.remove_resource::<ProductValue>().unwrap();
                    assert!(removed.matches_baseline(&original));
                    world.insert_resource(ProductValue::new(1));
                }
                "missing" => {
                    world.remove_resource::<ProductValue>().unwrap();
                }
                _ => unreachable!(),
            }
            let live_resource = world.get_resource::<ProductValue>().map(|resource| {
                assert!(!resource.matches_baseline(&original));
                resource.capture_baseline()
            });
            if let Some(ticks) = world.get_resource_change_ticks::<ProductValue>() {
                assert_eq!(ticks.changed, resource_ticks.changed, "case {mutation}");
            }
            let expected_code = if mutation == "missing" {
                "scene.product-resource-missing"
            } else {
                "scene.product-resource-changed"
            };
            let entities = world
                .iter_entities()
                .map(|entity| entity.id())
                .collect::<BTreeSet<_>>();
            let identity = world.resource::<WorldIdentityDomain>().stats();
            let hierarchy = world.resource::<HierarchyRoots>().as_slice().to_vec();
            let server = world.resource::<AssetServer>().clone();
            let server_ticks = world.get_resource_change_ticks::<AssetServer>().unwrap();
            let scratch = prepared.0.spawned_by_id[&id];
            let (prepared, report) = match prepared.commit(&mut world) {
                Err(failure) => failure,
                Ok(_) => panic!("changed resource must reject final publication: {mutation}"),
            };

            assert!(report.instance.is_none());
            assert_eq!(report.retired_entities(), 0);
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == expected_code),
                "case {mutation}: {:?}",
                report.diagnostics
            );
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                entities
            );
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
            assert_eq!(
                world.resource::<HierarchyRoots>().as_slice(),
                hierarchy.as_slice()
            );
            assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
            assert_eq!(world.get::<SceneEntitySource>(old), Some(&old_source));
            assert!(world.get::<SceneEntitySource>(scratch).is_none());
            assert!(world.get::<PreparedNativeValue>(scratch).is_none());
            assert_eq!(prepared.0.spawned_by_id[&id], scratch);
            assert_eq!(world.resource::<Removed>().0, 1);
            match live_resource {
                Some(baseline) => {
                    assert!(world.resource::<ProductValue>().matches_baseline(&baseline));
                }
                None => assert!(!world.contains_resource::<ProductValue>()),
            }
            assert_eq!(world.resource::<AssetServer>(), &server);
            let ticks = world.get_resource_change_ticks::<AssetServer>().unwrap();
            assert_eq!(ticks.added, server_ticks.added);
            assert_eq!(ticks.changed, server_ticks.changed);
            let handle = *probe.handle.get().unwrap();
            assert!(
                world
                    .resource::<AssetServer>()
                    .get_handle::<ProbeAsset>(handle.id())
                    .is_none()
            );
            assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
            assert_eq!(probe.drops.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    fn baseline_does_not_extend_scene_native_claims_to_additional_retirements() {
        let (mut world, registry, candidate, current, old, id) = fixture();
        let retirement = spawn_identity_entity(&mut world).unwrap();
        let claims = SceneNativeRetirementClaims::from_component_ids([world
            .component_id::<NativeState>()
            .unwrap()]);
        let bindings = SceneAssetBindings::from_world(&world);
        let prepared = prepare_expanded_scene_replacement_with_claims(
            &mut world,
            &registry,
            &candidate,
            None,
            &current,
            bindings,
            &claims,
            limits(),
            &[retirement],
            |_| {},
        )
        .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics));
        world.commands().queue(move |world: &mut World| {
            world.entity_mut(retirement.entity()).insert(NativeState);
        });

        let report = prepared.validate_baseline(&mut world).unwrap_err();
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.prepared-baseline-changed"
            })
        );
        assert_eq!(world.resource::<Removed>().0, 0);
        assert!(world.get::<NativeState>(old).is_some());
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        assert!(prepared.discard(&mut world).is_ok());
        assert!(world.get::<NativeState>(retirement.entity()).is_some());
    }

    #[test]
    fn discard_materializes_deferred_candidate_despawn_before_selecting_live_entities() {
        let (mut world, registry, candidate, current, old, id) = fixture();
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let scratch = prepared.0.spawned_by_id[&id];
        world.commands().queue(move |world: &mut World| {
            world.entity_mut(scratch).despawn();
        });

        assert!(prepared.discard(&mut world).is_ok());
        assert!(world.get_entity(scratch).is_err());
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        assert_eq!(world.resource::<Removed>().0, 0);
    }

    #[test]
    fn discard_cleans_remaining_candidates_after_one_is_missing() {
        let (mut world, registry, _, current, old, id) = fixture();
        let other = SceneEntityId::new("other").unwrap();
        let candidate = ExpandedSceneCandidate::plain(
            source_key(),
            Arc::new(SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id.clone()),
                SceneEntityRecord::new(other.clone()),
            ])),
        );
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let missing = prepared.0.spawned_by_id[&id];
        let remaining = prepared.0.spawned_by_id[&other];
        world.entity_mut(missing).despawn();

        assert!(prepared.discard(&mut world).is_ok());
        assert!(world.get_entity(remaining).is_err());
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
    }

    #[test]
    fn discard_does_not_clean_recycled_entity_generation() {
        let (mut world, registry, candidate, current, _, id) = fixture();
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let scratch = prepared.0.spawned_by_id[&id];
        let successor = world.despawn_no_free(scratch).unwrap();
        world.spawn_empty_at(successor).unwrap();
        assert_eq!(successor.index(), scratch.index());
        assert_ne!(successor, scratch);

        assert!(prepared.discard(&mut world).is_ok());
        assert!(world.get_entity(successor).is_ok());
    }

    #[test]
    fn discard_rejects_another_world_even_when_candidate_entities_are_missing() {
        let (mut world, registry, candidate, current, _, _) = fixture();
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let mut other_world = World::new();
        let other_domain =
            WorldIdentityDomain::new(&other_world, WorldIdentityDomainSettings::default()).unwrap();
        other_world.insert_resource(other_domain);
        let other = spawn_identity_entity(&mut other_world).unwrap();

        let prepared = match prepared.discard(&mut other_world) {
            Err(prepared) => prepared,
            Ok(()) => panic!("another World must not release candidate custody"),
        };
        assert!(other_world.get_entity(other.entity()).is_ok());
        assert!(prepared.discard(&mut world).is_ok());
    }

    #[test]
    fn discard_rejects_a_new_identity_incarnation_on_the_same_entity() {
        let (mut world, registry, candidate, current, _, id) = fixture();
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let original = prepared.0.identity_tokens[&id];
        world.entity_mut(original.entity()).clear();
        let successor =
            world.resource_scope(|world, domain: nara_ecs::Mut<WorldIdentityDomain>| {
                domain.adopt_entity(world, original.entity()).unwrap()
            });
        assert_eq!(successor.entity(), original.entity());
        assert_ne!(successor.incarnation(), original.incarnation());

        assert!(prepared.discard(&mut world).is_err());
        assert!(world.get_entity(successor.entity()).is_ok());
    }

    #[test]
    fn discard_rejects_candidate_with_a_new_semantic_identity_axis() {
        let (mut world, registry, candidate, current, _, id) = fixture();
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        let token = prepared.0.identity_tokens[&id];
        world.resource_scope(|world, mut domain: nara_ecs::Mut<WorldIdentityDomain>| {
            domain
                .register_persistent(
                    world,
                    token,
                    PersistentRuntimeReference::new(
                        PersistentRuntimeNamespaceId::new("test").unwrap(),
                        PersistentRuntimeId::parse_str("44444444-4444-4444-8444-444444444444")
                            .unwrap(),
                    ),
                )
                .unwrap();
        });

        assert!(prepared.discard(&mut world).is_err());
        assert!(world.get_entity(token.entity()).is_ok());
    }

    #[test]
    fn owned_replacement_retries_exact_candidate_without_replaying_configuration() {
        let (mut world, _, _, current, old, id) = fixture();
        let probe = Arc::new(NativePreparationProbe::default());
        let (registry, candidate) = native_value_candidate(id.clone(), &probe);
        let calls = Arc::new(AtomicUsize::new(0));
        let claims = SceneNativeRetirementClaims::from_component_ids([world
            .component_id::<NativeState>()
            .unwrap()]);
        let bindings = SceneAssetBindings::from_world(&world);
        let count = Arc::clone(&calls);
        let prepared = prepare_expanded_scene_replacement_with_claims(
            &mut world,
            &registry,
            &candidate,
            None,
            &current,
            bindings,
            &claims,
            limits(),
            &[],
            move |overlay| {
                count.fetch_add(1, Ordering::Relaxed);
                overlay.replace_resource(ProductValue::new(2));
            },
        )
        .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics));
        assert_eq!(world.resource::<Removed>().0, 0);
        assert_eq!(world.resource::<ProductValue>().0, 1);
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
        assert_eq!(probe.drops.load(Ordering::Relaxed), 0);
        prepared.validate_baseline(&mut world).unwrap();
        let handle = *probe.handle.get().unwrap();
        assert!(!world.contains_resource::<AssetServer>());
        let (prepared, report) = match prepared.commit(&mut world) {
            Err(failure) => failure,
            Ok(_) => panic!("native ownership was not retired"),
        };
        assert!(report.diagnostics.has_errors());
        assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
        assert_eq!(probe.drops.load(Ordering::Relaxed), 0);
        assert!(!world.contains_resource::<AssetServer>());
        world.entity_mut(old).remove::<NativeState>();
        let report = match prepared.commit(&mut world) {
            Ok(report) => report,
            Err((_prepared, report)) => panic!("finalization failed: {:?}", report.diagnostics),
        };
        assert!(report.instance.is_some());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(world.resource::<Removed>().0, 1);
        assert_eq!(world.resource::<ProductValue>().0, 2);
        assert!(world.get_entity(old).is_err());
        let successor = report.instance.unwrap();
        let EntityLookup::Resolved(entity) = successor.resolve(&world, &id) else {
            panic!("replacement must resolve")
        };
        assert_eq!(
            world.get::<PreparedNativeValue>(entity).unwrap().handle,
            handle
        );
        assert_eq!(
            world
                .resource::<AssetServer>()
                .path(handle.id())
                .map(nara_asset::AssetSourcePath::as_str),
            Some("textures/retained-native.png")
        );
        assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
        assert_eq!(probe.drops.load(Ordering::Relaxed), 0);
        retire_and_despawn_scene_instance(&mut world, &successor).unwrap();
        assert_eq!(probe.drops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn cancelled_owned_replacement_disposes_native_value_without_publishing_its_asset() {
        let (mut world, _, _, current, old, id) = fixture();
        let product_baseline = world.resource::<ProductValue>().capture_baseline();
        let probe = Arc::new(NativePreparationProbe::default());
        let (registry, candidate) = native_value_candidate(id.clone(), &probe);
        let prepared = prepare_fixture_replacement(&mut world, &registry, &candidate, &current);
        assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
        assert_eq!(probe.drops.load(Ordering::Relaxed), 0);
        assert!(prepared.discard(&mut world).is_ok());
        assert_eq!(probe.runs.load(Ordering::Relaxed), 1);
        assert_eq!(probe.drops.load(Ordering::Relaxed), 1);
        assert!(!world.contains_resource::<AssetServer>());
        assert!(
            world
                .resource::<ProductValue>()
                .matches_baseline(&product_baseline)
        );
        assert!(world.get::<NativeState>(old).is_some());
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        assert_eq!(world.resource::<Removed>().0, 0);
    }

    #[test]
    fn replacement_revalidates_additional_retirement_identity_before_publication() {
        let (mut world, registry, candidate, current, old, id) = fixture();
        let retirement = spawn_identity_entity(&mut world).unwrap();
        let claims = SceneNativeRetirementClaims::from_component_ids([world
            .component_id::<NativeState>()
            .unwrap()]);
        let bindings = SceneAssetBindings::from_world(&world);
        let prepared = prepare_expanded_scene_replacement_with_claims(
            &mut world,
            &registry,
            &candidate,
            None,
            &current,
            bindings,
            &claims,
            limits(),
            &[retirement],
            |_| {},
        )
        .unwrap_or_else(|report| panic!("preparation failed: {:?}", report.diagnostics));

        world.commands().queue(move |world: &mut World| {
            world.resource_scope(|world, mut domain: nara_ecs::Mut<WorldIdentityDomain>| {
                domain
                    .register_persistent(
                        world,
                        retirement,
                        PersistentRuntimeReference::new(
                            PersistentRuntimeNamespaceId::new("test").unwrap(),
                            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333")
                                .unwrap(),
                        ),
                    )
                    .unwrap();
            });
        });

        let (prepared, report) = prepared
            .commit(&mut world)
            .expect_err("identity-axis changes must invalidate retained retirement authority");
        assert!(report.diagnostics.has_errors());
        assert!(world.get_entity(retirement.entity()).is_ok());
        assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        assert!(prepared.discard(&mut world).is_ok());
    }

    #[test]
    fn claims_do_not_admit_global_despawn_observers_or_unclaimed_native_state() {
        for global_observer in [false, true] {
            let (mut world, registry, candidate, current, old, id) = fixture();
            let claims = if global_observer {
                world.add_observer(|_: On<Despawn<()>>| {});
                SceneNativeRetirementClaims::from_component_ids([world
                    .component_id::<NativeState>()
                    .unwrap()])
            } else {
                SceneNativeRetirementClaims::default()
            };
            world.flush();
            let count = world.entities().len();
            let bindings = SceneAssetBindings::from_world(&world);
            let result = prepare_expanded_scene_replacement_with_claims(
                &mut world,
                &registry,
                &candidate,
                None,
                &current,
                bindings,
                &claims,
                limits(),
                &[],
                |_| {},
            );
            assert!(result.is_err());
            assert_eq!(
                world.entities().len(),
                count,
                "rejection precedes candidate allocation"
            );
            assert_eq!(world.resource::<Removed>().0, 0);
            assert!(world.get::<NativeState>(old).is_some());
            assert_eq!(current.resolve(&world, &id), EntityLookup::Resolved(old));
        }
    }
}
