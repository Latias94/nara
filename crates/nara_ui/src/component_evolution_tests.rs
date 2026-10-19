use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentMigrationError, ComponentNumericDomain,
    ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueShape, validate_component_value,
};

use crate::{UI_SCHEMA_PROVIDER, UiText, register_ui_components};

const TEXT_ID: &str = "nara.ui.UiText";
const TEXT_VERSION_TWO: ComponentSchemaVersion = ComponentSchemaVersion(2);
const TEXT_VERSION_THREE: ComponentSchemaVersion = ComponentSchemaVersion(3);

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

fn text_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_ui_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

#[test]
fn text_size_successor_preserves_the_generation_two_catalog_and_other_components() {
    let predecessor = crate::codec::ui_schema_predecessor_catalog();
    let current = crate::codec::ui_schema_catalog();
    assert_eq!(predecessor.generation(), 2);
    // Frozen from the shipped generation-two authoring bundle before this successor.
    assert_eq!(
        predecessor.fingerprint().to_hex(),
        "d7468da88c3ad3ca5e92d1a02e00e2a3d091037773abcec0bb552f19da626238"
    );
    assert!(predecessor.predecessor().is_some());
    assert_eq!(current.generation(), 3);
    assert_eq!(current.predecessor(), Some(&predecessor.fingerprint()));
    assert_ne!(current.fingerprint(), predecessor.fingerprint());

    for previous in &predecessor.components {
        let successor = current
            .components
            .iter()
            .find(|schema| schema.id() == previous.id())
            .unwrap();
        if previous.id().as_str() != TEXT_ID {
            assert_eq!(successor, previous);
            continue;
        }
        assert_eq!(previous.version(), TEXT_VERSION_TWO);
        assert_eq!(successor.version(), TEXT_VERSION_THREE);
        assert_eq!(successor.field_tombstones(), previous.field_tombstones());
        let previous_size = previous
            .fields()
            .iter()
            .find(|field| field.id() == &ComponentFieldId::new("logical_size"))
            .unwrap();
        let successor_size = successor
            .fields()
            .iter()
            .find(|field| field.id() == previous_size.id())
            .unwrap();
        assert_eq!(
            previous_size.shape(),
            &ComponentValueShape::scalar(nara_reflect::ComponentValueKind::F64)
        );
        assert_eq!(
            successor_size.numeric_domain(),
            Some(ComponentNumericDomain::F32)
        );
        for previous_field in previous.fields() {
            if previous_field.id() != previous_size.id() {
                assert!(successor.fields().contains(previous_field));
            }
        }
    }

    // Exercise the actual owner admission as well as standalone native registration.
    let mut registry = ComponentRegistry::new();
    UI_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .unwrap();
    registry.freeze().unwrap();
    assert_eq!(
        registry
            .schema(&ComponentTypeId::new(TEXT_ID))
            .unwrap()
            .version(),
        TEXT_VERSION_THREE
    );
}

#[test]
fn shared_text_size_rules_match_native_f32_materialization_at_both_boundaries() {
    let registry = text_registry();
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
            registry
                .preflight_component(&text_id, &value)
                .unwrap()
                .is_ok(),
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
fn text_size_migration_preserves_valid_stored_values_and_rejects_invalid_history() {
    let registry = text_registry();
    let text_id = ComponentTypeId::new(TEXT_ID);
    for authored_size in [f64::from(f32::from_bits(1)), 16.1, 256.0] {
        let value = text_value(authored_size);
        let migrated = registry
            .migrate_component_value(&text_id, TEXT_VERSION_TWO, &value)
            .unwrap();
        assert_eq!(migrated.version, TEXT_VERSION_THREE);
        assert_eq!(migrated.value, value);
        assert_eq!(
            migrated.value.field_f64("logical_size").unwrap().to_bits(),
            authored_size.to_bits()
        );
    }

    for authored_size in [-1.0, -0.0, 0.0, f64::from_bits(1), 257.0, f64::MAX] {
        let value = text_value(authored_size);
        let before = value.clone();
        assert!(matches!(
            registry.migrate_component_value(&text_id, TEXT_VERSION_TWO, &value),
            Err(ComponentMigrationError::MigrationFailed {
                component_id,
                from_version: TEXT_VERSION_TWO,
                to_version: TEXT_VERSION_THREE,
                ..
            }) if component_id == text_id
        ));
        assert_eq!(value, before);
    }
}
