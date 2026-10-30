use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentApplyBatch, ComponentCapability, ComponentCodecError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentRegistryError,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PreparedComponentCandidate,
};

#[test]
fn public_asset_claims_preserve_durable_refs_across_independent_runtimes() {
    use nara_asset::{AssetPathScope, AssetRef, AssetServer, Assets};

    let source = AssetRef::path("textures/player.png").unwrap();
    let mut first_server = AssetServer::new();
    let mut second_server = AssetServer::new();
    let first = source
        .resolve::<String>(&mut first_server, &AssetPathScope::ROOT)
        .unwrap();
    let second = source
        .resolve::<String>(&mut second_server, &AssetPathScope::ROOT)
        .unwrap();
    let mut assets = Assets::default();
    assets.insert(first, String::from("pixels"));

    assert_ne!(first.id(), second.id());
    assert!(assets.get(second).is_none());
    assert_eq!(
        AssetRef::from_handle(&first_server, first, &AssetPathScope::ROOT).unwrap(),
        source
    );
    assert_eq!(
        AssetRef::from_handle(&second_server, second, &AssetPathScope::ROOT).unwrap(),
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
    let snapshot = registry.snapshot().unwrap();

    let component_id = ComponentTypeId::new("example.Health");
    let source = ComponentValue::map([("current", ComponentValue::I64(42))]);
    let prepared = snapshot
        .preflight_component(&component_id, &source)
        .expect("stored Health should decode");
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    prepared
        .apply(&mut world, entity)
        .expect("decoded Health should apply");
    assert_eq!(world.get::<Health>(entity).unwrap().current, 42);

    world.get_mut::<Health>(entity).unwrap().current = 17;
    let encoded = snapshot
        .encode_component(&component_id, &world, entity)
        .expect("native Health should encode")
        .expect("entity should contain Health");
    assert_eq!(
        encoded,
        ComponentValue::map([("current", ComponentValue::I64(17))])
    );

    let restored = world.spawn_empty().id();
    snapshot
        .preflight_component(&component_id, &encoded)
        .unwrap()
        .apply(&mut world, restored)
        .expect("encoded native value should decode and apply again");
    assert_eq!(world.get::<Health>(restored).unwrap().current, 17);
}

#[derive(Component)]
struct Portrait(nara_asset::Handle<String>);

#[derive(Component)]
struct Dialogue {
    portrait: nara_asset::Handle<String>,
    text: String,
}

fn asset_codec_schema(id: &str) -> ComponentSchema {
    ComponentSchema::new(ComponentTypeId::new(id), id, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("asset"),
                "Asset",
                ComponentFieldPath::from_fields(["asset"]),
                ComponentValueKind::AssetRef,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
            ComponentFieldSchema::required(
                ComponentFieldId::new("text"),
                "Text",
                ComponentFieldPath::from_fields(["text"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
}

fn asset_codec_value(text: &str) -> ComponentValue {
    ComponentValue::map([
        (
            "asset",
            ComponentValue::map([
                ("kind", ComponentValue::String("path".to_owned())),
                (
                    "value",
                    ComponentValue::String("portraits/hero.txt".to_owned()),
                ),
            ]),
        ),
        ("text", ComponentValue::String(text.to_owned())),
    ])
}

fn asset_codec_registry() -> ComponentRegistry {
    use nara_asset::{AssetRef, AssetServer};

    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_codec_with_context::<Portrait, _, _>(
            asset_codec_schema("example.Portrait"),
            |value, context| {
                let asset = AssetRef::path(value.field("asset")?.field_str("value")?)
                    .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                if let Some(handle) = context.resolve_asset_ref::<String>(&asset) {
                    return handle
                        .map(|handle| PreparedComponentCandidate::insert(Portrait(handle)))
                        .map_err(|error| ComponentCodecError::Message(error.to_string()));
                }
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        context
                            .resolve_asset_ref::<String>(&asset)
                            .map(Portrait)
                            .map_err(|error| ComponentCodecError::Message(error.to_string()))
                    },
                ))
            },
            |portrait, world, context| {
                let asset = AssetRef::from_handle_with_policy(
                    world.resource::<AssetServer>(),
                    portrait.0,
                    context.asset_path_scope(),
                    context.asset_ref_export_policy(),
                )
                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                assert_eq!(asset, AssetRef::path("portraits/hero.txt").unwrap());
                Ok(asset_codec_value("portrait"))
            },
        )
        .unwrap();
    registry
        .register_persistent_component_codec_with_context::<Dialogue, _, _>(
            asset_codec_schema("example.Dialogue"),
            |value, _context| {
                let asset = AssetRef::path(value.field("asset")?.field_str("value")?)
                    .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                let text = value.field_str("text")?.to_owned().into_boxed_str();
                Ok(PreparedComponentCandidate::with_asset_server(
                    move |context| {
                        let portrait = context
                            .resolve_asset_ref::<String>(&asset)
                            .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                        if &*text == "reject" {
                            return Err(ComponentCodecError::invalid_field(
                                "text",
                                "accepted dialogue",
                            ));
                        }
                        Ok(Dialogue {
                            portrait,
                            text: text.into_string(),
                        })
                    },
                ))
            },
            |dialogue, world, context| {
                let asset = AssetRef::from_handle_with_policy(
                    world.resource::<AssetServer>(),
                    dialogue.portrait,
                    context.asset_path_scope(),
                    context.asset_ref_export_policy(),
                )
                .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
                assert_eq!(asset, AssetRef::path("portraits/hero.txt").unwrap());
                Ok(asset_codec_value(&dialogue.text))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

#[test]
fn typed_asset_codecs_own_inputs_and_share_one_batch_asset_server() {
    use nara_asset::AssetServer;

    let registry = asset_codec_registry();
    let snapshot = registry.snapshot().unwrap();
    for eager_decode in [false, true] {
        for fail in [false, true] {
            let mut world = World::new();
            let target = world.spawn_empty().id();
            let mut batch = ComponentApplyBatch::from_world(&world);
            let portrait_id = ComponentTypeId::new("example.Portrait");
            let dialogue_id = ComponentTypeId::new("example.Dialogue");
            let portrait_value = asset_codec_value("portrait");
            let portrait = if eager_decode {
                batch.with_decode_context(None, |context| {
                    snapshot.preflight_component_with_context(
                        &portrait_id,
                        &portrait_value,
                        context,
                    )
                })
            } else {
                snapshot.preflight_component(&portrait_id, &portrait_value)
            }
            .unwrap();
            let text = if fail { "reject" } else { "Welcome" };
            let dialogue_value = asset_codec_value(text);
            let dialogue = snapshot
                .preflight_component(&dialogue_id, &dialogue_value)
                .unwrap();
            drop(portrait_value);
            drop(dialogue_value);
            let batch = batch.stage(target, portrait).unwrap();
            assert!(world.get::<Portrait>(target).is_none());
            assert!(!world.contains_resource::<AssetServer>());
            let batch = batch.stage(target, dialogue);
            if fail {
                assert!(matches!(
                    batch,
                    Err(ComponentCodecError::InvalidField { .. })
                ));
                assert!(world.get::<Portrait>(target).is_none());
                assert!(world.get::<Dialogue>(target).is_none());
                assert!(!world.contains_resource::<AssetServer>());
                continue;
            }
            batch.unwrap().commit(&mut world).unwrap();
            let portrait = world.get::<Portrait>(target).unwrap();
            let dialogue = world.get::<Dialogue>(target).unwrap();
            assert_eq!(portrait.0.id(), dialogue.portrait.id());
            assert_eq!(dialogue.text, text);
            assert_eq!(
                snapshot.encode_component(&dialogue_id, &world, target),
                Ok(Some(asset_codec_value(text))),
            );
            assert_eq!(
                snapshot.encode_component(&portrait_id, &world, target),
                Ok(Some(asset_codec_value("portrait"))),
            );
        }
    }
}
