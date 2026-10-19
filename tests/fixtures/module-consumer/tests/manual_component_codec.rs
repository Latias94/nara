use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

#[test]
fn public_asset_claims_preserve_durable_refs_across_independent_runtimes() {
    use nara_asset::{AssetRef, AssetServer, Assets};

    let source = AssetRef::path("textures/player.png").unwrap();
    let mut first_server = AssetServer::new();
    let mut second_server = AssetServer::new();
    let first = source.resolve::<String>(&mut first_server).unwrap();
    let second = source.resolve::<String>(&mut second_server).unwrap();
    let mut assets = Assets::default();
    assets.insert(first, String::from("pixels"));

    assert_ne!(first.id(), second.id());
    assert!(assets.get(second).is_none());
    assert_eq!(AssetRef::from_handle(&first_server, first).unwrap(), source);
    assert_eq!(
        AssetRef::from_handle(&second_server, second).unwrap(),
        source
    );
    assert!(first_server.claim::<u32>(first.id()).is_err());
}

#[derive(Component)]
struct Health {
    current: i64,
}

fn register_health(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("example.Health"),
        "Health",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("current"),
        "Current",
        ComponentFieldPath::from_fields(["current"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);

    registry.register_persistent_component_with_codec::<Health, _, _>(
        schema,
        |value| {
            let current = value
                .get("current")
                .and_then(ComponentValue::as_i64)
                .ok_or_else(|| ComponentCodecError::invalid_field("current", "i64"))?;
            Ok(Health { current })
        },
        |health| {
            Ok(ComponentValue::map([(
                "current",
                ComponentValue::I64(health.current),
            )]))
        },
    )?;
    Ok(())
}

#[test]
fn documented_manual_codec_round_trips_through_the_public_registry() {
    let mut registry = ComponentRegistry::new();
    register_health(&mut registry).expect("manual codec should register");
    registry
        .freeze()
        .expect("manual component schema should freeze");

    let component_id = ComponentTypeId::new("example.Health");
    let source = ComponentValue::map([("current", ComponentValue::I64(42))]);
    let prepared = registry
        .preflight_component(&component_id, &source)
        .expect("frozen registry should expose the manual codec")
        .expect("stored Health should decode");
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    prepared
        .apply(&mut world, entity)
        .expect("decoded Health should apply");
    assert_eq!(world.get::<Health>(entity).unwrap().current, 42);

    world.get_mut::<Health>(entity).unwrap().current = 17;
    let encoded = registry
        .encode_component(&component_id, &world, entity)
        .expect("manual encoder should be registered")
        .expect("native Health should encode")
        .expect("entity should contain Health");
    assert_eq!(
        encoded,
        ComponentValue::map([("current", ComponentValue::I64(17))])
    );

    let restored = world.spawn_empty().id();
    registry
        .preflight_component(&component_id, &encoded)
        .unwrap()
        .unwrap()
        .apply(&mut world, restored)
        .expect("encoded native value should decode and apply again");
    assert_eq!(world.get::<Health>(restored).unwrap().current, 17);
}
