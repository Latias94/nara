use super::*;
use crate::hierarchy::Children;
use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_ecs::identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentTypeId, ComponentValueKind,
};

#[derive(Component)]
struct MarkerLink(EntityReference);

fn link_type() -> ComponentTypeId {
    ComponentTypeId::new("test.MarkerLink")
}

fn marker_registry() -> ComponentRegistrySnapshot {
    let mut builder = nara_reflect::ComponentRegistryBuilder::new();
    builder
        .register_persistent_component_with_codec::<MarkerLink, _, _>(
            ComponentSchema::new(link_type(), "Marker link", ComponentSchemaVersion::ONE)
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("target"),
                    "Target",
                    ComponentFieldPath::from_fields(["target"]),
                    ComponentValueKind::EntityRef,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_capability(ComponentCapability::EntityRef)]),
            |value| Ok(MarkerLink(value.field_entity_reference("target")?.clone())),
            |link| {
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(link.0.clone()),
                )]))
            },
        )
        .unwrap();
    builder.freeze().unwrap()
}

fn marker_candidate(registry: &ComponentRegistrySnapshot) -> CompiledSceneCandidate {
    let source = AssetRef::path("prefabs/marker.prefab.ron").unwrap();
    let key = crate::tests::prefab_source_key(&source);
    let root = scene_id("authored-root");
    let child = scene_id("authored-child");
    let prefab = PrefabDocument::new(
        [
            SceneEntityPlacement::root(root.clone()),
            child_placement(child.clone(), root.clone()),
        ],
        [
            SceneEntityRecord::new(root.clone()).with_component(
                link_type(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: child,
                        }),
                    )]),
                ),
            ),
            SceneEntityRecord::new(scene_id("authored-child")),
        ],
    );
    let resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), key.clone(), prefab);
    let mut anchor = SceneEntityRecord::new(scene_id("marker"));
    anchor.prefab = Some(PrefabInstance::root(
        source,
        key,
        PrefabEntityTarget::source(root),
    ));
    let compiled = SceneDocument::from_ordered_roots([anchor]).compile(registry, &resolver);
    assert!(
        !compiled.diagnostics.has_errors(),
        "{:?}",
        compiled.diagnostics
    );
    compiled
        .candidate
        .unwrap()
        .bind_source(scene_source_key("marker.wrapper"))
}

struct MultiActivationFixture {
    base: ReplacementFixture,
    marker: CompiledSceneCandidate,
    markers: [SpawnedSceneInstance; 2],
}

#[test]
fn replacement_preserves_the_primary_root_slot_after_retiring_a_preceding_activation() {
    let mut fixture = MultiActivationFixture::new();
    let marker = fixture.entity(0, &scene_id("marker"));
    let survivor = spawn_identity_entity(&mut fixture.base.world)
        .unwrap()
        .entity();
    HierarchyWriter::new(&mut fixture.base.world)
        .apply(HierarchyOperation::Place {
            entity: survivor,
            placement: HierarchyPlacement::first(HierarchyPlacementOwner::Root),
        })
        .unwrap();
    let placement = HierarchyPlacement::current(&fixture.base.world, marker).unwrap();
    HierarchyWriter::new(&mut fixture.base.world)
        .apply(HierarchyOperation::Move {
            entity: marker,
            from: placement,
            to: HierarchyPlacement::before(HierarchyPlacementOwner::Root, fixture.base.old_entity),
        })
        .unwrap();
    let second = fixture.entity(1, &scene_id("marker"));
    let selections = [SceneProductRetirement::Instance(&fixture.markers[0])];
    let report = fixture.base.replace(&selections, 2);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let replacement = spawned_entity(&fixture.base.world, &report, &fixture.base.target);
    assert_eq!(
        fixture.base.world.resource::<HierarchyRoots>().as_slice(),
        [survivor, replacement, second]
    );
}

impl MultiActivationFixture {
    fn new() -> Self {
        let registry = marker_registry();
        let target = scene_id("level");
        let replacement = compile_scene(
            &SceneDocument::from_ordered_roots([SceneEntityRecord::new(target.clone())]),
            &registry,
            "multi-activation.level",
        );
        let marker = marker_candidate(&registry);
        let mut world = World::new();
        world.insert_resource(RunState::new(1));
        let initial = spawn_scene(
            &mut world,
            &registry,
            &replacement,
            &scene_occurrence("level"),
            None,
        );
        let current = spawned_instance(&initial).clone();
        let old_entity = spawned_entity(&world, &initial, &target);
        let markers = ["marker/left", "marker/right"].map(|slot| {
            let report = spawn_scene(
                &mut world,
                &registry,
                &marker,
                &scene_occurrence(slot),
                None,
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let receipt = report.instance.unwrap();
            assert_eq!(receipt.len(), 2);
            receipt
        });
        Self {
            base: ReplacementFixture {
                world,
                registry,
                replacement,
                current,
                target,
                old_entity,
            },
            marker,
            markers,
        }
    }

    fn entity(&self, marker: usize, id: &SceneEntityId) -> Entity {
        match self.markers[marker].resolve(&self.base.world, id) {
            EntityLookup::Resolved(entity) => entity,
            lookup => panic!("marker member must resolve: {lookup:?}"),
        }
    }

    fn child(&self) -> SceneEntityId {
        self.marker
            .canonical_provenance()
            .iter()
            .find_map(|(id, source)| {
                (source.local_entity_id() == &scene_id("authored-child")).then(|| id.clone())
            })
            .unwrap()
    }
}

impl ReplacementFixture {
    fn replace(
        &mut self,
        selections: &[SceneProductRetirement<'_>],
        limit: usize,
    ) -> SceneSpawnReport {
        replace_expanded_scene_with_product(
            &mut self.world,
            &self.registry,
            &self.replacement,
            None,
            &self.current,
            transaction_limits(1, limit),
            selections,
            |overlay| {
                overlay.replace_resource(RunState::new(2));
            },
        )
    }
}

#[test]
fn complete_markers_and_runtime_entity_retire_in_one_original_product_replacement() {
    let mut fixture = MultiActivationFixture::new();
    let root = scene_id("marker");
    let child = fixture.child();
    let mut old_members = Vec::new();
    for receipt in &fixture.markers {
        for id in receipt.entity_ids() {
            let EntityLookup::Resolved(entity) = receipt.resolve(&fixture.base.world, id) else {
                panic!("active marker member");
            };
            let source = fixture.base.world.get::<SceneEntitySource>(entity).unwrap();
            assert_eq!(source.containing_source(), fixture.marker.source_key());
            assert_eq!(
                source.provenance(),
                &fixture.marker.canonical_provenance()[id]
            );
            old_members.push(entity);
        }
    }
    for index in 0..2 {
        let root_entity = fixture.entity(index, &root);
        let child_entity = fixture.entity(index, &child);
        let link = &fixture.base.world.get::<MarkerLink>(root_entity).unwrap().0;
        assert_eq!(
            &fixture.base.world.get::<Children>(root_entity).unwrap()[..],
            &[child_entity]
        );
        assert_eq!(
            fixture
                .base
                .world
                .get::<ChildOf>(child_entity)
                .map(ChildOf::parent),
            Some(root_entity)
        );
        assert_eq!(
            fixture
                .base
                .world
                .resource::<WorldIdentityDomain>()
                .resolve_entity_reference(
                    &fixture.base.world,
                    link,
                    Some(fixture.markers[index].instance_id()),
                ),
            EntityLookup::Resolved(child_entity)
        );
    }
    assert_ne!(fixture.entity(0, &child), fixture.entity(1, &child));
    let token = adopt_token(&mut fixture.base.world, old_members[0]);
    let persistent = register_persistent_axis(
        &mut fixture.base.world,
        token,
        persistent_reference("31313131-3131-4131-8131-313131313131"),
    );
    let runtime = spawn_retirement_token(&mut fixture.base.world);
    let selections = [
        SceneProductRetirement::Instance(&fixture.markers[0]),
        SceneProductRetirement::Instance(&fixture.markers[1]),
        SceneProductRetirement::Entity(runtime),
    ];
    let report = fixture.base.replace(&selections, 5);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 6);
    assert_eq!(fixture.base.world.resource::<RunState>().0, 2);
    assert_eq!(
        fixture
            .base
            .world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        1
    );
    assert_eq!(
        fixture
            .base
            .world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_persistent_entities,
        0
    );
    assert!(!matches!(
        resolve_in_world(&fixture.base.world, &persistent),
        EntityLookup::Resolved(_)
    ));
    for entity in old_members
        .into_iter()
        .chain([runtime.entity(), fixture.base.old_entity])
    {
        assert!(fixture.base.world.get_entity(entity).is_err());
    }
    for receipt in &fixture.markers {
        for id in receipt.entity_ids() {
            assert!(matches!(
                receipt.resolve(&fixture.base.world, id),
                EntityLookup::Tombstoned(_)
            ));
        }
        assert!(
            fixture
                .base
                .world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(&fixture.base.world, receipt.instance_id())
                .is_err()
        );
    }
    let level = spawned_entity(&fixture.base.world, &report, &fixture.base.target);
    assert_eq!(
        fixture.base.world.resource::<HierarchyRoots>().as_slice(),
        &[level]
    );
    for slot in ["marker/left", "marker/right"] {
        let restored = spawn_scene(
            &mut fixture.base.world,
            &fixture.base.registry,
            &fixture.marker,
            &scene_occurrence(slot),
            None,
        );
        assert!(
            !restored.diagnostics.has_errors(),
            "{:?}",
            restored.diagnostics
        );
    }
}

#[test]
fn duplicate_foreign_stale_and_overlapping_selections_preserve_original_product() {
    let mut fixture = MultiActivationFixture::new();
    let overlap_entity = fixture.entity(0, &scene_id("marker"));
    let overlap = adopt_token(&mut fixture.base.world, overlap_entity);
    let mut foreign_world = World::new();
    let foreign = spawn_scene(
        &mut foreign_world,
        &fixture.base.registry,
        &fixture.marker,
        &scene_occurrence("foreign"),
        None,
    )
    .instance
    .unwrap();
    retire_and_despawn_scene_instance(&mut fixture.base.world, &fixture.markers[1]).unwrap();
    let replacement_marker = spawn_scene(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.marker,
        &scene_occurrence("marker/right"),
        None,
    )
    .instance
    .unwrap();
    let stale = std::mem::replace(&mut fixture.markers[1], replacement_marker);
    let duplicate = SceneProductRetirement::Instance(&fixture.markers[0]);
    let primary = SceneProductRetirement::Instance(&fixture.base.current);
    for selections in [
        vec![duplicate, duplicate],
        vec![primary],
        vec![SceneProductRetirement::Instance(&foreign)],
        vec![SceneProductRetirement::Instance(&stale)],
        vec![duplicate, SceneProductRetirement::Entity(overlap)],
    ] {
        let entities = fixture.base.entity_ids();
        let stats = fixture.base.world.resource::<WorldIdentityDomain>().stats();
        let report = replace_expanded_scene_with_product(
            &mut fixture.base.world,
            &fixture.base.registry,
            &fixture.base.replacement,
            None,
            &fixture.base.current,
            transaction_limits(1, 6),
            &selections,
            |overlay| {
                overlay.replace_resource(RunState::new(2));
            },
        );
        assert!(report.instance.is_none(), "{:?}", report.diagnostics);
        assert_eq!(report.retired_entities(), 0);
        fixture.base.assert_unchanged(&entities, stats);
        for (index, slot) in ["marker/left", "marker/right"].into_iter().enumerate() {
            let source = fixture
                .base
                .world
                .get::<SceneEntitySource>(fixture.entity(index, &scene_id("marker")))
                .unwrap();
            assert_eq!(source.occurrence(), &scene_occurrence(slot));
            assert_eq!(source.instance_id, fixture.markers[index].instance_id());
        }
    }
}

#[test]
fn original_membership_and_empty_activation_units_bound_retained_selections() {
    let mut fixture = MultiActivationFixture::new();
    let child = fixture.entity(0, &fixture.child());
    retire_and_despawn_scene_entity(&mut fixture.base.world, child).unwrap();
    let original = SceneProductRetirement::Instance(&fixture.markers[0]);
    let active = fixture
        .base
        .world
        .resource::<WorldIdentityDomain>()
        .active_scene_instance(&fixture.base.world, fixture.markers[0].instance_id())
        .unwrap();
    assert_eq!(active.len(), 1);
    let entities = fixture.base.entity_ids();
    let stats = fixture.base.world.resource::<WorldIdentityDomain>().stats();
    for selection in [original, SceneProductRetirement::Instance(&active)] {
        let report = fixture.base.replace(&[selection], 1);
        assert!(has_diagnostic(
            &report,
            "scene.product-retirement-limit-exceeded"
        ));
        fixture.base.assert_unchanged(&entities, stats);
    }
    let empty_candidate = compile_scene(
        &SceneDocument::default(),
        &fixture.base.registry,
        "empty.marker",
    );
    let empty_instances = ["empty/left", "empty/right"].map(|slot| {
        spawn_scene(
            &mut fixture.base.world,
            &fixture.base.registry,
            &empty_candidate,
            &scene_occurrence(slot),
            None,
        )
        .instance
        .unwrap()
    });
    let empties = empty_instances
        .each_ref()
        .map(SceneProductRetirement::Instance);
    let entities = fixture.base.entity_ids();
    let stats = fixture.base.world.resource::<WorldIdentityDomain>().stats();
    let report = fixture.base.replace(&empties, 1);
    assert!(has_diagnostic(
        &report,
        "scene.product-retirement-limit-exceeded"
    ));
    fixture.base.assert_unchanged(&entities, stats);
    let report = fixture.base.replace(&empties, 2);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 1);
    for slot in ["empty/left", "empty/right"] {
        assert!(
            spawn_scene(
                &mut fixture.base.world,
                &fixture.base.registry,
                &empty_candidate,
                &scene_occurrence(slot),
                None
            )
            .instance
            .is_some()
        );
    }
}

#[test]
fn aggregate_replacement_budgets_only_occurrences_retained_after_commit() {
    let mut fixture = MultiActivationFixture::new();
    let large_slot = SceneOccurrenceKey::new("test", "x".repeat(64 * 1024)).unwrap();
    let large = spawn_scene(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.marker,
        &large_slot,
        None,
    )
    .instance
    .unwrap();
    let limits = SceneOccurrenceLimits::default()
        .with_active_occurrences(ItemLimit::ONE)
        .with_retained_bytes(ByteLimit::new(16 * 1024).unwrap());
    fixture.base.world.insert_resource(limits);
    let selections =
        [&fixture.markers[0], &fixture.markers[1], &large].map(SceneProductRetirement::Instance);
    let refused = fixture.base.replace(&selections[..2], 4);
    assert!(has_diagnostic(&refused, "scene.occurrence-budget-exceeded"));
    let report = fixture.base.replace(&selections, 6);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    assert_eq!(report.retired_entities(), 7);
    let domain = fixture.base.world.resource::<WorldIdentityDomain>();
    assert_eq!(domain.stats().active_scene_instances, 1);
    assert_eq!(fixture.base.world.resource::<RunState>().0, 2);
}

#[test]
fn staged_replacement_promotes_surviving_children_added_after_staging() {
    let mut fixture = MultiActivationFixture::new();
    let extra = spawn_scene(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.base.replacement,
        &scene_occurrence("extra"),
        None,
    );
    let retired = spawned_entity(&fixture.base.world, &extra, &fixture.base.target);
    let placement = HierarchyPlacement::current(&fixture.base.world, retired).unwrap();
    HierarchyWriter::new(&mut fixture.base.world)
        .apply(HierarchyOperation::Remove {
            entity: retired,
            from: placement,
        })
        .unwrap();
    let survivor = spawn_identity_entity(&mut fixture.base.world)
        .unwrap()
        .entity();
    let reservations = SceneAssetReservations::from_world(&fixture.base.world);
    let extra_instance = extra.instance.unwrap();
    let staged = stage_expanded_scene_replacement_with_claims(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.base.replacement,
        None,
        &fixture.base.current,
        reservations,
        &SceneNativeRetirementClaims::default(),
        transaction_limits(1, 1),
        &[SceneProductRetirement::Instance(&extra_instance)],
        |overlay| {
            overlay.replace_resource(RunState::new(2));
        },
    )
    .unwrap_or_else(|report| panic!("{:?}", report.diagnostics));
    let mut hierarchy = HierarchyWriter::new(&mut fixture.base.world);
    hierarchy
        .apply(HierarchyOperation::Place {
            entity: retired,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        })
        .unwrap();
    hierarchy
        .apply(HierarchyOperation::Place {
            entity: survivor,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                parent: retired,
            }),
        })
        .unwrap();
    let report = staged
        .commit(&mut fixture.base.world)
        .unwrap_or_else(|(_, report)| panic!("{:?}", report.diagnostics));
    assert_eq!(report.retired_entities(), 2);
    assert!(!fixture.base.world.entities().contains(retired));
    assert!(fixture.base.world.get::<ChildOf>(survivor).is_none());
    let replacement = spawned_entity(&fixture.base.world, &report, &fixture.base.target);
    let first = fixture.entity(0, &scene_id("marker"));
    let second = fixture.entity(1, &scene_id("marker"));
    assert_eq!(
        fixture.base.world.resource::<HierarchyRoots>().as_slice(),
        [replacement, first, second, survivor]
    );
}

#[test]
fn original_receipts_end_partially_and_fully_retired_activations() {
    for retire_all in [false, true] {
        let mut fixture = MultiActivationFixture::new();
        let child = fixture.entity(0, &fixture.child());
        retire_and_despawn_scene_entity(&mut fixture.base.world, child).unwrap();
        if retire_all {
            let root = fixture.entity(0, &scene_id("marker"));
            retire_and_despawn_scene_entity(&mut fixture.base.world, root).unwrap();
        }
        let selection = SceneProductRetirement::Instance(&fixture.markers[0]);
        let report = fixture.base.replace(&[selection], 2);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(report.retired_entities(), if retire_all { 1 } else { 2 });
        assert!(
            fixture
                .base
                .world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(&fixture.base.world, fixture.markers[0].instance_id())
                .is_err()
        );
        assert!(
            spawn_scene(
                &mut fixture.base.world,
                &fixture.base.registry,
                &fixture.marker,
                &scene_occurrence("marker/left"),
                None
            )
            .instance
            .is_some()
        );
        assert!(
            fixture
                .base
                .world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(&fixture.base.world, fixture.markers[1].instance_id())
                .is_ok()
        );
    }
}

#[test]
fn retained_multi_activation_replacement_rejects_resource_aba_without_retirement() {
    let mut fixture = MultiActivationFixture::new();
    let runtime = spawn_retirement_token(&mut fixture.base.world);
    let selections = [
        SceneProductRetirement::Instance(&fixture.markers[0]),
        SceneProductRetirement::Instance(&fixture.markers[1]),
        SceneProductRetirement::Entity(runtime),
    ];
    let original_entities = fixture.base.entity_ids();
    let reservations = SceneAssetReservations::from_world(&fixture.base.world);
    let staged = stage_expanded_scene_replacement_with_claims(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.base.replacement,
        None,
        &fixture.base.current,
        reservations,
        &SceneNativeRetirementClaims::default(),
        transaction_limits(1, 5),
        &selections,
        |overlay| {
            overlay.replace_resource(RunState::new(2));
        },
    )
    .unwrap_or_else(|report| panic!("{:?}", report.diagnostics));
    let entities = fixture.base.entity_ids();
    let stats = fixture.base.world.resource::<WorldIdentityDomain>().stats();
    fixture.base.world.resource_mut::<RunState>().set(1);
    let (staged, report) = staged.commit(&mut fixture.base.world).unwrap_err();
    assert!(has_diagnostic(&report, "scene.product-resource-changed"));
    fixture.base.assert_unchanged(&entities, stats);
    assert!(staged.discard(&mut fixture.base.world).is_ok());
    fixture.base.assert_unchanged(&original_entities, stats);
    for index in 0..2 {
        let child = fixture.entity(index, &fixture.child());
        assert!(fixture.base.world.get::<ChildOf>(child).is_some());
    }
}

#[test]
fn retained_replacement_rejects_ended_additional_receipt_after_slot_reuse() {
    let mut fixture = MultiActivationFixture::new();
    let selections = [SceneProductRetirement::Instance(&fixture.markers[0])];
    let reservations = SceneAssetReservations::from_world(&fixture.base.world);
    let staged = stage_expanded_scene_replacement_with_claims(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.base.replacement,
        None,
        &fixture.base.current,
        reservations,
        &SceneNativeRetirementClaims::default(),
        transaction_limits(1, 2),
        &selections,
        |overlay| {
            overlay.replace_resource(RunState::new(2));
        },
    )
    .unwrap_or_else(|report| panic!("{:?}", report.diagnostics));
    retire_and_despawn_scene_instance(&mut fixture.base.world, &fixture.markers[0]).unwrap();
    fixture.markers[0] = spawn_scene(
        &mut fixture.base.world,
        &fixture.base.registry,
        &fixture.marker,
        &scene_occurrence("marker/left"),
        None,
    )
    .instance
    .unwrap();
    let entities = fixture.base.entity_ids();
    let stats = fixture.base.world.resource::<WorldIdentityDomain>().stats();
    let (staged, report) = staged.commit(&mut fixture.base.world).unwrap_err();
    assert!(has_diagnostic(&report, "scene.prepared-baseline-changed"));
    fixture.base.assert_unchanged(&entities, stats);
    assert!(
        fixture
            .base
            .world
            .get::<MarkerLink>(fixture.entity(0, &scene_id("marker")))
            .is_some()
    );
    assert!(staged.discard(&mut fixture.base.world).is_ok());
}
