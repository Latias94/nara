use super::*;
use crate::advanced::*;
use crate::tests::{scene_occurrence, scene_source_key};

use nara_app::App;
use nara_core::ItemLimit;
use nara_ecs::{
    Component, Entity, Mut, Resource, World,
    bevy_ecs::lifecycle::{Add, Despawn},
    bevy_ecs::observer::{Observer, On},
    system::ResMut,
};
use nara_hierarchy::{
    __private::completed_topology_generation, HierarchyOperation, HierarchyPlacement,
    HierarchyPlacementOwner, HierarchyPlugin, HierarchyRoots, HierarchyWriter, Parent,
};
use nara_identity::{
    EntityLookup, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    WorldEntityLocator, WorldEntityToken, WorldIdentityDomain, resolve_in_world,
    spawn_identity_entity,
};
use nara_reflect::{ComponentRegistry, ComponentSchemaVersion, ComponentValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
struct RuntimeValue(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
struct RuntimeTag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
struct RuntimeRetirement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
struct RunState(u32);

impl SceneProductResource for RunState {}

#[derive(Debug, Default, Resource)]
struct LifecycleCanary(u32);

struct ReplacementFixture {
    world: World,
    registry: ComponentRegistry,
    replacement: SceneDocument,
    current: SpawnedSceneInstance,
    target: SceneEntityId,
    old_entity: Entity,
}

impl ReplacementFixture {
    fn new() -> Self {
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let target = scene_id("player");
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(target.clone())]);
        let mut world = World::new();
        world.insert_resource(RunState(1));
        let initial = crate::spawn_scene(
            &mut world,
            &registry,
            &crate::ExpandedSceneCandidate::plain(
                scene_source_key("scene-product/fixture"),
                std::sync::Arc::new((document).clone()),
            ),
            &scene_occurrence("new/initial"),
            None,
        );
        assert!(!initial.diagnostics.has_errors());
        let current = spawned_instance(&initial).clone();
        let old_entity = spawned_entity(&world, &initial, &target);
        Self {
            world,
            registry,
            replacement: document,
            current,
            target,
            old_entity,
        }
    }

    fn entity_ids(&self) -> Vec<Entity> {
        let mut entities = self
            .world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>();
        entities.sort_unstable_by_key(|entity| entity.to_bits());
        entities
    }

    fn assert_unchanged(
        &self,
        before_entities: &[Entity],
        before_stats: nara_identity::IdentityDomainStats,
    ) {
        assert_eq!(self.entity_ids(), before_entities);
        assert_eq!(
            self.world.resource::<WorldIdentityDomain>().stats(),
            before_stats
        );
        assert_eq!(self.world.resource::<RunState>().0, 1);
        assert_eq!(
            self.current.resolve(&self.world, &self.target),
            EntityLookup::Resolved(self.old_entity)
        );
    }
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn item_limit(value: usize) -> ItemLimit {
    ItemLimit::new(value).unwrap()
}

fn transaction_limits(overlay_writes: usize, retirements: usize) -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(item_limit(overlay_writes), item_limit(retirements))
}

fn spawned_instance(report: &SceneSpawnReport) -> &SpawnedSceneInstance {
    report
        .instance
        .as_ref()
        .expect("successful scene spawn must publish an instance")
}

fn spawned_entity(world: &World, report: &SceneSpawnReport, id: &SceneEntityId) -> Entity {
    match spawned_instance(report).resolve(world, id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("spawned scene entity did not resolve: {lookup:?}"),
    }
}

fn has_diagnostic(report: &SceneSpawnReport, code: &str) -> bool {
    report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == code)
}

fn persistent_reference(value: &str) -> PersistentRuntimeReference {
    PersistentRuntimeReference::new(
        PersistentRuntimeNamespaceId::new("test").unwrap(),
        PersistentRuntimeId::parse_str(value).unwrap(),
    )
}

fn register_persistent_axis(
    world: &mut World,
    token: WorldEntityToken,
    persistent: PersistentRuntimeReference,
) -> WorldEntityLocator {
    world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
        domain
            .register_persistent(world, token, persistent)
            .unwrap()
    })
}

fn adopt_token(world: &mut World, entity: Entity) -> WorldEntityToken {
    world.resource_scope(|world, domain: Mut<WorldIdentityDomain>| {
        domain.adopt_entity(world, entity).unwrap()
    })
}

fn spawn_retirement_token(world: &mut World) -> WorldEntityToken {
    let token = spawn_identity_entity(world).unwrap();
    world.entity_mut(token.entity()).insert(RuntimeRetirement);
    token
}

#[test]
fn product_replacement_commits_one_coherent_scene_generation() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let root_id = scene_id("root");
    let child_id = scene_id("root/child");
    let document = SceneDocument::new(
        [root_id.clone()],
        [
            SceneEntityRecord::new(root_id.clone()).with_children([child_id.clone()]),
            SceneEntityRecord::new(child_id.clone()),
        ],
    );
    let mut world = World::new();
    world.insert_resource(RunState(1));
    world.register_component::<RuntimeValue>();
    let initial = crate::spawn_scene(
        &mut world,
        &registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("product_replacement_commits_one_coherent_scene_generation/document"),
            std::sync::Arc::new((document).clone()),
        ),
        &scene_occurrence("product_replacement_commits_one_coherent_scene_generation/initial"),
        None,
    );
    assert!(!initial.diagnostics.has_errors());
    let current = spawned_instance(&initial).clone();
    let old_root = spawned_entity(&world, &initial, &root_id);
    let old_child = spawned_entity(&world, &initial, &child_id);

    let additional = spawn_retirement_token(&mut world);
    let unrelated_same_shape = spawn_retirement_token(&mut world);
    let persistent_sentinel = spawn_identity_entity(&mut world).unwrap();
    let sentinel_locator = register_persistent_axis(
        &mut world,
        persistent_sentinel,
        persistent_reference("11111111-1111-4111-8111-111111111111"),
    );

    let report = replace_expanded_scene_with_product(
        &mut world,
        &registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("product_replacement_commits_one_coherent_scene_generation/document"),
            std::sync::Arc::new((document).clone()),
        ),
        None,
        &current,
        transaction_limits(2, 1),
        &[additional],
        |overlay| {
            overlay
                .insert_component(child_id.clone(), RuntimeValue(7))
                .replace_resource(RunState(2));
        },
    );

    assert!(
        !report.diagnostics.has_errors(),
        "product replacement diagnostics: {:#?}",
        report.diagnostics
    );
    assert_eq!(report.retired_entities(), 3);
    assert!(world.get_entity(old_root).is_err());
    assert!(world.get_entity(old_child).is_err());
    assert!(world.get_entity(additional.entity()).is_err());
    assert!(world.get_entity(unrelated_same_shape.entity()).is_ok());
    assert_eq!(
        resolve_in_world(&world, &sentinel_locator),
        EntityLookup::Resolved(persistent_sentinel.entity())
    );
    assert_eq!(world.resource::<RunState>().0, 2);

    let new_root = spawned_entity(&world, &report, &root_id);
    let new_child = spawned_entity(&world, &report, &child_id);
    assert_eq!(world.get::<RuntimeValue>(new_child), Some(&RuntimeValue(7)));
    assert_eq!(
        world.get::<Parent>(new_child).map(Parent::parent),
        Some(new_root)
    );
    assert!(matches!(
        current.resolve(&world, &child_id),
        EntityLookup::Tombstoned(_)
    ));
}

#[test]
fn product_replacement_accepts_empty_overlay_and_retirement_sets() {
    let mut fixture = ReplacementFixture::new();

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[],
        |_| {},
    );

    assert!(!report.diagnostics.has_errors());
    assert_eq!(report.retired_entities(), 1);
    assert_eq!(fixture.world.resource::<RunState>().0, 1);
}

#[test]
fn product_transaction_engine_ceilings_reject_before_world_mutation() {
    let mut fixture = ReplacementFixture::new();
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let limits = SceneProductTransactionLimits::new(
        item_limit(SceneProductTransactionLimits::MAX_OVERLAY_WRITES + 1),
        item_limit(1),
    );

    let overlay_report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        limits,
        &[],
        |_| {},
    );

    assert!(has_diagnostic(
        &overlay_report,
        "scene.product-overlay-ceiling-exceeded"
    ));
    fixture.assert_unchanged(&before_entities, before_stats);

    let limits = SceneProductTransactionLimits::new(
        item_limit(1),
        item_limit(SceneProductTransactionLimits::MAX_ADDITIONAL_RETIREMENTS + 1),
    );
    let retirement_report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        limits,
        &[],
        |_| {},
    );

    assert!(has_diagnostic(
        &retirement_report,
        "scene.product-retirement-ceiling-exceeded"
    ));
    fixture.assert_unchanged(&before_entities, before_stats);
}

#[test]
fn product_overlay_rejections_leave_the_old_generation_unchanged() {
    for rejection in ["missing", "duplicate", "unregistered", "limit"] {
        let mut fixture = ReplacementFixture::new();
        fixture.world.register_component::<RuntimeValue>();
        fixture.world.register_component::<RuntimeTag>();
        let before_entities = fixture.entity_ids();
        let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();
        let component_count = fixture.world.components().len();
        let missing = scene_id("missing");
        let target = fixture.target.clone();

        let overlay_limit = match rejection {
            "duplicate" => 3,
            "limit" => 1,
            _ => 2,
        };
        let report = replace_expanded_scene_with_product(
            &mut fixture.world,
            &fixture.registry,
            &crate::ExpandedSceneCandidate::plain(
                scene_source_key("scene-product/fixture"),
                std::sync::Arc::new((fixture.replacement).clone()),
            ),
            None,
            &fixture.current,
            transaction_limits(overlay_limit, 1),
            &[],
            |overlay| {
                overlay.replace_resource(RunState(2));
                match rejection {
                    "missing" => {
                        overlay.insert_component(missing, RuntimeValue(1));
                    }
                    "duplicate" => {
                        overlay
                            .insert_component(target.clone(), RuntimeValue(1))
                            .insert_component(target, RuntimeValue(2));
                    }
                    "unregistered" => {
                        overlay.insert_component(target, RuntimeRetirement);
                    }
                    "limit" => {
                        overlay.insert_component(target, RuntimeTag);
                    }
                    _ => unreachable!(),
                }
            },
        );

        assert!(report.instance.is_none());
        let expected = match rejection {
            "missing" => "scene.product-overlay-target-missing",
            "duplicate" => "scene.product-overlay-component-duplicate",
            "unregistered" => "scene.product-overlay-component-unregistered",
            "limit" => "scene.product-overlay-limit-exceeded",
            _ => unreachable!(),
        };
        assert!(has_diagnostic(&report, expected), "case {rejection}");
        fixture.assert_unchanged(&before_entities, before_stats);
        assert_eq!(fixture.world.components().len(), component_count);
    }
}

#[test]
fn overlay_rejection_does_not_flush_the_deferred_world_baseline() {
    let mut fixture = ReplacementFixture::new();
    fixture.world.commands().queue(|world: &mut World| {
        world.resource_mut::<RunState>().0 = 99;
    });
    let missing = scene_id("missing");

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[],
        |overlay| {
            overlay.insert_component(missing, RuntimeValue(1));
        },
    );

    assert!(has_diagnostic(
        &report,
        "scene.product-overlay-target-missing"
    ));
    assert_eq!(fixture.world.resource::<RunState>().0, 1);
    fixture.world.flush();
    assert_eq!(fixture.world.resource::<RunState>().0, 99);
}

#[test]
fn invalid_existing_hierarchy_rejects_product_replacement_without_scratch_mutation() {
    let mut fixture = ReplacementFixture::new();
    let parent = fixture.world.spawn_empty().id();
    let child = fixture.world.spawn_empty().id();
    HierarchyWriter::new(&mut fixture.world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: parent,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
            },
        ])
        .unwrap();
    fixture.world.entity_mut(child).remove::<Parent>();
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[],
        |_| {},
    );

    assert!(has_diagnostic(&report, "scene.hierarchy-invalid"));
    fixture.assert_unchanged(&before_entities, before_stats);
}

#[test]
fn authored_component_collision_is_rejected_before_candidate_allocation() {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    let target = scene_id("player");
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(target.clone())
        .with_component(
            nara_reflect::ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Player".to_owned()),
            ),
        )]);
    let mut world = World::new();
    world.insert_resource(RunState(1));
    let initial = crate::spawn_scene(
        &mut world,
        &registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key(
                "authored_component_collision_is_rejected_before_candidate_allocation/document",
            ),
            std::sync::Arc::new((document).clone()),
        ),
        &scene_occurrence(
            "authored_component_collision_is_rejected_before_candidate_allocation/initial",
        ),
        None,
    );
    assert!(!initial.diagnostics.has_errors());
    let current = spawned_instance(&initial).clone();
    let old_entity = spawned_entity(&world, &initial, &target);
    let before_entities = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let before_stats = world.resource::<WorldIdentityDomain>().stats();

    let report = replace_expanded_scene_with_product(
        &mut world,
        &registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key(
                "authored_component_collision_is_rejected_before_candidate_allocation/document",
            ),
            std::sync::Arc::new((document).clone()),
        ),
        None,
        &current,
        transaction_limits(1, 1),
        &[],
        |overlay| {
            overlay.insert_component(target.clone(), Name::new("Runtime"));
        },
    );

    assert!(has_diagnostic(
        &report,
        "scene.product-overlay-component-existing"
    ));
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        before_entities
    );
    assert_eq!(
        world.resource::<WorldIdentityDomain>().stats(),
        before_stats
    );
    assert_eq!(
        current.resolve(&world, &target),
        EntityLookup::Resolved(old_entity)
    );
    assert_eq!(world.get::<Name>(old_entity).unwrap().as_str(), "Player");
}

#[test]
fn overlay_lifecycle_rejection_rolls_back_scratch_and_resource_replacement() {
    let mut fixture = ReplacementFixture::new();
    fixture.world.init_resource::<LifecycleCanary>();
    fixture.world.register_component::<RuntimeValue>();
    fixture.world.add_observer(
        |_: On<Add<RuntimeValue>>, mut canary: ResMut<LifecycleCanary>| canary.0 += 1,
    );
    fixture.world.flush();
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let target = fixture.target.clone();

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(2, 1),
        &[],
        |overlay| {
            overlay
                .replace_resource(RunState(2))
                .insert_component(target, RuntimeValue(1));
        },
    );

    assert!(has_diagnostic(
        &report,
        "scene.product-overlay-lifecycle-ineligible"
    ));
    assert_eq!(fixture.world.resource::<LifecycleCanary>().0, 0);
    fixture.assert_unchanged(&before_entities, before_stats);
}

#[test]
fn overlay_lifecycle_rejection_preserves_completed_hierarchy_generation() {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let root = scene_id("root");
    let child = scene_id("root/child");
    let document = SceneDocument::new(
        [root.clone()],
        [
            SceneEntityRecord::new(root).with_children([child.clone()]),
            SceneEntityRecord::new(child.clone()),
        ],
    );
    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    app.insert_resource(RunState(1)).unwrap();
    app.init_resource::<OverlayLifecycleRejectionPort>()
        .unwrap();
    app.__register_driver_port::<OverlayLifecycleRejectionPort>(
        OverlayLifecycleRejectionPort::apply_driver_input_in_world,
    )
    .unwrap();
    let initial = crate::spawn_scene(
        app.world_mut().unwrap(),
        &registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key(
                "overlay_lifecycle_rejection_preserves_completed_hierarchy_generation/document",
            ),
            std::sync::Arc::new((document).clone()),
        ),
        &scene_occurrence(
            "overlay_lifecycle_rejection_preserves_completed_hierarchy_generation/initial",
        ),
        None,
    );
    assert!(!initial.diagnostics.has_errors());
    let current = spawned_instance(&initial).clone();
    let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let completed_before = completed_topology_generation(runtime.world())
        .expect("the initial scene hierarchy must complete before replacement");
    let report = runtime
        .with_driver_scope(|scope| {
            scope
                .__apply_port::<OverlayLifecycleRejectionPort>((registry, document, current, child))
                .unwrap()
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap();

    assert!(has_diagnostic(
        &report,
        "scene.product-overlay-lifecycle-ineligible"
    ));
    assert_eq!(runtime.world().resource::<LifecycleCanary>().0, 0);
    assert_eq!(
        completed_topology_generation(runtime.world()),
        Some(completed_before),
        "rejected candidate preparation must not dirty the published hierarchy"
    );
    runtime.finish().unwrap();
}

#[derive(Default, Resource)]
struct OverlayLifecycleRejectionPort;

impl nara_app::__RuntimeDriverPort for OverlayLifecycleRejectionPort {
    type Input = (
        ComponentRegistry,
        SceneDocument,
        SpawnedSceneInstance,
        SceneEntityId,
    );
    type Output = SceneSpawnReport;

    fn apply_driver_input(&mut self, _input: Self::Input) -> SceneSpawnReport {
        unreachable!("the overlay transaction needs the scene world")
    }
}

impl OverlayLifecycleRejectionPort {
    fn apply_driver_input_in_world(
        world: &mut World,
        (registry, document, current, child): <Self as nara_app::__RuntimeDriverPort>::Input,
    ) -> Result<
        SceneSpawnReport,
        (
            nara_app::RuntimeWorldAccessError,
            <Self as nara_app::__RuntimeDriverPort>::Input,
        ),
    > {
        world.init_resource::<LifecycleCanary>();
        world.register_component::<RuntimeValue>();
        world.add_observer(
            |_: On<Add<RuntimeValue>>, mut canary: ResMut<LifecycleCanary>| canary.0 += 1,
        );
        world.flush();
        Ok(replace_expanded_scene_with_product(
            world,
            &registry,
            &crate::ExpandedSceneCandidate::plain(
                scene_source_key("apply_driver_input_in_world/replacement"),
                std::sync::Arc::new((document).clone()),
            ),
            None,
            &current,
            transaction_limits(1, 1),
            &[],
            |overlay| {
                overlay.insert_component(child, RuntimeValue(1));
            },
        ))
    }
}

#[test]
fn additional_retirement_limits_and_identity_axes_are_fail_closed() {
    let mut fixture = ReplacementFixture::new();
    let first = spawn_retirement_token(&mut fixture.world);
    let second = spawn_retirement_token(&mut fixture.world);
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();

    let over_limit = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[first, second],
        |_| {},
    );
    assert!(has_diagnostic(
        &over_limit,
        "scene.product-retirement-limit-exceeded"
    ));
    fixture.assert_unchanged(&before_entities, before_stats);

    let duplicate = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 2),
        &[first, first],
        |_| {},
    );
    assert!(has_diagnostic(
        &duplicate,
        "scene.product-retirement-entity-duplicate"
    ));
    fixture.assert_unchanged(&before_entities, before_stats);

    let scene_owned_token = adopt_token(&mut fixture.world, fixture.old_entity);
    let scene_owned = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[scene_owned_token],
        |_| {},
    );
    assert!(has_diagnostic(
        &scene_owned,
        "scene.product-retirement-scene-owned"
    ));
    fixture.assert_unchanged(&before_entities, before_stats);
}

#[test]
fn additional_retirement_rejects_a_foreign_token_with_matching_entity_bits() {
    let mut fixture = ReplacementFixture::new();
    let local = spawn_retirement_token(&mut fixture.world);
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();

    let mut foreign_fixture = ReplacementFixture::new();
    let foreign = spawn_retirement_token(&mut foreign_fixture.world);
    assert_eq!(foreign.entity(), local.entity());

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[foreign],
        |_| {},
    );

    assert!(has_diagnostic(
        &report,
        "scene.product-retirement-identity-wrong-world"
    ));
    assert!(fixture.world.get_entity(local.entity()).is_ok());
    fixture.assert_unchanged(&before_entities, before_stats);
}

#[test]
fn deferred_persistent_registration_is_part_of_the_validated_baseline() {
    let mut fixture = ReplacementFixture::new();
    let retirement = spawn_retirement_token(&mut fixture.world);
    let persistent = persistent_reference("33333333-3333-4333-8333-333333333333");
    fixture.world.commands().queue(move |world: &mut World| {
        world.resource_scope(|world, mut domain: Mut<WorldIdentityDomain>| {
            domain
                .register_persistent(world, retirement, persistent)
                .unwrap();
        });
    });

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[retirement],
        |_| {},
    );

    assert!(has_diagnostic(
        &report,
        "scene.product-retirement-persistent-owned"
    ));
    assert!(fixture.world.get_entity(retirement.entity()).is_ok());
    let locators = fixture
        .world
        .resource::<WorldIdentityDomain>()
        .locators_for_token(&fixture.world, retirement)
        .unwrap()
        .unwrap();
    assert!(locators.persistent().is_some());
    assert_eq!(fixture.world.resource::<RunState>().0, 1);
    assert_eq!(
        fixture.current.resolve(&fixture.world, &fixture.target),
        EntityLookup::Resolved(fixture.old_entity)
    );
}

#[test]
fn additional_retirement_rejects_missing_persistent_and_lifecycle_ownership() {
    let mut fixture = ReplacementFixture::new();

    let missing = spawn_retirement_token(&mut fixture.world);
    fixture.world.despawn(missing.entity());
    let missing_report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[missing],
        |_| {},
    );
    assert!(has_diagnostic(
        &missing_report,
        "scene.product-retirement-entity-missing"
    ));

    let persistent = spawn_identity_entity(&mut fixture.world).unwrap();
    let persistent_locator = register_persistent_axis(
        &mut fixture.world,
        persistent,
        persistent_reference("22222222-2222-4222-8222-222222222222"),
    );
    let persistent_report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[persistent],
        |_| {},
    );
    assert!(has_diagnostic(
        &persistent_report,
        "scene.product-retirement-persistent-owned"
    ));
    assert_eq!(
        resolve_in_world(&fixture.world, &persistent_locator),
        EntityLookup::Resolved(persistent.entity())
    );

    fixture.world.init_resource::<LifecycleCanary>();
    let lifecycle = spawn_retirement_token(&mut fixture.world);
    fixture.world.spawn(
        Observer::new(|_: On<Despawn<()>>, mut canary: ResMut<LifecycleCanary>| canary.0 += 1)
            .with_entity(lifecycle.entity()),
    );
    fixture.world.flush();
    let lifecycle_report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[lifecycle],
        |_| {},
    );
    assert!(
        has_diagnostic(
            &lifecycle_report,
            "scene.product-retirement-lifecycle-active"
        ),
        "lifecycle rejection diagnostics: {:#?}",
        lifecycle_report.diagnostics
    );
    assert_eq!(fixture.world.resource::<LifecycleCanary>().0, 0);
    assert!(fixture.world.get_entity(lifecycle.entity()).is_ok());
}

#[test]
fn additional_retirement_commits_runtime_hierarchy_and_promotes_survivors() {
    let mut fixture = ReplacementFixture::new();
    let runtime_root = spawn_retirement_token(&mut fixture.world);
    let surviving_child = fixture.world.spawn_empty().id();
    HierarchyWriter::new(&mut fixture.world)
        .apply_batch(&[
            HierarchyOperation::Place {
                entity: runtime_root.entity(),
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            },
            HierarchyOperation::Place {
                entity: surviving_child,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                    parent: runtime_root.entity(),
                }),
            },
        ])
        .unwrap();
    fixture.world.flush();

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &crate::ExpandedSceneCandidate::plain(
            scene_source_key("scene-product/fixture"),
            std::sync::Arc::new((fixture.replacement).clone()),
        ),
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[runtime_root],
        |_| {},
    );

    assert!(
        !report.diagnostics.has_errors(),
        "product replacement diagnostics: {:#?}",
        report.diagnostics
    );
    assert_eq!(report.retired_entities(), 2);
    assert!(fixture.world.get_entity(runtime_root.entity()).is_err());
    assert!(fixture.world.get_entity(surviving_child).is_ok());
    assert!(fixture.world.get::<Parent>(surviving_child).is_none());
    let replacement_root = spawned_entity(&fixture.world, &report, &fixture.target);
    assert_eq!(
        fixture.world.resource::<HierarchyRoots>().as_slice(),
        &[replacement_root, surviving_child]
    );
}

#[test]
fn expanded_candidate_keeps_nested_source_paths_through_spawn_and_product_replacement() {
    use nara_asset::AssetRef;
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let outer = AssetRef::path("prefabs/outer.ron").unwrap();
    let inner = AssetRef::path("prefabs/inner.ron").unwrap();
    let mut branch = SceneEntityRecord::new(scene_id("branch"));
    branch.prefab = Some(PrefabInstance {
        source: inner.clone(),
        overrides: None,
    });
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(
            outer.clone(),
            crate::tests::prefab_source_key(&outer),
            PrefabDocument::from_ordered_roots([branch]),
        )
        .with_prefab(
            inner.clone(),
            crate::tests::prefab_source_key(&inner),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("leaf"))]),
        );
    let mut anchor = SceneEntityRecord::new(scene_id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: outer.clone(),
        overrides: None,
    });
    let expansion =
        SceneDocument::from_ordered_roots([anchor]).expand_prefabs(&registry, &resolver);
    assert!(!expansion.diagnostics.has_errors());
    let candidate = expansion.candidate.unwrap().bind_source(scene_source_key(
        "expanded_candidate_keeps_nested_source_paths_through_spawn_and_product_replacement/source",
    ));
    let (leaf_id, expected_source) = candidate
        .provenance()
        .iter()
        .find(|(_, source)| source.local_entity_id() == &scene_id("leaf"))
        .unwrap();
    let path = expected_source.projection_path().unwrap();
    assert_eq!(path.root_anchor(), &scene_id("anchor"));
    assert_eq!(
        path.steps()
            .iter()
            .map(|step| match step {
                crate::PrefabProjectionStep::SourceEntity {
                    source_document, ..
                } => source_document,
                crate::PrefabProjectionStep::InstanceAddition { .. } =>
                    panic!("expected inherited source"),
            })
            .collect::<Vec<_>>(),
        [&outer, &inner]
    );
    let mut world = World::new();
    let initial = crate::spawn_scene(
        &mut world,
        &registry,
        &candidate,
        &scene_occurrence(
            "expanded_candidate_keeps_nested_source_paths_through_spawn_and_product_replacement/initial",
        ),
        None,
    );
    assert!(!initial.diagnostics.has_errors());
    let old_leaf = spawned_entity(&world, &initial, leaf_id);
    let old_source = world.get::<SceneEntitySource>(old_leaf).unwrap().clone();
    assert_eq!(old_source.containing_source(), candidate.source_key());
    assert_eq!(
        old_source.occurrence(),
        &scene_occurrence(
            "expanded_candidate_keeps_nested_source_paths_through_spawn_and_product_replacement/initial"
        ),
    );
    assert_eq!(
        old_source.provenance().local_entity_id(),
        expected_source.local_entity_id()
    );
    assert_eq!(old_source.provenance().projection_path(), Some(path));
    let replacement = replace_expanded_scene_with_product(
        &mut world,
        &registry,
        &candidate,
        None,
        spawned_instance(&initial),
        transaction_limits(1, 1),
        &[],
        |_| {},
    );
    assert!(!replacement.diagnostics.has_errors());
    assert!(world.get_entity(old_leaf).is_err());
    let leaf = spawned_entity(&world, &replacement, leaf_id);
    let source = world.get::<SceneEntitySource>(leaf).unwrap();
    assert_eq!(source.containing_source(), candidate.source_key());
    assert_eq!(source.occurrence(), old_source.occurrence());
    assert_eq!(
        source.provenance().local_entity_id(),
        expected_source.local_entity_id()
    );
    assert_eq!(source.provenance().projection_path(), Some(path));
    assert_ne!(source.instance_id, old_source.instance_id);
}

#[test]
fn expanded_candidate_source_observer_rejects_before_publication_and_preserves_old_source() {
    let mut fixture = ReplacementFixture::new();
    let prefab_source = nara_asset::AssetRef::path("prefabs/projected.ron").unwrap();
    let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
        prefab_source.clone(),
        crate::tests::prefab_source_key(&prefab_source),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("projected"))]),
    );
    let mut anchor = SceneEntityRecord::new(fixture.target.clone());
    anchor.prefab = Some(PrefabInstance {
        source: prefab_source,
        overrides: None,
    });
    let expansion =
        SceneDocument::from_ordered_roots([anchor]).expand_prefabs(&fixture.registry, &resolver);
    assert!(!expansion.diagnostics.has_errors());
    let candidate = expansion.candidate.unwrap().bind_source(scene_source_key("expanded_candidate_source_observer_rejects_before_publication_and_preserves_old_source/source"));
    assert!(
        candidate
            .provenance()
            .values()
            .any(|source| source.projection_path().is_some())
    );
    fixture.world.init_resource::<LifecycleCanary>();
    fixture.world.add_observer(
        |_: On<Add<SceneEntitySource>>, mut canary: ResMut<LifecycleCanary>| canary.0 += 1,
    );
    fixture.world.flush();
    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let old_source = fixture
        .world
        .get::<SceneEntitySource>(fixture.old_entity)
        .unwrap()
        .clone();

    let report = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &candidate,
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[],
        |overlay| {
            overlay.replace_resource(RunState(2));
        },
    );

    assert!(report.instance.is_none());
    assert!(has_diagnostic(
        &report,
        "scene.runtime-projection-ineligible"
    ));
    assert_eq!(report.retired_entities(), 0);
    assert_eq!(fixture.world.resource::<LifecycleCanary>().0, 0);
    fixture.assert_unchanged(&before_entities, before_stats);
    assert_eq!(
        fixture.world.get::<SceneEntitySource>(fixture.old_entity),
        Some(&old_source)
    );
}

#[test]
fn plain_candidate_with_unresolved_prefab_rejects_spawn_and_replacement_without_mutation() {
    let mut fixture = ReplacementFixture::new();
    let mut anchor = SceneEntityRecord::new(fixture.target.clone());
    anchor.prefab = Some(PrefabInstance {
        source: nara_asset::AssetRef::path("prefabs/unresolved.ron").unwrap(),
        overrides: None,
    });
    let candidate = ExpandedSceneCandidate::plain(
        scene_source_key(
            "plain_candidate_with_unresolved_prefab_rejects_spawn_and_replacement_without_mutation/candidate",
        ),
        std::sync::Arc::new(SceneDocument::from_ordered_roots([anchor])),
    );
    let mut empty = World::new();
    let empty_entities = empty
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let spawn = crate::spawn_scene(
        &mut empty,
        &fixture.registry,
        &candidate,
        &scene_occurrence(
            "plain_candidate_with_unresolved_prefab_rejects_spawn_and_replacement_without_mutation/spawn",
        ),
        None,
    );
    assert!(spawn.instance.is_none());
    assert!(has_diagnostic(&spawn, "scene.prefab-instance-unsupported"));
    assert_eq!(
        empty
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        empty_entities
    );
    assert!(!empty.contains_resource::<WorldIdentityDomain>());

    let before_entities = fixture.entity_ids();
    let before_stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let old_source = fixture
        .world
        .get::<SceneEntitySource>(fixture.old_entity)
        .unwrap()
        .clone();
    let replacement = replace_expanded_scene_with_product(
        &mut fixture.world,
        &fixture.registry,
        &candidate,
        None,
        &fixture.current,
        transaction_limits(1, 1),
        &[],
        |_| panic!("unresolved Prefab must reject before product configuration"),
    );
    assert!(replacement.instance.is_none());
    assert!(has_diagnostic(
        &replacement,
        "scene.prefab-instance-unsupported"
    ));
    assert_eq!(replacement.retired_entities(), 0);
    fixture.assert_unchanged(&before_entities, before_stats);
    assert_eq!(
        fixture.world.get::<SceneEntitySource>(fixture.old_entity),
        Some(&old_source)
    );
}
