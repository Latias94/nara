use nara_reflect::{
    ComponentFieldId, ComponentPropertyKey, ComponentShapeMember, ComponentTypeId,
    ComponentValueKind,
};

use super::*;

fn id(value: &str) -> ComponentFieldId {
    ComponentFieldId::new(value)
}

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("game.OverlayProperties"),
        "Overlay properties",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

fn root_schema(shape: impl Into<ComponentValueShape>) -> ComponentSchema {
    schema([ComponentFieldSchema::required(
        id("root"),
        "Root",
        ComponentFieldPath::from_fields(["payload"]),
        shape,
    )])
}

fn member(name: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(name, shape).with_id(id(stable))
}

fn keyed_schema() -> ComponentSchema {
    root_schema(ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            member("wire_id", "id", ComponentValueKind::String),
            member(
                "wire_stats",
                "stats",
                ComponentValueShape::record_members([
                    member("health", "hp", ComponentValueKind::U64),
                    member("armor", "armor", ComponentValueKind::U64),
                ]),
            ),
            member(
                "wire_children",
                "children",
                ComponentValueShape::keyed_list(
                    ComponentValueShape::record_members([
                        member("wire_id", "id", ComponentValueKind::String),
                        member("wire_score", "score", ComponentValueKind::U64),
                    ]),
                    id("id"),
                ),
            ),
        ]),
        id("id"),
    ))
}

fn child(key: &str, score: u64) -> ComponentValue {
    ComponentValue::map([
        ("wire_id", ComponentValue::String(key.into())),
        ("wire_score", ComponentValue::U64(score)),
    ])
}

fn row(key: &str, hp: u64, armor: u64, children: Vec<ComponentValue>) -> ComponentValue {
    ComponentValue::map([
        ("wire_id", ComponentValue::String(key.into())),
        (
            "wire_stats",
            ComponentValue::map([
                ("health", ComponentValue::U64(hp)),
                ("armor", ComponentValue::U64(armor)),
            ]),
        ),
        ("wire_children", ComponentValue::List(children)),
    ])
}

fn payload(value: ComponentValue) -> ComponentValue {
    ComponentValue::map([("payload", value)])
}

fn base() -> ComponentValue {
    payload(ComponentValue::List(vec![
        row("a", 10, 20, vec![child("x", 7), child("y", 8)]),
        row("b", 30, 40, vec![]),
    ]))
}

fn selected(key: &str) -> ComponentPropertyStep {
    ComponentPropertyStep::ListItem {
        by: id("id"),
        key: ComponentPropertyKey::String(key.into()),
    }
}

fn path(steps: impl IntoIterator<Item = ComponentPropertyStep>) -> ComponentPropertyPath {
    ComponentPropertyPath::new(id("root")).with_steps(steps)
}

fn hp() -> ComponentPropertyPath {
    path([
        selected("a"),
        ComponentPropertyStep::Member(id("stats")),
        ComponentPropertyStep::Member(id("hp")),
    ])
}

fn set(path: ComponentPropertyPath, value: ComponentValue) -> PropertyClaim {
    PropertyClaim {
        path,
        intent: PropertyIntent::Set(value),
    }
}

fn absent(root: &str) -> PropertyClaim {
    PropertyClaim {
        path: ComponentPropertyPath::new(id(root)),
        intent: PropertyIntent::Absent,
    }
}

fn apply(
    schema: &ComponentSchema,
    original: &ComponentValue,
    claims: &[PropertyClaim],
) -> Result<ComponentValue, Conflict> {
    apply_claims(schema, schema.version(), original, claims)
}

#[test]
fn equal_pins_survive_rebase_and_nested_reorder_while_uncovered_siblings_inherit() {
    let schema = keyed_schema();
    let original = base();
    let mut claims = vec![
        set(hp(), ComponentValue::U64(10)),
        set(
            path([
                selected("a"),
                ComponentPropertyStep::Member(id("children")),
                selected("x"),
                ComponentPropertyStep::Member(id("score")),
            ]),
            ComponentValue::U64(7),
        ),
    ];
    let retained = claims.clone();
    assert_eq!(apply(&schema, &original, &claims), Ok(original.clone()));
    assert_eq!(claims, retained);
    let changed = payload(ComponentValue::List(vec![
        row("b", 300, 400, vec![]),
        row("a", 100, 200, vec![child("y", 80), child("x", 70)]),
    ]));
    let expected = payload(ComponentValue::List(vec![
        row("b", 300, 400, vec![]),
        row("a", 10, 200, vec![child("y", 80), child("x", 7)]),
    ]));
    assert_eq!(apply(&schema, &changed, &claims), Ok(expected.clone()));
    claims.reverse();
    assert_eq!(apply(&schema, &changed, &claims), Ok(expected));
    assert_eq!(apply(&schema, &changed, &[]), Ok(changed.clone()));
}

#[test]
fn duplicate_and_ancestor_claims_refuse_both_input_orders() {
    let schema = keyed_schema();
    let original = base();
    let leaf = set(hp(), ComponentValue::U64(99));
    let root = set(path([]), original.get("payload").unwrap().clone());
    let item = set(
        path([selected("a")]),
        row("a", 10, 20, vec![child("x", 7), child("y", 8)]),
    );
    for pair in [
        [leaf.clone(), leaf.clone()],
        [root, leaf.clone()],
        [item, leaf],
    ] {
        assert_eq!(
            apply(&schema, &original, &pair),
            Err(Conflict::DuplicateIntent)
        );
        let [first, second] = pair;
        assert_eq!(
            apply(&schema, &original, &[second, first]),
            Err(Conflict::DuplicateIntent),
        );
    }
}

#[test]
fn key_leaf_and_key_changing_item_record_refuse_at_every_enclosing_list() {
    let schema = keyed_schema();
    let original = base();
    for claim in [
        set(
            path([selected("a"), ComponentPropertyStep::Member(id("id"))]),
            ComponentValue::String("new".into()),
        ),
        set(path([selected("a")]), row("new", 10, 20, vec![])),
        set(
            path([
                selected("a"),
                ComponentPropertyStep::Member(id("children")),
                selected("x"),
            ]),
            child("new", 7),
        ),
        set(
            path([
                selected("a"),
                ComponentPropertyStep::Member(id("children")),
                selected("x"),
                ComponentPropertyStep::Member(id("id")),
            ]),
            ComponentValue::String("new".into()),
        ),
    ] {
        assert_eq!(
            apply(&schema, &original, &[claim]),
            Err(Conflict::Component)
        );
        assert_eq!(original, base());
    }
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                path([selected("a"), ComponentPropertyStep::Member(id("id"))]),
                ComponentValue::String("a".into()),
            )],
        ),
        Ok(original.clone()),
    );
    let replacement = row("a", 50, 60, vec![child("new", 9)]);
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(path([selected("a")]), replacement.clone())]
        ),
        Ok(payload(ComponentValue::List(vec![
            replacement,
            row("b", 30, 40, vec![])
        ]))),
    );
}

#[test]
fn whole_keyed_container_replacement_can_change_keys_but_excludes_descendants() {
    let schema = keyed_schema();
    let original = base();
    let new_rows = ComponentValue::List(vec![row("new", 1, 2, vec![child("new-child", 3)])]);
    assert_eq!(
        apply(&schema, &original, &[set(path([]), new_rows.clone())]),
        Ok(payload(new_rows)),
    );
    let children_path = path([selected("a"), ComponentPropertyStep::Member(id("children"))]);
    let container = set(children_path, ComponentValue::List(vec![child("new", 9)]));
    let expected = payload(ComponentValue::List(vec![
        row("a", 10, 20, vec![child("new", 9)]),
        row("b", 30, 40, vec![]),
    ]));
    assert_eq!(
        apply(&schema, &original, std::slice::from_ref(&container)),
        Ok(expected)
    );
    let leaf = set(
        path([
            selected("a"),
            ComponentPropertyStep::Member(id("children")),
            selected("x"),
            ComponentPropertyStep::Member(id("score")),
        ]),
        ComponentValue::U64(90),
    );
    for claims in [[container.clone(), leaf.clone()], [leaf, container]] {
        assert_eq!(
            apply(&schema, &original, &claims),
            Err(Conflict::DuplicateIntent)
        );
    }
}

#[test]
fn all_selectors_use_the_base_and_a_later_failure_preserves_original_and_claims() {
    let schema = keyed_schema();
    let original = base();
    let snapshot = original.clone();
    let claims = vec![
        set(hp(), ComponentValue::U64(99)),
        set(
            path([selected("b"), ComponentPropertyStep::Member(id("id"))]),
            ComponentValue::String("new".into()),
        ),
    ];
    let retained = claims.clone();
    // Both replacements pass complete shape validation. The final key-identity check fails.
    assert_eq!(apply(&schema, &original, &claims), Err(Conflict::Component));
    assert_eq!(original, snapshot);
    assert_eq!(claims, retained);
    let mut claims = vec![
        set(
            path([selected("a"), ComponentPropertyStep::Member(id("id"))]),
            ComponentValue::String("new".into()),
        ),
        set(
            path([
                selected("new"),
                ComponentPropertyStep::Member(id("stats")),
                ComponentPropertyStep::Member(id("hp")),
            ]),
            ComponentValue::U64(99),
        ),
    ];
    assert_eq!(apply(&schema, &original, &claims), Err(Conflict::Component));
    claims.reverse();
    assert_eq!(apply(&schema, &original, &claims), Err(Conflict::Component));
    assert_eq!(original, snapshot);
}

fn optional_schema(default: Option<ComponentValue>) -> ComponentSchema {
    let mut value = ComponentFieldSchema::optional(
        id("value"),
        "Value",
        ComponentFieldPath::from_fields(["settings", "wire_value"]),
        ComponentValueKind::U64,
    );
    value.default_value = default;
    schema([
        value,
        ComponentFieldSchema::required(
            id("keep"),
            "Keep",
            ComponentFieldPath::from_fields(["settings", "wire_keep"]),
            ComponentValueKind::U64,
        ),
    ])
}

fn settings(value: Option<ComponentValue>) -> ComponentValue {
    let mut settings = ComponentValue::map([("wire_keep", ComponentValue::U64(9))]);
    if let Some(value) = value {
        settings
            .set_path(&ComponentFieldPath::from_fields(["wire_value"]), value)
            .unwrap();
    }
    ComponentValue::map([("settings", settings)])
}

#[test]
fn absent_removes_only_root_storage_defaults_reapply_and_null_and_revert_differ() {
    let mut schema = optional_schema(Some(ComponentValue::U64(7)));
    let original = settings(Some(ComponentValue::U64(20)));
    let expected = settings(None);
    let claims = [absent("value")];
    assert_eq!(apply(&schema, &original, &claims), Ok(expected.clone()));
    assert_eq!(apply(&schema, &expected, &claims), Ok(expected.clone()));
    assert_eq!(
        effective_component_value(&schema, &expected, Default::default())
            .unwrap()
            .as_ref(),
        &settings(Some(ComponentValue::U64(7))),
    );
    let null = settings(Some(ComponentValue::Null));
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                ComponentPropertyPath::new(id("value")),
                ComponentValue::Null
            )],
        ),
        Ok(null.clone()),
    );
    assert_eq!(
        effective_component_value(&schema, &null, Default::default())
            .unwrap()
            .as_ref(),
        &null,
    );
    // Revert is deletion of the retained claim: the new base value is inherited again.
    assert_eq!(apply(&schema, &original, &[]), Ok(original.clone()));
    schema.fields[0].required = true;
    assert_eq!(apply(&schema, &original, &claims), Ok(expected));
}

#[test]
fn absence_preserves_an_empty_ancestor_and_missing_optional_root_set_is_supported() {
    let mut schema = optional_schema(None);
    schema.fields.pop();
    let empty = ComponentValue::map([(
        "settings",
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
    )]);
    let explicit = ComponentValue::map([(
        "settings",
        ComponentValue::map([("wire_value", ComponentValue::U64(5))]),
    )]);
    assert_eq!(
        apply(&schema, &explicit, &[absent("value")]),
        Ok(empty.clone())
    );
    assert_eq!(
        apply(&schema, &empty, &[absent("value")]),
        Ok(empty.clone())
    );
    assert_eq!(
        apply(
            &schema,
            &empty,
            &[set(
                ComponentPropertyPath::new(id("value")),
                ComponentValue::U64(5)
            )]
        ),
        Ok(explicit),
    );
    let missing_parent = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    assert_eq!(
        apply(
            &schema,
            &missing_parent,
            &[set(
                ComponentPropertyPath::new(id("value")),
                ComponentValue::U64(5)
            )]
        ),
        Err(Conflict::Component),
    );
    assert_eq!(
        apply(&schema, &missing_parent, &[absent("value")]),
        Ok(missing_parent.clone())
    );
    let malformed_parent = ComponentValue::map([("settings", ComponentValue::Null)]);
    assert_eq!(
        apply(&schema, &malformed_parent, &[absent("value")]),
        Err(Conflict::Component)
    );
}

#[test]
fn missing_default_root_initializes_only_that_root_for_a_descendant_set() {
    let default = ComponentValue::map([("wire_score", ComponentValue::U64(7))]);
    let shape = ComponentValueShape::record_members([member(
        "wire_score",
        "score",
        ComponentValueKind::U64,
    )]);
    let schema = schema(["first", "second"].map(|name| {
        ComponentFieldSchema::optional_with_default(
            id(name),
            name,
            ComponentFieldPath::from_fields([name]),
            shape.clone(),
            default.clone(),
        )
    }));
    let original = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let claim = set(
        ComponentPropertyPath::new(id("first"))
            .with_steps([ComponentPropertyStep::Member(id("score"))]),
        ComponentValue::U64(9),
    );
    assert_eq!(
        apply(&schema, &original, std::slice::from_ref(&claim)),
        Ok(ComponentValue::map([(
            "first",
            ComponentValue::map([("wire_score", ComponentValue::U64(9))])
        )])),
    );
    let mut no_default = schema.clone();
    no_default.fields[0].default_value = None;
    assert_eq!(
        apply(&no_default, &original, std::slice::from_ref(&claim)),
        Err(Conflict::Component)
    );
    let null = ComponentValue::map([("first", ComponentValue::Null)]);
    assert_eq!(
        apply(&schema, &null, std::slice::from_ref(&claim)),
        Err(Conflict::Component)
    );
    let mut nested_storage = schema.clone();
    nested_storage.fields[0].path = ComponentFieldPath::from_fields(["missing", "first"]);
    assert_eq!(
        apply(&nested_storage, &original, &[claim]),
        Err(Conflict::Component)
    );
}

#[test]
fn absent_rejects_required_nested_and_non_map_terminal_roots() {
    assert_eq!(
        apply(&keyed_schema(), &base(), &[absent("root")]),
        Err(Conflict::Component)
    );
    assert_eq!(
        apply(
            &keyed_schema(),
            &base(),
            &[PropertyClaim {
                path: hp(),
                intent: PropertyIntent::Absent
            }]
        ),
        Err(Conflict::Component),
    );
    let schema = schema([ComponentFieldSchema::optional(
        id("root"),
        "Root",
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("payload"),
            ComponentFieldPathSegment::index(0),
        ]),
        ComponentValueKind::U64,
    )]);
    let original = payload(ComponentValue::List(vec![ComponentValue::U64(3)]));
    assert_eq!(
        apply(&schema, &original, &[absent("root")]),
        Err(Conflict::Component)
    );
}

#[test]
fn version_unknown_retired_malformed_and_incomplete_values_refuse() {
    let schema = keyed_schema();
    let original = base();
    assert_eq!(
        apply_claims(&schema, ComponentSchemaVersion(2), &original, &[]),
        Err(Conflict::Component),
    );
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                ComponentPropertyPath::new(id("unknown")),
                ComponentValue::U64(1)
            )]
        ),
        Err(Conflict::Component),
    );
    let mut retired = schema.clone();
    retired.fields.clear();
    retired.field_tombstones.push(id("root"));
    assert_eq!(
        apply(
            &retired,
            &ComponentValue::map([] as [(&str, ComponentValue); 0]),
            &[absent("root")]
        ),
        Err(Conflict::Component),
    );
    assert_eq!(
        apply(&schema, &payload(ComponentValue::U64(1)), &[]),
        Err(Conflict::Component)
    );
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(hp(), ComponentValue::String("wrong".into()))]
        ),
        Err(Conflict::Component)
    );
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                path([selected("a")]),
                ComponentValue::map([("wire_id", ComponentValue::String("a".into()))])
            )]
        ),
        Err(Conflict::Component),
    );
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                path([]),
                ComponentValue::List(vec![row("a", 1, 2, vec![]), row("a", 3, 4, vec![])])
            )]
        ),
        Err(Conflict::Component),
    );
    assert_eq!(
        apply(
            &schema,
            &original,
            &[set(
                path([ComponentPropertyStep::ListItem {
                    by: id("wrong"),
                    key: ComponentPropertyKey::String("a".into())
                }]),
                row("a", 1, 2, vec![])
            )]
        ),
        Err(Conflict::Component),
    );
}

#[test]
fn map_keys_are_literal_and_exactly_sixty_four_disjoint_claims_are_admitted() {
    let schema = root_schema(ComponentValueShape::map(ComponentValueKind::U64.into()));
    let original =
        payload(ComponentValue::map((0..MAX_CLAIMS).map(|index| {
            (format!("entry.{index}"), ComponentValue::U64(0))
        })));
    let mut claims = (0..MAX_CLAIMS)
        .map(|index| {
            set(
                path([ComponentPropertyStep::MapEntry(format!("entry.{index}"))]),
                ComponentValue::U64(index as u64 + 1),
            )
        })
        .collect::<Vec<_>>();
    let expected = payload(ComponentValue::map((0..MAX_CLAIMS).map(|index| {
        (
            format!("entry.{index}"),
            ComponentValue::U64(index as u64 + 1),
        )
    })));
    assert_eq!(admit_claims(&claims.iter().collect::<Vec<_>>()), Ok(()));
    assert_eq!(apply(&schema, &original, &claims), Ok(expected.clone()));
    claims.reverse();
    assert_eq!(apply(&schema, &original, &claims), Ok(expected));
    claims.push(claims[0].clone());
    assert_eq!(
        admit_claims(&claims.iter().collect::<Vec<_>>()),
        Err(Conflict::WorkLimit)
    );
    assert_eq!(apply(&schema, &original, &claims), Err(Conflict::WorkLimit));
}

#[test]
fn selector_and_replacement_budgets_have_inclusive_boundaries_before_copy() {
    let scalar = root_schema(ComponentValueKind::String);
    let original = payload(ComponentValue::String(String::new()));
    for (bytes, expected) in [(MAX_CLAIM_BYTES - 4, true), (MAX_CLAIM_BYTES - 3, false)] {
        let value = ComponentValue::String("x".repeat(bytes));
        let claim = set(path([]), value.clone());
        let admitted = admit_claims(&[&claim]);
        let result = apply(&scalar, &original, &[claim]);
        if expected {
            assert_eq!(admitted, Ok(()));
            assert_eq!(result, Ok(payload(value)));
        } else {
            assert_eq!(admitted, Err(Conflict::WorkLimit));
            assert_eq!(result, Err(Conflict::WorkLimit));
        }
    }
    let map = root_schema(ComponentValueShape::map(ComponentValueShape::optional(
        ComponentValueKind::U64.into(),
    )));
    for (bytes, expected) in [(MAX_CLAIM_BYTES - 4, true), (MAX_CLAIM_BYTES - 3, false)] {
        let key = "x".repeat(bytes);
        let original = payload(ComponentValue::map([(key.clone(), ComponentValue::Null)]));
        let result = apply(
            &map,
            &original,
            &[set(
                path([ComponentPropertyStep::MapEntry(key)]),
                ComponentValue::Null,
            )],
        );
        if expected {
            assert_eq!(result, Ok(original));
        } else {
            assert_eq!(result, Err(Conflict::WorkLimit));
        }
    }
    let list = root_schema(ComponentValueShape::list(ComponentValueKind::U64.into()));
    let original = payload(ComponentValue::List(vec![]));
    for (items, expected) in [(MAX_CLAIM_NODES - 1, true), (MAX_CLAIM_NODES, false)] {
        let value = ComponentValue::List(vec![ComponentValue::U64(0); items]);
        let result = apply(&list, &original, &[set(path([]), value.clone())]);
        if expected {
            assert_eq!(result, Ok(payload(value)));
        } else {
            assert_eq!(result, Err(Conflict::WorkLimit));
        }
    }
    let original = payload(ComponentValue::List(vec![
        ComponentValue::U64(0);
        MAX_VALUE_NODES
    ]));
    assert_eq!(apply(&list, &original, &[]), Err(Conflict::WorkLimit));
}

#[test]
fn private_wire_claims_round_trip_and_reject_unknown_members() {
    let claim = set(hp(), ComponentValue::U64(10));
    let encoded = serde_json::to_value(&claim).unwrap();
    assert_eq!(
        serde_json::from_value::<PropertyClaim>(encoded.clone()).unwrap(),
        claim
    );
    let mut extra = encoded;
    extra["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PropertyClaim>(extra).is_err());
    let mut extra = serde_json::to_value(&claim).unwrap();
    extra["intent"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<PropertyClaim>(extra).is_err());
    let claim = absent("root");
    assert_eq!(
        serde_json::from_value::<PropertyClaim>(serde_json::to_value(&claim).unwrap()).unwrap(),
        claim
    );
}
