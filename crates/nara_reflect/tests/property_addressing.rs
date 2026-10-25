use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::Component;
use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentMemberPath,
    ComponentMemberPathError, ComponentMemberPathSegment, ComponentPropertyError,
    ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentRegistry, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeErrorKind, ComponentShapeMember, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, replace_component_property,
    resolve_component_property, resolve_component_property_in_root, validate_component_value,
};

fn id(value: &str) -> ComponentFieldId {
    ComponentFieldId::new(value)
}

fn member(name: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(name, shape).with_id(id(stable))
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.PropertyAddressing"),
        "Property addressing",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        id("root"),
        "Root",
        ComponentFieldPath::from_fields(["payload"]),
        shape,
    )])
}

fn payload(value: ComponentValue) -> ComponentValue {
    ComponentValue::map([("payload", value)])
}

fn path(steps: impl IntoIterator<Item = ComponentPropertyStep>) -> ComponentPropertyPath {
    ComponentPropertyPath::new(id("root")).with_steps(steps)
}

fn selected(key: &str) -> ComponentPropertyStep {
    ComponentPropertyStep::ListItem {
        by: id("id"),
        key: ComponentPropertyKey::String(key.to_owned()),
    }
}

fn declaration(
    segments: impl IntoIterator<Item = ComponentMemberPathSegment>,
) -> ComponentMemberPath {
    ComponentMemberPath::new(segments)
}

fn row_shape() -> ComponentValueShape {
    ComponentValueShape::record_members([
        member("wire_id", "id", ComponentValueKind::String),
        member("wire_slug", "slug", ComponentValueKind::String),
        member(
            "wire_stats",
            "stats",
            ComponentValueShape::record_members([member("health", "hp", ComponentValueKind::U64)]),
        ),
    ])
}

fn keyed_shape() -> ComponentValueShape {
    ComponentValueShape::keyed_list(row_shape(), id("id"))
}

fn row(identity: &str, slug: &str, hp: u64) -> ComponentValue {
    ComponentValue::map([
        ("wire_id", ComponentValue::String(identity.to_owned())),
        ("wire_slug", ComponentValue::String(slug.to_owned())),
        (
            "wire_stats",
            ComponentValue::map([("health", ComponentValue::U64(hp))]),
        ),
    ])
}

fn hp_path(identity: &str) -> ComponentPropertyPath {
    path([
        selected(identity),
        ComponentPropertyStep::Member(id("stats")),
        ComponentPropertyStep::Member(id("hp")),
    ])
}

#[test]
fn admitted_root_property_read_borrows_the_selected_scalar() {
    let schema = schema(keyed_shape());
    let value = ComponentValue::List(vec![row("first", "other", 1), row("second", "target", 7)]);
    let mut budget = ComponentPropertyReadBudget::new(
        ItemLimit::new(128).unwrap(),
        ByteLimit::new(4096).unwrap(),
    );
    let result = resolve_component_property_in_root(
        &schema.fields()[0],
        &value,
        &hp_path("second"),
        &mut budget,
    )
    .unwrap();
    assert_eq!(result.value, &ComponentValue::U64(7));
    let ComponentValue::List(items) = &value else {
        panic!("list")
    };
    let expected = items[1].get("wire_stats").unwrap().get("health").unwrap();
    assert!(std::ptr::eq(result.value, expected));
}

#[test]
fn admitted_root_property_read_refuses_exhausted_lookup_work() {
    let schema = schema(keyed_shape());
    let value = ComponentValue::List(vec![row("first", "other", 1), row("second", "target", 7)]);
    let mut budget =
        ComponentPropertyReadBudget::new(ItemLimit::ONE, ByteLimit::new(4096).unwrap());
    assert!(matches!(
        resolve_component_property_in_root(
            &schema.fields()[0],
            &value,
            &hp_path("second"),
            &mut budget,
        ),
        Err(ComponentPropertyError::ReadBudgetExceeded)
    ));
}

#[test]
fn admitted_root_property_read_has_independent_exact_scan_entry_and_byte_limits() {
    let schema = schema(keyed_shape());
    let value = ComponentValue::List(vec![row("first", "other", 1), row("second", "target", 7)]);
    // Three identity declarations, two probes per item, three stats declarations and one hp.
    // Declaration names/IDs cost 37 + 37 + 8 bytes; item lookups/keys cost 12 + 13.
    for (entries, bytes, accepted) in [(11, 107, true), (10, 107, false), (11, 106, false)] {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(entries).unwrap(),
            ByteLimit::new(bytes).unwrap(),
        );
        let result = resolve_component_property_in_root(
            &schema.fields()[0],
            &value,
            &hp_path("second"),
            &mut budget,
        );
        if accepted {
            assert_eq!(result.unwrap().value, &ComponentValue::U64(7));
            assert_eq!(budget.scanned_entries(), 11);
            assert_eq!(budget.scanned_logical_bytes(), 107);
        } else {
            assert!(matches!(
                result,
                Err(ComponentPropertyError::ReadBudgetExceeded)
            ));
        }
    }
}

#[test]
fn admitted_root_property_read_charges_optional_unwrap_and_literal_map_lookup() {
    let schema = schema(ComponentValueShape::optional(ComponentValueShape::map(
        ComponentValueKind::U64.into(),
    )));
    let value = ComponentValue::map([("a.b", ComponentValue::U64(9))]);
    let path = path([ComponentPropertyStep::MapEntry("a.b".into())]);
    for (entries, bytes, accepted) in [(2, 3, true), (1, 3, false), (2, 2, false)] {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(entries).unwrap(),
            ByteLimit::new(bytes).unwrap(),
        );
        let result =
            resolve_component_property_in_root(&schema.fields()[0], &value, &path, &mut budget);
        if accepted {
            assert_eq!(result.unwrap().value, &ComponentValue::U64(9));
            assert_eq!(budget.scanned_entries(), 2);
            assert_eq!(budget.scanned_logical_bytes(), 3);
        } else {
            assert!(matches!(
                result,
                Err(ComponentPropertyError::ReadBudgetExceeded)
            ));
        }
    }
}

#[test]
fn admitted_root_property_read_charges_retired_member_prefix_checks() {
    let mut schema = schema(ComponentValueShape::record_members([member(
        "v",
        "live",
        ComponentValueKind::U64,
    )]));
    schema.fields[0].member_tombstones = vec![declaration([ComponentMemberPathSegment::Member(
        id("retired"),
    )])];
    let value = ComponentValue::map([("v", ComponentValue::U64(9))]);
    let path = path([ComponentPropertyStep::Member(id("live"))]);
    for (entries, bytes, accepted) in [(2, 12, true), (1, 12, false), (2, 11, false)] {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(entries).unwrap(),
            ByteLimit::new(bytes).unwrap(),
        );
        let result =
            resolve_component_property_in_root(&schema.fields()[0], &value, &path, &mut budget);
        if accepted {
            assert_eq!(result.unwrap().value, &ComponentValue::U64(9));
            assert_eq!(budget.scanned_entries(), 2);
            assert_eq!(budget.scanned_logical_bytes(), 12);
        } else {
            assert!(matches!(
                result,
                Err(ComponentPropertyError::ReadBudgetExceeded)
            ));
        }
    }
}

#[test]
fn root_only_builders_keep_plain_records_and_lists() {
    let shape = ComponentValueShape::record([("value".to_owned(), ComponentValueKind::U64.into())]);
    let ComponentValueShape::Record { members } = &shape else {
        panic!("record")
    };
    assert_eq!(members[0].id, None);
    assert_eq!(
        members[0],
        ComponentShapeMember::new("value", ComponentValueKind::U64)
    );
    assert_eq!(
        ComponentValueShape::list(shape.clone()),
        ComponentValueShape::List {
            item: Box::new(shape)
        },
    );
}

#[test]
fn reusable_member_ids_are_scoped_to_sibling_records_not_components() {
    let vector = ComponentValueShape::record_members([
        member("x", "x", ComponentValueKind::I64),
        member("y", "y", ComponentValueKind::I64),
    ]);
    let schema = schema(ComponentValueShape::record_members([
        member("position", "position", vector.clone()),
        member("velocity", "velocity", vector.clone()),
    ]))
    .with_fields([ComponentFieldSchema::required(
        id("second"),
        "Second",
        ComponentFieldPath::from_fields(["second"]),
        vector,
    )])
    .with_field_tombstones([id("x")]);
    let vector_value =
        ComponentValue::map([("x", ComponentValue::I64(2)), ("y", ComponentValue::I64(3))]);
    let value = ComponentValue::map([
        (
            "payload",
            ComponentValue::map([
                ("position", vector_value.clone()),
                ("velocity", vector_value.clone()),
            ]),
        ),
        ("second", vector_value),
    ]);
    assert!(validate_component_value(&schema, &value).is_ok());
    for root in ["position", "velocity"] {
        assert_eq!(
            resolve_component_property(
                &schema,
                schema.version,
                &value,
                &path([
                    ComponentPropertyStep::Member(id(root)),
                    ComponentPropertyStep::Member(id("x")),
                ])
            )
            .unwrap()
            .value,
            &ComponentValue::I64(2),
        );
    }
    let second = ComponentPropertyPath::new(id("second"))
        .with_steps([ComponentPropertyStep::Member(id("x"))]);
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &second)
            .unwrap()
            .value,
        &ComponentValue::I64(2)
    );
}

#[test]
fn duplicate_sibling_ids_and_invalid_member_ids_are_shape_errors() {
    let duplicate = ComponentValueShape::record_members([
        member("first", "same", ComponentValueKind::U64),
        member("second", "same", ComponentValueKind::U64),
    ]);
    let error = duplicate.validate().unwrap_err();
    assert_eq!(
        error.kind,
        ComponentShapeErrorKind::DuplicateMemberId(id("same"))
    );
    assert_eq!(error.path, ComponentFieldPath::from_fields(["second"]));
    assert_eq!(
        ComponentValueShape::record_members([member(
            "value",
            "invalid member",
            ComponentValueKind::U64
        )])
        .validate()
        .unwrap_err()
        .kind,
        ComponentShapeErrorKind::InvalidMemberId(id("invalid member")),
    );
}

#[test]
fn nested_members_resolve_by_parent_chain_not_name_or_leaf_id() {
    let schema = schema(ComponentValueShape::record_members([member(
        "wire_parent",
        "parent",
        ComponentValueShape::record_members([member("wire_leaf", "leaf", ComponentValueKind::U64)]),
    )]));
    let value = payload(ComponentValue::map([(
        "wire_parent",
        ComponentValue::map([("wire_leaf", ComponentValue::U64(7))]),
    )]));
    let correct = path([
        ComponentPropertyStep::Member(id("parent")),
        ComponentPropertyStep::Member(id("leaf")),
    ]);
    let resolved = resolve_component_property(&schema, schema.version, &value, &correct).unwrap();
    assert_eq!(resolved.value, &ComponentValue::U64(7));
    assert_eq!(
        resolved.shape,
        &ComponentValueShape::scalar(ComponentValueKind::U64)
    );
    for wrong in ["leaf", "wire_parent"] {
        assert!(matches!(
            resolve_component_property(
                &schema,
                schema.version,
                &value,
                &path([ComponentPropertyStep::Member(id(wrong))])
            ),
            Err(ComponentPropertyError::MemberNotFound { step: 0, .. }),
        ));
    }
}

#[test]
fn unidentified_ancestor_cannot_be_skipped_but_root_replacement_still_works() {
    let schema = schema(ComponentValueShape::record([(
        "anonymous".to_owned(),
        ComponentValueShape::record_members([member("leaf", "leaf", ComponentValueKind::U64)]),
    )]));
    let root = ComponentValue::map([(
        "anonymous",
        ComponentValue::map([("leaf", ComponentValue::U64(1))]),
    )]);
    let mut value = payload(root.clone());
    assert!(matches!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &path([ComponentPropertyStep::Member(id("leaf"))])
        ),
        Err(ComponentPropertyError::MemberNotFound { .. }),
    ));
    assert_eq!(
        replace_component_property(&schema, schema.version, &mut value, &path([]), root.clone())
            .unwrap(),
        root
    );
}

#[test]
fn keyed_reorder_preserves_selection_and_nested_replace_preserves_other_items() {
    let schema = schema(keyed_shape());
    let mut value = payload(ComponentValue::List(vec![
        row("b", "beta", 20),
        row("a", "alpha", 10),
    ]));
    let target = hp_path("a");
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(10)
    );
    assert_eq!(
        replace_component_property(
            &schema,
            schema.version,
            &mut value,
            &target,
            ComponentValue::U64(99)
        )
        .unwrap(),
        ComponentValue::U64(10)
    );
    assert_eq!(
        value,
        payload(ComponentValue::List(vec![
            row("b", "beta", 20),
            row("a", "alpha", 99)
        ]))
    );
    let ComponentValue::List(rows) = value.get_path_mut(&schema.fields[0].path).unwrap() else {
        panic!("list")
    };
    rows.reverse();
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(99)
    );
}

#[test]
fn changed_key_policy_never_reinterprets_an_old_selector() {
    let value = payload(ComponentValue::List(vec![
        row("a", "b", 10),
        row("b", "a", 20),
    ]));
    let mut schema = schema(keyed_shape());
    let old = hp_path("a");
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &old)
            .unwrap()
            .value,
        &ComponentValue::U64(10)
    );
    schema.fields[0].shape = ComponentValueShape::keyed_list(row_shape(), id("slug"));
    schema.version = ComponentSchemaVersion(2);
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &value, &old),
        Err(ComponentPropertyError::ListIdentityMismatch { step: 0, expected, actual })
            if expected == id("slug") && actual == id("id"),
    ));
    let mut current = old;
    current.steps[0] = ComponentPropertyStep::ListItem {
        by: id("slug"),
        key: ComponentPropertyKey::String("a".to_owned()),
    };
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &current)
            .unwrap()
            .value,
        &ComponentValue::U64(20)
    );
}

#[test]
fn changing_or_deleting_a_key_makes_old_selector_unresolved_reuse_asserts_continuity() {
    let schema = schema(keyed_shape());
    let mut value = payload(ComponentValue::List(vec![row("a", "alpha", 10)]));
    let key = path([selected("a"), ComponentPropertyStep::Member(id("id"))]);
    replace_component_property(
        &schema,
        schema.version,
        &mut value,
        &key,
        ComponentValue::String("c".to_owned()),
    )
    .unwrap();
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &value, &hp_path("a")),
        Err(ComponentPropertyError::ListItemNotFound { .. })
    ));
    let empty = payload(ComponentValue::List(Vec::new()));
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &empty, &hp_path("a")),
        Err(ComponentPropertyError::ListItemNotFound { .. })
    ));
    let restored = payload(ComponentValue::List(vec![row("a", "restored", 42)]));
    assert_eq!(
        resolve_component_property(&schema, schema.version, &restored, &hp_path("a"))
            .unwrap()
            .value,
        &ComponentValue::U64(42)
    );
}

#[test]
fn keyed_shapes_require_one_identified_non_optional_supported_scalar() {
    assert_eq!(
        ComponentValueShape::keyed_list(ComponentValueKind::U64.into(), id("id"))
            .validate()
            .unwrap_err()
            .kind,
        ComponentShapeErrorKind::InvalidKeyedListItem
    );
    assert_eq!(
        ComponentValueShape::keyed_list(
            ComponentValueShape::record([("id".to_owned(), ComponentValueKind::String.into())]),
            id("id")
        )
        .validate()
        .unwrap_err()
        .kind,
        ComponentShapeErrorKind::MissingListKey(id("id"))
    );
    for invalid in [
        ComponentValueShape::optional(ComponentValueKind::String.into()),
        ComponentValueKind::F64.into(),
        ComponentValueShape::EntityRef,
    ] {
        assert_eq!(
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([member("key", "id", invalid)]),
                id("id")
            )
            .validate()
            .unwrap_err()
            .kind,
            ComponentShapeErrorKind::InvalidListKeyShape(id("id"))
        );
    }
    for kind in [
        ComponentValueKind::String,
        ComponentValueKind::I64,
        ComponentValueKind::U64,
    ] {
        assert!(
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([member("key", "id", kind)]),
                id("id")
            )
            .validate()
            .is_ok()
        );
    }
}

#[test]
fn malformed_missing_and_duplicate_current_keys_refuse_without_first_match_fallback() {
    let shape = keyed_shape();
    let schema = schema(shape.clone());
    let duplicate = ComponentValue::List(vec![row("a", "first", 1), row("a", "second", 2)]);
    assert!(
        matches!(shape.validate_value(&duplicate).unwrap_err().kind, ComponentShapeErrorKind::DuplicateListKey(ComponentPropertyKey::String(key)) if key == "a")
    );
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &payload(duplicate), &hp_path("a")),
        Err(ComponentPropertyError::AmbiguousListItem { .. })
    ));
    for key in [
        None,
        Some(ComponentValue::U64(1)),
        Some(ComponentValue::Null),
    ] {
        let mut malformed = row("a", "alpha", 1);
        malformed
            .remove_path(&ComponentFieldPath::from_fields(["wire_id"]))
            .unwrap();
        if let Some(key) = key {
            malformed
                .set_path(&ComponentFieldPath::from_fields(["wire_id"]), key)
                .unwrap();
        }
        let invalid = ComponentValue::List(vec![malformed]);
        assert!(shape.validate_value(&invalid).is_err());
        assert!(matches!(
            resolve_component_property(&schema, schema.version, &payload(invalid), &hp_path("a")),
            Err(ComponentPropertyError::InvalidListItemKey { index: 0, .. })
        ));
    }
}

#[test]
fn keys_are_exact_typed_values_and_unkeyed_lists_have_no_durable_item_selector() {
    let schema = schema(ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([member("key", "id", ComponentValueKind::I64)]),
        id("id"),
    ));
    let value = payload(ComponentValue::List(vec![ComponentValue::map([(
        "key",
        ComponentValue::I64(7),
    )])]));
    let correct = path([ComponentPropertyStep::ListItem {
        by: id("id"),
        key: ComponentPropertyKey::I64(7),
    }]);
    assert!(resolve_component_property(&schema, schema.version, &value, &correct).is_ok());
    for key in [
        ComponentPropertyKey::U64(7),
        ComponentPropertyKey::String("7".to_owned()),
    ] {
        assert!(matches!(
            resolve_component_property(
                &schema,
                schema.version,
                &value,
                &path([ComponentPropertyStep::ListItem { by: id("id"), key }])
            ),
            Err(ComponentPropertyError::KeyTypeMismatch { .. })
        ));
    }
    let mut unkeyed = schema;
    unkeyed.fields[0].shape =
        ComponentValueShape::list(ComponentValueShape::record_members([member(
            "key",
            "id",
            ComponentValueKind::I64,
        )]));
    assert!(matches!(
        resolve_component_property(&unkeyed, unkeyed.version, &value, &correct),
        Err(ComponentPropertyError::UnkeyedList { .. })
    ));
}

#[test]
fn map_entries_and_record_members_are_not_interchangeable() {
    let schema = schema(ComponentValueShape::map(ComponentValueShape::optional(
        ComponentValueShape::record_members([member("wire", "member", ComponentValueKind::U64)]),
    )));
    let value = payload(ComponentValue::map([(
        "entry",
        ComponentValue::map([("wire", ComponentValue::U64(3))]),
    )]));
    let correct = path([
        ComponentPropertyStep::MapEntry("entry".to_owned()),
        ComponentPropertyStep::Member(id("member")),
    ]);
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &correct)
            .unwrap()
            .value,
        &ComponentValue::U64(3)
    );
    assert!(matches!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &path([ComponentPropertyStep::Member(id("entry"))])
        ),
        Err(ComponentPropertyError::ExpectedRecord { .. })
    ));
    assert!(matches!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &path([ComponentPropertyStep::MapEntry("missing".to_owned())])
        ),
        Err(ComponentPropertyError::MapEntryNotFound { .. })
    ));
    let wrong_inner = path([
        ComponentPropertyStep::MapEntry("entry".to_owned()),
        ComponentPropertyStep::MapEntry("wire".to_owned()),
    ]);
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &value, &wrong_inner),
        Err(ComponentPropertyError::ExpectedMap { step: 1 })
    ));
}

#[test]
fn optional_containers_are_transparent_only_when_present_and_never_created() {
    let schema = schema(ComponentValueShape::optional(
        ComponentValueShape::record_members([member("wire", "member", ComponentValueKind::U64)]),
    ));
    let target = path([ComponentPropertyStep::Member(id("member"))]);
    let mut absent = payload(ComponentValue::Null);
    let before = absent.clone();
    assert_eq!(
        resolve_component_property(&schema, schema.version, &absent, &target).unwrap_err(),
        ComponentPropertyError::OptionalAbsent { step: 0 }
    );
    assert!(
        replace_component_property(
            &schema,
            schema.version,
            &mut absent,
            &target,
            ComponentValue::U64(1)
        )
        .is_err()
    );
    assert_eq!(absent, before);
    assert_eq!(
        resolve_component_property(&schema, schema.version, &absent, &path([]))
            .unwrap()
            .value,
        &ComponentValue::Null
    );
    replace_component_property(
        &schema,
        schema.version,
        &mut absent,
        &path([]),
        ComponentValue::map([("wire", ComponentValue::U64(1))]),
    )
    .unwrap();
    assert_eq!(
        resolve_component_property(&schema, schema.version, &absent, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(1)
    );
}

#[test]
fn missing_root_values_are_not_synthesized_even_with_a_default() {
    let mut schema = schema(ComponentValueKind::U64.into());
    schema.fields[0].default_value = Some(ComponentValue::U64(42));
    let value = ComponentValue::map(Vec::<(String, ComponentValue)>::new());
    assert!(validate_component_value(&schema, &value).is_ok());
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &value, &path([])),
        Err(ComponentPropertyError::ValueUnavailable(_))
    ));
}

#[test]
fn tombstones_are_root_scoped_and_retired_parent_prefix_protects_descendants() {
    let parent = declaration([ComponentMemberPathSegment::Member(id("old"))]);
    let mut schema = schema(ComponentValueKind::U64.into());
    schema.fields[0].member_tombstones.push(parent.clone());
    let value = payload(ComponentValue::U64(1));
    assert_eq!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &path([
                ComponentPropertyStep::Member(id("old")),
                ComponentPropertyStep::Member(id("child"))
            ])
        )
        .unwrap_err(),
        ComponentPropertyError::RetiredMember(parent)
    );
    schema.field_tombstones.push(id("retired-root"));
    assert_eq!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &ComponentPropertyPath::new(id("retired-root"))
        )
        .unwrap_err(),
        ComponentPropertyError::RetiredField(id("retired-root"))
    );
    assert_eq!(
        resolve_component_property(
            &schema,
            schema.version,
            &value,
            &ComponentPropertyPath::new(id("unknown-root"))
        )
        .unwrap_err(),
        ComponentPropertyError::UnknownRoot(id("unknown-root"))
    );
}

#[test]
fn collection_tombstones_use_wildcards_and_do_not_confuse_same_id_at_other_scopes() {
    let schema = schema(ComponentValueShape::map(ComponentValueKind::U64.into()));
    let mut retired = schema.clone();
    let old = declaration([
        ComponentMemberPathSegment::MapEntry,
        ComponentMemberPathSegment::Member(id("old")),
    ]);
    retired.fields[0].member_tombstones.push(old.clone());
    let value = payload(ComponentValue::map([
        ("a", ComponentValue::U64(1)),
        ("b", ComponentValue::U64(2)),
    ]));
    for key in ["a", "b"] {
        assert_eq!(
            resolve_component_property(
                &retired,
                retired.version,
                &value,
                &path([
                    ComponentPropertyStep::MapEntry(key.to_owned()),
                    ComponentPropertyStep::Member(id("old"))
                ])
            )
            .unwrap_err(),
            ComponentPropertyError::RetiredMember(old.clone())
        );
    }
    let mut other_scope = schema;
    other_scope.fields[0]
        .member_tombstones
        .push(declaration([ComponentMemberPathSegment::Member(id("old"))]));
    assert!(matches!(
        resolve_component_property(
            &other_scope,
            other_scope.version,
            &value,
            &path([
                ComponentPropertyStep::MapEntry("a".to_owned()),
                ComponentPropertyStep::Member(id("old"))
            ])
        ),
        Err(ComponentPropertyError::ExpectedRecord { .. })
    ));
}

#[test]
fn nested_keyed_map_and_optional_scopes_replace_and_report_retired_descendants() {
    let row = ComponentValueShape::record_members([
        member("wire_id", "id", ComponentValueKind::String),
        member(
            "wire_settings",
            "settings",
            ComponentValueShape::map(ComponentValueShape::optional(
                ComponentValueShape::record_members([member(
                    "wire_value",
                    "value",
                    ComponentValueKind::U64,
                )]),
            )),
        ),
    ]);
    let mut schema = schema(ComponentValueShape::optional(
        ComponentValueShape::keyed_list(row, id("id")),
    ));
    let retired = declaration([
        ComponentMemberPathSegment::ListItem,
        ComponentMemberPathSegment::Member(id("settings")),
        ComponentMemberPathSegment::MapEntry,
        ComponentMemberPathSegment::Member(id("old")),
    ]);
    schema.fields[0].member_tombstones.push(retired.clone());
    let mut value = payload(ComponentValue::List(vec![ComponentValue::map([
        ("wire_id", ComponentValue::String("a".to_owned())),
        (
            "wire_settings",
            ComponentValue::map([(
                "slot",
                ComponentValue::map([("wire_value", ComponentValue::U64(5))]),
            )]),
        ),
    ])]));
    let target = path([
        selected("a"),
        ComponentPropertyStep::Member(id("settings")),
        ComponentPropertyStep::MapEntry("slot".to_owned()),
        ComponentPropertyStep::Member(id("value")),
    ]);
    assert_eq!(
        replace_component_property(
            &schema,
            schema.version,
            &mut value,
            &target,
            ComponentValue::U64(6)
        )
        .unwrap(),
        ComponentValue::U64(5)
    );
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(6)
    );
    let mut old = target;
    old.steps[3] = ComponentPropertyStep::Member(id("old"));
    assert_eq!(
        resolve_component_property(&schema, schema.version, &value, &old).unwrap_err(),
        ComponentPropertyError::RetiredMember(retired)
    );
}

#[test]
fn replacement_is_atomic_for_invalid_shape_missing_target_and_duplicate_key() {
    let schema = schema(keyed_shape());
    let mut value = payload(ComponentValue::List(vec![
        row("a", "alpha", 1),
        row("b", "beta", 2),
    ]));
    let before = value.clone();
    assert!(matches!(
        replace_component_property(
            &schema,
            schema.version,
            &mut value,
            &hp_path("a"),
            ComponentValue::String("wrong".to_owned())
        ),
        Err(ComponentPropertyError::InvalidReplacementShape(_))
    ));
    assert_eq!(value, before);
    let key = path([selected("a"), ComponentPropertyStep::Member(id("id"))]);
    assert!(matches!(
        replace_component_property(
            &schema,
            schema.version,
            &mut value,
            &key,
            ComponentValue::String("b".to_owned())
        ),
        Err(ComponentPropertyError::InvalidReplacement(_))
    ));
    assert_eq!(value, before);
    assert!(
        replace_component_property(
            &schema,
            schema.version,
            &mut value,
            &hp_path("missing"),
            ComponentValue::U64(3)
        )
        .is_err()
    );
    assert_eq!(value, before);
}

#[test]
fn resolver_rejects_invalid_siblings_even_if_selected_target_is_valid() {
    let schema = schema(keyed_shape());
    let value = payload(ComponentValue::List(vec![
        row("a", "alpha", 1),
        row("b", "beta", 2),
        row("b", "duplicate", 3),
    ]));
    assert!(matches!(
        resolve_component_property(&schema, schema.version, &value, &hp_path("a")),
        Err(ComponentPropertyError::InvalidValue(_))
    ));
}

#[test]
fn candidate_total_size_is_checked_before_atomic_publication() {
    let schema = schema(ComponentValueShape::record_members([
        member("a", "a", ComponentValueKind::String),
        member("b", "b", ComponentValueKind::String),
    ]));
    let large = "x".repeat(2 * 1024 * 1024);
    let mut value = payload(ComponentValue::map([
        ("a", ComponentValue::String(large.clone())),
        ("b", ComponentValue::String(String::new())),
    ]));
    let before = value.clone();
    assert!(
        matches!(replace_component_property(&schema, schema.version, &mut value, &path([ComponentPropertyStep::Member(id("b"))]), ComponentValue::String(large)), Err(ComponentPropertyError::ValueBudget(error)) if error.kind == ComponentShapeErrorKind::ByteLimit)
    );
    assert_eq!(value, before);
}

#[derive(Component)]
struct NativeValue(ComponentValue);

#[test]
fn selector_survives_explicit_wire_migration_but_unmigrated_and_future_versions_refuse() {
    let old_schema = schema(ComponentValueShape::record_members([member(
        "old_wire",
        "member",
        ComponentValueKind::U64,
    )]));
    let old_value = payload(ComponentValue::map([("old_wire", ComponentValue::U64(5))]));
    let target = path([ComponentPropertyStep::Member(id("member"))]);
    assert_eq!(
        resolve_component_property(&old_schema, old_schema.version, &old_value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(5)
    );
    let mut current = schema(ComponentValueShape::record_members([member(
        "new_wire",
        "member",
        ComponentValueKind::U64,
    )]));
    current.version = ComponentSchemaVersion(2);
    let mut registry = ComponentRegistry::new();
    registry.register_component_schema(current.clone()).unwrap();
    registry
        .register_native_component_with_codec::<NativeValue, _, _>(
            &current.id,
            |value| Ok(NativeValue(value.clone())),
            |value| Ok(value.0.clone()),
        )
        .unwrap();
    registry
        .register_component_migration(
            &current.id,
            ComponentSchemaVersion::ONE,
            current.version,
            |mut value| {
                let previous = value
                    .remove_path(&ComponentFieldPath::from_fields(["payload", "old_wire"]))
                    .map_err(|error| {
                        nara_reflect::ComponentCodecError::Message(error.to_string())
                    })?;
                value
                    .set_path(
                        &ComponentFieldPath::from_fields(["payload", "new_wire"]),
                        previous,
                    )
                    .map_err(|error| {
                        nara_reflect::ComponentCodecError::Message(error.to_string())
                    })?;
                Ok(value)
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let admitted = registry.schema(&current.id).unwrap();
    for version in [ComponentSchemaVersion::ONE, ComponentSchemaVersion(3)] {
        assert!(matches!(
            resolve_component_property(admitted, version, &old_value, &target),
            Err(ComponentPropertyError::VersionMismatch { .. })
        ));
    }
    assert!(resolve_component_property(admitted, admitted.version, &old_value, &target).is_err());
    let migrated = registry
        .migrate_component_value(&current.id, ComponentSchemaVersion::ONE, &old_value)
        .unwrap();
    assert_eq!(
        resolve_component_property(admitted, migrated.version, &migrated.value, &target)
            .unwrap()
            .value,
        &ComponentValue::U64(5)
    );
}

#[test]
fn declaration_paths_require_a_member_terminal_and_valid_bounded_ids() {
    for invalid in [
        declaration([]),
        declaration([ComponentMemberPathSegment::ListItem]),
        declaration([ComponentMemberPathSegment::MapEntry]),
    ] {
        assert_eq!(
            invalid.validate(),
            Err(ComponentMemberPathError::MustEndWithMember)
        );
    }
    assert!(
        declaration([
            ComponentMemberPathSegment::ListItem,
            ComponentMemberPathSegment::MapEntry,
            ComponentMemberPathSegment::Member(id("member"))
        ])
        .validate()
        .is_ok()
    );
    assert!(matches!(
        declaration([ComponentMemberPathSegment::Member(id("bad id"))]).validate(),
        Err(ComponentMemberPathError::InvalidMemberId(_))
    ));
    assert_eq!(
        declaration((0..65).map(|_| ComponentMemberPathSegment::Member(id("member")))).validate(),
        Err(ComponentMemberPathError::DepthLimit)
    );
    assert_eq!(
        path((0..65).map(|_| ComponentPropertyStep::Member(id("member")))).validate(),
        Err(ComponentPropertyError::PathDepthLimit)
    );
}

fn catalog(schema: ComponentSchema) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: vec![schema],
        ..ComponentSchemaCatalog::default()
    }
}

#[test]
fn identity_metadata_key_policy_and_root_scoped_tombstones_change_fingerprints() {
    let plain = catalog(schema(ComponentValueShape::record([(
        "wire".to_owned(),
        ComponentValueKind::U64.into(),
    )])));
    let identified = catalog(schema(ComponentValueShape::record_members([member(
        "wire",
        "member",
        ComponentValueKind::U64,
    )])));
    assert_ne!(plain.fingerprint(), identified.fingerprint());
    let keyed = catalog(schema(keyed_shape()));
    let unkeyed = catalog(schema(ComponentValueShape::list(row_shape())));
    let other_key = catalog(schema(ComponentValueShape::keyed_list(
        row_shape(),
        id("slug"),
    )));
    let fingerprints = [
        keyed.fingerprint(),
        unkeyed.fingerprint(),
        other_key.fingerprint(),
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(fingerprints.len(), 3);
    let mut reordered_keyed = keyed.clone();
    let ComponentValueShape::KeyedList { item, .. } =
        &mut reordered_keyed.components[0].fields[0].shape
    else {
        panic!("keyed list")
    };
    let ComponentValueShape::Record { members } = item.as_mut() else {
        panic!("record")
    };
    members.reverse();
    assert_eq!(keyed.fingerprint(), reordered_keyed.fingerprint());
    reordered_keyed.canonicalize();
    let mut canonical_keyed = keyed;
    canonical_keyed.canonicalize();
    assert_eq!(canonical_keyed, reordered_keyed);
    let mut retired = identified.clone();
    retired.components[0].fields[0]
        .member_tombstones
        .push(declaration([ComponentMemberPathSegment::Member(id(
            "retired",
        ))]));
    assert_ne!(identified.fingerprint(), retired.fingerprint());
    let first = declaration([
        ComponentMemberPathSegment::Member(id("parent")),
        ComponentMemberPathSegment::ListItem,
        ComponentMemberPathSegment::Member(id("leaf")),
    ]);
    let second = declaration([
        ComponentMemberPathSegment::Member(id("parent")),
        ComponentMemberPathSegment::MapEntry,
        ComponentMemberPathSegment::Member(id("leaf")),
    ]);
    retired.components[0].fields[0].member_tombstones = vec![first.clone(), second.clone()];
    let mut reordered = retired.clone();
    reordered.components[0].fields[0]
        .member_tombstones
        .reverse();
    let ComponentValueShape::Record { members } = &mut reordered.components[0].fields[0].shape
    else {
        panic!("record")
    };
    members.reverse();
    assert_eq!(retired.fingerprint(), reordered.fingerprint());
    retired.canonicalize();
    reordered.canonicalize();
    assert_eq!(retired, reordered);
}

#[cfg(feature = "serde")]
#[test]
fn selector_serde_uses_explicit_step_key_and_key_member_tags_in_json_and_ron() {
    let selector = path([
        ComponentPropertyStep::Member(id("rows")),
        ComponentPropertyStep::ListItem {
            by: id("id"),
            key: ComponentPropertyKey::U64(u64::MAX),
        },
        ComponentPropertyStep::MapEntry("entry".to_owned()),
    ]);
    let json = serde_json::to_value(&selector).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "root": "root", "steps": [
                { "kind": "member", "value": "rows" },
                { "kind": "list_item", "value": { "by": "id", "key": { "kind": "u64", "value": u64::MAX } } },
                { "kind": "map_entry", "value": "entry" },
            ],
        })
    );
    assert_eq!(
        serde_json::from_value::<ComponentPropertyPath>(json).unwrap(),
        selector
    );
    assert_eq!(
        ron::from_str::<ComponentPropertyPath>(&ron::to_string(&selector).unwrap()).unwrap(),
        selector
    );
    for key in [
        ComponentPropertyKey::I64(i64::MIN),
        ComponentPropertyKey::String("exact".to_owned()),
    ] {
        assert_eq!(
            serde_json::from_str::<ComponentPropertyKey>(&serde_json::to_string(&key).unwrap())
                .unwrap(),
            key
        );
    }
    for invalid in [
        serde_json::json!({ "kind": "index", "value": 0 }),
        serde_json::json!({ "kind": "list_item", "value": { "key": { "kind": "string", "value": "a" } } }),
        serde_json::json!({ "kind": "list_item", "value": { "by": "id", "key": { "kind": "f64", "value": 1.0 } } }),
    ] {
        assert!(serde_json::from_value::<ComponentPropertyStep>(invalid).is_err());
    }
}

#[cfg(feature = "serde")]
#[test]
fn declaration_paths_and_schema_identity_metadata_roundtrip_without_erasing_tombstones() {
    let tombstone = declaration([
        ComponentMemberPathSegment::ListItem,
        ComponentMemberPathSegment::Member(id("old")),
    ]);
    let mut schema = schema(keyed_shape());
    schema.fields[0] = schema.fields[0]
        .clone()
        .with_member_tombstones([tombstone.clone()]);
    assert_eq!(
        serde_json::to_value(&tombstone).unwrap(),
        serde_json::json!([{ "kind": "list_item" }, { "kind": "member", "value": "old" }])
    );
    assert_eq!(
        serde_json::from_str::<ComponentSchema>(&serde_json::to_string(&schema).unwrap()).unwrap(),
        schema
    );
    assert_eq!(
        ron::from_str::<ComponentSchema>(&ron::to_string(&schema).unwrap()).unwrap(),
        schema
    );
    schema.fields[0].shape = ComponentValueKind::U64.into();
    let decoded =
        serde_json::from_str::<ComponentSchema>(&serde_json::to_string(&schema).unwrap()).unwrap();
    assert_eq!(decoded.fields[0].member_tombstones, vec![tombstone]);
    assert!(
        serde_json::from_value::<ComponentMemberPath>(serde_json::json!([{ "kind": "list_item" }]))
            .is_err()
    );
}
