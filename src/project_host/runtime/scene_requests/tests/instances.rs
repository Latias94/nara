use std::{fs, sync::Arc};
#[cfg(feature = "runtime-2d")]
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::fs::ContentDigest;
use nara_app::{App, PluginError};
#[cfg(feature = "runtime-2d")]
use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, Assets};
use nara_asset::{
    AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetReferenceDemand, AssetServer,
    AssetSourceKind, AssetStates, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_core::ItemLimit;
use nara_ecs::identity::{
    EntityLookup, EntityReference, PrefabEntityTarget, SceneEntityId, SpawnedSceneInstance,
    WorldIdentityDomain,
};
use nara_ecs::{Entity, World};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId, encode_asset_reference_value,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument,
    SceneEntityPlacement, SceneEntityRecord, SceneEntitySource, SceneOccurrenceKey, ScenePlacement,
    ScenePlacementParent, SceneSpawnReport, SourceDocumentKey, SourceDocumentKind,
    advanced::SceneProductTransactionLimits,
    hierarchy::{Children, HierarchyRoots, Parent},
    retire_and_despawn_scene_instance, scene_component_bindings,
};

#[cfg(feature = "runtime-2d")]
use super::super::super::{CleanupDriveOutcome, ProjectHost};
use super::{
    Context, Fixture, ProjectSceneRequestError, ProjectSceneRequestState, ProjectScenes,
    activate_prepared_project_scene, begin_prepared_project_scene_travel,
    commit_prepared_project_scene, poll_project_scene_requests, pools, processed_configuration,
    wait_for_request, with_context_world,
};
#[cfg(feature = "runtime-2d")]
use super::{
    PreparedSceneContent, RequestOwner, Slot, assert_active_budget,
    preparation::{TestGate, TestPhase},
    with_context_world_and_startup,
};
#[cfg(feature = "runtime-2d")]
use crate::{
    project_host::ProjectContentSnapshot, startup_scene::restart_current_scene_with_product,
};
use crate::{
    project_host::{
        inspect_current_scene_dependency, instantiate_current_scene_dependency,
        open_explicit_product_project_root, resolve_runtime_configuration,
        unload_project_scene_addition,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentBase, RuntimeContentBudgetKind,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
    startup_scene::{CurrentScene, CurrentSceneOwner},
};

const IMMEDIATE: &str = "test.scene-dependency.Immediate";
const DEFERRED: &str = "test.scene-dependency.Deferred";
const LINK: &str = "test.scene-dependency.Link";

#[cfg(feature = "runtime-2d")]
#[test]
fn additive_cancellation_retains_the_original_slot_until_its_worker_returns() {
    let (_fixture, context, snapshot) = source_fixture();
    with_context_world_and_startup(context, snapshot.scene_source(), Some(snapshot), |world| {
        world.insert_resource(pools());
        let context = Arc::clone(&world.resource::<RequestOwner>().context);
        let Context::Source { loader, .. } = context.as_ref() else {
            unreachable!()
        };
        let original_budget = loader.budget_snapshot();
        let (gate, control) = TestGate::new(TestPhase::AfterPreparation);
        world.resource_mut::<RequestOwner>().gate = Some(gate);
        let primary = CurrentScene::get(world).unwrap().receipt().clone();
        let request = ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.request_additive(product(3)),
                Err(ProjectSceneRequestError::NotDeferredScene)
            );
            assert_eq!(
                scenes.request_additive(product(2)),
                Err(ProjectSceneRequestError::NotDeferredScene)
            );
            scenes.request_additive(product(4)).unwrap()
        });
        assert!(control.wait_until_started());
        ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(request)));
        poll_project_scene_requests(world);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Cancelling)
            );
            assert!(!scenes.acknowledge(request));
            assert_eq!(
                scenes.request_additive(product(4)),
                Err(ProjectSceneRequestError::Busy)
            );
        });
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
        control.release();
        wait_for_request(world, request, ProjectSceneRequestState::Cancelled);
        assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
        assert_active_budget(&context, original_budget);
        let request =
            ProjectScenes::scope(world, |scenes| scenes.request_additive(product(4))).unwrap();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        assert!(matches!(
            world.resource::<RequestOwner>().slot.as_ref(),
            Some(Slot::Ready {content: PreparedSceneContent::Source(snapshot), .. })
                if matches!(snapshot.selected_scene_source_key().origin(),
                    nara_scene::SourceDocumentOrigin::ProjectLocation { path, .. }
                        if path.as_str() == "assets/deferred.scene.json")
        ));
        assert_error(
            &commit_prepared_project_scene(world, request, limits(), &[], |_| {}),
            "project.scene.request-purpose-mismatch",
        );
        let occurrence = SceneOccurrenceKey::new("test.scene-dependency", "deferred").unwrap();
        let report = activate_prepared_project_scene(world, request, &occurrence);
        let receipt = report
            .instance
            .unwrap_or_else(|| panic!("{:?}", report.diagnostics));
        assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
        assert!(unload_project_scene_addition(world, &primary).is_err());
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
        assert_error(
            &restart_current_scene_with_product(world, &primary, limits(), &[], |_| {}),
            "project.scene.addition-active",
        );
        assert_eq!(unload_project_scene_addition(world, &receipt).unwrap(), 1);
        assert!(unload_project_scene_addition(world, &receipt).is_err());
        assert_active_budget(&context, original_budget);

        let request =
            ProjectScenes::scope(world, |scenes| scenes.request_additive(product(4))).unwrap();
        wait_for_request(world, request, ProjectSceneRequestState::Ready);
        let receipt = activate_prepared_project_scene(world, request, &occurrence)
            .instance
            .unwrap();
        assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
        assert_eq!(
            retire_and_despawn_scene_instance(world, &receipt).unwrap(),
            1
        );
        assert_eq!(unload_project_scene_addition(world, &receipt).unwrap(), 0);
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
        assert_active_budget(&context, original_budget);
    });
}

#[test]
fn coordinated_travel_revalidates_and_retires_two_occurrences_of_the_same_deferred_scene() {
    let fixture = Fixture::empty();
    let base = processed_base(&fixture);
    let source = crate::startup_scene::SceneSource::from_hosted_runtime_content(
        base.acquire_startup_scene().unwrap(),
    );
    with_context_world(Arc::new(Context::Processed { base }), source, |world| {
        world.insert_resource(pools());
        let primary = CurrentScene::get(world).unwrap().receipt().clone();
        ProjectScenes::scope(world, |scenes| {
            for product in [product(2), product(3)] {
                assert_eq!(
                    scenes.request_additive(product),
                    Err(ProjectSceneRequestError::NotDeferredScene)
                );
            }
            assert_eq!(
                scenes.request(product(4)),
                Err(ProjectSceneRequestError::UndeclaredScene)
            );
        });
        let activate = |world: &mut World, slot| {
            let request =
                ProjectScenes::scope(world, |scenes| scenes.request_additive(product(4))).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            let report = activate_prepared_project_scene(world, request, &occurrence(slot));
            let receipt = report
                .instance
                .unwrap_or_else(|| panic!("{:?}", report.diagnostics));
            assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
            receipt
        };
        let travel = |world: &mut World| {
            let request = ProjectScenes::scope(world, |scenes| scenes.request(product(2))).unwrap();
            wait_for_request(world, request, ProjectSceneRequestState::Ready);
            request
        };
        let original = activate(world, "first");
        let second = activate(world, "second");
        let deferred = entity("deferred");
        assert_ne!(original.instance_id(), second.instance_id());
        let EntityLookup::Resolved(first_entity) = original.resolve(world, &deferred) else {
            panic!("missing first occurrence")
        };
        let EntityLookup::Resolved(second_entity) = second.resolve(world, &deferred) else {
            panic!("missing second occurrence")
        };
        assert_ne!(first_entity, second_entity);
        let request = travel(world);
        assert_error(
            &commit_prepared_project_scene(world, request, limits(), &[], |_| {
                panic!("unadmitted overlay")
            }),
            "scene.product-retirement-limit-exceeded",
        );
        assert!(ProjectScenes::scope(world, |scenes| scenes.acknowledge(request)));
        assert_eq!(
            original.resolve(world, &deferred),
            EntityLookup::Resolved(first_entity)
        );
        assert_eq!(
            second.resolve(world, &deferred),
            EntityLookup::Resolved(second_entity)
        );
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);

        let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::new(2).unwrap());
        let request = travel(world);
        begin_prepared_project_scene_travel(world, request, limits, &[], |_| {}).unwrap();
        assert_eq!(unload_project_scene_addition(world, &original).unwrap(), 1);
        world.resource_mut::<nara_app::RealTime>().frame += 1;
        poll_project_scene_requests(world);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Rejected(
                    ProjectSceneRequestError::CommitRejected
                ))
            );
            assert!(
                scenes
                    .diagnostics(request)
                    .unwrap()
                    .iter()
                    .any(|entry| entry.code().as_str() == "project.scene.additions-changed")
            );
            assert!(scenes.acknowledge(request));
        });
        assert_eq!(
            second.resolve(world, &deferred),
            EntityLookup::Resolved(second_entity)
        );
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
        let fresh = activate(world, "first");
        assert_ne!(fresh.instance_id(), original.instance_id());

        let request = travel(world);
        world
            .resource_mut::<super::super::domains::ManagedSceneDomains>()
            .register(Box::new(RetireOnce))
            .unwrap();
        begin_prepared_project_scene_travel(world, request, limits, &[], |_| {}).unwrap();
        world.resource_mut::<nara_app::RealTime>().frame += 1;
        poll_project_scene_requests(world);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Degraded)
            );
        });
        let error = unload_project_scene_addition(world, &second).unwrap_err();
        assert!(
            error
                .iter()
                .any(|entry| entry.code().as_str() == "project.scene.additions-retiring")
        );
        assert_eq!(
            second.resolve(world, &deferred),
            EntityLookup::Resolved(second_entity)
        );
        assert!(ProjectScenes::scope(world, |scenes| scenes.retry(request)));
        poll_project_scene_requests(world);
        ProjectScenes::scope(world, |scenes| {
            assert_eq!(
                scenes.state(request),
                Some(ProjectSceneRequestState::Committed)
            );
            assert!(scenes.acknowledge(request));
        });
        assert_ne!(CurrentScene::get(world).unwrap().receipt(), &primary);
        for receipt in [fresh, second] {
            assert!(matches!(
                receipt.resolve(world, &deferred),
                EntityLookup::Tombstoned(_)
            ));
            assert!(unload_project_scene_addition(world, &receipt).is_err());
        }
    });
}

struct RetireOnce;
impl super::super::domains::ManagedSceneDomain for RetireOnce {
    fn id(&self) -> super::super::domains::ManagedSceneDomainId {
        super::super::domains::ManagedSceneDomainId::new("test.concurrent-room-retire")
    }

    fn prepare(
        &self,
        _: &World,
        _: &super::super::PreparedProjectSceneView<'_>,
    ) -> Result<
        super::super::domains::ManagedSceneDomainPreparation,
        super::super::domains::ManagedSceneDomainError,
    > {
        Ok(super::super::domains::ManagedSceneDomainPreparation::new(
            nara_scene::advanced::SceneNativeRetirementClaims::default(),
            FailFirstRetirement(false),
        ))
    }
}

struct FailFirstRetirement(bool);
impl super::super::domains::ManagedSceneDomainAttempt for FailFirstRetirement {
    fn retire(
        &mut self,
        _: &mut World,
        _: &SpawnedSceneInstance,
    ) -> Result<(), super::super::domains::ManagedSceneDomainError> {
        if std::mem::replace(&mut self.0, true) {
            return Ok(());
        }
        Err(super::super::domains::ManagedSceneDomainError::new(
            nara_core::diagnostic::DiagnosticCode::new("test.concurrent-room-retire-failed")
                .unwrap(),
        ))
    }

    fn reconcile(
        &mut self,
        _: &mut World,
        _: &SpawnedSceneInstance,
    ) -> Result<(), super::super::domains::ManagedSceneDomainError> {
        Ok(())
    }
}

#[cfg(feature = "runtime-2d")]
#[test]
fn source_dependency_instances_share_capture_and_isolate_hierarchy_references_and_occurrences() {
    let (fixture, context, snapshot) = source_fixture();
    let dependency = &snapshot.scene_dependencies()[0];
    let source = dependency.source_key().clone();
    let address = std::ptr::from_ref(dependency.compiled_scene()) as usize;
    let root = fixture.root.clone();
    with_context_world_and_startup(
        context,
        snapshot.scene_source(),
        Some(snapshot),
        move |world| {
            let context = Arc::clone(&world.resource::<RequestOwner>().context);
            let Context::Source { loader, .. } = context.as_ref() else {
                unreachable!()
            };
            let budget = loader.budget_snapshot();
            fs::write(root.join("scenes/dependency.scene.json"), b"{ malformed").unwrap();
            fs::write(
                root.join("scenes/dependency.scene.json.meta"),
                b"{ malformed",
            )
            .unwrap();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            assert_eq!(
                inspect_current_scene_dependency(world, &expected, product(3), |document| {
                    std::ptr::from_ref(document) as usize
                })
                .unwrap(),
                address,
            );
            exercise_instances(world, &expected, &source);
            let restarted =
                restart_current_scene_with_product(world, &expected, limits(), &[], |_| {});
            assert!(!restarted.diagnostics.has_errors(), "{restarted:?}");
            assert_ne!(restarted.instance.as_ref().unwrap(), &expected);
            assert_refused(
                world,
                &expected,
                product(3),
                "scene.current.expected-instance-mismatch",
            );
            assert_active_budget(&context, budget);
        },
    );
}

#[test]
fn processed_dependency_instances_use_original_hosted_immediate_members_without_acquisition() {
    let fixture = Fixture::empty();
    let base = processed_base(&fixture);
    assert!(base.acquire_scene(product(3)).is_err());
    fs::write(fixture.root.join("cooked/3.runtime.json"), b"{ malformed").unwrap();
    let source = crate::startup_scene::SceneSource::from_hosted_runtime_content(
        base.acquire_startup_scene().unwrap(),
    );
    with_context_world(
        Arc::new(Context::Processed { base: base.clone() }),
        source,
        move |world| {
            assert!(!world.contains_resource::<ProjectAssetDatabase>());
            assert!(
                world
                    .resource::<CurrentSceneOwner>()
                    .hosted_runtime_base()
                    .unwrap()
                    .same_base(&base)
            );
            let cap = RuntimeContentLimits::default().retained_bytes().get();
            let Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                required,
                limit,
            }) = base.reserve_retained(cap)
            else {
                panic!("the hosted base already retains its original content")
            };
            assert_eq!(limit, cap);
            let _remaining = base.reserve_retained(cap - (required - cap)).unwrap();
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            exercise_instances(
                world,
                &expected,
                &SourceDocumentKey::stable_asset(product(3).source(), SourceDocumentKind::Scene),
            );
            assert!(!world.contains_resource::<ProjectAssetDatabase>());
        },
    );
}

#[cfg(feature = "runtime-2d")]
#[test]
fn dependency_inspection_accepts_installed_startup_and_rejects_foreign_and_root_only_receipts() {
    let foreign = Arc::new(Mutex::new(None));
    let captured = Arc::clone(&foreign);
    Fixture::new().with_world(move |world| {
        *captured.lock().unwrap() = Some(CurrentScene::get(world).unwrap().receipt().clone());
    });
    let foreign = foreign.lock().unwrap().take().unwrap();
    Fixture::new().with_world(move |world| {
        let expected = CurrentScene::get(world).unwrap().receipt().clone();
        assert_eq!(foreign.instance_id(), expected.instance_id());
        assert_eq!(foreign.entity_ids(), expected.entity_ids());
        assert_ne!(foreign.domain_id(), expected.domain_id());
        assert_refused(
            world,
            &foreign,
            product(3),
            "scene.current.expected-instance-mismatch",
        );
        assert_refused(
            world,
            &expected,
            product(1),
            "project.scene.dependency-not-resident",
        );
    });

    let (_fixture, context, snapshot) = source_fixture();
    let Context::Source { plan, .. } = context.as_ref() else {
        unreachable!()
    };
    let mut host = ProjectHost::new(nara_app::RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan.clone()).unwrap();
    host.complete_start(&mut attempt).unwrap();
    assert!(
        host.with_running_world(|world| world.contains_resource::<StartupInspected>())
            .unwrap()
    );
    host.retire_running();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let CleanupDriveOutcome::Complete {
            failed,
            diagnostics,
            ..
        } = host.drive_cleanup_once()
        {
            assert!(!failed, "{diagnostics:?}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the dependency inspection Host must retire"
        );
        std::thread::yield_now();
    }
}

fn exercise_instances(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    source: &SourceDocumentKey,
) {
    let baseline = world.entities().count_spawned();
    let resources = resource_counts(world);
    assert_eq!(
        inspect_current_scene_dependency(world, expected, product(3), |document| {
            document.entities().len()
        })
        .unwrap(),
        3,
    );
    for selected in [product(1), product(2), product(4), product(9)] {
        assert_refused(
            world,
            expected,
            selected,
            "project.scene.dependency-not-resident",
        );
    }
    let auxiliary = AssetProductRef::new(
        product(3).source(),
        ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000099").unwrap(),
    )
    .unwrap();
    assert_refused(
        world,
        expected,
        auxiliary,
        "project.scene.dependency-not-resident",
    );
    let first_slot = occurrence("first");
    let second_slot = occurrence("second");
    let first = spawn(world, expected, &first_slot);
    let second = spawn(world, expected, &second_slot);
    assert_ne!(first.instance_id(), second.instance_id());
    assert_eq!(world.entities().count_spawned(), baseline + 6);
    let first_entities = assert_instance(world, &first, &first_slot, source);
    let second_entities = assert_instance(world, &second, &second_slot, source);
    assert!(
        first_entities
            .iter()
            .all(|entity| !second_entities.contains(entity))
    );
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), expected);
    let identities = world.resource::<WorldIdentityDomain>().stats();
    let occupied = instantiate_current_scene_dependency(world, expected, product(3), &first_slot);
    assert_error(&occupied, "scene.occurrence-occupied");
    assert_eq!(world.entities().count_spawned(), baseline + 6);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
    assert_eq!(retire_and_despawn_scene_instance(world, &first).unwrap(), 3);
    assert!(
        first_entities
            .iter()
            .all(|entity| world.get_entity(*entity).is_err())
    );
    assert_instance(world, &second, &second_slot, source);
    let reused = spawn(world, expected, &first_slot);
    assert_ne!(reused.instance_id(), first.instance_id());
    let reused_entities = assert_instance(world, &reused, &first_slot, source);
    assert!(
        reused_entities
            .iter()
            .all(|entity| !first_entities.contains(entity))
    );
    assert!(retire_and_despawn_scene_instance(world, &first).is_err());
    assert_instance(world, &reused, &first_slot, source);
    assert_eq!(
        retire_and_despawn_scene_instance(world, &reused).unwrap(),
        3
    );
    assert_eq!(
        retire_and_despawn_scene_instance(world, &second).unwrap(),
        3
    );
    assert_eq!(world.entities().count_spawned(), baseline);
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), expected);
    assert_eq!(resource_counts(world), resources);
    nara_scene::hierarchy::validate_hierarchy(world).unwrap();
}

fn assert_refused(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    selected: AssetProductRef,
    code: &str,
) {
    let current = CurrentScene::get(world).unwrap();
    let receipt = current.receipt().clone();
    let source = std::ptr::from_ref(current.source());
    let entities = world.entities().count_spawned();
    let identities = world.resource::<WorldIdentityDomain>().stats();
    let roots = world
        .resource::<HierarchyRoots>()
        .iter()
        .collect::<Vec<_>>();
    let resources = resource_counts(world);
    let inspected = inspect_current_scene_dependency(world, expected, selected, |_| {
        panic!("a refused dependency must not invoke inspection")
    })
    .unwrap_err();
    assert_error(&inspected, code);
    let spawned =
        instantiate_current_scene_dependency(world, expected, selected, &occurrence("refused"));
    assert_error(&spawned, code);
    assert_eq!(world.entities().count_spawned(), entities);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
    assert_eq!(
        world
            .resource::<HierarchyRoots>()
            .iter()
            .collect::<Vec<_>>(),
        roots
    );
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &receipt);
    assert_eq!(
        std::ptr::from_ref(CurrentScene::get(world).unwrap().source()),
        source
    );
    assert_eq!(resource_counts(world), resources);
}

fn assert_instance(
    world: &World,
    instance: &SpawnedSceneInstance,
    slot: &SceneOccurrenceKey,
    source: &SourceDocumentKey,
) -> [Entity; 3] {
    assert_eq!(instance.len(), 3);
    let entities = ["node", "branch", "leaf"].map(|name| {
        let EntityLookup::Resolved(entity) = instance.resolve(world, &entity(name)) else {
            panic!("every dependency member must resolve in its own activation")
        };
        let origin = world.get::<SceneEntitySource>(entity).unwrap();
        assert_eq!(origin.instance_id, instance.instance_id());
        assert_eq!(origin.containing_source(), source);
        assert_eq!(origin.occurrence(), slot);
        entity
    });
    let [node, branch, leaf] = entities;
    assert!(world.get::<Parent>(node).is_none());
    assert_eq!(world.get::<Parent>(branch).unwrap().parent(), node);
    assert_eq!(world.get::<Parent>(leaf).unwrap().parent(), branch);
    assert_eq!(
        world
            .get::<Children>(node)
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [branch]
    );
    assert_eq!(
        world
            .get::<Children>(branch)
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [leaf]
    );
    let record = scene_component_bindings(world)
        .unwrap()
        .data_component(world, node, &ComponentTypeId::new(LINK))
        .unwrap()
        .unwrap();
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .resolve_entity_reference(
                world,
                record.value().as_entity_reference().unwrap(),
                Some(instance.instance_id()),
            ),
        EntityLookup::Resolved(leaf),
    );
    entities
}

fn spawn(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    slot: &SceneOccurrenceKey,
) -> SpawnedSceneInstance {
    let report = instantiate_current_scene_dependency(world, expected, product(3), slot);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    report.instance.unwrap()
}

fn assert_error(report: &SceneSpawnReport, code: &str) {
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{report:?}"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct ResourceCounts {
    retained_bytes: usize,
    asset_states: Option<usize>,
    #[cfg(feature = "runtime-2d")]
    images: Option<usize>,
    #[cfg(feature = "runtime-2d")]
    tilesets: Option<usize>,
}

fn resource_counts(world: &World) -> ResourceCounts {
    ResourceCounts {
        retained_bytes: world
            .resource::<AssetServer>()
            .checked_retained_bytes()
            .unwrap(),
        asset_states: world.get_resource::<AssetStates>().map(AssetStates::len),
        #[cfg(feature = "runtime-2d")]
        images: world
            .get_resource::<Assets<nara_render::image::ImageAsset>>()
            .map(Assets::len),
        #[cfg(feature = "runtime-2d")]
        tilesets: world
            .get_resource::<Assets<nara_render::tilemap::TileSet>>()
            .map(Assets::len),
    }
}

#[cfg(feature = "runtime-2d")]
fn source_fixture() -> (Fixture, Arc<Context>, ProjectContentSnapshot) {
    let fixture = Fixture::new();
    let manifest = fixture.root.join("nara.toml");
    let settings = fs::read_to_string(&manifest).unwrap().replace(
        "[authoring]\nstate = \"ready\"",
        concat!(
            "[runtime]\npreset = \"local-headless\"\n",
            "[authoring]\nstate = \"ready\"\nschema_bundle = \"schema/authoring.bundle.json\"",
        ),
    );
    fs::write(&manifest, settings).unwrap();
    for (id, path, document) in documents() {
        let directory = fixture.root.join(if id == 4 { "assets" } else { "scenes" });
        fs::write(directory.join(path), document.to_json_string().unwrap()).unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                product(id).source(),
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            directory.join(format!("{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    let context = fixture.context_with_recipe(recipe());
    let Context::Source {
        candidate,
        plan,
        loader,
        ..
    } = context.as_ref()
    else {
        unreachable!()
    };
    let validation = plan.schema_validation();
    fs::create_dir(fixture.root.join("schema")).unwrap();
    fs::write(
        fixture.root.join("schema/authoring.bundle.json"),
        validation
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
        &loader.project_root(),
        candidate,
        plan,
    )
    .unwrap();
    let snapshot = loader
        .load_with_runtime_agreement(candidate, plan, agreement)
        .unwrap();
    assert_eq!(snapshot.scene_dependencies().len(), 1);
    (fixture, context, snapshot)
}

fn processed_base(fixture: &Fixture) -> RuntimeContentBase {
    let directory = fixture.root.join("cooked");
    fs::create_dir(&directory).unwrap();
    let configuration = processed_configuration();
    let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe()).unwrap();
    let mut database = ProjectAssetDatabase::default();
    for (id, path, _) in documents() {
        database
            .insert(AssetRecord::new(
                product(id).source(),
                nara_asset::AssetSourcePath::new(format!("scenes/{path}")).unwrap(),
                AssetSourceKind::Scene,
            ))
            .unwrap();
    }
    let mut members = Vec::new();
    for (id, _, document) in documents() {
        let source =
            SourceDocumentKey::stable_asset(product(id).source(), SourceDocumentKind::Scene);
        let compiled = crate::startup_scene::compile_test_scene(
            &document,
            plan.schema_validation().bindings(),
            source.clone(),
        );
        let encoded = RuntimeSceneCandidate::cook(
            &compiled,
            plan.schema_validation().bindings(),
            database.resolver(&AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        let path = format!("{id}.runtime.json");
        fs::write(directory.join(&path), &encoded).unwrap();
        let dependencies = if id == 1 {
            vec![
                RuntimeContentDependency::new(product(3), RuntimeContentDependencyRole::Required),
                RuntimeContentDependency::new(product(4), RuntimeContentDependencyRole::Reference),
            ]
        } else {
            Vec::new()
        };
        members.push(
            RuntimeContentMember::new(
                path,
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Scene { source },
                vec![product(id)],
                dependencies,
            )
            .unwrap(),
        );
    }
    let target = RuntimeContentTarget::new("test-host", "scene-dependencies").unwrap();
    let catalog = RuntimeContentCatalog::new(
        RuntimeContentCatalogDefinition {
            target: target.clone(),
            configuration,
            plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
            schema_fingerprint: plan
                .schema_validation()
                .composition_fingerprint()
                .as_bytes(),
            runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
            startup_scene: product(1),
            scenes: vec![product(1), product(2)],
            addressable_roots: vec![],
            members,
        },
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap();
    fs::write(directory.join(RUNTIME_CONTENT_CATALOG_FILE), &catalog).unwrap();
    RuntimeContentBase::open(
        &open_explicit_product_project_root(&directory).unwrap(),
        &RuntimeContentExpectation::new(ContentDigest::of_bytes(&catalog), target),
        recipe(),
        RuntimeContentLimits::default(),
    )
    .unwrap()
}

fn documents() -> [(u64, &'static str, SceneDocument); 4] {
    let startup = SceneEntityRecord::new(entity("startup"))
        .with_component(
            ComponentTypeId::new(IMMEDIATE),
            record(encode_asset_reference_value(&AssetRef::Product(product(3)))),
        )
        .with_component(
            ComponentTypeId::new(DEFERRED),
            record(encode_asset_reference_value(&AssetRef::Product(product(4)))),
        );
    let node = SceneEntityRecord::new(entity("node")).with_component(
        ComponentTypeId::new(LINK),
        record(ComponentValue::EntityReference(
            EntityReference::SceneLocal {
                entity: entity("leaf"),
            },
        )),
    );
    let dependency = SceneDocument::new(
        [
            SceneEntityPlacement::root(entity("node")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(entity("branch")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    entity("node"),
                ))),
            ),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(entity("leaf")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    entity("branch"),
                ))),
            ),
        ],
        [
            node,
            SceneEntityRecord::new(entity("branch")),
            SceneEntityRecord::new(entity("leaf")),
        ],
    );
    [
        (
            1,
            "startup.scene.json",
            SceneDocument::from_ordered_roots([startup]),
        ),
        (
            2,
            "second.scene.json",
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity("second"))]),
        ),
        (3, "dependency.scene.json", dependency),
        (
            4,
            "deferred.scene.json",
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity("deferred"))]),
        ),
    ]
}

fn product(id: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{id:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn entity(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}
fn occurrence(slot: &str) -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("test.scene-dependency", slot).unwrap()
}
fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE)
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.scene-dependency").unwrap()
}
fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}

#[derive(nara_ecs::Resource)]
struct StartupInspected;

#[derive(Default)]
struct DependencyPlugin;

impl nara_app::Plugin for DependencyPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        static DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
            nara_app::PluginId::new("test.scene-dependency"),
            nara_app::PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        crate::startup_scene::add_startup_scene_activation_systems(app, |world: &mut World| {
            let expected = CurrentScene::get(world).unwrap().receipt().clone();
            let entities = world.entities().count_spawned();
            assert_eq!(
                inspect_current_scene_dependency(world, &expected, product(3), |document| document
                    .entities()
                    .len())
                .unwrap(),
                3
            );
            let report = instantiate_current_scene_dependency(
                world,
                &expected,
                product(3),
                &occurrence("startup"),
            );
            assert_error(&report, "scene.current.startup-not-finalized");
            assert_eq!(world.entities().count_spawned(), entities);
            assert_eq!(CurrentScene::get(world).unwrap().receipt(), &expected);
            world.insert_resource(StartupInspected);
        })?;
        Ok(())
    }
}

fn recipe() -> crate::ProductRecipe {
    let components = [
        (
            IMMEDIATE,
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
            ComponentCapability::AssetRef,
        ),
        (
            DEFERRED,
            ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
            ComponentCapability::AssetRef,
        ),
        (
            LINK,
            ComponentValueKind::EntityRef.into(),
            ComponentCapability::EntityRef,
        ),
    ]
    .map(|(name, shape, capability)| {
        ComponentSchema::new(
            ComponentTypeId::new(name),
            name,
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::empty(),
            shape,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(capability)])
    });
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: components.into(),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    crate::ProductRecipe::new()
        .add_contribution(
            crate::SchemaContribution::<DependencyPlugin>::data_for_default([owner]).unwrap(),
        )
        .unwrap()
}
