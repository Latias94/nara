use nara_ecs::{Component, World};
use nara_identity::{EntityReference, SceneEntityId};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentRegistry,
    ComponentRegistryError, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PersistentComponent, PersistentComponentProvider,
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
fn runtime_only_component_needs_no_persistent_provider() {
    #[derive(Component)]
    struct RuntimeOnly(u32);

    let mut world = World::new();
    let entity = world.spawn(RuntimeOnly(9)).id();
    assert_eq!(
        world.get::<RuntimeOnly>(entity).map(|value| value.0),
        Some(9)
    );
}
