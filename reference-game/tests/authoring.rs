use nara::{
    identity::EntityLookup,
    prelude::{Component, ComponentRegistry, ComponentTypeId, PersistentComponent, World},
    reflect::{
        ComponentCatalogFileLimits, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentValue,
        PersistentComponentProvider,
    },
    scene::{
        SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchDocumentCandidate,
        ScenePatchOperation,
    },
};
use nara_reference_game::{
    Enemy, Player, Projectile, REFERENCE_GAME_SCHEMA_PROVIDER, ReferenceGamePlugin, RuntimeOnlyTag,
    WaveSpawn, Weapon,
};

const LINEAGE_PROBE_ID: &str = "nara.test.LineageProbe";
const PLAYER_ID: &str = "reference_game.Player";

#[derive(Component, PersistentComponent, Debug, PartialEq)]
#[nara(
    id = "nara.test.LineageProbe",
    version = 2,
    alias = "Current probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    tombstone = "removed"
)]
struct LineageProbe {
    #[nara(id = "value", alias = "Current value")]
    current_value: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.LineageProbe",
    version = 2,
    alias = "Current probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct MissingTombstoneProbe {
    #[nara(id = "value", alias = "Current value")]
    current_value: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.LineageProbe",
    version = 3,
    alias = "Reactivated probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct ReactivatedProbe {
    #[nara(id = "value", alias = "Current value")]
    current_value: i64,
    #[nara(id = "removed", alias = "Reactivated value")]
    reactivated_value: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.LineageProbe",
    version = 3,
    alias = "Dropped tombstone probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct DroppedTombstoneProbe {
    #[nara(id = "value", alias = "Current value")]
    current_value: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "reference_game.Player",
    version = 2,
    alias = "Player",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct PlayerWithoutVelocity {
    #[nara(id = "position", alias = "Position")]
    position: nara::prelude::Vec2,
    #[nara(id = "hit-points", alias = "Hit points")]
    hit_points: i64,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "reference_game.Player",
    version = 1,
    alias = "Player",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct PlayerHealthKindChanged {
    #[nara(id = "position", alias = "Position")]
    position: nara::prelude::Vec2,
    #[nara(id = "velocity", alias = "Velocity")]
    velocity: nara::prelude::Vec2,
    #[nara(id = "hit-points", alias = "Hit points")]
    hit_points: u64,
}

#[test]
fn five_game_components_register_and_round_trip_through_public_api() {
    let mut app = nara::prelude::App::new();
    app.add_plugins(nara::prelude::MinimalPlugins).unwrap();
    app.add_plugin(ReferenceGamePlugin).unwrap();
    let app = app.seal().unwrap();

    let registry = app.world().resource::<ComponentRegistry>();
    for id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
        "reference_game.Projectile",
    ] {
        assert!(registry.schema(&ComponentTypeId::new(id)).is_some());
    }
    assert!(
        registry
            .schema(&ComponentTypeId::new("reference_game.RuntimeOnlyTag"))
            .is_none()
    );

    let mut runtime_only_world = World::new();
    let runtime_only_entity = runtime_only_world.spawn(RuntimeOnlyTag).id();
    assert!(
        runtime_only_world
            .get::<RuntimeOnlyTag>(runtime_only_entity)
            .is_some()
    );

    assert_round_trip(Player::fixture(), "reference_game.Player", registry);
    assert_round_trip(Enemy::fixture(), "reference_game.Enemy", registry);
    assert_round_trip(WaveSpawn::fixture(), "reference_game.WaveSpawn", registry);
    assert_round_trip(Weapon::fixture(), "reference_game.Weapon", registry);
    assert_round_trip(Projectile::fixture(), "reference_game.Projectile", registry);
}

fn migrate_lineage_v1_to_v2(value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    let ComponentValue::Map(mut fields) = value else {
        return Err(ComponentCodecError::invalid_field("LineageProbe", "map"));
    };
    fields.remove("removed");
    Ok(ComponentValue::Map(fields))
}

fn register_lineage_successor<T>(
    predecessor: ComponentSchemaCatalog,
    from_version: ComponentSchemaVersion,
    to_version: ComponentSchemaVersion,
) -> ComponentRegistry
where
    T: PersistentComponentProvider,
{
    let id = ComponentTypeId::new(LINEAGE_PROBE_ID);
    let mut registry = ComponentRegistry::successor_of(predecessor).unwrap();
    registry.register_persistent_component::<T>().unwrap();
    registry
        .register_component_migration(&id, from_version, to_version, Ok)
        .unwrap();
    registry
}

fn frozen_lineage_successor(predecessor: ComponentSchemaCatalog) -> ComponentRegistry {
    let id = ComponentTypeId::new(LINEAGE_PROBE_ID);
    let mut registry = ComponentRegistry::successor_of(predecessor).unwrap();
    registry
        .register_persistent_component::<LineageProbe>()
        .unwrap();
    registry
        .register_component_migration(
            &id,
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).unwrap(),
            migrate_lineage_v1_to_v2,
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

#[test]
fn generated_catalog_preserves_stable_ids_across_rename_and_deletion() {
    let v1_fixture = include_str!("../../tests/fixtures/schema-catalog/lineage-probe-v1.json");
    let v2_fixture = include_str!("../../tests/fixtures/schema-catalog/lineage-probe-v2.json");
    let predecessor = ComponentSchemaCatalog::from_json_bytes(v1_fixture.as_bytes()).unwrap();
    assert_eq!(
        format!("{}\n", predecessor.to_json_string().unwrap()),
        v1_fixture
    );

    let expected = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v2_fixture.as_bytes(),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();
    let registry = frozen_lineage_successor(predecessor.clone());
    assert_eq!(registry.catalog().unwrap(), &expected);
    assert_eq!(
        format!(
            "{}\n",
            registry
                .catalog()
                .unwrap()
                .to_json_string_with_predecessor(Some(&predecessor))
                .unwrap()
        ),
        v2_fixture
    );

    let id = ComponentTypeId::new(LINEAGE_PROBE_ID);
    let previous_schema = predecessor
        .components()
        .iter()
        .find(|schema| schema.id() == &id)
        .unwrap();
    let current_schema = registry.schema(&id).unwrap();
    assert_eq!(previous_schema.aliases(), &["Previous probe"]);
    assert_eq!(current_schema.aliases(), &["Current probe"]);
    assert_eq!(
        previous_schema
            .fields()
            .iter()
            .find(|field| field.id() == &ComponentFieldId::new("value"))
            .unwrap()
            .path(),
        &ComponentFieldPath::from_fields(["value"])
    );
    assert_eq!(
        registry
            .resolve_field(&id, &ComponentFieldId::new("value"))
            .unwrap()
            .path(),
        &ComponentFieldPath::from_fields(["value"])
    );
    assert_eq!(
        current_schema.field_tombstones(),
        &[ComponentFieldId::new("removed")]
    );

    let migrated = registry
        .migrate_component_value(
            &id,
            ComponentSchemaVersion::ONE,
            &ComponentValue::map([
                ("value", ComponentValue::I64(42)),
                ("removed", ComponentValue::I64(7)),
            ]),
        )
        .unwrap();
    assert_eq!(migrated.version, ComponentSchemaVersion::new(2).unwrap());
    assert_eq!(
        migrated.value,
        ComponentValue::map([("value", ComponentValue::I64(42))])
    );
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    registry
        .preflight_component(&id, &migrated.value)
        .unwrap()
        .unwrap()
        .apply(&mut world, entity)
        .unwrap();
    assert_eq!(
        world.get::<LineageProbe>(entity),
        Some(&LineageProbe { current_value: 42 })
    );
}

#[test]
fn generated_catalog_rejects_missing_reused_and_dropped_tombstones() {
    let v1_fixture = include_str!("../../tests/fixtures/schema-catalog/lineage-probe-v1.json");
    let v2_fixture = include_str!("../../tests/fixtures/schema-catalog/lineage-probe-v2.json");
    let predecessor = ComponentSchemaCatalog::from_json_bytes(v1_fixture.as_bytes()).unwrap();
    let v2 = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v2_fixture.as_bytes(),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();

    let mut missing = register_lineage_successor::<MissingTombstoneProbe>(
        predecessor,
        ComponentSchemaVersion::ONE,
        ComponentSchemaVersion::new(2).unwrap(),
    );
    let missing_before = missing.catalog_candidate().clone();
    assert!(matches!(
        missing.freeze(),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));
    assert_eq!(missing.catalog_candidate(), &missing_before);
    assert!(!missing.is_frozen());

    let mut reactivated = register_lineage_successor::<ReactivatedProbe>(
        v2.clone(),
        ComponentSchemaVersion::new(2).unwrap(),
        ComponentSchemaVersion::new(3).unwrap(),
    );
    assert!(matches!(
        reactivated.freeze(),
        Err(ComponentRegistryError::ReactivatedFieldId { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));

    let mut dropped = register_lineage_successor::<DroppedTombstoneProbe>(
        v2,
        ComponentSchemaVersion::new(2).unwrap(),
        ComponentSchemaVersion::new(3).unwrap(),
    );
    assert!(matches!(
        dropped.freeze(),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));
}

#[test]
fn reference_game_catalog_preserves_v1_and_matches_the_v2_successor() {
    assert_eq!(REFERENCE_GAME_SCHEMA_PROVIDER.binding().version(), 2);
    let predecessor = reference_game_predecessor_catalog();
    let v1_fixture = include_str!("../schema/component-schema-v1.json");
    let v2_fixture = include_str!("../schema/component-schema-v2.json");
    assert_eq!(
        format!("{}\n", predecessor.to_json_string().unwrap()),
        v1_fixture
    );
    let expected = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v2_fixture.as_bytes(),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();

    let mut fresh = ComponentRegistry::new();
    register_reference_game_v1_components_with_player::<Player>(&mut fresh);
    fresh.freeze().unwrap();
    assert_eq!(fresh.catalog().unwrap(), &predecessor);

    let registry = frozen_reference_game_successor();
    assert_eq!(registry.catalog().unwrap(), &expected);
    assert_eq!(
        format!(
            "{}\n",
            registry
                .catalog()
                .unwrap()
                .to_json_string_with_predecessor(Some(&predecessor))
                .unwrap()
        ),
        v2_fixture
    );
    for id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
        "reference_game.Projectile",
    ] {
        assert!(registry.schema(&ComponentTypeId::new(id)).is_some());
    }
}

#[test]
fn reference_game_catalog_rejects_unversioned_semantic_change_and_missing_tombstone() {
    let mut missing_tombstone = reference_game_successor_with_player::<PlayerWithoutVelocity>();
    missing_tombstone
        .register_component_migration(
            &ComponentTypeId::new(PLAYER_ID),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).unwrap(),
            migrate_player_v1_to_v2,
        )
        .unwrap();
    assert!(matches!(
        missing_tombstone.freeze(),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("velocity")
    ));

    let mut changed_kind = reference_game_successor_with_player::<PlayerHealthKindChanged>();
    assert!(matches!(
        changed_kind.freeze(),
        Err(ComponentRegistryError::ComponentSchemaChangedWithoutVersionBump {
            component_id,
        }) if component_id == ComponentTypeId::new(PLAYER_ID)
    ));
}

#[test]
fn canonical_scene_and_stable_field_patch_round_trip_into_the_live_world() {
    let registry = frozen_reference_game_successor();
    let player_id = ComponentTypeId::new(PLAYER_ID);
    let enemy_id = ComponentTypeId::new("reference_game.Enemy");
    let wave_spawn_id = ComponentTypeId::new("reference_game.WaveSpawn");
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let projectile_id = ComponentTypeId::new("reference_game.Projectile");
    let player_entity_id = scene_id("player");
    let document = SceneDocument::new([
        SceneEntityRecord::new(player_entity_id.clone())
            .with_component(
                player_id.clone(),
                scene_component_record(Player::fixture(), &player_id, &registry),
            )
            .with_component(
                weapon_id.clone(),
                scene_component_record(Weapon::fixture(), &weapon_id, &registry),
            ),
        SceneEntityRecord::new(scene_id("enemy"))
            .with_component(
                enemy_id.clone(),
                scene_component_record(Enemy::fixture(), &enemy_id, &registry),
            )
            .with_component(
                wave_spawn_id.clone(),
                scene_component_record(WaveSpawn::fixture(), &wave_spawn_id, &registry),
            ),
        SceneEntityRecord::new(scene_id("projectile")).with_component(
            projectile_id.clone(),
            scene_component_record(Projectile::fixture(), &projectile_id, &registry),
        ),
    ]);
    let scene_json = document.to_json_string().unwrap();
    let scene_candidate = SceneDocumentCandidate::decode_json_bytes(scene_json.as_bytes()).unwrap();
    assert_eq!(scene_candidate.to_json_string().unwrap(), scene_json);
    let mut session = SceneAuthoringSession::new(SceneDocument::default());
    session
        .try_replace_file_candidate(scene_candidate, &registry)
        .unwrap();
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision().generation(), 1);
    assert_eq!(session.history_status().undo_depth, 0);
    assert!(!session.source_upgrade_required());

    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player_entity_id.clone(),
        component: player_id,
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("hit-points"),
        value: ComponentValue::I64(12),
    }]);
    let patch_json = patch.to_json_string().unwrap();
    assert!(patch_json.contains("\"hit-points\""));
    assert!(!patch_json.contains("hit_points"));
    let patch_candidate =
        ScenePatchDocumentCandidate::decode_json_bytes(patch_json.as_bytes()).unwrap();
    assert_eq!(patch_candidate.to_json_string().unwrap(), patch_json);
    let patch_report = session.apply_file_patch_candidate(patch_candidate, &registry);
    assert!(patch_report.applied, "{:#?}", patch_report.diagnostics);
    assert!(patch_report.inverse.is_some());
    assert_eq!(session.revision().generation(), 2);
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.is_live_dirty());

    let mut world = World::new();
    let sync_report = session.sync_world(&mut world, &registry);
    assert!(
        !sync_report.diagnostics.has_errors(),
        "{:#?}",
        sync_report.diagnostics
    );
    let live_instance = sync_report.live_instance.as_ref().unwrap();
    assert_eq!(live_instance.len(), 3);

    let player_entity = resolved_entity(live_instance, &world, &player_entity_id);
    let enemy_entity = resolved_entity(live_instance, &world, &scene_id("enemy"));
    let projectile_entity = resolved_entity(live_instance, &world, &scene_id("projectile"));
    let mut expected_player = Player::fixture();
    expected_player.hit_points = 12;
    assert_eq!(world.get::<Player>(player_entity), Some(&expected_player));
    assert_eq!(world.get::<Weapon>(player_entity), Some(&Weapon::fixture()));
    assert_eq!(world.get::<Enemy>(enemy_entity), Some(&Enemy::fixture()));
    assert_eq!(
        world.get::<WaveSpawn>(enemy_entity),
        Some(&WaveSpawn::fixture())
    );
    assert_eq!(
        world.get::<Projectile>(projectile_entity),
        Some(&Projectile::fixture())
    );
}

fn assert_round_trip<T>(expected: T, id: &str, registry: &ComponentRegistry)
where
    T: nara::prelude::Component + Clone + PartialEq + std::fmt::Debug,
{
    let mut source = World::new();
    let source_entity = source.spawn(expected.clone()).id();
    let component_id = ComponentTypeId::new(id);
    let encoded = registry
        .encode_component(&component_id, &source, source_entity)
        .unwrap()
        .unwrap()
        .unwrap();

    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    registry
        .preflight_component(&component_id, &encoded)
        .unwrap()
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    assert_eq!(target.get::<T>(target_entity), Some(&expected));
}

fn reference_game_predecessor_catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog::from_json_bytes(include_bytes!("../schema/component-schema-v1.json"))
        .unwrap()
}

fn reference_game_successor_with_player<T>() -> ComponentRegistry
where
    T: PersistentComponentProvider,
{
    let mut registry =
        ComponentRegistry::successor_of(reference_game_predecessor_catalog()).unwrap();
    register_reference_game_v1_components_with_player::<T>(&mut registry);
    registry
        .register_persistent_component::<WaveSpawn>()
        .unwrap();
    registry
}

fn frozen_reference_game_successor() -> ComponentRegistry {
    let mut registry = reference_game_successor_with_player::<Player>();
    registry.freeze().unwrap();
    registry
}

fn register_reference_game_v1_components_with_player<T>(registry: &mut ComponentRegistry)
where
    T: PersistentComponentProvider,
{
    registry.register_persistent_component::<T>().unwrap();
    registry.register_persistent_component::<Enemy>().unwrap();
    registry.register_persistent_component::<Weapon>().unwrap();
    registry
        .register_persistent_component::<Projectile>()
        .unwrap();
}

fn migrate_player_v1_to_v2(value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    let ComponentValue::Map(mut fields) = value else {
        return Err(ComponentCodecError::invalid_field("Player", "map"));
    };
    fields.remove("velocity");
    Ok(ComponentValue::Map(fields))
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

fn scene_component_record<T>(
    component: T,
    component_id: &ComponentTypeId,
    registry: &ComponentRegistry,
) -> SceneComponentRecord
where
    T: Component,
{
    let mut world = World::new();
    let entity = world.spawn(component).id();
    let value = registry
        .encode_component(component_id, &world, entity)
        .unwrap()
        .unwrap()
        .unwrap();
    SceneComponentRecord::new(registry.schema(component_id).unwrap().version(), value)
}

fn resolved_entity(
    instance: &nara::scene::SpawnedSceneInstance,
    world: &World,
    entity_id: &SceneEntityId,
) -> nara::prelude::Entity {
    match instance.resolve(world, entity_id) {
        EntityLookup::Resolved(entity) => entity,
        outcome => panic!("scene entity {entity_id} did not resolve: {outcome:?}"),
    }
}
