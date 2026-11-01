use nara_asset::AssetServer;
use nara_ecs::World;
use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentNumericDomain, ComponentRegistryBuilder,
    ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, validate_component_value,
};

use crate::{
    UI_SCHEMA_OWNER_ID, UI_SCHEMA_PROVIDER, UiButton, UiButtonVisuals, UiColorRole, UiNode,
    UiStyle, UiText, UiVisual,
};

const TEXT_ID: &str = "nara.ui.UiText";
const CREATION_IDS: [&str; 5] = [
    "nara.ui.UiNode",
    "nara.ui.UiVisual",
    TEXT_ID,
    "nara.ui.UiButton",
    "nara.ui.UiButtonVisuals",
];

fn text_value(logical_size: f64) -> ComponentValue {
    let mut value = crate::codec::encode_text(&UiText::new("Text", 16.0).unwrap()).unwrap();
    value
        .set_path(
            &ComponentFieldPath::from_fields(["logical_size"]),
            ComponentValue::f64(logical_size).unwrap(),
        )
        .unwrap();
    value
}

fn ui_registry() -> ComponentRegistrySnapshot {
    let mut registry = ComponentRegistryBuilder::new();
    UI_SCHEMA_PROVIDER.register_into(&mut registry).unwrap();
    registry.freeze().unwrap()
}

#[test]
fn schema_provider_admits_the_version_one_native_baseline() {
    assert_eq!(
        UI_SCHEMA_PROVIDER.binding(),
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.ui.components.native", 1)
    );
    let mut catalog = crate::codec::ui_schema_catalog();
    catalog.canonicalize();
    let mut registry = ComponentRegistryBuilder::new();
    UI_SCHEMA_PROVIDER.register_into(&mut registry).unwrap();
    let registry = registry.freeze().unwrap();
    let owner = registry.owner_receipt(UI_SCHEMA_OWNER_ID).unwrap();
    assert_eq!(owner.generation(), 1);
    assert_eq!(owner.predecessor(), None);
    assert_eq!(owner.catalog(), catalog.fingerprint());
    assert_eq!(
        registry.provider_receipt(UI_SCHEMA_PROVIDER.id()),
        Some(UI_SCHEMA_PROVIDER.receipt())
    );
    for schema in &catalog.components {
        assert_eq!(registry.schema(schema.id()), Some(schema));
        assert_eq!(
            registry.component_schema_owner(schema.id()),
            Some(UI_SCHEMA_OWNER_ID)
        );
        assert_eq!(
            crate::codec::ui_component_schema_version(schema.id()),
            Some(ComponentSchemaVersion::ONE)
        );
    }
    let logical_size = registry
        .schema(&ComponentTypeId::new(TEXT_ID))
        .unwrap()
        .fields()
        .iter()
        .find(|field| field.id() == &ComponentFieldId::new("logical_size"))
        .unwrap();
    assert_eq!(logical_size.value_kind(), ComponentValueKind::F64);
    assert_eq!(
        logical_size.numeric_domain(),
        Some(ComponentNumericDomain::F32)
    );
}

#[test]
fn shared_text_size_rules_match_native_f32_materialization_at_both_boundaries() {
    let registry = ui_registry();
    let text_id = ComponentTypeId::new(TEXT_ID);
    let schema = registry.schema(&text_id).unwrap();
    let above_limit = f64::from(f32::from_bits(256.0_f32.to_bits() + 1));
    let rounds_to_limit = 256.0 + (above_limit - 256.0) / 4.0;
    for (authored_size, expected) in [
        (-1.0, false),
        (-0.0, false),
        (0.0, false),
        (f64::from_bits(1), false),
        (f64::from(f32::from_bits(1)), true),
        (f64::from(f32::MIN_POSITIVE), true),
        (16.125, true),
        (256.0, true),
        (rounds_to_limit, true),
        (above_limit, false),
        (f64::from(f32::MAX), false),
        (f64::MAX, false),
    ] {
        let value = text_value(authored_size);
        assert_eq!(UiText::new("Text", authored_size as f32).is_ok(), expected);
        assert_eq!(
            validate_component_value(schema, &value).is_ok(),
            expected,
            "{authored_size:?}"
        );
        assert_eq!(
            registry.preflight_component(&text_id, &value).is_ok(),
            expected,
            "{authored_size:?}"
        );
        assert_eq!(
            value.field_f64("logical_size").unwrap().to_bits(),
            authored_size.to_bits()
        );
    }
}

#[test]
fn native_text_materialization_rounds_without_mutating_authored_values() {
    let registry = ui_registry();
    let text_id = ComponentTypeId::new(TEXT_ID);
    for authored_size in [f64::from(f32::from_bits(1)), 16.1, 256.0] {
        let value = text_value(authored_size);
        let before = value.clone();
        let prepared = registry.preflight_component(&text_id, &value).unwrap();
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        prepared.apply(&mut world, entity).unwrap();
        assert_eq!(
            world
                .get::<UiText>(entity)
                .unwrap()
                .kernel()
                .logical_size()
                .to_bits(),
            (authored_size as f32).to_bits()
        );
        let encoded = registry
            .encode_component(&text_id, &world, entity)
            .unwrap()
            .unwrap();
        assert_eq!(
            encoded.field_f64("logical_size").unwrap().to_bits(),
            f64::from(authored_size as f32).to_bits()
        );
        assert_eq!(value, before);
    }
}

#[test]
fn creation_templates_are_complete_under_the_actual_ui_schema_provider() {
    let registry = ui_registry();
    UI_SCHEMA_PROVIDER.validate_snapshot(&registry).unwrap();
    for id in CREATION_IDS {
        let id = ComponentTypeId::new(id);
        let schema = registry.schema(&id).unwrap();
        let initial = schema.authoring_initial_value().unwrap();
        assert_eq!(schema.version(), ComponentSchemaVersion::ONE);
        assert_eq!(
            registry.component_schema_owner(&id),
            Some(UI_SCHEMA_OWNER_ID)
        );
        validate_component_value(schema, initial).unwrap();
        for field in schema.fields() {
            assert!(
                initial.get_path(field.path()).is_ok(),
                "{id}: {}",
                field.id()
            );
        }
    }
    for (id, path) in [
        ("nara.ui.UiVisual", "background.image"),
        ("nara.ui.UiVisual", "border_brush.image"),
        (TEXT_ID, "brush.image"),
    ] {
        let id = ComponentTypeId::new(id);
        let schema = registry.schema(&id).unwrap();
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == &ComponentFieldId::new(path))
            .unwrap();
        assert!(!field.is_required());
        assert_eq!(field.value_kind(), ComponentValueKind::AssetRef);
        assert_eq!(
            schema
                .authoring_initial_value()
                .unwrap()
                .get_path(field.path())
                .unwrap(),
            &ComponentValue::Null
        );
    }
    for id in ["nara.ui.UiRoot", "nara.ui.UiTheme", "nara.ui.UiSemantics"] {
        assert!(
            registry
                .schema(&ComponentTypeId::new(id))
                .unwrap()
                .authoring_initial_value()
                .is_none()
        );
    }
}

#[test]
fn copied_creation_templates_do_not_mutate_the_admitted_ui_schemas() {
    let registry = ui_registry();
    for (id, path, replacement) in [
        (
            "nara.ui.UiNode",
            "style.gap",
            ComponentValue::f64(8.0).unwrap(),
        ),
        (
            "nara.ui.UiVisual",
            "background.role",
            ComponentValue::String("accent".to_owned()),
        ),
        (
            TEXT_ID,
            "content",
            ComponentValue::String("Changed text".to_owned()),
        ),
        (
            "nara.ui.UiButton",
            "label",
            ComponentValue::String("Changed button".to_owned()),
        ),
        (
            "nara.ui.UiButtonVisuals",
            "focus_border_width",
            ComponentValue::f64(4.0).unwrap(),
        ),
    ] {
        let id = ComponentTypeId::new(id);
        let schema = registry.schema(&id).unwrap();
        let before = schema.clone();
        let mut copied = schema.authoring_initial_value().unwrap().clone();
        copied
            .set_path(
                &ComponentFieldPath::from_fields(path.split('.')),
                replacement,
            )
            .unwrap();
        assert_ne!(Some(&copied), schema.authoring_initial_value());
        assert_eq!(schema, &before);
    }
    UI_SCHEMA_PROVIDER.validate_snapshot(&registry).unwrap();
}

#[test]
fn creation_templates_materialize_and_roundtrip_without_an_asset_server() {
    let registry = ui_registry();
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    assert!(!world.contains_resource::<AssetServer>());
    for id in CREATION_IDS {
        let id = ComponentTypeId::new(id);
        let initial = registry
            .schema(&id)
            .unwrap()
            .authoring_initial_value()
            .unwrap();
        registry
            .preflight_component(&id, initial)
            .unwrap()
            .apply(&mut world, entity)
            .unwrap();
        let encoded = registry.encode_component(&id, &world, entity).unwrap();
        assert_eq!(encoded.as_ref(), Some(initial));
    }
    assert_eq!(
        world.get::<UiNode>(entity).unwrap().style(),
        UiStyle::default()
    );
    assert_eq!(
        world.get::<UiVisual>(entity),
        Some(&UiVisual::role(UiColorRole::Panel))
    );
    assert_eq!(
        world.get::<UiText>(entity),
        Some(&UiText::new("Text", 16.0).unwrap())
    );
    let button = world.get::<UiButton>(entity).unwrap();
    assert_eq!(button.label(), "Button");
    assert_eq!(button.action().as_str(), "activate");
    assert!(button.enabled());
    assert_eq!(button.logical_text_size(), 16.0);
    assert_eq!(
        world.get::<UiButtonVisuals>(entity),
        Some(&UiButtonVisuals::default())
    );
    assert!(!world.contains_resource::<AssetServer>());
}

#[test]
fn creation_templates_do_not_default_missing_required_ui_fields() {
    let registry = ui_registry();
    for id in CREATION_IDS {
        let id = ComponentTypeId::new(id);
        let schema = registry.schema(&id).unwrap();
        for field in schema.fields().iter().filter(|field| field.is_required()) {
            let mut incomplete = schema.authoring_initial_value().unwrap().clone();
            incomplete.remove_path(field.path()).unwrap();
            assert!(
                validate_component_value(schema, &incomplete).is_err(),
                "{id}: {}",
                field.id()
            );
            assert!(
                registry.preflight_component(&id, &incomplete).is_err(),
                "{id}: {}",
                field.id()
            );
        }
    }
}

#[test]
fn creation_templates_preserve_native_rejection_of_invalid_ui_values() {
    let registry = ui_registry();
    for (id, path, replacement) in [
        (
            "nara.ui.UiNode",
            "style.gap",
            ComponentValue::f64(-1.0).unwrap(),
        ),
        (
            "nara.ui.UiVisual",
            "background.role",
            ComponentValue::String("unknown".to_owned()),
        ),
        (
            TEXT_ID,
            "alignment",
            ComponentValue::String("sideways".to_owned()),
        ),
        (
            "nara.ui.UiButton",
            "action",
            ComponentValue::String(String::new()),
        ),
        (
            "nara.ui.UiButtonVisuals",
            "focus_border_width",
            ComponentValue::f64(0.0).unwrap(),
        ),
    ] {
        let id = ComponentTypeId::new(id);
        let schema = registry.schema(&id).unwrap();
        let mut invalid = schema.authoring_initial_value().unwrap().clone();
        invalid
            .set_path(
                &ComponentFieldPath::from_fields(path.split('.')),
                replacement,
            )
            .unwrap();
        validate_component_value(schema, &invalid).unwrap();
        assert!(
            crate::codec::validate_ui_component_value(&id, &invalid).is_err(),
            "{id}"
        );
        assert!(registry.preflight_component(&id, &invalid).is_err(), "{id}");
    }
}
