use nara_ecs::{Component, Resource, World, lifecycle::HookContext, world::DeferredWorld};
use nara_identity::{EntityReference, SceneEntityId};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentNumericDomain, ComponentRegistry, ComponentRegistryError,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind, PersistentComponent,
    PersistentComponentProvider, validate_component_value,
};

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.GeneratedProbe",
    version = 1,
    alias = "Generated probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    tombstone = "removed-field"
)]
struct GeneratedProbe {
    #[nara(id = "position", alias = "Position")]
    position: nara_core::Vec2,
    #[nara(id = "health", alias = "Health")]
    health: i64,
    #[nara(id = "cooldown", alias = "Cooldown")]
    cooldown: u64,
    #[nara(
        id = "target",
        alias = "Target",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    target: EntityReference,
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.ScalarProbe",
    version = 1,
    alias = "Scalar probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct ScalarProbe {
    #[nara(id = "enabled", alias = "Enabled")]
    enabled: bool,
    #[nara(id = "label", alias = "Label")]
    label: String,
    #[nara(id = "speed", alias = "Speed")]
    speed: f32,
    #[nara(id = "distance", alias = "Distance")]
    distance: f64,
    #[nara(id = "signed8", alias = "Signed 8")]
    signed8: i8,
    #[nara(id = "signed16", alias = "Signed 16")]
    signed16: i16,
    #[nara(id = "signed32", alias = "Signed 32")]
    signed32: i32,
    #[nara(id = "signed64", alias = "Signed 64")]
    signed64: i64,
    #[nara(id = "unsigned8", alias = "Unsigned 8")]
    unsigned8: u8,
    #[nara(id = "unsigned16", alias = "Unsigned 16")]
    unsigned16: u16,
    #[nara(id = "unsigned32", alias = "Unsigned 32")]
    unsigned32: u32,
    #[nara(id = "unsigned64", alias = "Unsigned 64")]
    unsigned64: u64,
}

fn scalar_probe() -> ScalarProbe {
    ScalarProbe {
        enabled: true,
        label: "Player 配置".to_owned(),
        speed: 200.25,
        distance: f64::MAX,
        signed8: i8::MIN,
        signed16: i16::MAX,
        signed32: i32::MIN,
        signed64: i64::MAX,
        unsigned8: u8::MAX,
        unsigned16: u16::MAX,
        unsigned32: u32::MAX,
        unsigned64: u64::MAX,
    }
}

#[test]
fn generated_scalar_provider_round_trips_native_components_and_declares_numeric_domains() {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<ScalarProbe>()
        .unwrap();
    registry.freeze().unwrap();
    let id = ComponentTypeId::new("nara.test.ScalarProbe");
    let schema = registry.schema(&id).unwrap();
    for (field_id, kind, domain) in [
        ("enabled", ComponentValueKind::Bool, None),
        ("label", ComponentValueKind::String, None),
        (
            "speed",
            ComponentValueKind::F64,
            Some(ComponentNumericDomain::F32),
        ),
        ("distance", ComponentValueKind::F64, None),
        (
            "signed8",
            ComponentValueKind::I64,
            Some(ComponentNumericDomain::I8),
        ),
        (
            "signed16",
            ComponentValueKind::I64,
            Some(ComponentNumericDomain::I16),
        ),
        (
            "signed32",
            ComponentValueKind::I64,
            Some(ComponentNumericDomain::I32),
        ),
        ("signed64", ComponentValueKind::I64, None),
        (
            "unsigned8",
            ComponentValueKind::U64,
            Some(ComponentNumericDomain::U8),
        ),
        (
            "unsigned16",
            ComponentValueKind::U64,
            Some(ComponentNumericDomain::U16),
        ),
        (
            "unsigned32",
            ComponentValueKind::U64,
            Some(ComponentNumericDomain::U32),
        ),
        ("unsigned64", ComponentValueKind::U64, None),
    ] {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == field_id)
            .unwrap();
        assert_eq!(field.value_kind(), kind);
        assert_eq!(field.numeric_domain(), domain);
    }

    let expected = scalar_probe();
    let mut source = World::new();
    let source_entity = source.spawn(expected.clone()).id();
    let value = registry
        .encode_component(&id, &source, source_entity)
        .unwrap()
        .unwrap()
        .unwrap();
    validate_component_value(schema, &value).unwrap();
    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    registry
        .preflight_component(&id, &value)
        .unwrap()
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    assert_eq!(target.get::<ScalarProbe>(target_entity), Some(&expected));
}

#[test]
fn generated_scalar_codecs_reject_wrong_kinds_and_narrowing_overflow() {
    let original = scalar_probe().__encode_persistent_component().unwrap();
    let schema = ScalarProbe::persistent_component_schema();
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<ScalarProbe>()
        .unwrap();
    registry.freeze().unwrap();
    for (field, invalid) in [
        ("enabled", ComponentValue::U64(1)),
        ("label", ComponentValue::Bool(true)),
        ("speed", ComponentValue::I64(200)),
        ("speed", ComponentValue::f64(f64::MAX).unwrap()),
        ("speed", ComponentValue::f64(f64::MIN).unwrap()),
        ("distance", ComponentValue::I64(1)),
        ("signed8", ComponentValue::I64(i64::from(i8::MIN) - 1)),
        ("signed8", ComponentValue::I64(i64::from(i8::MAX) + 1)),
        ("signed16", ComponentValue::I64(i64::from(i16::MIN) - 1)),
        ("signed16", ComponentValue::I64(i64::from(i16::MAX) + 1)),
        ("signed32", ComponentValue::I64(i64::from(i32::MIN) - 1)),
        ("signed32", ComponentValue::I64(i64::from(i32::MAX) + 1)),
        ("signed64", ComponentValue::U64(1)),
        ("unsigned8", ComponentValue::U64(u64::from(u8::MAX) + 1)),
        ("unsigned16", ComponentValue::U64(u64::from(u16::MAX) + 1)),
        ("unsigned32", ComponentValue::U64(u64::from(u32::MAX) + 1)),
        ("unsigned64", ComponentValue::I64(-1)),
    ] {
        let mut value = original.clone();
        value
            .replace_path(&ComponentFieldPath::from_fields([field]), invalid)
            .unwrap();
        assert!(
            ScalarProbe::__decode_persistent_component(&value).is_err(),
            "native codec accepted {field}: {value:?}"
        );
        assert!(
            validate_component_value(&schema, &value).is_err(),
            "data validator accepted {field}: {value:?}"
        );
        assert!(
            registry
                .preflight_component(schema.id(), &value)
                .unwrap()
                .is_err(),
            "runtime preflight accepted {field}: {value:?}"
        );
    }
}

#[test]
fn generated_numeric_codecs_accept_edges_and_normal_float_rounding() {
    let mut value = scalar_probe().__encode_persistent_component().unwrap();
    let schema = ScalarProbe::persistent_component_schema();
    for (field, valid) in [
        ("signed8", ComponentValue::I64(i64::from(i8::MAX))),
        ("signed16", ComponentValue::I64(i64::from(i16::MIN))),
        ("signed32", ComponentValue::I64(i64::from(i32::MAX))),
        ("signed64", ComponentValue::I64(i64::MIN)),
        ("unsigned8", ComponentValue::U64(0)),
        ("unsigned16", ComponentValue::U64(0)),
        ("unsigned32", ComponentValue::U64(0)),
        ("unsigned64", ComponentValue::U64(0)),
        ("speed", ComponentValue::f64(f64::from(f32::MIN)).unwrap()),
        ("speed", ComponentValue::f64(f64::from(f32::MAX)).unwrap()),
        ("speed", ComponentValue::f64(0.1).unwrap()),
        ("speed", ComponentValue::f64(f64::MIN_POSITIVE).unwrap()),
        ("distance", ComponentValue::f64(f64::MIN).unwrap()),
    ] {
        value
            .replace_path(&ComponentFieldPath::from_fields([field]), valid)
            .unwrap();
        validate_component_value(&schema, &value).unwrap();
        ScalarProbe::__decode_persistent_component(&value).unwrap();
    }
    value
        .replace_path(
            &ComponentFieldPath::from_fields(["speed"]),
            ComponentValue::f64(0.1).unwrap(),
        )
        .unwrap();
    assert_eq!(
        ScalarProbe::__decode_persistent_component(&value)
            .unwrap()
            .speed,
        0.1_f32
    );
}

#[test]
fn generated_float_codecs_reject_non_finite_encoding() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut probe = scalar_probe();
        probe.distance = invalid;
        assert!(probe.__encode_persistent_component().is_err());
        probe = scalar_probe();
        probe.speed = invalid as f32;
        assert!(probe.__encode_persistent_component().is_err());
        assert!(ComponentValue::f64(invalid).is_err());
    }
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.PlayerRole",
    version = 1,
    alias = "Player role",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct PlayerRole {}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.EnemyRole",
    version = 1,
    alias = "Enemy role",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct EnemyRole {}

#[derive(Component, Default)]
struct ImplicitDependency;

#[derive(Component, PersistentComponent)]
#[require(ImplicitDependency)]
#[nara(
    id = "nara.test.RequiredPersistentProbe",
    version = 1,
    alias = "Required persistent probe",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct RequiredPersistentProbe {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

#[derive(Component, PersistentComponent)]
#[component(on_add = persistent_probe_on_add)]
#[nara(
    id = "nara.test.HookedPersistentProbe",
    version = 1,
    alias = "Hooked persistent probe",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct HookedPersistentProbe {
    #[nara(id = "value", alias = "Value")]
    value: i64,
}

#[derive(Component, Default)]
struct ManualCodecDependency;

#[derive(Component)]
#[require(ManualCodecDependency)]
struct ManualRequiredCodecProbe {
    value: i64,
}

#[derive(Component)]
#[component(on_add = persistent_probe_on_add)]
struct ManualHookedCodecProbe {
    value: i64,
}

fn persistent_probe_on_add(_world: DeferredWorld<'_>, _context: HookContext) {}

#[derive(Component)]
struct InvalidProviderSchema;

impl PersistentComponentProvider for InvalidProviderSchema {
    fn persistent_component_schema() -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("invalid component id"),
            "Invalid provider schema",
            ComponentSchemaVersion::ONE,
        )
    }

    fn __decode_persistent_component(_value: &ComponentValue) -> Result<Self, ComponentCodecError> {
        Ok(Self)
    }

    fn __encode_persistent_component(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::I64(0))
    }
}

#[test]
fn generated_provider_registers_schema_and_round_trips_component() {
    let component_id = ComponentTypeId::new("nara.test.GeneratedProbe");
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<GeneratedProbe>()
        .unwrap();
    registry.freeze().unwrap();

    let schema = registry.schema(&component_id).unwrap();
    assert_eq!(schema.aliases(), &["Generated probe"]);
    assert_eq!(schema.version().get(), 1);
    assert!(schema.has_capability(ComponentCapability::Scene));
    assert_eq!(
        schema.field_tombstones(),
        &[ComponentFieldId::new("removed-field")]
    );
    assert_eq!(
        registry
            .resolve_field(&component_id, &ComponentFieldId::new("position"))
            .unwrap()
            .path(),
        &nara_reflect::ComponentFieldPath::from_fields(["position"])
    );

    let expected = GeneratedProbe {
        position: nara_core::Vec2::new(2.0, -3.0),
        health: 17,
        cooldown: 4,
        target: EntityReference::SceneLocal {
            entity: SceneEntityId::new("enemy-1").unwrap(),
        },
    };
    let mut source = World::new();
    let source_entity = source.spawn(expected.clone()).id();
    let value = registry
        .encode_component(&component_id, &source, source_entity)
        .unwrap()
        .unwrap()
        .unwrap();

    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    registry
        .preflight_component(&component_id, &value)
        .unwrap()
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    assert_eq!(target.get::<GeneratedProbe>(target_entity), Some(&expected));
}

#[test]
fn generated_empty_markers_register_and_round_trip_empty_maps() {
    let player_id = ComponentTypeId::new("nara.test.PlayerRole");
    let enemy_id = ComponentTypeId::new("nara.test.EnemyRole");
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<PlayerRole>()
        .unwrap();
    registry
        .register_persistent_component::<EnemyRole>()
        .unwrap();
    registry.freeze().unwrap();

    for component_id in [&player_id, &enemy_id] {
        let schema = registry.schema(component_id).unwrap();
        assert!(schema.has_capability(ComponentCapability::Scene));
        assert!(schema.fields().is_empty());
    }

    let mut source = World::new();
    let player_entity = source.spawn(PlayerRole {}).id();
    let enemy_entity = source.spawn(EnemyRole {}).id();
    let encoded_player = registry
        .encode_component(&player_id, &source, player_entity)
        .unwrap()
        .unwrap()
        .unwrap();
    let encoded_enemy = registry
        .encode_component(&enemy_id, &source, enemy_entity)
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(encoded_player, ComponentValue::Map(Default::default()));
    assert_eq!(encoded_enemy, ComponentValue::Map(Default::default()));

    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    registry
        .preflight_component(&player_id, &encoded_player)
        .unwrap()
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    registry
        .preflight_component(&enemy_id, &encoded_enemy)
        .unwrap()
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    assert_eq!(
        target.get::<PlayerRole>(target_entity),
        Some(&PlayerRole {})
    );
    assert_eq!(target.get::<EnemyRole>(target_entity), Some(&EnemyRole {}));

    let unknown_value = ComponentValue::map([("unknown", ComponentValue::Bool(true))]);
    let error = registry
        .preflight_component(&player_id, &unknown_value)
        .unwrap()
        .err()
        .expect("empty marker schemas must reject unknown values");
    assert!(error.to_string().contains("unknown"));
    assert!(error.to_string().contains("not declared by schema"));
}

#[test]
fn generated_empty_marker_catalogs_preserve_lineage_and_fingerprint() {
    let predecessor = ComponentSchemaCatalog {
        components: vec![
            PlayerRole::persistent_component_schema(),
            EnemyRole::persistent_component_schema(),
        ],
        ..ComponentSchemaCatalog::default()
    };
    let predecessor_fingerprint = predecessor.fingerprint();
    assert_ne!(
        predecessor_fingerprint,
        ComponentSchemaCatalog::default().fingerprint()
    );

    let mut successor = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
    successor.components = predecessor.components.clone();
    assert_eq!(successor.predecessor(), Some(&predecessor_fingerprint));
    assert_eq!(successor.fingerprint(), predecessor_fingerprint);

    ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("nara.test.empty-markers"),
        successor,
        Some(predecessor),
    )
    .expect("unchanged empty marker schemas must preserve valid owner lineage");
}

#[test]
fn provider_schema_uses_explicit_ids_instead_of_rust_field_names() {
    let schema = GeneratedProbe::persistent_component_schema();
    assert_eq!(
        schema.id(),
        &ComponentTypeId::new("nara.test.GeneratedProbe")
    );
    assert!(schema.fields().iter().any(|field| {
        field.id() == &ComponentFieldId::new("health")
            && field.path() == &nara_reflect::ComponentFieldPath::from_fields(["health"])
    }));
}

#[test]
fn provider_validation_rejects_invalid_schema_without_registering_it() {
    let mut registry = ComponentRegistry::new();

    let validation_error = registry
        .validate_persistent_component::<InvalidProviderSchema>()
        .unwrap_err();

    assert!(matches!(
        validation_error,
        ComponentRegistryError::InvalidComponentTypeId { .. }
    ));
    assert!(registry.catalog_candidate().components().is_empty());

    let registration_error = registry
        .register_persistent_component::<InvalidProviderSchema>()
        .err()
        .expect("invalid provider schema must fail registration");
    assert_eq!(validation_error, registration_error);
    assert!(registry.catalog_candidate().components().is_empty());
}

#[test]
fn provider_validation_rejects_implicit_components_and_intrinsic_hooks() {
    let registry = ComponentRegistry::new();

    assert!(matches!(
        registry.validate_persistent_component::<RequiredPersistentProbe>(),
        Err(ComponentRegistryError::PersistentComponentRequiresImplicitComponents {
            component_id,
        }) if component_id == ComponentTypeId::new("nara.test.RequiredPersistentProbe")
    ));
    assert!(matches!(
        registry.validate_persistent_component::<HookedPersistentProbe>(),
        Err(ComponentRegistryError::PersistentComponentHasLifecycleHook { component_id })
            if component_id == ComponentTypeId::new("nara.test.HookedPersistentProbe")
    ));
    assert!(registry.catalog_candidate().components().is_empty());

    let mut registry = registry;
    assert!(matches!(
        registry.register_persistent_component::<RequiredPersistentProbe>(),
        Err(ComponentRegistryError::PersistentComponentRequiresImplicitComponents { .. })
    ));
    assert!(matches!(
        registry.register_persistent_component::<HookedPersistentProbe>(),
        Err(ComponentRegistryError::PersistentComponentHasLifecycleHook { .. })
    ));
    assert!(registry.catalog_candidate().components().is_empty());
}

#[test]
fn manual_persistent_codecs_cannot_bypass_component_metadata_validation() {
    let mut registry = ComponentRegistry::new();
    let required_id = ComponentTypeId::new("nara.test.ManualRequiredCodecProbe");
    let required_schema = ComponentSchema::new(
        required_id.clone(),
        "Manual required codec probe",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::I64,
    )
    .with_capabilities([ComponentCapability::Scene])]);

    let required_error = registry
        .register_persistent_component_with_codec::<ManualRequiredCodecProbe, _, _>(
            required_schema,
            |_| Ok(ManualRequiredCodecProbe { value: 0 }),
            |component| {
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::I64(component.value),
                )]))
            },
        )
        .err()
        .expect("manual required-component codec must reject");
    assert!(
        matches!(
            &required_error,
            ComponentRegistryError::PersistentComponentRequiresImplicitComponents { component_id }
                if component_id == &required_id
        ),
        "unexpected required-component error: {required_error:?}"
    );

    let hooked_id = ComponentTypeId::new("nara.test.ManualHookedCodecProbe");
    let hooked_schema = ComponentSchema::new(
        hooked_id.clone(),
        "Manual hooked codec probe",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::I64,
    )
    .with_capabilities([ComponentCapability::Scene])]);
    let hooked_error = registry
        .register_persistent_component_with_codec::<ManualHookedCodecProbe, _, _>(
            hooked_schema,
            |_| Ok(ManualHookedCodecProbe { value: 0 }),
            |component| {
                Ok(ComponentValue::map([(
                    "value",
                    ComponentValue::I64(component.value),
                )]))
            },
        )
        .err()
        .expect("manual intrinsic-hook codec must reject");
    assert!(
        matches!(
            &hooked_error,
            ComponentRegistryError::PersistentComponentHasLifecycleHook { component_id }
                if component_id == &hooked_id
        ),
        "unexpected intrinsic-hook error: {hooked_error:?}"
    );
    assert!(registry.catalog_candidate().components().is_empty());
}

#[test]
fn runtime_only_component_needs_no_persistent_provider() {
    #[derive(Resource, Default)]
    struct HookRuns(u32);

    #[derive(Component, Default)]
    struct RuntimeRequired;

    #[derive(Component)]
    #[require(RuntimeRequired)]
    #[component(on_add = runtime_only_on_add)]
    struct RuntimeOnly(u32);

    fn runtime_only_on_add(mut world: DeferredWorld<'_>, _context: HookContext) {
        world.resource_mut::<HookRuns>().0 += 1;
    }

    let mut world = World::new();
    world.init_resource::<HookRuns>();
    let entity = world.spawn(RuntimeOnly(9)).id();
    assert_eq!(
        world.get::<RuntimeOnly>(entity).map(|value| value.0),
        Some(9)
    );
    assert!(world.get::<RuntimeRequired>(entity).is_some());
    assert_eq!(world.resource::<HookRuns>().0, 1);
}

#[test]
fn inspect_only_native_binding_keeps_normal_bevy_component_semantics() {
    #[derive(Component, Default)]
    struct InspectRequired;

    #[derive(Component)]
    #[require(InspectRequired)]
    #[component(on_add = inspect_only_on_add)]
    struct InspectOnly;

    fn inspect_only_on_add(_world: DeferredWorld<'_>, _context: HookContext) {}

    let id = ComponentTypeId::new("nara.test.InspectOnly");
    let mut registry = ComponentRegistry::new();
    registry
        .register_component_schema(
            ComponentSchema::new(id.clone(), "Inspect only", ComponentSchemaVersion::ONE)
                .with_capabilities([ComponentCapability::Inspect]),
        )
        .unwrap();
    registry
        .register_native_component_with_codec::<InspectOnly, _, _>(
            &id,
            |_value| Ok(InspectOnly),
            |_component| Ok(ComponentValue::Map(Default::default())),
        )
        .unwrap();
    registry.freeze().unwrap();

    let mut world = World::new();
    let entity = world.spawn(InspectOnly).id();
    assert!(world.get::<InspectRequired>(entity).is_some());
}
