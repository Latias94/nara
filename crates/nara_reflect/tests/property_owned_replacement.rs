use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentPropertyError,
    ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyStep, ComponentSchema,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, replace_component_property, validate_component_value,
};

fn id(name: &str) -> ComponentFieldId {
    ComponentFieldId::new(name)
}

fn field(name: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        id(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
}

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.OwnedPropertyReplacement"),
        "Owned property replacement",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

#[test]
fn repeated_owned_replacements_retain_the_untouched_sibling_allocation() {
    let schema = schema([
        field("blob", ComponentValueKind::String),
        field(
            "values",
            ComponentValueShape::map(ComponentValueKind::U64.into()),
        ),
    ]);
    let mut candidate = ComponentValue::map([
        ("blob", ComponentValue::String("x".repeat(64 * 1024))),
        (
            "values",
            ComponentValue::map(
                (0..32).map(|index| (format!("field_{index}"), ComponentValue::U64(0))),
            ),
        ),
    ]);
    let blob_allocation = match candidate.get("blob").unwrap() {
        ComponentValue::String(blob) => blob.as_ptr(),
        _ => unreachable!(),
    };

    for index in 0..32 {
        let path = ComponentPropertyPath::new(id("values"))
            .with_steps([ComponentPropertyStep::MapEntry(format!("field_{index}"))]);
        candidate = path
            .replace_in_owned_value(&schema, schema.version(), candidate, ComponentValue::U64(1))
            .unwrap();
        let ComponentValue::String(blob) = candidate.get("blob").unwrap() else {
            unreachable!();
        };
        // A whole-value clone cannot retain this allocation while the old value is live.
        assert_eq!(blob.as_ptr(), blob_allocation);
    }
    let ComponentValue::Map(values) = candidate.get("values").unwrap() else {
        unreachable!();
    };
    assert!(
        values
            .values()
            .all(|value| value == &ComponentValue::U64(1))
    );
}

fn keyed_schema() -> ComponentSchema {
    schema([field(
        "rows",
        ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("wire_id", ComponentValueKind::String).with_id(id("id")),
                ComponentShapeMember::new("wire_score", ComponentValueKind::U64)
                    .with_id(id("score")),
            ]),
            id("id"),
        ),
    )])
}

fn rows() -> ComponentValue {
    ComponentValue::map([(
        "rows",
        ComponentValue::List(vec![
            ComponentValue::map([
                ("wire_id", ComponentValue::String("a".into())),
                ("wire_score", ComponentValue::U64(1)),
            ]),
            ComponentValue::map([
                ("wire_id", ComponentValue::String("b".into())),
                ("wire_score", ComponentValue::U64(2)),
            ]),
        ]),
    )])
}

fn selected(member: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(id("rows")).with_steps([
        ComponentPropertyStep::ListItem {
            by: id("id"),
            key: ComponentPropertyKey::String("a".into()),
        },
        ComponentPropertyStep::Member(id(member)),
    ])
}

#[test]
fn owned_and_atomic_replacements_share_nested_keyed_selection() {
    let schema = keyed_schema();
    let original = rows();
    let mut atomic = original.clone();
    let path = selected("score");
    let previous = replace_component_property(
        &schema,
        schema.version(),
        &mut atomic,
        &path,
        ComponentValue::U64(7),
    )
    .unwrap();
    let owned = path
        .replace_in_owned_value(&schema, schema.version(), original, ComponentValue::U64(7))
        .unwrap();
    assert_eq!(previous, ComponentValue::U64(1));
    assert_eq!(owned, atomic);
}

#[test]
fn a_duplicate_key_keeps_atomic_input_unchanged_and_requires_owned_owner_validation() {
    let schema = keyed_schema();
    let original = rows();
    let mut atomic = original.clone();
    let path = selected("id");
    let error = replace_component_property(
        &schema,
        schema.version(),
        &mut atomic,
        &path,
        ComponentValue::String("b".into()),
    )
    .unwrap_err();
    assert!(matches!(
        &error,
        ComponentPropertyError::InvalidReplacement(_)
    ));
    assert_eq!(atomic, original);
    let candidate = path
        .replace_in_owned_value(
            &schema,
            schema.version(),
            original,
            ComponentValue::String("b".into()),
        )
        .unwrap();
    assert_eq!(
        ComponentPropertyError::InvalidReplacement(
            validate_component_value(&schema, &candidate).unwrap_err(),
        ),
        error,
    );
}
