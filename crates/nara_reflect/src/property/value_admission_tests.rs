use super::*;
use crate::{
    __private::validate_component_property_value_with_admission, AssetReferenceDemand,
    ComponentNumericBound, ComponentNumericDomain, ComponentNumericInterval, ComponentShapeMember,
    ComponentTypeId,
};
use nara_core::DepthLimit;

fn path() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("root"))
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn keyed_shape() -> ComponentValueShape {
    ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("key", ComponentValueKind::String)
                .with_id(ComponentFieldId::new("identity")),
            ComponentShapeMember::new("value", ComponentValueKind::U64),
        ]),
        ComponentFieldId::new("identity"),
    )
}

fn entry(key: &str, value: u64) -> ComponentValue {
    ComponentValue::map([
        ("key", ComponentValue::String(key.into())),
        ("value", ComponentValue::U64(value)),
    ])
}

fn assert_equivalent_and_refusable(
    shape: &ComponentValueShape,
    path: &ComponentPropertyPath,
    value: &ComponentValue,
    limits: ComponentValueMaterializationLimits,
) -> Vec<(usize, usize, usize)> {
    let expected = validate_property_replacement(shape, path, value, Some(limits));
    let mut charges = Vec::new();
    let actual = validate_component_property_value_with_admission(
        shape,
        path,
        value,
        limits,
        |nodes, bytes, allocated| {
            charges.push((nodes, bytes, allocated));
            Ok::<_, ()>(())
        },
    );
    assert_eq!(
        actual,
        expected.map_err(ComponentPropertyAdmissionError::Property)
    );
    assert!(
        charges.first().unwrap().2 > 0,
        "admit the initial raw stack first"
    );

    #[derive(Debug)]
    struct Refusal<'a>(&'a str);
    let message = String::from("replacement owner refusal");
    for denied in 0..charges.len() {
        let mut calls = 0;
        let result = validate_component_property_value_with_admission(
            shape,
            path,
            value,
            limits,
            |nodes, bytes, allocated| {
                let call = calls;
                calls += 1;
                assert_eq!((nodes, bytes, allocated), charges[call]);
                if call == denied {
                    Err(Refusal(&message))
                } else {
                    Ok(())
                }
            },
        );
        let Err(ComponentPropertyAdmissionError::Admission(Refusal(returned))) = result else {
            panic!("expected exact caller refusal at boundary {denied}");
        };
        assert!(std::ptr::eq(returned, message.as_str()));
        assert_eq!(calls, denied + 1);
    }
    charges
}

#[test]
fn replacement_semantics_and_every_success_or_failure_boundary_use_the_existing_validator() {
    let record = ComponentValueShape::record_members([ComponentShapeMember::new(
        "member",
        ComponentValueKind::U64,
    )]);
    let interval = ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
        ComponentNumericInterval::U64 {
            min: None,
            max: Some(ComponentNumericBound::Included(3)),
        },
    );
    let duplicate = "duplicate\n\"\\".repeat(64);
    let cases = [
        (
            record.clone(),
            ComponentValue::map([("member", ComponentValue::U64(7))]),
        ),
        (record.clone(), ComponentValue::Map(Default::default())),
        (
            record.clone(),
            ComponentValue::map([("member", ComponentValue::Bool(true))]),
        ),
        (
            record,
            ComponentValue::map([
                ("member", ComponentValue::U64(7)),
                ("undeclared\n\"\\", ComponentValue::Null),
            ]),
        ),
        (interval.clone(), ComponentValue::U64(3)),
        (interval, ComponentValue::U64(4)),
        (
            ComponentValueShape::scalar(ComponentValueKind::U64)
                .with_numeric_domain(ComponentNumericDomain::U8),
            ComponentValue::U64(256),
        ),
        (
            ComponentValueShape::optional(ComponentValueKind::Bool.into()),
            ComponentValue::Null,
        ),
        (
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueKind::Bool.into())),
            ComponentValue::map([("name", ComponentValue::List(vec![ComponentValue::U64(1)]))]),
        ),
        (
            keyed_shape(),
            ComponentValue::List(vec![entry("a", 1), entry("b", 2)]),
        ),
        (
            keyed_shape(),
            ComponentValue::List(vec![entry(&duplicate, 1), entry(&duplicate, 2)]),
        ),
        (
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("image.png".into())),
            ]),
        ),
        (
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("invalid\npath".into())),
            ]),
        ),
    ];
    for (shape, value) in cases {
        assert_equivalent_and_refusable(
            &shape,
            &path(),
            &value,
            ComponentValueMaterializationLimits::default(),
        );
    }
}

#[test]
fn raw_preflight_stops_at_the_finite_ceiling_before_shape_work_or_unvisited_payload() {
    let shape = ComponentValueShape::opaque(ComponentValueKind::List);
    let measure = |length| {
        assert_equivalent_and_refusable(
            &shape,
            &path(),
            &ComponentValue::List(vec![ComponentValue::Null; length]),
            limits(2, 1024, 64),
        )
    };
    let charges = measure(128);
    assert_eq!(charges, measure(4096));
    assert_eq!(charges.iter().map(|charge| charge.0).sum::<usize>(), 3);
    assert_eq!(charges.last(), Some(&(1, 0, 0)));

    for (value, limits) in [
        (ComponentValue::String("payload".into()), limits(8, 1, 64)),
        (
            ComponentValue::map([(
                "long-key",
                ComponentValue::List(vec![ComponentValue::Null; 4096]),
            )]),
            limits(8, 1, 64),
        ),
        (
            ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::Null])]),
            limits(8, 1024, 2),
        ),
    ] {
        assert!(matches!(
            validate_property_replacement(&shape, &path(), &value, Some(limits)),
            Err(ComponentPropertyError::InvalidValue(_))
        ));
        assert_equivalent_and_refusable(&shape, &path(), &value, limits);
    }
}

#[test]
fn opaque_contents_are_charged_by_one_raw_walk_without_payload_copies() {
    let measure = |length| {
        let mut cost = (0, 0, 0);
        validate_component_property_value_with_admission(
            &ComponentValueShape::opaque(ComponentValueKind::List),
            &path(),
            &ComponentValue::List(vec![ComponentValue::String("value".into()); length]),
            limits(1024, 8192, 64),
            |nodes, bytes, allocated| {
                cost.0 += nodes;
                cost.1 += bytes;
                cost.2 += allocated;
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        cost
    };
    let small = measure(1);
    let large = measure(128);
    assert_eq!(large.0 - small.0, 127);
    assert_eq!(large.1 - small.1, 127 * "value".len());
    assert_eq!(
        large.2, small.2,
        "flat payload length does not grow iterator scratch"
    );
}

#[test]
fn nested_raw_iterator_growth_and_inclusive_limits_are_admitted_before_work() {
    let mut value = ComponentValue::U64(7);
    for _ in 0..12 {
        value = ComponentValue::map([("entry", ComponentValue::List(vec![value]))]);
    }
    let shape = ComponentValueShape::opaque(ComponentValueKind::Map);
    let cost = value.cost();
    let exact = limits(cost.nodes(), cost.logical_bytes(), 25);
    let charges = assert_equivalent_and_refusable(&shape, &path(), &value, exact);
    assert_eq!(
        validate_property_replacement(&shape, &path(), &value, Some(exact)),
        Ok(())
    );
    assert!(
        charges.iter().filter(|charge| charge.2 > 0).count() >= 5,
        "nested traversal admits stack growth as well as the final error envelope"
    );

    for (limited, expected) in [
        (
            limits(cost.nodes() - 1, cost.logical_bytes(), 25),
            ComponentCodecError::MaterializationNodeLimit {
                observed: cost.nodes(),
                maximum: cost.nodes() - 1,
            },
        ),
        (
            limits(cost.nodes(), cost.logical_bytes() - 1, 25),
            ComponentCodecError::MaterializationByteLimit {
                observed: cost.logical_bytes(),
                maximum: cost.logical_bytes() - 1,
            },
        ),
        (
            limits(cost.nodes(), cost.logical_bytes(), 24),
            ComponentCodecError::MaterializationDepthLimit {
                observed: 25,
                maximum: 24,
            },
        ),
    ] {
        assert_equivalent_and_refusable(&shape, &path(), &value, limited);
        assert_eq!(
            validate_component_property_value_with_admission(
                &shape,
                &path(),
                &value,
                limited,
                |_, _, _| Ok::<_, ()>(()),
            ),
            Err(ComponentPropertyAdmissionError::Property(
                ComponentPropertyError::InvalidValue(expected),
            ))
        );
    }
}

#[test]
fn declaration_scratch_is_admitted_even_when_the_replacement_has_the_wrong_kind() {
    let measure = |count| {
        let shape = ComponentValueShape::record_members((0..count).map(|index| {
            ComponentShapeMember::new(format!("wire_{index}"), ComponentValueKind::Bool)
                .with_id(ComponentFieldId::new(format!("member-{index}")))
        }));
        let charges = assert_equivalent_and_refusable(
            &shape,
            &path(),
            &ComponentValue::Bool(false),
            ComponentValueMaterializationLimits::default(),
        );
        assert!(
            charges.last().unwrap().2 > 0,
            "reserve the possible semantic error"
        );
        charges.iter().map(|charge| charge.2).sum::<usize>()
    };
    assert!(measure(32) >= measure(1) + 31 * 2 * 96);
}

#[test]
fn selected_claim_validation_needs_no_target_and_keeps_the_shared_selector_budget() {
    let shape = ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
        ComponentNumericInterval::U64 {
            min: None,
            max: Some(ComponentNumericBound::Included(3)),
        },
    );
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.PropertyValueAdmission"),
        "Property value admission",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::from_fields(["wire_root"]),
        ComponentValueShape::record_members([ComponentShapeMember::new("wire_member", shape)
            .with_id(ComponentFieldId::new("member"))]),
    )]);
    let path = path().with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
        "member",
    ))]);
    let mut budget = ComponentPropertyReadBudget::new(
        ItemLimit::new(1024).unwrap(),
        ByteLimit::new(1024).unwrap(),
    );
    let claim =
        resolve_component_property_schema_claim(&schema, schema.version(), &path, &mut budget)
            .unwrap();
    let before = budget;
    for value in [ComponentValue::U64(3), ComponentValue::U64(4)] {
        assert_equivalent_and_refusable(
            claim.shape,
            &path,
            &value,
            ComponentValueMaterializationLimits::default(),
        );
    }
    assert_eq!(budget, before);
    assert_eq!(
        validate_component_property_value_with_admission(
            claim.shape,
            &path,
            &ComponentValue::U64(4),
            ComponentValueMaterializationLimits::default(),
            |_, _, _| Ok::<_, ()>(()),
        ),
        Err(ComponentPropertyAdmissionError::Property(
            ComponentPropertyError::InvalidReplacement(crate::registry::numeric_range_error(
                &path.root,
            )),
        ))
    );
}
