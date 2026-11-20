use std::mem::size_of;

use nara_core::{ByteLimit, DepthLimit, ItemLimit};

use crate::{
    __private::{
        ComponentPropertyAdmissionError, freeze_component_defaults, index_component_property_reads,
        index_component_property_reads_with_admission, read_admitted_component_property_in_root,
        resolve_component_property_storage_path_with_admission,
        validate_component_property_value_with_admission,
    },
    ComponentCodecError, ComponentFieldId, ComponentFieldPath, ComponentFieldPathSegment,
    ComponentFieldSchema, ComponentMemberPath, ComponentMemberPathSegment, ComponentPropertyError,
    ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueShape, replace_component_property, resolve_component_property,
    resolve_component_property_in_root, resolve_component_property_with_limits,
};

fn id(name: &str) -> ComponentFieldId {
    ComponentFieldId::new(name)
}

fn storage_path(suffix: impl IntoIterator<Item = ComponentFieldPathSegment>) -> ComponentFieldPath {
    ComponentFieldPath::new(
        [
            ComponentFieldPathSegment::field("envelope"),
            ComponentFieldPathSegment::index(0),
            ComponentFieldPathSegment::field("payload"),
        ]
        .into_iter()
        .chain(suffix),
    )
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.PropertyStoragePath"),
        "Property storage path",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        id("root"),
        "Root",
        storage_path([]),
        shape,
    )])
}

fn payload(value: ComponentValue) -> ComponentValue {
    ComponentValue::map([(
        "envelope",
        ComponentValue::List(vec![ComponentValue::map([("payload", value)])]),
    )])
}

fn assert_storage_matches_replacement(
    schema: &ComponentSchema,
    value: &ComponentValue,
    path: &ComponentPropertyPath,
    expected: ComponentFieldPath,
) {
    let limits = ComponentValueMaterializationLimits::default();
    let storage = resolve_component_property_storage_path_with_admission(
        schema,
        schema.version(),
        value,
        path,
        limits,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(storage, expected);
    let resolved =
        resolve_component_property_with_limits(schema, schema.version(), value, path, limits)
            .unwrap();
    assert!(std::ptr::eq(
        value.get_path(&storage).unwrap(),
        resolved.value.as_value().unwrap(),
    ));

    let replacement = ComponentValue::U64(42);
    let mut direct = value.clone();
    let previous = direct.replace_path(&storage, replacement.clone()).unwrap();
    assert_eq!(&previous, resolved.value.as_value().unwrap());
    let owned = path
        .replace_in_owned_value_with_limits(
            schema,
            schema.version(),
            value.clone(),
            replacement.clone(),
            limits,
        )
        .unwrap();
    let mut atomic = value.clone();
    assert_eq!(
        replace_component_property(schema, schema.version(), &mut atomic, path, replacement)
            .unwrap(),
        previous,
    );
    assert_eq!(direct, owned);
    assert_eq!(direct, atomic);
}

fn assert_refusal(
    schema: &ComponentSchema,
    value: &ComponentValue,
    path: &ComponentPropertyPath,
    expected: ComponentPropertyError,
) {
    let limits = ComponentValueMaterializationLimits::default();
    assert_eq!(
        resolve_component_property_storage_path_with_admission(
            schema,
            schema.version(),
            value,
            path,
            limits,
            |_| Ok::<_, ()>(()),
        ),
        Err(ComponentPropertyAdmissionError::Property(expected.clone())),
    );
    assert_eq!(
        resolve_component_property_with_limits(schema, schema.version(), value, path, limits)
            .unwrap_err(),
        expected,
    );
    assert_eq!(
        path.replace_in_owned_value_with_limits(
            schema,
            schema.version(),
            value.clone(),
            ComponentValue::U64(42),
            limits,
        )
        .unwrap_err(),
        expected,
    );
    let mut atomic = value.clone();
    assert_eq!(
        replace_component_property(
            schema,
            schema.version(),
            &mut atomic,
            path,
            ComponentValue::U64(42),
        )
        .unwrap_err(),
        expected,
    );
    assert_eq!(&atomic, value);
}

#[test]
fn storage_path_keeps_the_full_root_and_follows_identified_members_across_wire_renames() {
    let path = ComponentPropertyPath::new(id("root")).with_steps([
        ComponentPropertyStep::Member(id("parent")),
        ComponentPropertyStep::Member(id("leaf")),
    ]);
    for (parent, leaf) in [("old_parent", "old_leaf"), ("new_parent", "new_leaf")] {
        let schema = schema(ComponentValueShape::record_members([
            ComponentShapeMember::new(
                parent,
                ComponentValueShape::optional(ComponentValueShape::record_members([
                    ComponentShapeMember::new(leaf, ComponentValueKind::U64).with_id(id("leaf")),
                ])),
            )
            .with_id(id("parent")),
        ]));
        let value = payload(ComponentValue::map([(
            parent,
            ComponentValue::map([(leaf, ComponentValue::U64(7))]),
        )]));
        assert_eq!(
            resolve_component_property_storage_path_with_admission(
                &schema,
                schema.version(),
                &value,
                &ComponentPropertyPath::new(id("root")),
                ComponentValueMaterializationLimits::default(),
                |_| Ok::<_, ()>(()),
            )
            .unwrap(),
            storage_path([]),
        );
        assert_storage_matches_replacement(
            &schema,
            &value,
            &path,
            storage_path([
                ComponentFieldPathSegment::field(parent),
                ComponentFieldPathSegment::field(leaf),
            ]),
        );
    }
}

fn keyed_schema() -> ComponentSchema {
    schema(ComponentValueShape::optional(
        ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("wire_key", ComponentValueKind::String)
                    .with_id(id("key")),
                ComponentShapeMember::new(
                    "wire_settings",
                    ComponentValueShape::map(ComponentValueShape::optional(
                        ComponentValueShape::record_members([ComponentShapeMember::new(
                            "wire_value",
                            ComponentValueKind::U64,
                        )
                        .with_id(id("value"))]),
                    )),
                )
                .with_id(id("settings")),
            ]),
            id("key"),
        ),
    ))
}

fn row(key: &str) -> ComponentValue {
    ComponentValue::map([
        ("wire_key", ComponentValue::String(key.into())),
        (
            "wire_settings",
            ComponentValue::map([(
                "slot.with[syntax]",
                ComponentValue::map([("wire_value", ComponentValue::U64(7))]),
            )]),
        ),
    ])
}

fn selected() -> ComponentPropertyPath {
    ComponentPropertyPath::new(id("root")).with_steps([
        ComponentPropertyStep::ListItem {
            by: id("key"),
            key: ComponentPropertyKey::String("b".into()),
        },
        ComponentPropertyStep::Member(id("settings")),
        ComponentPropertyStep::MapEntry("slot.with[syntax]".into()),
        ComponentPropertyStep::Member(id("value")),
    ])
}

#[test]
fn admitted_key_positions_follow_each_original_nested_collection() {
    let inner = ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("wire_id", ComponentValueKind::I64).with_id(id("inner")),
            ComponentShapeMember::new("wire_score", ComponentValueKind::U64).with_id(id("score")),
        ]),
        id("inner"),
    );
    let schema = schema(ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("wire_name", ComponentValueKind::String).with_id(id("outer")),
            ComponentShapeMember::new("wire_groups", ComponentValueShape::map(inner))
                .with_id(id("groups")),
        ]),
        id("outer"),
    ));
    let inner_row = |key, score| {
        ComponentValue::map([
            ("wire_id", ComponentValue::I64(key)),
            ("wire_score", ComponentValue::U64(score)),
        ])
    };
    let outer_row = |name: &str, rows| {
        ComponentValue::map([
            ("wire_name", ComponentValue::String(name.into())),
            (
                "wire_groups",
                ComponentValue::map([("active.group", ComponentValue::List(rows))]),
            ),
        ])
    };
    let value = payload(ComponentValue::List(vec![
        outer_row("north", vec![inner_row(-7, 11), inner_row(7, 12)]),
        outer_row("south", vec![inner_row(7, 21), inner_row(-7, 22)]),
    ]));
    let ordinary = index_component_property_reads(&schema, (&value).into(), |_| true).unwrap();
    let mut allocated = 0usize;
    let admitted = index_component_property_reads_with_admission(
        &schema,
        (&value).into(),
        |_| true,
        |_, _, bytes| {
            allocated += bytes;
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert_eq!(admitted, ordinary);
    assert!(allocated >= admitted.retained_bytes());

    let field = &schema.fields[0];
    let root = value.get_path(field.path()).unwrap();
    for (name, position, score) in [("north", 0, 11), ("south", 1, 22)] {
        let path = ComponentPropertyPath::new(id("root")).with_steps([
            ComponentPropertyStep::ListItem {
                by: id("outer"),
                key: ComponentPropertyKey::String(name.into()),
            },
            ComponentPropertyStep::Member(id("groups")),
            ComponentPropertyStep::MapEntry("active.group".into()),
            ComponentPropertyStep::ListItem {
                by: id("inner"),
                key: ComponentPropertyKey::I64(-7),
            },
            ComponentPropertyStep::Member(id("score")),
        ]);
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(12).unwrap(),
            ByteLimit::new(512).unwrap(),
        );
        let (selected, storage) = read_admitted_component_property_in_root(
            field,
            root.into(),
            &path,
            &mut budget,
            true,
            Some(&admitted),
        )
        .unwrap();
        assert_eq!(selected.as_u64(), Some(score));
        assert_eq!(
            storage,
            ComponentFieldPath::new([
                ComponentFieldPathSegment::index(position),
                ComponentFieldPathSegment::field("wire_groups"),
                ComponentFieldPathSegment::field("active.group"),
                ComponentFieldPathSegment::index(position),
                ComponentFieldPathSegment::field("wire_score"),
            ])
        );
        assert_eq!(
            root.get_path(&storage).unwrap(),
            &ComponentValue::U64(score)
        );
    }

    let duplicate = payload(ComponentValue::List(vec![outer_row(
        "north",
        vec![inner_row(-7, 11), inner_row(-7, 12)],
    )]));
    assert_eq!(
        index_component_property_reads(&schema, (&duplicate).into(), |_| true).unwrap_err(),
        crate::validate_component_value(&schema, &duplicate).unwrap_err(),
    );
}

#[test]
fn nested_key_indexes_keep_only_one_copy_of_their_shared_map_key() {
    let list = ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([ComponentShapeMember::new(
            "id",
            ComponentValueKind::U64,
        )
        .with_id(id("key"))]),
        id("key"),
    );
    let schema = schema(ComponentValueShape::map(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("first", list.clone()).with_id(id("first")),
            ComponentShapeMember::new("second", list).with_id(id("second")),
        ]),
    ));
    let retained_bytes = |name: &str| {
        let row = ComponentValue::List(vec![ComponentValue::map([("id", ComponentValue::U64(1))])]);
        let value = payload(ComponentValue::map([(
            name,
            ComponentValue::map([("first", row.clone()), ("second", row)]),
        )]));
        index_component_property_reads(&schema, (&value).into(), |_| true)
            .unwrap()
            .retained_bytes()
    };
    let long = "x".repeat(32 * 1024);
    assert_eq!(retained_bytes(&long) - retained_bytes("x"), long.len() - 1);
}

#[test]
fn storage_path_maps_a_stable_key_to_the_actual_index_and_keeps_map_keys_literal() {
    let schema = keyed_schema();
    for (keys, index) in [(["a", "b"], 1), (["b", "a"], 0)] {
        let value = payload(ComponentValue::List(keys.into_iter().map(row).collect()));
        assert_storage_matches_replacement(
            &schema,
            &value,
            &selected(),
            storage_path([
                ComponentFieldPathSegment::index(index),
                ComponentFieldPathSegment::field("wire_settings"),
                ComponentFieldPathSegment::field("slot.with[syntax]"),
                ComponentFieldPathSegment::field("wire_value"),
            ]),
        );
    }
}

#[test]
fn frozen_nested_selectors_borrow_insertions_and_preserve_exact_lookup_budgets() {
    let schema = keyed_schema();
    let raw = payload(ComponentValue::List(vec![
        row("a"),
        ComponentValue::map([
            ("wire_key", ComponentValue::String("b".into())),
            (
                "wire_settings",
                ComponentValue::map([(
                    "slot.with[syntax]",
                    ComponentValue::Map(Default::default()),
                )]),
            ),
        ]),
    ]));
    let original = raw.clone();
    let path = selected();
    let expected_storage = storage_path([
        ComponentFieldPathSegment::index(1),
        ComponentFieldPathSegment::field("wire_settings"),
        ComponentFieldPathSegment::field("slot.with[syntax]"),
        ComponentFieldPathSegment::field("wire_value"),
    ]);
    let mut defaults_schema = ComponentSchema::new(
        ComponentTypeId::new("test.PropertyFrozenDefaults"),
        "Property frozen defaults",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        id("amount"),
        "Amount",
        expected_storage.clone(),
        ComponentValueKind::U64,
    )]);
    defaults_schema.fields[0].default_value = Some(ComponentValue::U64(7));
    let limits = ComponentValueMaterializationLimits::default();
    let frozen = freeze_component_defaults(&defaults_schema, &raw, &Default::default(), limits)
        .unwrap()
        .unwrap();
    drop(defaults_schema);
    let read = frozen.read(&raw);
    let root = read.get_path(schema.fields[0].path()).unwrap();
    let rows = root.as_list().unwrap();
    assert!(read.as_value().is_none());
    assert!(root.as_value().is_none());
    assert!(rows.get(1).unwrap().as_value().is_none());
    assert!(std::ptr::eq(
        rows.get(0).unwrap().as_value().unwrap(),
        raw.get_path(&storage_path([ComponentFieldPathSegment::index(0)]))
            .unwrap(),
    ));

    let mut budget =
        ComponentPropertyReadBudget::new(ItemLimit::new(12).unwrap(), ByteLimit::new(114).unwrap());
    let resolved =
        resolve_component_property_in_root(&schema.fields[0], root, &path, &mut budget).unwrap();
    assert_eq!(resolved.value.as_u64(), Some(7));
    assert_eq!(budget.scanned_entries(), 12);
    assert_eq!(budget.scanned_logical_bytes(), 114);
    assert!(std::ptr::eq(
        resolved.value.as_value().unwrap(),
        read.get_path(&expected_storage)
            .unwrap()
            .as_value()
            .unwrap(),
    ));
    assert_eq!(
        resolve_component_property(&schema, schema.version(), read, &path)
            .unwrap()
            .value,
        resolved.value,
    );
    assert_eq!(
        resolve_component_property_with_limits(&schema, schema.version(), read, &path, limits)
            .unwrap()
            .value,
        resolved.value,
    );
    assert_eq!(
        resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            read,
            &path,
            limits,
            |_| Ok::<_, ()>(()),
        ),
        Ok(expected_storage.clone()),
    );
    validate_component_property_value_with_admission(
        &schema.fields[0].shape,
        &ComponentPropertyPath::new(id("root")),
        root,
        limits,
        |_, _, _| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(
        ComponentPropertyKey::from_value(rows.get(1).unwrap().get("wire_key").unwrap()),
        Some(ComponentPropertyKey::String("b".into())),
    );
    for (entries, bytes) in [(11, 114), (12, 113)] {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(entries).unwrap(),
            ByteLimit::new(bytes).unwrap(),
        );
        assert_eq!(
            resolve_component_property_in_root(&schema.fields[0], root, &path, &mut budget)
                .unwrap_err(),
            ComponentPropertyError::ReadBudgetExceeded,
        );
    }
    let mut owned = read.to_owned();
    assert_eq!(
        replace_component_property(
            &schema,
            schema.version(),
            &mut owned,
            &path,
            ComponentValue::U64(42),
        )
        .unwrap(),
        ComponentValue::U64(7),
    );
    assert_eq!(
        read.get_path(&expected_storage).unwrap(),
        ComponentValue::U64(7),
    );
    assert_eq!(
        owned.get_path(&expected_storage).unwrap(),
        &ComponentValue::U64(42),
    );
    assert_eq!(raw, original);
}

#[test]
fn storage_path_shares_retired_ambiguous_and_absent_selector_refusals() {
    let mut schema = keyed_schema().with_field_tombstones([id("retired-root")]);
    let retired = ComponentMemberPath::new([
        ComponentMemberPathSegment::ListItem,
        ComponentMemberPathSegment::Member(id("settings")),
        ComponentMemberPathSegment::MapEntry,
        ComponentMemberPathSegment::Member(id("retired")),
    ]);
    schema.fields[0].member_tombstones.push(retired.clone());
    let value = payload(ComponentValue::List(vec![row("a"), row("b")]));
    assert_refusal(
        &schema,
        &value,
        &ComponentPropertyPath::new(id("retired-root")),
        ComponentPropertyError::RetiredField(id("retired-root")),
    );
    let mut path = selected();
    path.steps[3] = ComponentPropertyStep::Member(id("retired"));
    assert_refusal(
        &schema,
        &value,
        &path,
        ComponentPropertyError::RetiredMember(retired),
    );
    assert_refusal(
        &schema,
        &payload(ComponentValue::List(vec![row("b"), row("b")])),
        &selected(),
        ComponentPropertyError::AmbiguousListItem {
            step: 0,
            key: ComponentPropertyKey::String("b".into()),
        },
    );
    assert_refusal(
        &schema,
        &payload(ComponentValue::List(vec![row("a")])),
        &selected(),
        ComponentPropertyError::ListItemNotFound {
            step: 0,
            key: ComponentPropertyKey::String("b".into()),
        },
    );
    let mut path = selected();
    path.steps[2] = ComponentPropertyStep::MapEntry("missing".into());
    assert_refusal(
        &schema,
        &value,
        &path,
        ComponentPropertyError::MapEntryNotFound {
            step: 2,
            key: "missing".into(),
        },
    );
    assert_refusal(
        &schema,
        &payload(ComponentValue::Null),
        &selected(),
        ComponentPropertyError::OptionalAbsent { step: 0 },
    );
}

#[test]
fn storage_path_preserves_exact_value_limits_and_version_errors() {
    let schema = schema(ComponentValueKind::U64.into());
    let value = payload(ComponentValue::U64(7));
    let path = ComponentPropertyPath::new(id("root"));
    let cost = value.cost();
    let limits = |nodes, bytes, depth| {
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        )
    };
    let exact = limits(cost.nodes(), cost.logical_bytes(), 4);
    assert_eq!(
        resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            &value,
            &path,
            exact,
            |_| Ok::<_, ()>(()),
        ),
        Ok(storage_path([])),
    );
    let version = ComponentSchemaVersion::new(2).unwrap();
    for (version, limits, expected) in [
        (
            schema.version(),
            limits(cost.nodes() - 1, cost.logical_bytes(), 4),
            ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationNodeLimit {
                observed: cost.nodes(),
                maximum: cost.nodes() - 1,
            }),
        ),
        (
            schema.version(),
            limits(cost.nodes(), cost.logical_bytes() - 1, 4),
            ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationByteLimit {
                observed: cost.logical_bytes(),
                maximum: cost.logical_bytes() - 1,
            }),
        ),
        (
            schema.version(),
            limits(cost.nodes(), cost.logical_bytes(), 3),
            ComponentPropertyError::InvalidValue(ComponentCodecError::MaterializationDepthLimit {
                observed: 4,
                maximum: 3,
            }),
        ),
        (
            version,
            exact,
            ComponentPropertyError::VersionMismatch {
                expected: schema.version(),
                actual: version,
            },
        ),
    ] {
        assert_eq!(
            resolve_component_property_storage_path_with_admission(
                &schema,
                version,
                &value,
                &path,
                limits,
                |_| -> Result<(), ()> {
                    panic!("invalid values and versions must fail before storage allocation")
                },
            ),
            Err(ComponentPropertyAdmissionError::Property(expected.clone())),
        );
        assert_eq!(
            resolve_component_property_with_limits(&schema, version, &value, &path, limits)
                .unwrap_err(),
            expected,
        );
        assert_eq!(
            path.replace_in_owned_value_with_limits(
                &schema,
                version,
                value.clone(),
                ComponentValue::U64(42),
                limits,
            )
            .unwrap_err(),
            expected,
        );
    }
}

#[test]
fn scalar_storage_path_admits_the_complete_long_root_at_exact_and_one_short_allowances() {
    let prefix = "p".repeat(64 * 1024);
    let mut schema = schema(ComponentValueKind::U64.into());
    schema.fields[0].path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field(&prefix),
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("scalar"),
    ]);
    let value = ComponentValue::map([(
        prefix.as_str(),
        ComponentValue::List(vec![ComponentValue::map([(
            "scalar",
            ComponentValue::U64(7),
        )])]),
    )]);
    let path = ComponentPropertyPath::new(id("root"));
    let bytes = 3 * size_of::<ComponentFieldPathSegment>() + prefix.len() + "scalar".len();
    for allowance in [bytes, bytes - 1] {
        let mut remaining = allowance;
        let mut charges = Vec::new();
        let result = resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            &value,
            &path,
            ComponentValueMaterializationLimits::default(),
            |allocated| {
                charges.push(allocated);
                remaining = remaining
                    .checked_sub(allocated)
                    .ok_or((remaining, allocated))?;
                Ok(())
            },
        );
        assert_eq!(charges, [bytes]);
        if allowance == bytes {
            assert_eq!(result, Ok(schema.fields[0].path.clone()));
            assert_eq!(remaining, 0);
        } else {
            assert_eq!(
                result,
                Err(ComponentPropertyAdmissionError::Admission((
                    bytes - 1,
                    bytes
                ))),
            );
            assert_eq!(remaining, allowance);
        }
    }
}

#[test]
fn selected_long_member_and_map_strings_are_admitted_before_copying() {
    let root_bytes =
        3 * size_of::<ComponentFieldPathSegment>() + "envelope".len() + "payload".len();
    let growth_bytes = 6 * size_of::<ComponentFieldPathSegment>();
    for is_member in [true, false] {
        let wire_name = "w".repeat(if is_member {
            ComponentFieldId::MAX_BYTES
        } else {
            64 * 1024
        });
        let (shape, step) = if is_member {
            (
                ComponentValueShape::record_members([ComponentShapeMember::new(
                    &wire_name,
                    ComponentValueKind::U64,
                )
                .with_id(id("leaf"))]),
                ComponentPropertyStep::Member(id("leaf")),
            )
        } else {
            (
                ComponentValueShape::map(ComponentValueKind::U64.into()),
                ComponentPropertyStep::MapEntry(wire_name.clone()),
            )
        };
        let expected_charges = [root_bytes, growth_bytes, wire_name.len()];
        let schema = schema(shape);
        let value = payload(ComponentValue::map([(
            wire_name.as_str(),
            ComponentValue::U64(7),
        )]));
        let path = ComponentPropertyPath::new(id("root")).with_steps([step]);
        let exact = expected_charges.iter().sum::<usize>();
        let mut remaining = exact;
        let mut charges = Vec::new();
        let storage = resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            &value,
            &path,
            ComponentValueMaterializationLimits::default(),
            |allocated| {
                charges.push(allocated);
                remaining = remaining.checked_sub(allocated).ok_or(())?;
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        assert_eq!(
            storage,
            storage_path([ComponentFieldPathSegment::field(&wire_name)]),
        );
        assert_eq!(charges, expected_charges);
        assert_eq!(remaining, 0);

        let copy = expected_charges.len() - 1;
        let before_copy = expected_charges[..copy].iter().sum::<usize>();
        let mut remaining = before_copy + wire_name.len() - 1;
        let mut calls = 0;
        let result = resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            &value,
            &path,
            ComponentValueMaterializationLimits::default(),
            |allocated| {
                assert_eq!(allocated, expected_charges[calls]);
                calls += 1;
                remaining = remaining
                    .checked_sub(allocated)
                    .ok_or((remaining, allocated))?;
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(ComponentPropertyAdmissionError::Admission((
                wire_name.len() - 1,
                wire_name.len(),
            ))),
        );
        assert_eq!(calls, copy + 1);
        assert_eq!(remaining, wire_name.len() - 1);
    }
}

#[test]
fn storage_vector_growth_refusal_precedes_reserve_and_existing_capacity_needs_no_admission() {
    for initial in [0, 3] {
        let mut storage = Vec::with_capacity(initial);
        storage.extend(
            (0..initial)
                .map(|index| ComponentFieldPathSegment::index(u32::try_from(index).unwrap())),
        );
        let original = storage.clone();
        let capacity = storage.capacity();
        let next_capacity = (storage.len() + 1).max(capacity * 2).max(4);
        let mut calls = 0;
        let mut deny = |nodes, bytes, allocated| {
            calls += 1;
            assert_eq!((nodes, bytes), (0, 0));
            assert_eq!(
                allocated,
                next_capacity * size_of::<ComponentFieldPathSegment>(),
            );
            false
        };
        assert_eq!(
            super::reserve_storage(&mut storage, &mut Some(&mut deny)),
            Err(ComponentPropertyError::ReadBudgetExceeded),
        );
        assert_eq!(calls, 1);
        assert_eq!(storage, original);
        assert_eq!(storage.capacity(), capacity);

        let mut allow = |_, _, _| true;
        super::reserve_storage(&mut storage, &mut Some(&mut allow)).unwrap();
        assert_eq!(storage, original);
        assert!(storage.capacity() >= next_capacity);
        let capacity = storage.capacity();
        let mut unexpected =
            |_, _, _| -> bool { panic!("spare storage capacity must not be charged again") };
        super::reserve_storage(&mut storage, &mut Some(&mut unexpected)).unwrap();
        assert_eq!(storage.capacity(), capacity);
    }
}

#[test]
fn every_storage_allocation_refusal_retains_the_exact_nonclone_caller_error() {
    let schema = keyed_schema();
    let value = payload(ComponentValue::List(vec![row("a"), row("b")]));
    let path = selected();
    let limits = ComponentValueMaterializationLimits::default();
    let mut charges = Vec::new();
    resolve_component_property_storage_path_with_admission(
        &schema,
        schema.version(),
        &value,
        &path,
        limits,
        |allocated| {
            charges.push(allocated);
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert!(charges.contains(&(6 * size_of::<ComponentFieldPathSegment>())));
    assert!(charges.contains(&(12 * size_of::<ComponentFieldPathSegment>())));

    #[derive(Debug)]
    struct Refusal<'a>(&'a str);
    let message = String::from("storage caller refusal");
    for denied in 0..charges.len() {
        let mut calls = 0;
        let result = resolve_component_property_storage_path_with_admission(
            &schema,
            schema.version(),
            &value,
            &path,
            limits,
            |allocated| {
                let call = calls;
                calls += 1;
                assert_eq!(allocated, charges[call]);
                if call == denied {
                    Err(Refusal(&message))
                } else {
                    Ok(())
                }
            },
        );
        let Err(ComponentPropertyAdmissionError::Admission(Refusal(returned))) = result else {
            panic!("storage refusal must retain its caller error");
        };
        assert!(std::ptr::eq(returned, message.as_str()));
        assert_eq!(calls, denied + 1);
    }
}
