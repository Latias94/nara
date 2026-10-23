use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::{Component, World};
use nara_identity::{EntityReference, SceneEntityId};
use nara_reflect::{
    ComponentCapability, ComponentFieldPath, ComponentNumericDomain, ComponentRegistry,
    ComponentRegistryError, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, PersistentComponent, PersistentComponentProvider,
    validate_component_value,
};

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.NestedReferenceProbe",
    version = 1,
    alias = "Nested reference probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct NestedReferenceProbe {
    #[nara(
        id = "targets",
        alias = "Targets",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    targets: Vec<Option<EntityReference>>,
    #[nara(
        id = "assets",
        alias = "Assets",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    assets: BTreeMap<String, Vec<Option<AssetRef>>>,
    #[nara(id = "weights", alias = "Weights")]
    weights: Vec<u8>,
    #[nara(id = "fallback", alias = "Fallback")]
    fallback: Option<i64>,
}

fn nested_probe() -> NestedReferenceProbe {
    NestedReferenceProbe {
        targets: vec![
            Some(EntityReference::SceneLocal {
                entity: SceneEntityId::new("enemy").unwrap(),
            }),
            None,
        ],
        assets: BTreeMap::from([(
            "portraits".to_owned(),
            vec![
                Some(AssetRef::path("textures/player.png").unwrap()),
                None,
                Some(AssetRef::stable_id("3c2d4005-63bd-4e2a-9792-bb2f48a12821").unwrap()),
            ],
        )]),
        weights: vec![0, u8::MAX],
        fallback: None,
    }
}

#[test]
fn generated_nested_reference_codecs_round_trip_through_native_registry() {
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component::<NestedReferenceProbe>()
        .unwrap();
    registry.freeze().unwrap();
    let snapshot = registry.snapshot().unwrap();
    let component_id = ComponentTypeId::new("nara.test.NestedReferenceProbe");
    let schema = registry.schema(&component_id).unwrap();
    assert!(schema.fields().iter().all(|field| field.is_required()));
    assert_eq!(
        schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == "targets")
            .unwrap()
            .shape(),
        &ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::EntityRef
        ))
    );
    assert_eq!(
        schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == "assets")
            .unwrap()
            .shape(),
        &ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::AssetRef
        )))
    );
    assert_eq!(
        schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == "weights")
            .unwrap()
            .shape(),
        &ComponentValueShape::list(
            ComponentValueShape::scalar(ComponentValueKind::U64)
                .with_numeric_domain(ComponentNumericDomain::U8)
        )
    );

    let expected = nested_probe();
    let mut source = World::new();
    let source_entity = source.spawn(expected.clone()).id();
    let encoded = snapshot
        .encode_component(&component_id, &source, source_entity)
        .unwrap()
        .unwrap();
    assert_eq!(encoded.field("fallback").unwrap(), &ComponentValue::Null);
    assert_eq!(
        encoded.field("targets").unwrap(),
        &ComponentValue::List(vec![
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: SceneEntityId::new("enemy").unwrap(),
            }),
            ComponentValue::Null,
        ])
    );
    validate_component_value(schema, &encoded).unwrap();

    let mut target = World::new();
    let target_entity = target.spawn_empty().id();
    snapshot
        .preflight_component(&component_id, &encoded)
        .unwrap()
        .apply(&mut target, target_entity)
        .unwrap();
    assert_eq!(
        target.get::<NestedReferenceProbe>(target_entity),
        Some(&expected)
    );
}

#[test]
fn generated_nested_codecs_keep_required_presence_and_leaf_validation() {
    let schema = NestedReferenceProbe::persistent_component_schema();
    let empty = NestedReferenceProbe {
        targets: Vec::new(),
        assets: BTreeMap::new(),
        weights: Vec::new(),
        fallback: Some(7),
    };
    let empty_encoded = empty.__encode_persistent_component().unwrap();
    assert_eq!(
        empty_encoded.field("fallback").unwrap(),
        &ComponentValue::I64(7)
    );
    assert_eq!(
        empty_encoded.field("targets").unwrap(),
        &ComponentValue::List(Vec::new())
    );
    assert_eq!(
        empty_encoded.field("assets").unwrap(),
        &ComponentValue::Map(BTreeMap::new())
    );
    validate_component_value(&schema, &empty_encoded).unwrap();
    assert_eq!(
        NestedReferenceProbe::__decode_persistent_component(&empty_encoded).unwrap(),
        empty
    );
    let original = nested_probe().__encode_persistent_component().unwrap();
    let ComponentValue::Map(mut missing) = original.clone() else {
        panic!("generated component must encode a map");
    };
    missing.remove("fallback");
    let missing = ComponentValue::Map(missing);
    assert!(validate_component_value(&schema, &missing).is_err());
    assert!(NestedReferenceProbe::__decode_persistent_component(&missing).is_err());

    for (field, invalid) in [
        (
            "weights",
            ComponentValue::List(vec![ComponentValue::U64(256)]),
        ),
        (
            "targets",
            ComponentValue::List(vec![ComponentValue::Bool(true)]),
        ),
        ("assets", ComponentValue::List(Vec::new())),
        ("fallback", ComponentValue::String("invalid".to_owned())),
    ] {
        let mut candidate = original.clone();
        candidate
            .replace_path(&ComponentFieldPath::from_fields([field]), invalid)
            .unwrap();
        assert!(
            validate_component_value(&schema, &candidate).is_err(),
            "schema accepted invalid nested field {field}"
        );
        assert!(
            NestedReferenceProbe::__decode_persistent_component(&candidate).is_err(),
            "native codec accepted invalid nested field {field}"
        );
    }
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "nara.test.AmbiguousOptionalProbe",
    version = 1,
    alias = "Ambiguous optional probe",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct AmbiguousOptionalProbe {
    #[nara(id = "value", alias = "Value")]
    value: Option<Option<i64>>,
}

#[test]
fn generated_ambiguous_optional_shape_cannot_admit_a_registry() {
    let mut registry = ComponentRegistry::new();
    assert!(
        registry
            .register_persistent_component::<AmbiguousOptionalProbe>()
            .and_then(ComponentRegistry::freeze)
            .is_err()
    );
    assert!(!registry.is_frozen());
}

#[test]
fn generated_nested_reference_markers_are_checked_against_actual_shape() {
    for field_id in ["targets", "assets", "weights"] {
        let mut schema = NestedReferenceProbe::persistent_component_schema();
        let field = schema
            .fields
            .iter_mut()
            .find(|field| field.id().as_str() == field_id)
            .unwrap();
        match field_id {
            "targets" => {
                field.capabilities.remove(&ComponentCapability::EntityRef);
            }
            "assets" => {
                field.capabilities.remove(&ComponentCapability::AssetRef);
            }
            _ => {
                field.capabilities.insert(ComponentCapability::EntityRef);
            }
        }
        let mut registry = ComponentRegistry::new();
        let failure = registry
            .register_component_schema(schema)
            .and_then(ComponentRegistry::freeze)
            .err()
            .expect("reference markers must match the generated recursive shape");
        assert!(matches!(
            failure,
            ComponentRegistryError::InvalidFieldCapability { .. }
        ));
    }
}

#[derive(Component, PersistentComponent, Debug, PartialEq)]
#[nara(
    id = "nara.test.NestedMath",
    version = 1,
    alias = "Nested math",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct NestedMath {
    #[nara(id = "vectors", alias = "Vectors")]
    vectors: BTreeMap<String, Vec<Option<Vec2>>>,
}

#[test]
fn generated_nested_math_composes_closed_axes_through_maps_lists_and_optional() {
    let expected = NestedMath {
        vectors: BTreeMap::from([("route".into(), vec![Some(Vec2::new(0.5, -2.0)), None])]),
    };
    let schema = NestedMath::persistent_component_schema();
    let value = expected.__encode_persistent_component().unwrap();
    validate_component_value(&schema, &value).unwrap();
    assert_eq!(
        NestedMath::__decode_persistent_component(&value).unwrap(),
        expected
    );
    let ComponentValueShape::Map { value: list } = schema.fields()[0].shape() else {
        panic!("the stable root has a typed map");
    };
    let ComponentValueShape::List { item } = list.as_ref() else {
        panic!("map values contain lists");
    };
    let ComponentValueShape::Optional { value: vector } = item.as_ref() else {
        panic!("list items retain explicit Null");
    };
    let ComponentValueShape::Record { members } = vector.as_ref() else {
        panic!("Vec2 remains closed inside containers");
    };
    assert_eq!(
        members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<Vec<_>>(),
        ["x", "y"]
    );
    assert!(
        members
            .iter()
            .all(|member| member.shape.numeric_domain() == Some(ComponentNumericDomain::F32))
    );

    let invalid = ComponentValue::map([(
        "vectors",
        ComponentValue::map([(
            "route",
            ComponentValue::List(vec![ComponentValue::map([
                ("x", ComponentValue::f64(0.0).unwrap()),
                ("y", ComponentValue::f64(0.0).unwrap()),
                ("z", ComponentValue::f64(0.0).unwrap()),
            ])]),
        )]),
    )]);
    assert!(validate_component_value(&schema, &invalid).is_err());
    assert!(NestedMath::__decode_persistent_component(&invalid).is_err());
}
