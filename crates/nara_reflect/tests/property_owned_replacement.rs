use std::mem::size_of;

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    __private::ComponentPropertyAdmissionError, ComponentCodecError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyError, ComponentPropertyKey,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueShape, replace_component_property,
    resolve_component_property, validate_component_value,
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

fn value_limits(nodes: usize, bytes: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(3).unwrap(),
    )
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

fn keyed_rows_shape(key_kind: ComponentValueKind) -> ComponentValueShape {
    ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("wire_id", key_kind).with_id(id("id")),
            ComponentShapeMember::new("wire_score", ComponentValueKind::U64).with_id(id("score")),
        ]),
        id("id"),
    )
}

fn keyed_schema() -> ComponentSchema {
    schema([field("rows", keyed_rows_shape(ComponentValueKind::String))])
}

fn row(key: ComponentValue, score: u64) -> ComponentValue {
    ComponentValue::map([("wire_id", key), ("wire_score", ComponentValue::U64(score))])
}

fn rows() -> ComponentValue {
    ComponentValue::map([(
        "rows",
        ComponentValue::List(vec![
            row(ComponentValue::String("a".into()), 1),
            row(ComponentValue::String("b".into()), 2),
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

#[test]
fn candidate_key_edits_retarget_nested_items_and_reverse_to_the_original_selector() {
    let limits = ComponentValueMaterializationLimits::default();
    for (before, after, peer) in [
        (
            ComponentValue::String("old".into()),
            ComponentValue::String("new".into()),
            ComponentValue::String("peer".into()),
        ),
        (
            ComponentValue::I64(i64::MIN),
            ComponentValue::I64(-1),
            ComponentValue::I64(0),
        ),
        (
            ComponentValue::U64(u64::MAX),
            ComponentValue::U64(7),
            ComponentValue::U64(0),
        ),
    ] {
        let schema = schema([field(
            "groups",
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("wire_group", ComponentValueKind::String)
                        .with_id(id("id")),
                    ComponentShapeMember::new("wire_rows", keyed_rows_shape(before.kind()))
                        .with_id(id("rows")),
                ]),
                id("id"),
            ),
        )]);
        let original = ComponentValue::map([(
            "groups",
            ComponentValue::List(vec![ComponentValue::map([
                ("wire_group", ComponentValue::String("group".into())),
                (
                    "wire_rows",
                    ComponentValue::List(vec![row(peer.clone(), 1), row(before.clone(), 2)]),
                ),
            ])]),
        )]);
        for key_only in [true, false] {
            let path = |key: &ComponentValue| {
                ComponentPropertyPath::new(id("groups"))
                    .with_steps([
                        ComponentPropertyStep::ListItem {
                            by: id("id"),
                            key: ComponentPropertyKey::String("group".into()),
                        },
                        ComponentPropertyStep::Member(id("rows")),
                        ComponentPropertyStep::ListItem {
                            by: id("id"),
                            key: ComponentPropertyKey::from_value(key).unwrap(),
                        },
                    ])
                    .with_steps(key_only.then(|| ComponentPropertyStep::Member(id("id"))))
            };
            let payload = |key: &ComponentValue, score| {
                if key_only {
                    key.clone()
                } else {
                    row(key.clone(), score)
                }
            };
            let forward = path(&before);
            let replacement = payload(&after, 3);
            let mut candidate = original.clone();
            let (inverse, previous) = forward
                .replace_in_candidate(
                    &schema,
                    schema.version(),
                    &mut candidate,
                    replacement.clone(),
                    limits,
                )
                .unwrap();
            assert_eq!(inverse, path(&after));
            assert_eq!(previous, payload(&before, 2));
            assert_eq!(
                resolve_component_property(&schema, schema.version(), &candidate, &inverse)
                    .unwrap()
                    .value,
                &replacement,
            );
            let (replay, undone) = inverse
                .replace_in_candidate(&schema, schema.version(), &mut candidate, previous, limits)
                .unwrap();
            assert_eq!(replay, forward);
            assert_eq!(undone, replacement);
            assert_eq!(candidate, original);

            assert_eq!(
                forward
                    .replace_in_candidate(
                        &schema,
                        schema.version(),
                        &mut candidate,
                        payload(&peer, 4),
                        limits,
                    )
                    .unwrap_err(),
                ComponentPropertyError::AmbiguousListItem {
                    step: 2,
                    key: ComponentPropertyKey::from_value(&peer).unwrap(),
                },
            );
        }
    }
}

#[test]
fn candidate_replacement_moves_the_previous_value_and_preserves_sibling_allocations() {
    let schema = schema([
        field("previous", ComponentValueKind::String),
        field("sibling", ComponentValueKind::String),
    ]);
    let mut candidate = ComponentValue::map([
        ("previous", ComponentValue::String("previous value".into())),
        ("sibling", ComponentValue::String("sibling value".into())),
    ]);
    let previous_allocation = candidate
        .get("previous")
        .unwrap()
        .as_str()
        .unwrap()
        .as_ptr();
    let sibling_allocation = candidate.get("sibling").unwrap().as_str().unwrap().as_ptr();
    let path = ComponentPropertyPath::new(id("previous"));
    let (inverse, previous) = path
        .replace_in_candidate(
            &schema,
            schema.version(),
            &mut candidate,
            ComponentValue::String("replacement".into()),
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap();
    assert_eq!(inverse, path);
    assert_eq!(previous.as_str().unwrap().as_ptr(), previous_allocation);
    assert_eq!(
        candidate.get("sibling").unwrap().as_str().unwrap().as_ptr(),
        sibling_allocation,
    );
}

#[test]
fn candidate_inverse_enforces_the_complete_selector_byte_limit() {
    let limits = ComponentValueMaterializationLimits::default();
    let root = id(&"r".repeat(ComponentFieldId::MAX_BYTES));
    let schema = schema([ComponentFieldSchema::required(
        root.clone(),
        "Rows",
        ComponentFieldPath::from_fields(["rows"]),
        keyed_rows_shape(ComponentValueKind::String),
    )]);
    let path = ComponentPropertyPath::new(root).with_steps([
        ComponentPropertyStep::ListItem {
            by: id("id"),
            key: ComponentPropertyKey::String("a".into()),
        },
        ComponentPropertyStep::Member(id("id")),
    ]);
    let limit = 4 * 1024 * 1024;
    let key_bytes = limit - path.logical_bytes().unwrap() + 1;
    let original = rows();
    let mut candidate = original.clone();
    let (inverse, previous) = path
        .replace_in_candidate(
            &schema,
            schema.version(),
            &mut candidate,
            ComponentValue::String("x".repeat(key_bytes)),
            limits,
        )
        .unwrap();
    assert_eq!(inverse.logical_bytes().unwrap(), limit);
    let (forward, _) = inverse
        .replace_in_candidate(&schema, schema.version(), &mut candidate, previous, limits)
        .unwrap();
    assert_eq!(forward, path);
    assert_eq!(candidate, original);
    assert_eq!(
        path.replace_in_candidate(
            &schema,
            schema.version(),
            &mut candidate,
            ComponentValue::String("x".repeat(key_bytes + 1)),
            limits,
        )
        .unwrap_err(),
        ComponentPropertyError::PathByteLimit,
    );
}

#[test]
fn candidate_replacement_budgets_only_the_selected_values() {
    let schema = schema([
        field("left", ComponentValueKind::String),
        field("right", ComponentValueKind::String),
    ]);
    let source = ComponentValue::map([
        ("left", ComponentValue::String("xx".into())),
        ("right", ComponentValue::String(String::new())),
    ]);
    let path = ComponentPropertyPath::new(id("right"));
    let replacement = ComponentValue::String("yy".into());
    let limits = value_limits(1, 2);
    let expected = (path.clone(), ComponentValue::String(String::new()));
    let mut ordinary = source.clone();
    assert_eq!(
        path.replace_in_candidate(
            &schema,
            schema.version(),
            &mut ordinary,
            replacement.clone(),
            limits,
        ),
        Ok(expected.clone()),
    );
    let mut metered = source;
    assert_eq!(
        path.replace_in_candidate_with_admission(
            &schema,
            schema.version(),
            &mut metered,
            replacement,
            limits,
            |_, _, _| Ok::<_, ()>(()),
        ),
        Ok(expected),
    );
    assert_eq!(metered, ordinary);
    assert!(ordinary.cost().logical_bytes() > limits.bytes());
}

#[test]
fn candidate_replacement_accepts_a_large_source_and_keeps_the_explicit_replacement_limit() {
    let item_count = 17_000;
    let schema = schema([field(
        "items",
        ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueKind::U64.into(),
        )),
    )]);
    let items = ComponentValue::List(vec![ComponentValue::U64(1); item_count]);
    let source = ComponentValue::map([("items", items.clone())]);
    let cost = source.cost();
    let limits = value_limits(cost.nodes(), cost.logical_bytes());
    let path = ComponentPropertyPath::new(id("items"));
    let replacement = ComponentValue::List(vec![ComponentValue::U64(7)]);

    let mut ordinary = source.clone();
    let mut metered = source.clone();
    let parts = path
        .replace_in_candidate(
            &schema,
            schema.version(),
            &mut ordinary,
            replacement.clone(),
            limits,
        )
        .unwrap();
    assert_eq!(parts, (path.clone(), items));
    assert_eq!(
        path.replace_in_candidate_with_admission(
            &schema,
            schema.version(),
            &mut metered,
            replacement,
            limits,
            |_, _, _| Ok::<_, ()>(()),
        ),
        Ok(parts),
    );
    assert_eq!(metered, ordinary);

    let oversized = ComponentValue::List(vec![ComponentValue::Null; cost.nodes()]);
    let mut candidate = source;
    let error =
        ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationNodeLimit {
            observed: cost.nodes() + 1,
            maximum: cost.nodes(),
        });
    assert_eq!(
        path.replace_in_candidate(
            &schema,
            schema.version(),
            &mut candidate,
            oversized.clone(),
            limits,
        ),
        Err(error.clone()),
    );
    assert_eq!(
        path.replace_in_candidate_with_admission(
            &schema,
            schema.version(),
            &mut candidate,
            oversized,
            limits,
            |_, _, _| Ok::<_, ()>(()),
        ),
        Err(ComponentPropertyAdmissionError::Property(error)),
    );
}

#[test]
fn candidate_admission_covers_inverse_storage_and_duplicate_diagnostics() {
    let schema = keyed_schema();
    let path = selected("id");
    let limits = ComponentValueMaterializationLimits::default();
    let next_key = "next-key".repeat(128);
    for duplicate in [false, true] {
        let peer = if duplicate { next_key.as_str() } else { "b" };
        let original = ComponentValue::map([(
            "rows",
            ComponentValue::List(vec![
                row(ComponentValue::String("a".into()), 1),
                row(ComponentValue::String(peer.into()), 2),
            ]),
        )]);
        let replacement = ComponentValue::String(next_key.clone());
        let mut ordinary = original.clone();
        let expected = path.replace_in_candidate(
            &schema,
            schema.version(),
            &mut ordinary,
            replacement.clone(),
            limits,
        );
        let targeted_charge = match &expected {
            Ok((inverse, _)) => {
                let bytes = inverse.logical_bytes().unwrap();
                (
                    inverse.steps.len() + 1,
                    bytes,
                    inverse.steps.len() * size_of::<ComponentPropertyStep>() + bytes,
                )
            }
            Err(ComponentPropertyError::AmbiguousListItem { .. }) => (0, 0, next_key.len()),
            Err(error) => panic!("unexpected property error: {error:?}"),
        };
        let mut observed = Vec::new();
        let mut metered = original.clone();
        let actual = path.replace_in_candidate_with_admission(
            &schema,
            schema.version(),
            &mut metered,
            replacement.clone(),
            limits,
            |nodes, bytes, allocated| {
                observed.push((nodes, bytes, allocated));
                Ok::<_, ()>(())
            },
        );
        assert_eq!(
            actual,
            expected.map_err(ComponentPropertyAdmissionError::Property),
        );
        assert_eq!(metered, ordinary);

        let denied = observed
            .iter()
            .position(|charge| *charge == targeted_charge)
            .unwrap();
        let caller_error = String::from("owner refused property storage");
        let mut count = 0;
        let mut rejected = original.clone();
        let result = path.replace_in_candidate_with_admission(
            &schema,
            schema.version(),
            &mut rejected,
            replacement,
            limits,
            |nodes, bytes, allocated| {
                let index = count;
                count += 1;
                assert_eq!(observed[index], (nodes, bytes, allocated));
                if index == denied {
                    Err(&caller_error)
                } else {
                    Ok(())
                }
            },
        );
        let Err(ComponentPropertyAdmissionError::Admission(error)) = result else {
            panic!("expected the owner's storage refusal");
        };
        assert!(std::ptr::eq(error, &caller_error));
        assert_eq!(count, denied + 1);
        assert_eq!(rejected, original);
    }
}
