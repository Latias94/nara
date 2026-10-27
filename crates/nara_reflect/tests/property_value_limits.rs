use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaVersion, ComponentShapeErrorKind, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueShape, replace_component_property, resolve_component_property,
    resolve_component_property_with_limits, validate_component_value,
};

fn field(name: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
}

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.PropertyValueLimits"),
        "Property value limits",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn large_fixture() -> (ComponentSchema, ComponentValue, ComponentPropertyPath) {
    let schema = schema([
        field(
            "first",
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        ),
        field(
            "second",
            ComponentValueShape::list(ComponentValueKind::U64.into()),
        ),
        field("x", ComponentValueKind::U64),
    ]);
    let value = ComponentValue::map([
        (
            "first",
            ComponentValue::List(vec![ComponentValue::U64(0); 9_000]),
        ),
        (
            "second",
            ComponentValue::List(vec![ComponentValue::U64(1); 9_000]),
        ),
        ("x", ComponentValue::U64(2)),
    ]);
    let path = ComponentPropertyPath::new(ComponentFieldId::new("x"));
    (schema, value, path)
}

#[test]
fn explicit_property_limits_admit_a_large_value_without_weakening_default_admission() {
    let (schema, value, path) = large_fixture();
    validate_component_value(&schema, &value).unwrap();
    let is_default_budget = |error| {
        matches!(
            error,
            ComponentPropertyError::ValueBudget(error)
                if error.kind == ComponentShapeErrorKind::NodeLimit
        )
    };
    assert!(is_default_budget(
        resolve_component_property(&schema, schema.version(), &value, &path).unwrap_err()
    ));
    assert!(is_default_budget(
        path.replace_in_owned_value(
            &schema,
            schema.version(),
            value.clone(),
            ComponentValue::U64(7),
        )
        .unwrap_err()
    ));
    let mut arbitrary = value.clone();
    assert!(is_default_budget(
        replace_component_property(
            &schema,
            schema.version(),
            &mut arbitrary,
            &path,
            ComponentValue::U64(7),
        )
        .unwrap_err()
    ));
    assert_eq!(arbitrary, value);

    let cost = value.cost();
    let exact = limits(cost.nodes(), cost.logical_bytes(), 3);
    let resolved =
        resolve_component_property_with_limits(&schema, schema.version(), &value, &path, exact)
            .unwrap();
    assert!(std::ptr::eq(resolved.value, value.get("x").unwrap()));
    let ComponentValue::List(first) = value.get("first").unwrap() else {
        unreachable!()
    };
    let allocation = first.as_ptr();
    let candidate = path
        .replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            value,
            ComponentValue::U64(7),
            exact,
        )
        .unwrap();
    let ComponentValue::List(first) = candidate.get("first").unwrap() else {
        unreachable!()
    };
    assert_eq!(first.as_ptr(), allocation);
    assert_eq!(candidate.get("x"), Some(&ComponentValue::U64(7)));
    validate_component_value(&schema, &candidate).unwrap();
}

#[test]
fn explicit_property_limits_enforce_nodes_bytes_and_depth_even_without_defaults() {
    let (schema, value, path) = large_fixture();
    let cost = value.cost();
    for (bound, expected) in [
        (limits(cost.nodes() - 1, cost.logical_bytes(), 3), "nodes"),
        (limits(cost.nodes(), cost.logical_bytes() - 1, 3), "bytes"),
        (limits(cost.nodes(), cost.logical_bytes(), 2), "depth"),
    ] {
        let resolve =
            resolve_component_property_with_limits(&schema, schema.version(), &value, &path, bound)
                .unwrap_err();
        let replace = path
            .replace_in_owned_value_with_limits(
                &schema,
                schema.version(),
                value.clone(),
                ComponentValue::U64(7),
                bound,
            )
            .unwrap_err();
        for error in [resolve, replace] {
            let ComponentPropertyError::InvalidValue(error) = error else {
                panic!("explicit value admission must retain its typed failure: {error:?}")
            };
            assert!(match expected {
                "nodes" => matches!(error, ComponentCodecError::MaterializationNodeLimit { .. }),
                "bytes" => matches!(error, ComponentCodecError::MaterializationByteLimit { .. }),
                "depth" => matches!(error, ComponentCodecError::MaterializationDepthLimit { .. }),
                _ => unreachable!(),
            });
        }
    }
}

#[test]
fn explicit_owned_replacement_checks_result_growth_with_the_same_value_budget() {
    let schema = schema([
        field("x", ComponentValueKind::String),
        field("sibling", ComponentValueKind::String),
    ]);
    let original = ComponentValue::map([
        ("x", ComponentValue::String("a".into())),
        ("sibling", ComponentValue::String("b".into())),
    ]);
    let path = ComponentPropertyPath::new(ComponentFieldId::new("x"));
    let cost = original.cost();
    let exact = limits(cost.nodes(), cost.logical_bytes(), 2);
    let error = path
        .replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            original.clone(),
            ComponentValue::String("ab".into()),
            exact,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationByteLimit {
            observed, maximum,
        }) if observed == cost.logical_bytes() + 1 && maximum == cost.logical_bytes()
    ));
    let admitted = path
        .replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            original,
            ComponentValue::String("ab".into()),
            limits(cost.nodes(), cost.logical_bytes() + 1, 2),
        )
        .unwrap();
    assert_eq!(
        admitted.get("x"),
        Some(&ComponentValue::String("ab".into()))
    );
}

#[test]
fn explicit_property_admission_does_not_turn_invalid_selectors_or_values_into_budget_errors() {
    let (schema, value, _) = large_fixture();
    let bound = limits(20_000, 256 * 1024, 64);
    let unknown = ComponentPropertyPath::new(ComponentFieldId::new("missing"));
    assert!(matches!(
        resolve_component_property_with_limits(&schema, schema.version(), &value, &unknown, bound),
        Err(ComponentPropertyError::UnknownRoot(_))
    ));
    let path = ComponentPropertyPath::new(ComponentFieldId::new("x"));
    assert!(matches!(
        path.replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            value.clone(),
            ComponentValue::Bool(false),
            bound,
        ),
        Err(ComponentPropertyError::InvalidReplacementShape(_))
    ));
    let mut invalid = value;
    invalid
        .set_path(
            &ComponentFieldPath::from_fields(["x"]),
            ComponentValue::Bool(false),
        )
        .unwrap();
    assert!(matches!(
        resolve_component_property_with_limits(&schema, schema.version(), &invalid, &path, bound),
        Err(ComponentPropertyError::InvalidValue(
            ComponentCodecError::InvalidField { .. }
        ))
    ));
}

#[test]
fn explicit_owned_replacement_shares_keyed_selection_and_defers_complete_candidate_semantics() {
    let schema = schema([field(
        "rows",
        ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("wire_id", ComponentValueKind::String)
                    .with_id(ComponentFieldId::new("id")),
                ComponentShapeMember::new("wire_score", ComponentValueKind::U64)
                    .with_id(ComponentFieldId::new("score")),
            ]),
            ComponentFieldId::new("id"),
        ),
    )]);
    let value = ComponentValue::map([(
        "rows",
        ComponentValue::List(
            [("a", 1), ("b", 2)]
                .into_iter()
                .map(|(key, score)| {
                    ComponentValue::map([
                        ("wire_id", ComponentValue::String(key.into())),
                        ("wire_score", ComponentValue::U64(score)),
                    ])
                })
                .collect(),
        ),
    )]);
    let selected = |member: &str| {
        ComponentPropertyPath::new(ComponentFieldId::new("rows")).with_steps([
            ComponentPropertyStep::ListItem {
                by: ComponentFieldId::new("id"),
                key: ComponentPropertyKey::String("a".into()),
            },
            ComponentPropertyStep::Member(ComponentFieldId::new(member)),
        ])
    };
    let bound = limits(128, 4096, 64);
    let score = selected("score");
    assert_eq!(
        resolve_component_property_with_limits(&schema, schema.version(), &value, &score, bound)
            .unwrap()
            .value,
        &ComponentValue::U64(1)
    );
    let mut atomic = value.clone();
    replace_component_property(
        &schema,
        schema.version(),
        &mut atomic,
        &score,
        ComponentValue::U64(7),
    )
    .unwrap();
    let owned = score
        .replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            value.clone(),
            ComponentValue::U64(7),
            bound,
        )
        .unwrap();
    assert_eq!(owned, atomic);
    let duplicate = selected("id")
        .replace_in_owned_value_with_limits(
            &schema,
            schema.version(),
            value,
            ComponentValue::String("b".into()),
            bound,
        )
        .unwrap();
    assert!(validate_component_value(&schema, &duplicate).is_err());
}
