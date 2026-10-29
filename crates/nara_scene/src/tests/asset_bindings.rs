use std::{collections::BTreeSet, sync::Arc};

use super::*;
use crate::advanced::{
    SceneAssetBindings, SceneProductResource, SceneProductTransactionLimits,
    replace_expanded_scene_with_product_and_bindings,
};

#[derive(Resource)]
struct RetainedProduct(Arc<()>);

impl SceneProductResource for RetainedProduct {
    type Baseline = Arc<()>;

    fn capture_baseline(&self) -> Self::Baseline {
        Arc::clone(&self.0)
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        Arc::ptr_eq(&self.0, baseline)
    }
}

struct Fixture {
    world: World,
    registry: ComponentRegistry,
    current: SpawnedSceneInstance,
    old_entity: Entity,
}

impl Fixture {
    fn new(registry: ComponentRegistry) -> Self {
        let mut world = World::new();
        world.insert_resource(AssetServer::new());
        let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("old"))]);
        let report = crate::spawn_scene(
            &mut world,
            &registry,
            &crate::ExpandedSceneCandidate::plain(
                scene_source_key("new/source"),
                std::sync::Arc::new((source).clone()),
            ),
            &scene_occurrence("new/report"),
            None,
        );
        assert!(!report.diagnostics.has_errors());
        let old_entity = spawned_entity(&world, &report, &scene_id("old"));
        Self {
            world,
            registry,
            current: report.instance.unwrap(),
            old_entity,
        }
    }

    fn replace(
        &mut self,
        candidate: &ExpandedSceneCandidate,
        database: &ProjectAssetDatabase,
        bindings: SceneAssetBindings,
        configure: impl FnOnce(&mut crate::advanced::SceneProductOverlayWriter<'_>),
    ) -> SceneSpawnReport {
        replace_expanded_scene_with_product_and_bindings(
            &mut self.world,
            &self.registry,
            candidate,
            Some(database),
            &self.current,
            bindings,
            SceneProductTransactionLimits::new(
                ItemLimit::new(4).unwrap(),
                ItemLimit::new(4).unwrap(),
            ),
            &[],
            configure,
        )
    }

    fn assert_current(
        &self,
        server: &AssetServer,
        entities: &BTreeSet<Entity>,
        stats: nara_identity::IdentityDomainStats,
    ) {
        assert_eq!(self.world.resource::<AssetServer>(), server);
        // Allocated entity indices can grow while scratch entities are spawned and despawned.
        // Compare the actual live identities, including resource entities, instead.
        let current_entities = self
            .world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            &current_entities,
            entities,
            "unexpected live entities and components: {:?}",
            current_entities
                .difference(entities)
                .map(|entity| {
                    (
                        *entity,
                        self.world
                            .inspect_entity(*entity)
                            .unwrap()
                            .map(|(_, info)| info.name().to_string())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            self.world.entities().count_spawned() as usize,
            entities.len()
        );
        assert_eq!(self.world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(
            self.current.resolve(&self.world, &scene_id("old")),
            EntityLookup::Resolved(self.old_entity)
        );
    }
}

fn next_source(reference: AssetRef) -> ExpandedSceneCandidate {
    ExpandedSceneCandidate::plain(
        scene_source_key("next_source/candidate"),
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            scene_id("next"),
        )
        .with_component(asset_link_type_id(), asset_link_record(reference))])),
    )
}

#[test]
fn prebound_asset_identity_is_shared_by_decode_apply_and_the_committed_server() {
    let asset_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let dependency_id = stable_id("4e438770-1757-44f0-bbeb-6237cd6768ab");
    let mut database = test_database(asset_id, "textures/new.png");
    database
        .insert(AssetRecord::new(
            dependency_id,
            AssetPath::new("textures/dependency.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();

    for (registry, reference) in [
        (test_asset_registry(), AssetRef::StableId(asset_id)),
        (
            deferred_asset_registry(),
            AssetRef::path("textures/new.png").unwrap(),
        ),
    ] {
        let mut fixture = Fixture::new(registry);
        let before = fixture.world.resource::<AssetServer>().clone();
        let mut bindings = SceneAssetBindings::from_world(&fixture.world);
        let expected = bindings
            .reserve::<TestAsset>(
                &AssetRef::StableId(asset_id),
                &database,
                &AssetSourceKind::Image,
            )
            .unwrap();
        let dependency = bindings
            .reserve::<TestAsset>(
                &AssetRef::StableId(dependency_id),
                &database,
                &AssetSourceKind::Image,
            )
            .unwrap();
        assert_eq!(
            bindings
                .staged_server()
                .get_handle::<TestAsset>(expected.id()),
            Some(expected)
        );
        assert_eq!(
            bindings.staged_server().stable_id(dependency.id()),
            Some(dependency_id)
        );
        assert_eq!(fixture.world.resource::<AssetServer>(), &before);
        bindings.validate(&fixture.world).unwrap();

        let report = fixture.replace(&next_source(reference), &database, bindings, |_| {});
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let entity = spawned_entity(&fixture.world, &report, &scene_id("next"));
        assert_eq!(
            fixture.world.get::<TestAssetLink>(entity).unwrap().handle,
            expected
        );
        let server = fixture.world.resource::<AssetServer>();
        assert_eq!(
            server.get_handle::<TestAsset>(expected.id()),
            Some(expected)
        );
        assert_eq!(server.path(expected.id()), Some("textures/new.png"));
        assert_eq!(server.stable_id(expected.id()), Some(asset_id));
        assert_eq!(
            server.get_handle::<TestAsset>(dependency.id()),
            Some(dependency)
        );
        assert!(fixture.world.get_entity(fixture.old_entity).is_err());
        assert_ne!(
            report.instance.unwrap().instance_id(),
            fixture.current.instance_id()
        );
    }
}

#[test]
fn untouched_bindings_from_another_world_reject_before_overlay_or_scene_mutation() {
    let foreign = World::new();
    let bindings = SceneAssetBindings::from_world(&foreign);
    let mut fixture = Fixture::new(test_asset_registry());
    assert!(matches!(
        bindings.validate(&fixture.world),
        Err(ComponentCodecError::WrongWorld)
    ));
    let server = fixture.world.resource::<AssetServer>().clone();
    let entities = fixture
        .world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<BTreeSet<_>>();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let candidate = ExpandedSceneCandidate::plain(
        scene_source_key(
            "untouched_bindings_from_another_world_reject_before_overlay_or_scene_mutation/candidate",
        ),
        Arc::new(SceneDocument::default()),
    );
    let report = fixture.replace(
        &candidate,
        &ProjectAssetDatabase::default(),
        bindings,
        |_| {
            panic!("foreign bindings must reject before product configuration");
        },
    );
    assert!(report.instance.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.asset-bindings-invalid")
    );
    fixture.assert_current(&server, &entities, stats);
}

#[test]
fn captured_asset_server_changes_reject_even_when_bindings_are_untouched() {
    let asset_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let database = test_database(asset_id, "textures/new.png");
    for prebind in [false, true] {
        let mut fixture = Fixture::new(test_asset_registry());
        let mut bindings = SceneAssetBindings::from_world(&fixture.world);
        if prebind {
            bindings
                .reserve::<TestAsset>(
                    &AssetRef::StableId(asset_id),
                    &database,
                    &AssetSourceKind::Image,
                )
                .unwrap();
        }
        fixture
            .world
            .resource_mut::<AssetServer>()
            .reserve::<TestAsset>("textures/arrived-later.png")
            .unwrap();
        let server = fixture.world.resource::<AssetServer>().clone();
        let entities = fixture
            .world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<BTreeSet<_>>();
        let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
        assert!(matches!(
            bindings.validate(&fixture.world),
            Err(ComponentCodecError::AssetServerChanged)
        ));
        let report = fixture.replace(
            &next_source(AssetRef::StableId(asset_id)),
            &database,
            bindings,
            |_| {
                panic!("stale bindings must reject before product configuration");
            },
        );
        assert!(report.instance.is_none());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.asset-bindings-invalid")
        );
        fixture.assert_current(&server, &entities, stats);
    }
}

#[test]
fn apply_rejection_discards_prebound_catalogue_scratch_entities_and_owned_overlay_values() {
    let asset_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let database = test_database(asset_id, "textures/new.png");
    let mut fixture = Fixture::new(failing_apply_registry());
    let original = Arc::new(());
    fixture
        .world
        .insert_resource(RetainedProduct(Arc::clone(&original)));
    let replacement = Arc::new(());
    let released = Arc::downgrade(&replacement);
    let mut bindings = SceneAssetBindings::from_world(&fixture.world);
    let planned = bindings
        .reserve::<TestAsset>(
            &AssetRef::StableId(asset_id),
            &database,
            &AssetSourceKind::Image,
        )
        .unwrap();
    let candidate = ExpandedSceneCandidate::plain(
        scene_source_key(
            "apply_rejection_discards_prebound_catalogue_scratch_entities_and_owned_overlay_values/candidate",
        ),
        Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(scene_id("resolves")).with_component(
                apply_resolves_asset_type_id(),
                asset_link_record(AssetRef::path("textures/new.png").unwrap()),
            ),
            SceneEntityRecord::new(scene_id("fails"))
                .with_component(apply_fails_type_id(), apply_fails_record(None)),
        ])),
    );
    let server = fixture.world.resource::<AssetServer>().clone();
    let entities = fixture
        .world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<BTreeSet<_>>();
    let stats = fixture.world.resource::<WorldIdentityDomain>().stats();
    let mut configured = false;
    let report = fixture.replace(&candidate, &database, bindings, |writer| {
        configured = true;
        writer.replace_resource(RetainedProduct(replacement));
    });
    assert!(report.instance.is_none());
    assert!(configured, "{:?}", report.diagnostics);
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.component-apply-failed"
                && diagnostic_has_text_field(diagnostic, "entity-id", "fails")
                && diagnostic_has_text_field(
                    diagnostic,
                    "component-id",
                    apply_fails_type_id().as_str(),
                )
        }),
        "{:?}",
        report.diagnostics
    );
    assert!(released.upgrade().is_none());
    assert!(Arc::ptr_eq(
        &fixture.world.resource::<RetainedProduct>().0,
        &original
    ));
    assert!(
        fixture
            .world
            .resource::<AssetServer>()
            .get_handle::<TestAsset>(planned.id())
            .is_none()
    );
    fixture.assert_current(&server, &entities, stats);
}

#[test]
fn reservation_respects_source_kind_type_claims_and_dropping_bindings_publishes_nothing() {
    struct OtherAsset;
    let asset_id = stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f");
    let database = test_database(asset_id, "textures/new.png");
    let fixture = Fixture::new(test_asset_registry());
    let before = fixture.world.resource::<AssetServer>().clone();
    let mut bindings = SceneAssetBindings::from_world(&fixture.world);
    let reference = AssetRef::StableId(asset_id);
    assert!(matches!(
        bindings.reserve::<TestAsset>(&reference, &database, &AssetSourceKind::Prefab),
        Err(AssetRefError::UnexpectedSourceKind { .. })
    ));
    let planned = bindings
        .reserve::<TestAsset>(&reference, &database, &AssetSourceKind::Image)
        .unwrap();
    assert!(matches!(
        bindings.reserve::<OtherAsset>(&reference, &database, &AssetSourceKind::Image),
        Err(AssetRefError::Asset(
            nara_asset::AssetError::AssetTypeMismatch { .. }
        ))
    ));
    assert_eq!(fixture.world.resource::<AssetServer>(), &before);
    drop(bindings);
    assert_eq!(fixture.world.resource::<AssetServer>(), &before);
    assert!(
        fixture
            .world
            .resource::<AssetServer>()
            .get_handle::<TestAsset>(planned.id())
            .is_none()
    );
    assert_eq!(
        fixture.current.resolve(&fixture.world, &scene_id("old")),
        EntityLookup::Resolved(fixture.old_entity)
    );
}
