use nara::{
    identity::EntityLookup,
    prelude::{
        Component, ComponentRegistry, ComponentTypeId, Parent, PersistentComponent, Transform2d,
        Vec2, World,
    },
    reflect::{
        ComponentCatalogFileLimits, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentMigrationError, ComponentRegistryError, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentValue,
        PersistentComponentProvider,
    },
    scene::{
        SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchDocumentCandidate,
        ScenePatchOperation,
    },
};
use nara_reference_game::{
    EnemyRole, InitialHealth, InitialVelocity2d, PlayerRole, REFERENCE_GAME_SCHEMA_OWNER_ID,
    REFERENCE_GAME_SCHEMA_PROVIDER, ReferenceGamePlugin, RuntimeOnlyTag, WaveSpawn, Weapon,
};

const LINEAGE_PROBE_ID: &str = "nara.test.LineageProbe";
const LINEAGE_PROBE_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("nara.test.lineage-probe");

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

#[test]
fn authored_components_register_and_round_trip_while_runtime_state_stays_out_of_schema() {
    let mut app = nara::prelude::App::new();
    app.add_plugins((
        nara::prelude::MinimalPlugins,
        nara::advanced_prelude::StartupSceneActivationPlugin,
    ))
    .unwrap();
    app.add_plugin(ReferenceGamePlugin).unwrap();
    let app = app.seal().unwrap();

    let registry = nara::reflect::component_registry(app.world()).unwrap();
    for id in [
        "reference_game.PlayerRole",
        "reference_game.EnemyRole",
        "reference_game.InitialHealth",
        "reference_game.InitialVelocity2d",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
    ] {
        assert!(registry.schema(&ComponentTypeId::new(id)).is_some());
    }
    for id in [
        "reference_game.Health",
        "reference_game.Velocity2d",
        "reference_game.WeaponCooldown",
        "reference_game.ProjectileRole",
        "reference_game.ProjectileDamage",
        "reference_game.ProjectileLifetime",
        "reference_game.ProjectileId",
        "reference_game.RuntimeOnlyTag",
        "nara.transform.GlobalTransform2d",
    ] {
        assert!(
            registry.schema(&ComponentTypeId::new(id)).is_none(),
            "runtime-only component {id} entered the persistent schema",
        );
    }

    let mut runtime_only_world = World::new();
    let runtime_only_entity = runtime_only_world.spawn(RuntimeOnlyTag).id();
    assert!(
        runtime_only_world
            .get::<RuntimeOnlyTag>(runtime_only_entity)
            .is_some()
    );

    assert_round_trip(PlayerRole {}, "reference_game.PlayerRole", registry);
    assert_round_trip(EnemyRole {}, "reference_game.EnemyRole", registry);
    assert_round_trip(
        InitialHealth { hit_points: 20 },
        "reference_game.InitialHealth",
        registry,
    );
    assert_round_trip(
        InitialVelocity2d {
            velocity: Vec2::ZERO,
        },
        "reference_game.InitialVelocity2d",
        registry,
    );
    assert_round_trip(WaveSpawn::fixture(), "reference_game.WaveSpawn", registry);
    assert_round_trip(Weapon::fixture(), "reference_game.Weapon", registry);
}

fn migrate_lineage_v1_to_v2(value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    let ComponentValue::Map(mut fields) = value else {
        return Err(ComponentCodecError::invalid_field("LineageProbe", "map"));
    };
    fields.remove("removed");
    Ok(ComponentValue::Map(fields))
}

fn bind_persistent_component<T>(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError>
where
    T: PersistentComponentProvider,
{
    let id = T::persistent_component_schema().id().clone();
    registry
        .register_native_component_with_codec::<T, _, _>(
            &id,
            T::__decode_persistent_component,
            T::__encode_persistent_component,
        )
        .map(|_| ())
}

fn register_lineage_successor<T>(
    predecessor: ComponentSchemaCatalog,
    from_version: ComponentSchemaVersion,
    to_version: ComponentSchemaVersion,
) -> Result<ComponentRegistry, ComponentRegistryError>
where
    T: PersistentComponentProvider,
{
    let id = ComponentTypeId::new(LINEAGE_PROBE_ID);
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the lineage fixture has a successor generation");
    current.components.push(T::persistent_component_schema());
    let mut registry = ComponentRegistry::from_owner_catalog_candidate(
        LINEAGE_PROBE_OWNER_ID,
        current,
        Some(predecessor),
    )?;
    bind_persistent_component::<T>(&mut registry)?;
    registry
        .register_component_migration(&id, from_version, to_version, Ok)
        .map(|_| ())?;
    Ok(registry)
}

fn frozen_lineage_successor(predecessor: ComponentSchemaCatalog) -> ComponentRegistry {
    let id = ComponentTypeId::new(LINEAGE_PROBE_ID);
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the lineage fixture has a successor generation");
    current
        .components
        .push(LineageProbe::persistent_component_schema());
    let mut registry = ComponentRegistry::from_owner_catalog_candidate(
        LINEAGE_PROBE_OWNER_ID,
        current,
        Some(predecessor),
    )
    .unwrap();
    bind_persistent_component::<LineageProbe>(&mut registry).unwrap();
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

    assert!(matches!(
        register_lineage_successor::<MissingTombstoneProbe>(
            predecessor,
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).unwrap(),
        ),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));

    assert!(matches!(
        register_lineage_successor::<ReactivatedProbe>(
            v2.clone(),
            ComponentSchemaVersion::new(2).unwrap(),
            ComponentSchemaVersion::new(3).unwrap(),
        ),
        Err(ComponentRegistryError::ReactivatedFieldId { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));

    assert!(matches!(
        register_lineage_successor::<DroppedTombstoneProbe>(
            v2,
            ComponentSchemaVersion::new(2).unwrap(),
            ComponentSchemaVersion::new(3).unwrap(),
        ),
        Err(ComponentRegistryError::MissingFieldTombstone { field_id, .. })
            if field_id == ComponentFieldId::new("removed")
    ));
}

#[test]
fn reference_game_catalog_preserves_v1_through_v3_and_matches_the_v4_successor() {
    assert_eq!(REFERENCE_GAME_SCHEMA_PROVIDER.binding().version(), 4);
    let v1_fixture = include_str!("../schema/component-schema-v1.json");
    let v2_fixture = include_str!("../schema/component-schema-v2.json");
    let v3_fixture = include_str!("../schema/component-schema-v3.json");
    let v4_fixture = include_str!("../schema/component-schema-v4.json");
    let v1 = reference_game_predecessor_catalog();
    assert_eq!(format!("{}\n", v1.to_json_string().unwrap()), v1_fixture);
    let v2 = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v2_fixture.as_bytes(),
        &v1,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();
    let v3 = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v3_fixture.as_bytes(),
        &v2,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();
    let v4 = ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        v4_fixture.as_bytes(),
        &v3,
        ComponentCatalogFileLimits::default(),
    )
    .unwrap();
    for (catalog, predecessor, fixture) in [
        (&v2, &v1, v2_fixture),
        (&v3, &v2, v3_fixture),
        (&v4, &v3, v4_fixture),
    ] {
        assert_eq!(
            format!(
                "{}\n",
                catalog
                    .to_json_string_with_predecessor(Some(predecessor))
                    .unwrap()
            ),
            fixture,
        );
    }

    let registry = frozen_reference_game_successor();
    let snapshot = registry.snapshot().unwrap();
    let owner_receipt = snapshot
        .owner_receipt(REFERENCE_GAME_SCHEMA_OWNER_ID)
        .unwrap();
    assert_eq!(owner_receipt.generation(), v4.generation());
    assert_eq!(owner_receipt.catalog(), v4.fingerprint());
    assert_eq!(owner_receipt.predecessor(), v4.predecessor().copied());
    assert_eq!(registry.catalog().unwrap().components(), v4.components());
    assert_eq!(
        registry.catalog().unwrap().type_tombstones(),
        v4.type_tombstones()
    );
    for id in [
        "reference_game.PlayerRole",
        "reference_game.EnemyRole",
        "reference_game.InitialHealth",
        "reference_game.InitialVelocity2d",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
    ] {
        assert!(registry.schema(&ComponentTypeId::new(id)).is_some());
    }
}

#[test]
fn weapon_v1_migrates_to_v2_without_persisting_runtime_cooldown() {
    let registry = frozen_reference_game_successor();
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let legacy_value = ComponentValue::map([
        ("cooldown-ticks", ComponentValue::U64(3)),
        ("remaining-ticks", ComponentValue::U64(2)),
        ("damage", ComponentValue::I64(3)),
    ]);
    let migrated = registry
        .migrate_component_value(&weapon_id, ComponentSchemaVersion::ONE, &legacy_value)
        .unwrap();
    assert_eq!(migrated.version, ComponentSchemaVersion::new(2).unwrap());
    let ComponentValue::Map(fields) = &migrated.value else {
        panic!("migrated Weapon value must remain a map");
    };
    assert!(!fields.contains_key("remaining-ticks"));
    assert_eq!(fields.get("cooldown-ticks"), Some(&ComponentValue::U64(3)));
    assert_eq!(fields.get("damage"), Some(&ComponentValue::I64(3)));

    let mut current_world = World::new();
    let current_entity = current_world.spawn_empty().id();
    registry
        .preflight_component(&weapon_id, &migrated.value)
        .unwrap()
        .unwrap()
        .apply(&mut current_world, current_entity)
        .unwrap();
    assert_eq!(
        current_world.get::<Weapon>(current_entity),
        Some(&Weapon::fixture())
    );
}

#[test]
fn removed_aggregate_component_records_are_explicitly_tombstoned_and_rejected() {
    let registry = frozen_reference_game_successor();
    let tombstones = registry.catalog().unwrap().type_tombstones();
    for id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.Projectile",
    ] {
        let component_id = ComponentTypeId::new(id);
        assert!(tombstones.contains(&component_id));
        assert!(registry.schema(&component_id).is_none());
        assert!(matches!(
            registry.migrate_component_value(
                &component_id,
                ComponentSchemaVersion::ONE,
                &ComponentValue::Map(Default::default()),
            ),
            Err(ComponentMigrationError::UnknownComponentId { component_id: rejected })
                if rejected == component_id
        ));
    }
}

#[test]
fn canonical_scene_and_stable_field_patch_round_trip_into_the_live_world() {
    let registry = frozen_reference_game_authoring_registry();
    let player_role_id = ComponentTypeId::new("reference_game.PlayerRole");
    let enemy_role_id = ComponentTypeId::new("reference_game.EnemyRole");
    let health_id = ComponentTypeId::new("reference_game.InitialHealth");
    let velocity_id = ComponentTypeId::new("reference_game.InitialVelocity2d");
    let wave_spawn_id = ComponentTypeId::new("reference_game.WaveSpawn");
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let player_entity_id = scene_id("player");
    let weapon_entity_id = scene_id("player-weapon");
    let document = SceneDocument::new([
        SceneEntityRecord::new(player_entity_id.clone())
            .with_component(
                player_role_id.clone(),
                scene_component_record(PlayerRole {}, &player_role_id, &registry),
            )
            .with_component(
                health_id.clone(),
                scene_component_record(InitialHealth { hit_points: 20 }, &health_id, &registry),
            )
            .with_component(
                velocity_id.clone(),
                scene_component_record(
                    InitialVelocity2d {
                        velocity: Vec2::ZERO,
                    },
                    &velocity_id,
                    &registry,
                ),
            )
            .with_component(
                transform_id.clone(),
                scene_component_record(Transform2d::IDENTITY, &transform_id, &registry),
            ),
        SceneEntityRecord::new(weapon_entity_id.clone())
            .with_parent(player_entity_id.clone())
            .with_component(
                weapon_id.clone(),
                scene_component_record(Weapon::fixture(), &weapon_id, &registry),
            )
            .with_component(
                transform_id.clone(),
                scene_component_record(
                    Transform2d::from_translation(Vec2::new(1.2, 0.0)),
                    &transform_id,
                    &registry,
                ),
            ),
        SceneEntityRecord::new(scene_id("enemy"))
            .with_component(
                enemy_role_id.clone(),
                scene_component_record(EnemyRole {}, &enemy_role_id, &registry),
            )
            .with_component(
                health_id.clone(),
                scene_component_record(InitialHealth { hit_points: 10 }, &health_id, &registry),
            )
            .with_component(
                velocity_id.clone(),
                scene_component_record(
                    InitialVelocity2d {
                        velocity: Vec2::new(-0.5, 0.0),
                    },
                    &velocity_id,
                    &registry,
                ),
            )
            .with_component(
                wave_spawn_id.clone(),
                scene_component_record(WaveSpawn::fixture(), &wave_spawn_id, &registry),
            )
            .with_component(
                transform_id.clone(),
                scene_component_record(
                    Transform2d::from_translation(Vec2::new(5.0, 0.0)),
                    &transform_id,
                    &registry,
                ),
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
        component: health_id,
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
    let weapon_entity = resolved_entity(live_instance, &world, &weapon_entity_id);
    let enemy_entity = resolved_entity(live_instance, &world, &scene_id("enemy"));
    assert_eq!(world.get::<PlayerRole>(player_entity), Some(&PlayerRole {}));
    assert_eq!(
        world.get::<InitialHealth>(player_entity),
        Some(&InitialHealth { hit_points: 12 })
    );
    assert_eq!(
        world.get::<Transform2d>(player_entity),
        Some(&Transform2d::IDENTITY)
    );
    assert_eq!(world.get::<Weapon>(weapon_entity), Some(&Weapon::fixture()));
    assert_eq!(
        world.get::<Transform2d>(weapon_entity),
        Some(&Transform2d::from_translation(Vec2::new(1.2, 0.0)))
    );
    assert_eq!(
        world.get::<Parent>(weapon_entity).map(Parent::parent),
        Some(player_entity)
    );
    assert_eq!(world.get::<EnemyRole>(enemy_entity), Some(&EnemyRole {}));
    assert_eq!(
        world.get::<WaveSpawn>(enemy_entity),
        Some(&WaveSpawn::fixture())
    );
    assert_eq!(
        world.get::<Transform2d>(enemy_entity),
        Some(&Transform2d::from_translation(Vec2::new(5.0, 0.0)))
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

fn frozen_reference_game_successor() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    REFERENCE_GAME_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn frozen_reference_game_authoring_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    nara::transform::TRANSFORM_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    REFERENCE_GAME_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    registry
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
