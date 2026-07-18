use nara_ecs::{Component, Resource, World, lifecycle::HookContext, world::DeferredWorld};
use nara_identity::{EntityReference, SceneEntityId};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponent, PersistentComponentProvider,
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
