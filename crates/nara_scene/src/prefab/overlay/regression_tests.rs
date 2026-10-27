use nara_reflect::{
    ComponentFieldId, ComponentFieldPathSegment, ComponentFieldSchema, ComponentPropertyKey,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentShapeMember,
    ComponentTypeId, ComponentValueKind, ComponentValueMaterializationLimits, ComponentValueShape,
    effective_component_value,
};

use super::*;
use super::{
    PrefabExpansionFailureKind as Conflict, PrefabPropertyClaim as PropertyClaim,
    PrefabPropertyIntent as PropertyIntent,
};

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
        bindings: vec![],
    }
}

fn absent(root: &str) -> PropertyClaim {
    PropertyClaim {
        path: ComponentPropertyPath::new(id(root)),
        intent: PropertyIntent::Absent,
        bindings: vec![],
    }
}

fn apply(
    schema: &ComponentSchema,
    original: &ComponentValue,
    claims: &[PropertyClaim],
) -> Result<ComponentValue, Conflict> {
    super::properties::apply_claims(
        schema,
        schema.version(),
        original,
        claims,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
    )
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
    let expected = settings(Some(ComponentValue::U64(7)));
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
                intent: PropertyIntent::Absent,
                bindings: vec![],
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
        super::properties::apply_claims(
            &schema,
            ComponentSchemaVersion(2),
            &original,
            &[],
            &BTreeMap::new(),
            ComponentValueMaterializationLimits::default(),
        ),
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
fn map_keys_are_literal_and_disjoint_claims_are_order_independent() {
    let count = 64;
    let schema = root_schema(ComponentValueShape::map(ComponentValueKind::U64.into()));
    let original =
        payload(ComponentValue::map((0..count).map(|index| {
            (format!("entry.{index}"), ComponentValue::U64(0))
        })));
    let mut claims = (0..count)
        .map(|index| {
            set(
                path([ComponentPropertyStep::MapEntry(format!("entry.{index}"))]),
                ComponentValue::U64(index as u64 + 1),
            )
        })
        .collect::<Vec<_>>();
    let expected = payload(ComponentValue::map((0..count).map(|index| {
        (
            format!("entry.{index}"),
            ComponentValue::U64(index as u64 + 1),
        )
    })));
    assert_eq!(apply(&schema, &original, &claims), Ok(expected.clone()));
    claims.reverse();
    assert_eq!(apply(&schema, &original, &claims), Ok(expected));
    claims.push(claims[0].clone());
    assert_eq!(
        apply(&schema, &original, &claims),
        Err(Conflict::DuplicateIntent)
    );
}

#[test]
fn selector_and_replacement_budgets_have_inclusive_boundaries_before_copy() {
    use nara_core::{ByteLimit, ItemLimit};

    let scalar = root_schema(ComponentValueKind::String);
    let original = payload(ComponentValue::String(String::new()));
    let claim = set(path([]), ComponentValue::String("pin".into()));
    let base_cost = original.cost();
    let claim_cost = match &claim.intent {
        PropertyIntent::Set(value) => value.cost(),
        PropertyIntent::Absent => unreachable!(),
    };
    let bytes = base_cost.logical_bytes()
        + claim.path.logical_bytes().unwrap()
        + claim_cost.logical_bytes()
        + "payload".len();
    let nodes = base_cost.nodes() + claim_cost.nodes();
    let limits = ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        ComponentValueMaterializationLimits::default().depth(),
    );
    let evaluate = |limits| {
        super::properties::apply_claims(
            &scalar,
            scalar.version(),
            &original,
            std::slice::from_ref(&claim),
            &BTreeMap::new(),
            limits,
        )
    };
    assert_eq!(
        evaluate(limits),
        Ok(payload(ComponentValue::String("pin".into())))
    );
    assert_eq!(
        evaluate(ComponentValueMaterializationLimits::new(
            limits.nodes(),
            ByteLimit::new(bytes - 1).unwrap(),
            limits.depth(),
        )),
        Err(Conflict::WorkLimit)
    );
    assert_eq!(
        evaluate(ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes - 1).unwrap(),
            limits.bytes(),
            limits.depth(),
        )),
        Err(Conflict::WorkLimit)
    );

    let map = root_schema(ComponentValueShape::map(ComponentValueKind::U64.into()));
    let key = "literal.key";
    let value = payload(ComponentValue::map([(key, ComponentValue::U64(1))]));
    let selected = set(
        path([ComponentPropertyStep::MapEntry(key.into())]),
        ComponentValue::U64(2),
    );
    let defaults = ComponentValueMaterializationLimits::default();
    let limits = ComponentValueMaterializationLimits::new(
        defaults.nodes(),
        ByteLimit::new(selected.path.logical_bytes().unwrap() - 1).unwrap(),
        defaults.depth(),
    );
    assert_eq!(
        super::properties::apply_claims(
            &map,
            map.version(),
            &value,
            &[selected],
            &BTreeMap::new(),
            limits,
        ),
        Err(Conflict::WorkLimit)
    );

    let list = root_schema(ComponentValueShape::list(ComponentValueKind::U64.into()));
    let value = payload(ComponentValue::List(vec![ComponentValue::U64(0); 8]));
    assert_eq!(
        super::properties::apply_claims(
            &list,
            list.version(),
            &value,
            &[],
            &BTreeMap::new(),
            ComponentValueMaterializationLimits::new(
                ItemLimit::new(value.cost().nodes() - 1).unwrap(),
                defaults.bytes(),
                defaults.depth(),
            ),
        ),
        Err(Conflict::WorkLimit)
    );
}

#[test]
fn production_wire_claims_round_trip_and_reject_unknown_members() {
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

mod reference_regressions {
    use std::collections::BTreeMap;

    use nara_asset::AssetRef;
    use nara_core::{ByteLimit, DepthLimit, ItemLimit};
    use nara_identity::{
        EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
        PersistentRuntimeReference,
    };
    use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldPathSegment,
        ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentShapeMember,
        ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits,
        ComponentValueShape, collect_declared_asset_references_from_schema,
    };

    use super::super::references::bind_references;
    use super::super::*;

    fn id(name: &str) -> SceneEntityId {
        SceneEntityId::new(name).unwrap()
    }

    fn local(name: &str) -> ComponentValue {
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) })
    }

    fn nested(anchor: &str) -> PrefabEntityTarget {
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
                expected_base: crate::tests::named_prefab_source_key("inner"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("socket")),
        }
    }

    fn schema(shape: ComponentValueShape) -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("game.OverlayReferences"),
            "References",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
        .with_capabilities([
            ComponentCapability::EntityRef,
            ComponentCapability::AssetRef,
        ])])
    }

    fn list_value() -> ComponentValue {
        ComponentValue::map([(
            "links",
            ComponentValue::List(vec![local("socket"), local("socket")]),
        )])
    }

    fn slot(index: u32) -> ComponentFieldPath {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("links"),
            ComponentFieldPathSegment::index(index),
        ])
    }

    fn binding(slot: ComponentFieldPath, target: PrefabEntityTarget) -> PrefabReferenceBinding {
        PrefabReferenceBinding {
            slot,
            expected: id("socket"),
            target,
        }
    }

    #[test]
    fn equal_spelling_and_repeated_sources_bind_each_payload_slot_to_its_complete_target() {
        let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        for right in [PrefabEntityTarget::addition(id("socket")), nested("right")] {
            let targets = BTreeMap::from([
                (nested("left"), id("left-output")),
                (right.clone(), id("right-output")),
            ]);
            let bindings = vec![
                binding(slot(0), nested("left")),
                binding(slot(1), right.clone()),
            ];
            let loaded: Vec<PrefabReferenceBinding> =
                serde_json::from_slice(&serde_json::to_vec(&bindings).unwrap()).unwrap();
            assert_eq!(bindings, loaded);
            let bound = bind_references(
                &schema,
                &list_value(),
                &loaded,
                &targets,
                Default::default(),
            )
            .unwrap();
            assert_eq!(bound.get_path(&slot(0)).unwrap(), &local("left-output"));
            assert_eq!(bound.get_path(&slot(1)).unwrap(), &local("right-output"));
            if !right.through.is_empty() {
                let mut rebound = targets.clone();
                rebound.remove(&right);
                let mut changed = right.clone();
                changed.through[0].expected_base =
                    crate::tests::named_prefab_source_key("replacement-inner");
                rebound.insert(changed, id("right-output"));
                assert_eq!(
                    bind_references(
                        &schema,
                        &list_value(),
                        &loaded,
                        &rebound,
                        Default::default()
                    ),
                    Err(PrefabExpansionFailureKind::MissingTarget)
                );
            }
        }
    }

    #[test]
    fn incomplete_duplicate_stray_stale_and_missing_target_bindings_preserve_all_inputs() {
        let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let value = list_value();
        let target = PrefabEntityTarget::addition(id("socket"));
        let targets = BTreeMap::from([(target.clone(), id("output"))]);
        let first = binding(slot(0), target.clone());
        let second = binding(slot(1), target.clone());
        let mut stale = second.clone();
        stale.expected = id("old-local");
        let missing = PrefabEntityTarget::addition(id("missing"));
        let cases = [
            (vec![], PrefabExpansionFailureKind::Reference),
            (vec![first.clone()], PrefabExpansionFailureKind::Reference),
            (
                vec![first.clone(), first.clone(), second.clone()],
                PrefabExpansionFailureKind::Reference,
            ),
            (
                vec![first.clone(), second, binding(slot(2), target.clone())],
                PrefabExpansionFailureKind::Reference,
            ),
            (
                vec![first.clone(), stale],
                PrefabExpansionFailureKind::Reference,
            ),
            (
                vec![first, binding(slot(1), missing.clone())],
                PrefabExpansionFailureKind::MissingTarget,
            ),
        ];
        for (bindings, expected) in cases {
            let before = (value.clone(), bindings.clone(), targets.clone());
            assert_eq!(
                bind_references(&schema, &value, &bindings, &targets, Default::default()),
                Err(expected)
            );
            assert_eq!(
                serde_json::from_slice::<Vec<PrefabReferenceBinding>>(
                    &serde_json::to_vec(&bindings).unwrap()
                )
                .unwrap(),
                bindings
            );
            assert_eq!((value.clone(), bindings, targets.clone()), before);
        }
    }

    #[test]
    fn nested_container_replacement_replans_slots_and_preserves_external_reference_types() {
        let schema = schema(ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("id", ComponentValueKind::String)
                    .with_id(ComponentFieldId::new("row-id")),
                ComponentShapeMember::new(
                    "target",
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ),
                ComponentShapeMember::new(
                    "others",
                    ComponentValueShape::map(ComponentValueShape::EntityRef),
                ),
                ComponentShapeMember::new("portrait", ComponentValueShape::AssetRef),
            ]),
            ComponentFieldId::new("row-id"),
        ));
        let stable = AssetRef::stable_id("23456789-2345-4345-8345-234567890abc").unwrap();
        let portrait = ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".into())),
            (
                "value",
                ComponentValue::String(stable.as_stable_id().unwrap().to_string()),
            ),
        ]);
        let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
            entity: PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new("overlay.fixture").unwrap(),
                PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
            ),
        });
        let first = ComponentValue::map([
            ("id", ComponentValue::String("a".into())),
            ("target", persistent.clone()),
            (
                "others",
                ComponentValue::map([("name.with.dot", local("socket"))]),
            ),
            ("portrait", portrait.clone()),
        ]);
        let second = ComponentValue::map([
            ("id", ComponentValue::String("b".into())),
            ("target", ComponentValue::Null),
            (
                "others",
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ),
            ("portrait", portrait.clone()),
        ]);
        let value = ComponentValue::map([(
            "links",
            ComponentValue::List(vec![first.clone(), second.clone()]),
        )]);
        let coordinate = |index, fields: &[&str]| {
            let mut segments = slot(index).into_segments();
            segments.extend(
                fields
                    .iter()
                    .map(|name| ComponentFieldPathSegment::field(*name)),
            );
            ComponentFieldPath::new(segments)
        };
        let target = PrefabEntityTarget::addition(id("socket"));
        let targets = BTreeMap::from([(target.clone(), id("output"))]);
        let valid = binding(coordinate(0, &["others", "name.with.dot"]), target.clone());
        let bound = bind_references(
            &schema,
            &value,
            std::slice::from_ref(&valid),
            &targets,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            bound
                .get_path(&coordinate(0, &["others", "name.with.dot"]))
                .unwrap(),
            &local("output")
        );
        assert_eq!(
            bound.get_path(&coordinate(0, &["target"])).unwrap(),
            &persistent
        );
        assert_eq!(
            bound.get_path(&coordinate(1, &["target"])).unwrap(),
            &ComponentValue::Null
        );
        assert_eq!(
            bound.get_path(&coordinate(0, &["portrait"])).unwrap(),
            &portrait
        );
        let assets =
            collect_declared_asset_references_from_schema(&schema, schema.version(), &bound)
                .unwrap();
        assert_eq!(assets.len(), 2);
        assert!(
            assets
                .iter()
                .all(|reference| reference.asset_ref() == &stable)
        );
        for forbidden in [
            coordinate(0, &["target"]),
            coordinate(1, &["target"]),
            coordinate(0, &["portrait"]),
            coordinate(0, &["id"]),
        ] {
            assert_eq!(
                bind_references(
                    &schema,
                    &value,
                    &[valid.clone(), binding(forbidden, target.clone())],
                    &targets,
                    Default::default()
                ),
                Err(PrefabExpansionFailureKind::Reference)
            );
        }
        let replacement =
            ComponentValue::map([("links", ComponentValue::List(vec![second, first]))]);
        assert_eq!(
            bind_references(
                &schema,
                &replacement,
                &[valid],
                &targets,
                Default::default()
            ),
            Err(PrefabExpansionFailureKind::Reference)
        );
        let rebound = bind_references(
            &schema,
            &replacement,
            &[binding(coordinate(1, &["others", "name.with.dot"]), target)],
            &targets,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            rebound
                .get_path(&coordinate(1, &["others", "name.with.dot"]))
                .unwrap(),
            &local("output")
        );
    }

    #[test]
    fn unsupported_reference_shapes_and_explicit_work_limits_reject_before_copy() {
        let targets = BTreeMap::new();
        let opaque = schema(ComponentValueKind::Map.into());
        let hidden =
            ComponentValue::map([("links", ComponentValue::map([("hidden", local("socket"))]))]);
        assert_eq!(
            bind_references(&opaque, &hidden, &[], &targets, Default::default()),
            Err(PrefabExpansionFailureKind::Reference)
        );
        let required = schema(ComponentValueShape::EntityRef);
        for invalid in [
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ComponentValue::map([("links", ComponentValue::Null)]),
            ComponentValue::map([("links", ComponentValue::String("socket".into()))]),
        ] {
            assert_eq!(
                bind_references(&required, &invalid, &[], &targets, Default::default()),
                Err(PrefabExpansionFailureKind::Reference)
            );
        }
        let limits = ComponentValueMaterializationLimits::new(
            ItemLimit::new(8).unwrap(),
            ByteLimit::new(1024).unwrap(),
            DepthLimit::new(8).unwrap(),
        );
        let list = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let target = PrefabEntityTarget::addition(id("socket"));
        let targets = BTreeMap::from([(target.clone(), id("output"))]);
        let excessive = (0..9)
            .map(|index| binding(slot(index), target.clone()))
            .collect::<Vec<_>>();
        let excessive_value =
            ComponentValue::map([("links", ComponentValue::List(vec![local("socket"); 9]))]);
        assert_eq!(
            bind_references(&list, &excessive_value, &excessive, &targets, limits),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
        let nullable = schema(ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::EntityRef,
        )));
        let oversized =
            ComponentValue::map([("links", ComponentValue::List(vec![ComponentValue::Null; 9]))]);
        assert_eq!(
            bind_references(&nullable, &oversized, &[], &targets, limits),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
    }
}

mod expansion {
    use std::collections::BTreeSet;

    use nara_asset::AssetRef;
    use nara_identity::EntityReference;
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
    };

    use super::super::*;
    use crate::{
        AuthoringDocumentSource, InMemoryPrefabSourceResolver, PrefabDocument,
        PrefabExpansionReport, PrefabProjectionStep, SceneAuthoringSession, SceneDocument,
        SceneDocumentCandidate, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        ScenePlacement, ScenePlacementOwner,
    };

    fn id(name: &str) -> SceneEntityId {
        SceneEntityId::new(name).unwrap()
    }

    fn key(name: &str) -> SourceDocumentKey {
        crate::tests::named_prefab_source_key(name)
    }

    fn path(name: &str) -> AssetRef {
        AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap()
    }

    fn direct(name: &str) -> PrefabEntityTarget {
        PrefabEntityTarget::source(id(name))
    }

    fn added(name: &str) -> PrefabEntityTarget {
        PrefabEntityTarget::addition(id(name))
    }

    fn inside(
        anchor: PrefabEntityOrigin,
        base: &str,
        entity: PrefabEntityOrigin,
    ) -> PrefabEntityTarget {
        PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor,
                expected_base: key(base),
            }],
            entity,
        }
    }

    fn nested(anchor: &str, entity: &str) -> PrefabEntityTarget {
        inside(
            PrefabEntityOrigin::SourceEntity(id(anchor)),
            "leaf",
            PrefabEntityOrigin::SourceEntity(id(entity)),
        )
    }

    fn instance(
        name: &str,
        source: AssetRef,
        overrides: Option<PrefabOverlay>,
    ) -> SceneEntityRecord {
        let mut record = SceneEntityRecord::new(id(name));
        record.prefab = Some(PrefabInstance { source, overrides });
        record
    }

    fn outer(order: &[&str]) -> PrefabDocument {
        PrefabDocument::from_ordered_roots(order.iter().map(|name| {
            if name.starts_with("nest_") {
                instance(name, path("leaf"), None)
            } else {
                SceneEntityRecord::new(id(name))
            }
        }))
    }

    fn resolver(base: PrefabDocument) -> InMemoryPrefabSourceResolver {
        InMemoryPrefabSourceResolver::new()
            .with_prefab(path("outer"), key("outer"), base.clone())
            .with_prefab(path("alias"), key("outer"), base)
            .with_prefab(
                path("leaf"),
                key("leaf"),
                PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("socket")),
                    SceneEntityRecord::new(id("tip")),
                ]),
            )
    }

    fn schema() -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("game.Link"),
            "Link",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("target"),
                "Target",
                ComponentFieldPath::from_fields(["target"]),
                ComponentValueShape::optional(ComponentValueShape::EntityRef),
            )
            .with_capabilities(
                ComponentCapability::SCENE_AUTHORING
                    .into_iter()
                    .chain([ComponentCapability::EntityRef]),
            ),
            ComponentFieldSchema::required(
                ComponentFieldId::new("note"),
                "Note",
                ComponentFieldPath::from_fields(["note"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ])
    }

    fn registry_with(schemas: impl IntoIterator<Item = ComponentSchema>) -> ComponentRegistry {
        #[derive(nara_ecs::Component)]
        struct Value(ComponentValue);
        let mut registry = ComponentRegistry::new();
        for schema in schemas {
            registry
                .register_persistent_component_with_codec::<Value, _, _>(
                    schema,
                    |value| Ok(Value(value.clone())),
                    |value| Ok(value.0.clone()),
                )
                .unwrap();
        }
        registry.freeze().unwrap();
        registry
    }

    fn registry() -> ComponentRegistry {
        registry_with([schema()])
    }

    fn scene(overlay: PrefabOverlay, alias: bool) -> SceneDocument {
        SceneDocument::from_ordered_roots([
            instance(
                "host_a",
                path(if alias { "alias" } else { "outer" }),
                Some(overlay),
            ),
            instance("host_b", path("outer"), None),
            SceneEntityRecord::new(id("unrelated")),
            SceneEntityRecord::new(id("socket")),
        ])
    }

    fn expand(base: PrefabDocument, overlay: PrefabOverlay) -> PrefabExpansionReport {
        let document = scene(overlay, true);
        let resolver = resolver(base);
        let before = (document.clone(), resolver.clone());
        let report = document.expand_prefabs(&registry(), &resolver);
        assert_eq!((document, resolver), before);
        report
    }

    fn accepted(base: PrefabDocument, overlay: PrefabOverlay) -> PrefabExpansionReport {
        let report = expand(base, overlay);
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        report
    }

    fn rejected(base: PrefabDocument, overlay: PrefabOverlay) -> PrefabExpansionReport {
        let report = expand(base, overlay);
        assert!(report.candidate.is_none());
        assert!(report.provenance().is_none());
        assert!(report.diagnostics.has_errors());
        report
    }

    // This observes published provenance only; production alone interprets authored targets.
    fn projected(
        report: &PrefabExpansionReport,
        root: &str,
        target: &PrefabEntityTarget,
    ) -> SceneEntityId {
        let origins = target
            .through
            .iter()
            .map(|hop| &hop.anchor)
            .chain(std::iter::once(&target.entity))
            .collect::<Vec<_>>();
        let mut matches = report.provenance().unwrap().iter().filter(|(_, origin)| {
            origin.projection_path().is_some_and(|path| {
                path.root_anchor() == &id(root)
                    && path.steps().len() == origins.len()
                    && path.steps().iter().zip(&origins).all(|(step, origin)| {
                        match (step, origin) {
                            (
                                PrefabProjectionStep::SourceEntity { entity, .. },
                                PrefabEntityOrigin::SourceEntity(expected),
                            )
                            | (
                                PrefabProjectionStep::InstanceAddition { entity },
                                PrefabEntityOrigin::InstanceAddition(expected),
                            ) => entity == expected,
                            _ => false,
                        }
                    })
            })
        });
        let entity = matches.next().expect("projection must exist").0.clone();
        assert!(matches.next().is_none(), "projection must be unique");
        entity
    }

    fn record<'a>(
        report: &'a PrefabExpansionReport,
        entity: &SceneEntityId,
    ) -> &'a SceneEntityRecord {
        report
            .document()
            .unwrap()
            .entities
            .iter()
            .find(|record| &record.id == entity)
            .unwrap()
    }

    fn children(report: &PrefabExpansionReport, parent: &SceneEntityId) -> Vec<SceneEntityId> {
        record(report, parent).children.clone()
    }

    fn place(
        target: PrefabEntityTarget,
        parent: PrefabPlacementParent,
        slot: PrefabPlacementSlot,
    ) -> PrefabPlacement {
        PrefabPlacement {
            target,
            parent,
            slot,
        }
    }

    fn local(name: &str) -> ComponentValue {
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) })
    }

    fn link_record(note: &str, target: ComponentValue) -> SceneComponentRecord {
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("target", target),
                ("note", ComponentValue::String(note.into())),
            ]),
        )
    }

    fn binding(slot: ComponentFieldPath, target: PrefabEntityTarget) -> PrefabReferenceBinding {
        PrefabReferenceBinding {
            slot,
            expected: id("socket"),
            target,
        }
    }

    fn link(
        target: PrefabEntityTarget,
        referent: PrefabEntityTarget,
        note: &str,
    ) -> PrefabComponentIntent {
        PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Add,
            target,
            component: ComponentTypeId::new("game.Link"),
            record: link_record(note, local("socket")),
            bindings: vec![binding(
                ComponentFieldPath::from_fields(["target"]),
                referent,
            )],
        }
    }

    fn assert_link(
        report: &PrefabExpansionReport,
        from: PrefabEntityTarget,
        to: PrefabEntityTarget,
        note: &str,
    ) {
        assert_eq!(
            record(report, &projected(report, "host_a", &from)).components
                [&ComponentTypeId::new("game.Link")],
            link_record(
                note,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: projected(report, "host_a", &to),
                })
            )
        );
    }

    fn with_link(note: &str, target: ComponentValue) -> PrefabDocument {
        let mut base = outer(&["socket", "nest_a", "nest_b", "tail"]);
        base.entities
            .iter_mut()
            .find(|record| record.id == id("socket"))
            .unwrap()
            .components
            .insert(ComponentTypeId::new("game.Link"), link_record(note, target));
        base
    }

    fn property(
        name: &str,
        intent: PrefabPropertyIntent,
        bindings: Vec<PrefabReferenceBinding>,
    ) -> PrefabComponentIntent {
        PrefabComponentIntent::Properties {
            target: direct("socket"),
            component: ComponentTypeId::new("game.Link"),
            version: ComponentSchemaVersion::ONE,
            claims: vec![PrefabPropertyClaim {
                path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new(name)),
                intent,
                bindings,
            }],
        }
    }

    #[test]
    fn desired_placement_rebases_while_exact_source_move_keeps_its_precondition() {
        let original = outer(&["socket", "nest_a", "nest_b", "tail"]);
        let patch = ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
            entity: id("tail"),
            from: ScenePlacement::current_prefab(&original, &id("tail")).unwrap(),
            to: ScenePlacement::after(ScenePlacementOwner::Root, id("socket")),
        }]);
        assert!(
            patch
                .apply_to_scene(&mut original.instantiate(), &registry())
                .applied
        );
        let changed = outer(&["tail", "nest_a", "socket", "nest_b", "new_base"]);
        let mut exact = changed.instantiate();
        assert!(!patch.apply_to_scene(&mut exact, &registry()).applied);
        assert_eq!(exact, changed.instantiate());
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.placements.push(place(
            direct("tail"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::After(direct("socket")),
        ));
        let encoded = serde_json::to_vec(&overlay).unwrap();
        assert!(
            !String::from_utf8(encoded.clone())
                .unwrap()
                .contains("prefab_")
        );
        let loaded = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(overlay, loaded);
        let report = accepted(changed, loaded);
        assert_eq!(
            children(&report, &id("host_a")),
            ["nest_a", "socket", "tail", "nest_b", "new_base"].map(|name| projected(
                &report,
                "host_a",
                &direct(name)
            ))
        );
        assert_eq!(
            children(&report, &id("host_b")),
            ["tail", "nest_a", "socket", "nest_b", "new_base"].map(|name| projected(
                &report,
                "host_b",
                &direct(name)
            ))
        );
    }

    #[test]
    fn placement_cohorts_follow_authored_order_independently_of_addition_allocation() {
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.additions = vec![
            PrefabAddition::new(id("second")),
            PrefabAddition::new(id("first")),
        ];
        overlay.placements = vec![
            place(
                added("first"),
                PrefabPlacementParent::Root,
                PrefabPlacementSlot::Before(direct("tail")),
            ),
            place(
                added("second"),
                PrefabPlacementParent::Root,
                PrefabPlacementSlot::Before(direct("tail")),
            ),
        ];
        for reverse in [false, true] {
            if reverse {
                overlay.additions.reverse();
            }
            let report = accepted(outer(&["socket", "tail"]), overlay.clone());
            assert_eq!(
                children(&report, &id("host_a")),
                [
                    direct("socket"),
                    added("first"),
                    added("second"),
                    direct("tail")
                ]
                .map(|target| projected(&report, "host_a", &target))
            );
        }
        overlay.placements.reverse();
        let report = accepted(outer(&["socket", "tail"]), overlay);
        assert_eq!(
            children(&report, &id("host_a")),
            [
                direct("socket"),
                added("second"),
                added("first"),
                direct("tail")
            ]
            .map(|target| projected(&report, "host_a", &target))
        );
    }

    #[test]
    fn parent_rotation_is_simultaneous_and_suppression_can_rescue_a_nested_child() {
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.placements = vec![
            place(
                direct("nest_a"),
                PrefabPlacementParent::Entity(nested("nest_a", "socket")),
                PrefabPlacementSlot::Last,
            ),
            place(
                nested("nest_a", "socket"),
                PrefabPlacementParent::Root,
                PrefabPlacementSlot::First,
            ),
        ];
        let report = accepted(
            outer(&["socket", "nest_a", "nest_b", "tail"]),
            overlay.clone(),
        );
        assert_eq!(
            children(
                &report,
                &projected(&report, "host_a", &nested("nest_a", "socket"))
            ),
            [projected(&report, "host_a", &direct("nest_a"))]
        );
        assert_eq!(
            children(&report, &projected(&report, "host_a", &direct("nest_a"))),
            [projected(&report, "host_a", &nested("nest_a", "tip"))]
        );
        overlay.placements.remove(0);
        overlay.suppressions.push(direct("nest_a"));
        let report = accepted(outer(&["socket", "nest_a", "nest_b", "tail"]), overlay);
        assert!(children(&report, &id("host_a")).contains(&projected(
            &report,
            "host_a",
            &nested("nest_a", "socket")
        )));
        assert!(
            record(&report, &projected(&report, "host_b", &direct("nest_a")))
                .children
                .len()
                == 2
        );
        let origins = report
            .provenance()
            .unwrap()
            .values()
            .filter_map(|origin| origin.projection_path())
            .filter(|path| path.root_anchor() == &id("host_a"))
            .map(|path| {
                path.steps()
                    .iter()
                    .map(|step| step.entity_id().as_str())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert!(!origins.contains(&vec!["nest_a"]));
        assert!(!origins.contains(&vec!["nest_a", "tip"]));
    }

    #[test]
    fn invalid_structure_round_trips_as_source_intent_without_a_candidate() {
        let cases = [
            vec![place(
                direct("socket"),
                PrefabPlacementParent::Root,
                PrefabPlacementSlot::Before(direct("missing")),
            )],
            vec![place(
                nested("nest_a", "socket"),
                PrefabPlacementParent::Entity(direct("nest_a")),
                PrefabPlacementSlot::Before(nested("nest_b", "socket")),
            )],
            vec![
                place(
                    direct("socket"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::After(direct("tail")),
                ),
                place(
                    direct("tail"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::After(direct("socket")),
                ),
            ],
            vec![place(
                direct("nest_a"),
                PrefabPlacementParent::Entity(nested("nest_a", "socket")),
                PrefabPlacementSlot::Last,
            )],
            vec![
                place(
                    direct("socket"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::First,
                ),
                place(
                    direct("socket"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::Last,
                ),
            ],
        ];
        for placements in cases {
            let mut overlay = PrefabOverlay::new(key("outer"));
            overlay.placements = placements;
            let source = scene(overlay.clone(), false);
            let reopened =
                SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .canonicalize(&registry())
                    .unwrap()
                    .into_source_document()
                    .0;
            assert_eq!(source, reopened);
            rejected(outer(&["socket", "nest_a", "nest_b", "tail"]), overlay);
        }
        for reverse in [false, true] {
            let mut overlay = PrefabOverlay::new(key("outer"));
            overlay.suppressions = vec![direct("nest_a"), nested("nest_a", "socket")];
            if reverse {
                overlay.suppressions.reverse();
            }
            rejected(outer(&["socket", "nest_a", "nest_b", "tail"]), overlay);
        }
    }

    #[test]
    fn removed_target_can_be_saved_and_reprepared_when_the_base_returns() {
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.placements.push(place(
            direct("socket"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::Before(direct("tail")),
        ));
        rejected(outer(&["socket"]), overlay.clone());
        let source = scene(overlay, true);
        let reopened = SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
            .unwrap()
            .canonicalize(&registry())
            .unwrap()
            .into_source_document()
            .0;
        assert_eq!(source, reopened);
        let report = reopened.expand_prefabs(&registry(), &resolver(outer(&["tail", "socket"])));
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert_eq!(
            children(&report, &id("host_a")),
            [direct("socket"), direct("tail")].map(|target| projected(&report, "host_a", &target))
        );
    }

    #[test]
    fn empty_equal_and_reordered_overlays_preserve_containing_records_and_authored_intent() {
        for mode in 0..3 {
            let mut overlay = PrefabOverlay::new(key("outer"));
            if mode == 1 {
                overlay.placements.push(place(
                    direct("tail"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::After(direct("socket")),
                ));
            }
            if mode == 2 {
                overlay.additions.push(PrefabAddition::new(id("addition")));
                overlay.placements = vec![
                    place(
                        direct("tail"),
                        PrefabPlacementParent::Root,
                        PrefabPlacementSlot::First,
                    ),
                    place(
                        added("addition"),
                        PrefabPlacementParent::Root,
                        PrefabPlacementSlot::Last,
                    ),
                ];
            }
            let mut document = scene(overlay.clone(), true);
            document
                .entities
                .iter_mut()
                .find(|record| record.id == id("host_a"))
                .unwrap()
                .children = vec![id("local_a"), id("local_b")];
            document.roots.retain(|entity| entity != &id("socket"));
            document.entities.extend([
                SceneEntityRecord::new(id("local_a")).with_children([id("socket")]),
                SceneEntityRecord::new(id("local_b")),
            ]);
            let before = document.clone();
            let report =
                document.expand_prefabs(&registry(), &resolver(outer(&["socket", "tail"])));
            assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
            let mut expected = vec![
                projected(&report, "host_a", &direct("socket")),
                projected(&report, "host_a", &direct("tail")),
            ];
            if mode == 2 {
                expected.reverse();
                expected.push(projected(&report, "host_a", &added("addition")));
            }
            expected.extend([id("local_a"), id("local_b")]);
            assert_eq!(children(&report, &id("host_a")), expected);
            for name in ["local_a", "local_b", "socket"] {
                assert_eq!(
                    record(&report, &id(name)),
                    document
                        .entities
                        .iter()
                        .find(|record| record.id == id(name))
                        .unwrap()
                );
            }
            assert_eq!(
                document
                    .entities
                    .iter()
                    .find(|record| record.id == id("host_a"))
                    .unwrap()
                    .prefab
                    .as_ref()
                    .unwrap()
                    .overrides
                    .as_ref()
                    .unwrap()
                    .placements
                    .len(),
                mode
            );
            assert_eq!(document, before);
        }
    }

    #[test]
    fn mixed_origin_references_keep_complete_paths_and_the_repeated_occurrence() {
        let mut leaf_overlay = PrefabOverlay::new(key("leaf"));
        leaf_overlay
            .additions
            .push(PrefabAddition::new(id("added_leaf")));
        leaf_overlay.placements.push(place(
            added("added_leaf"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::Last,
        ));
        let base = PrefabDocument::from_ordered_roots([
            instance("nested", path("leaf"), Some(leaf_overlay.clone())),
            SceneEntityRecord::new(id("added_leaf")),
        ]);
        let mut overlay = PrefabOverlay::new(key("outer"));
        let mut anchor = PrefabAddition::new(id("added_anchor"));
        anchor.prefab = Some(PrefabInstance {
            source: path("leaf"),
            overrides: Some(leaf_overlay),
        });
        overlay.additions.push(anchor);
        overlay.placements.push(place(
            added("added_anchor"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::Last,
        ));
        let inherited_addition = inside(
            PrefabEntityOrigin::SourceEntity(id("nested")),
            "leaf",
            PrefabEntityOrigin::InstanceAddition(id("added_leaf")),
        );
        let added_inherited = inside(
            PrefabEntityOrigin::InstanceAddition(id("added_anchor")),
            "leaf",
            PrefabEntityOrigin::SourceEntity(id("socket")),
        );
        let added_addition = inside(
            PrefabEntityOrigin::InstanceAddition(id("added_anchor")),
            "leaf",
            PrefabEntityOrigin::InstanceAddition(id("added_leaf")),
        );
        overlay.components = vec![
            link(
                inherited_addition.clone(),
                added_inherited.clone(),
                "source to addition",
            ),
            link(
                added_inherited.clone(),
                added_addition.clone(),
                "added anchor source",
            ),
            link(
                added_addition.clone(),
                direct("added_leaf"),
                "added anchor addition",
            ),
        ];
        let loaded: PrefabOverlay = ron::from_str(&ron::to_string(&overlay).unwrap()).unwrap();
        assert_eq!(loaded, overlay);
        assert_eq!(
            serde_json::from_slice::<PrefabOverlay>(&serde_json::to_vec(&overlay).unwrap())
                .unwrap(),
            overlay
        );
        let report = accepted(base.clone(), loaded);
        assert_link(
            &report,
            inherited_addition,
            added_inherited.clone(),
            "source to addition",
        );
        assert_link(
            &report,
            added_inherited,
            added_addition.clone(),
            "added anchor source",
        );
        assert_link(
            &report,
            added_addition,
            direct("added_leaf"),
            "added anchor addition",
        );
        let untouched = accepted(base, PrefabOverlay::new(key("outer")));
        for (entity, origin) in untouched.provenance().unwrap() {
            if origin
                .projection_path()
                .is_some_and(|path| path.root_anchor() == &id("host_b"))
            {
                // Generated IDs are locators; compare corresponding structured provenance.
                let (actual, _) = report
                    .provenance()
                    .unwrap()
                    .iter()
                    .find(|(_, value)| *value == origin)
                    .unwrap();
                let mut expected = record(&untouched, entity).clone();
                expected.id = actual.clone();
                expected.children = expected
                    .children
                    .iter()
                    .map(|child| {
                        let origin = &untouched.provenance().unwrap()[child];
                        report
                            .provenance()
                            .unwrap()
                            .iter()
                            .find(|(_, value)| *value == origin)
                            .unwrap()
                            .0
                            .clone()
                    })
                    .collect();
                assert_eq!(record(&report, actual), &expected);
            }
        }
    }

    #[test]
    fn equal_local_names_bind_inherited_addition_and_repeated_nested_records() {
        let mut overlay = PrefabOverlay::new(key("outer"));
        let mut addition = PrefabAddition::new(id("socket"));
        addition.components.insert(
            ComponentTypeId::new("game.Link"),
            link_record("added to nested", local("socket")),
        );
        addition.bindings.insert(
            ComponentTypeId::new("game.Link"),
            vec![binding(
                ComponentFieldPath::from_fields(["target"]),
                nested("nest_a", "socket"),
            )],
        );
        overlay.additions.push(addition);
        overlay.placements.push(place(
            added("socket"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::Before(direct("socket")),
        ));
        overlay.components = vec![
            link(direct("socket"), added("socket"), "inherited to added"),
            link(
                nested("nest_a", "socket"),
                nested("nest_b", "socket"),
                "nested to repeated",
            ),
        ];
        let report = accepted(outer(&["socket", "nest_a", "nest_b", "tail"]), overlay);
        let identities = [
            direct("socket"),
            added("socket"),
            nested("nest_a", "socket"),
            nested("nest_b", "socket"),
        ]
        .map(|target| projected(&report, "host_a", &target));
        assert_eq!(identities.iter().collect::<BTreeSet<_>>().len(), 4);
        assert_link(
            &report,
            direct("socket"),
            added("socket"),
            "inherited to added",
        );
        assert_link(
            &report,
            added("socket"),
            nested("nest_a", "socket"),
            "added to nested",
        );
        assert_link(
            &report,
            nested("nest_a", "socket"),
            nested("nest_b", "socket"),
            "nested to repeated",
        );
    }

    #[test]
    fn leaf_pin_preserves_inherited_reference_sibling_and_revert_resumes_inheritance() {
        let base = with_link("new base", local("tail"));
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.components.push(property(
            "note",
            PrefabPropertyIntent::Set(ComponentValue::String("old base".into())),
            vec![],
        ));
        let report = accepted(base.clone(), overlay.clone());
        assert_link(&report, direct("socket"), direct("tail"), "old base");
        overlay.components.clear();
        let report = accepted(base, overlay);
        assert_link(&report, direct("socket"), direct("tail"), "new base");
    }

    #[test]
    fn scalar_property_reference_uses_payload_relative_empty_slot() {
        let base = with_link("kept", ComponentValue::Null);
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.components.push(property(
            "target",
            PrefabPropertyIntent::Set(local("socket")),
            vec![binding(
                ComponentFieldPath::empty(),
                nested("nest_b", "socket"),
            )],
        ));
        let report = accepted(base.clone(), overlay.clone());
        assert_link(
            &report,
            direct("socket"),
            nested("nest_b", "socket"),
            "kept",
        );
        overlay.suppressions.push(nested("nest_b", "socket"));
        rejected(base, overlay);
    }

    #[test]
    fn modes_overlap_and_last_invalid_value_reject_without_mutating_source_or_history() {
        let base = with_link("base", ComponentValue::Null);
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay
            .components
            .push(link(direct("socket"), direct("tail"), "add"));
        rejected(base.clone(), overlay.clone());
        let PrefabComponentIntent::Set { mode, target, .. } = &mut overlay.components[0] else {
            unreachable!()
        };
        *mode = PrefabComponentSetMode::Replace;
        *target = direct("tail");
        rejected(base.clone(), overlay.clone());
        overlay.components = vec![
            property(
                "note",
                PrefabPropertyIntent::Set(ComponentValue::String("pin".into())),
                vec![],
            ),
            property(
                "target",
                PrefabPropertyIntent::Set(ComponentValue::Null),
                vec![],
            ),
        ];
        rejected(base.clone(), overlay.clone());

        overlay.components = vec![
            link(direct("tail"), direct("socket"), "valid first"),
            property(
                "note",
                PrefabPropertyIntent::Set(ComponentValue::U64(3)),
                vec![],
            ),
        ];
        let source = scene(overlay, true);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.overlay-regressions").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema()],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let session = SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(path("containing-scene")),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let session = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
            session,
            crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
        );
        let before = (
            session.document().clone(),
            session.revision(),
            session.history_status(),
        );
        let report = session
            .document()
            .expand_prefabs(&registry(), &resolver(base));
        assert!(report.candidate.is_none());
        assert!(report.provenance().is_none());
        assert_eq!(
            (
                session.document().clone(),
                session.revision(),
                session.history_status()
            ),
            before
        );
    }

    #[test]
    fn replacement_suppression_and_dangling_reference_use_final_owner_admission() {
        let base = with_link("base", ComponentValue::Null);
        let mut overlay = PrefabOverlay::new(key("outer"));
        let mut replacement = link(direct("socket"), direct("tail"), "replacement");
        let PrefabComponentIntent::Set { mode, .. } = &mut replacement else {
            unreachable!()
        };
        *mode = PrefabComponentSetMode::Replace;
        overlay.components.push(replacement);
        let report = accepted(base.clone(), overlay.clone());
        assert_link(&report, direct("socket"), direct("tail"), "replacement");
        overlay.suppressions.push(direct("tail"));
        rejected(base.clone(), overlay.clone());
        let PrefabComponentIntent::Set { bindings, .. } = &mut overlay.components[0] else {
            unreachable!()
        };
        bindings.clear();
        rejected(base.clone(), overlay.clone());
        overlay.suppressions.clear();
        overlay.components = vec![PrefabComponentIntent::Suppress {
            target: direct("socket"),
            component: ComponentTypeId::new("game.Link"),
        }];
        let report = accepted(base, overlay);
        assert!(
            record(&report, &projected(&report, "host_a", &direct("socket")))
                .components
                .is_empty()
        );
    }

    #[test]
    fn addition_only_layers_bind_actual_canonical_sources_including_nested_hops() {
        for nested_layer in [false, true] {
            let make = |root_key: &str, inner_key: &str, alias: bool| {
                let mut inner_overlay = PrefabOverlay::new(key(inner_key));
                inner_overlay
                    .additions
                    .push(PrefabAddition::new(id("only_addition")));
                inner_overlay.placements.push(place(
                    added("only_addition"),
                    PrefabPlacementParent::Root,
                    PrefabPlacementSlot::Last,
                ));
                let root = if nested_layer {
                    PrefabDocument::from_ordered_roots([instance(
                        "nested",
                        path(if alias { "inner_alias" } else { "inner" }),
                        Some(inner_overlay.clone()),
                    )])
                } else {
                    PrefabDocument::default()
                };
                let resolver = InMemoryPrefabSourceResolver::new()
                    .with_prefab(path("outer"), key(root_key), root.clone())
                    .with_prefab(path("alias"), key(root_key), root)
                    .with_prefab(path("inner"), key(inner_key), PrefabDocument::default())
                    .with_prefab(
                        path("inner_alias"),
                        key(inner_key),
                        PrefabDocument::default(),
                    );
                let mut overlay = if nested_layer {
                    PrefabOverlay::new(key("outer"))
                } else {
                    PrefabOverlay {
                        base: key("outer"),
                        ..inner_overlay
                    }
                };
                if nested_layer {
                    overlay.components.push(link(
                        inside(
                            PrefabEntityOrigin::SourceEntity(id("nested")),
                            "inner",
                            PrefabEntityOrigin::InstanceAddition(id("only_addition")),
                        ),
                        direct("nested"),
                        "bound addition",
                    ));
                }
                let scene = SceneDocument::from_ordered_roots([instance(
                    "host_a",
                    path(if alias { "alias" } else { "outer" }),
                    Some(overlay),
                )]);
                (scene, resolver)
            };
            for alias in [false, true] {
                let (scene, resolver) = make("outer", "inner", alias);
                let mut requested_keys = Vec::new();
                let report = scene
                    .try_expand_prefabs_with_loader(
                        &registry(),
                        Default::default(),
                        None,
                        |request, _| {
                            let resolved =
                                crate::PrefabSourceResolver::resolve_prefab(&resolver, request);
                            if let Some(resolved) = &resolved {
                                requested_keys.push(resolved.key().clone());
                            }
                            Ok::<_, std::convert::Infallible>(resolved)
                        },
                    )
                    .unwrap();
                assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
                assert_eq!(
                    requested_keys,
                    if nested_layer {
                        vec![key("outer"), key("inner")]
                    } else {
                        vec![key("outer")]
                    }
                );
                let target = if nested_layer {
                    inside(
                        PrefabEntityOrigin::SourceEntity(id("nested")),
                        "inner",
                        PrefabEntityOrigin::InstanceAddition(id("only_addition")),
                    )
                } else {
                    added("only_addition")
                };
                assert!(
                    report
                        .document()
                        .unwrap()
                        .entities
                        .iter()
                        .any(|record| record.id == projected(&report, "host_a", &target))
                );
            }
            let (scene, resolver) = make("replacement_root", "inner", true);
            assert!(
                scene
                    .expand_prefabs(&registry(), &resolver)
                    .candidate
                    .is_none()
            );
            if nested_layer {
                let (mut scene, resolver) = make("outer", "replacement_inner", true);
                assert!(
                    scene
                        .expand_prefabs(&registry(), &resolver)
                        .candidate
                        .is_none()
                );
                let PrefabComponentIntent::Set { target, .. } = &mut scene.entities[0]
                    .prefab
                    .as_mut()
                    .unwrap()
                    .overrides
                    .as_mut()
                    .unwrap()
                    .components[0]
                else {
                    unreachable!()
                };
                target.through[0].expected_base = key("replacement_inner");
                let repaired = scene.expand_prefabs(&registry(), &resolver);
                assert!(repaired.candidate.is_some(), "{:?}", repaired.diagnostics);
            }
        }
    }

    #[test]
    fn wrong_document_kind_is_preserved_by_wire_and_rejected_at_root_and_nested_hop() {
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay
            .components
            .push(link(nested("nest_a", "socket"), direct("tail"), "bound"));
        let wrong_kind = |name| {
            let crate::SourceDocumentOrigin::Named { universe, .. } = key(name).origin().clone()
            else {
                unreachable!()
            };
            SourceDocumentKey::named(universe, name, crate::SourceDocumentKind::Scene).unwrap()
        };
        for root in [false, true] {
            let mut invalid = overlay.clone();
            if root {
                invalid.base = wrong_kind("outer");
            } else {
                let PrefabComponentIntent::Set { target, .. } = &mut invalid.components[0] else {
                    unreachable!()
                };
                target.through[0].expected_base = wrong_kind("leaf");
            }
            let loaded: PrefabOverlay = ron::from_str(&ron::to_string(&invalid).unwrap()).unwrap();
            assert_eq!(invalid, loaded);
            assert_eq!(
                serde_json::from_slice::<PrefabOverlay>(&serde_json::to_vec(&invalid).unwrap())
                    .unwrap(),
                invalid
            );
            rejected(outer(&["socket", "nest_a", "tail"]), loaded);
        }
    }

    #[test]
    fn absent_defaults_bind_only_the_new_payload_and_never_capture_containing_names() {
        let mut schema = schema();
        schema.fields[0].required = false;
        schema.fields[0].default_value = Some(local("socket"));
        let registry = registry_with([schema]);
        let base = with_link("kept", ComponentValue::Null);
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay
            .components
            .push(property("target", PrefabPropertyIntent::Absent, vec![]));
        let document = scene(overlay.clone(), true);
        assert!(
            document
                .expand_prefabs(&registry, &resolver(base.clone()))
                .candidate
                .is_none()
        );
        let PrefabComponentIntent::Properties { claims, .. } = &mut overlay.components[0] else {
            unreachable!()
        };
        claims[0].bindings.push(binding(
            ComponentFieldPath::empty(),
            nested("nest_b", "socket"),
        ));
        let source = scene(overlay.clone(), true);
        let report = source.expand_prefabs(&registry, &resolver(base.clone()));
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert_link(
            &report,
            direct("socket"),
            nested("nest_b", "socket"),
            "kept",
        );
        assert_ne!(
            projected(&report, "host_a", &nested("nest_b", "socket")),
            id("socket")
        );
        assert!(
            matches!(&overlay.components[0], PrefabComponentIntent::Properties { claims, .. }
            if matches!(&claims[0].intent, PrefabPropertyIntent::Absent))
        );
        overlay.suppressions.push(nested("nest_b", "socket"));
        assert!(
            scene(overlay, true)
                .expand_prefabs(&registry, &resolver(base.clone()))
                .candidate
                .is_none()
        );

        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.components.push(PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Add,
            target: direct("tail"),
            component: ComponentTypeId::new("game.Link"),
            record: SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("note", ComponentValue::String("omitted target".into()))]),
            ),
            bindings: vec![],
        });
        assert!(
            scene(overlay.clone(), true)
                .expand_prefabs(&registry, &resolver(base.clone()))
                .candidate
                .is_none()
        );
        let PrefabComponentIntent::Set { bindings, .. } = &mut overlay.components[0] else {
            unreachable!()
        };
        bindings.push(binding(
            ComponentFieldPath::from_fields(["target"]),
            direct("socket"),
        ));
        let report = scene(overlay, true).expand_prefabs(&registry, &resolver(base));
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert_link(&report, direct("tail"), direct("socket"), "omitted target");
    }

    #[test]
    fn absent_materializes_missing_storage_ancestors_only_in_the_effective_value() {
        let component = ComponentTypeId::new("game.NestedDefault");
        for reference in [false, true] {
            let default = if reference {
                local("socket")
            } else {
                ComponentValue::U64(7)
            };
            let field = ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::from_fields(["settings", "wire_value"]),
                if reference {
                    ComponentValueShape::EntityRef
                } else {
                    ComponentValueKind::U64.into()
                },
                default,
            )
            .with_capabilities(
                ComponentCapability::SCENE_AUTHORING
                    .into_iter()
                    .chain(reference.then_some(ComponentCapability::EntityRef)),
            );
            let schema = ComponentSchema::new(
                component.clone(),
                "Nested default",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field]);
            let registry = registry_with([schema]);
            for original in [
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
                ComponentValue::map([(
                    "settings",
                    ComponentValue::map([] as [(&str, ComponentValue); 0]),
                )]),
            ] {
                let base = PrefabDocument::from_ordered_roots([
                    SceneEntityRecord::new(id("item")).with_component(
                        component.clone(),
                        SceneComponentRecord::new(ComponentSchemaVersion::ONE, original),
                    ),
                    SceneEntityRecord::new(id("socket")),
                ]);
                let mut overlay = PrefabOverlay::new(key("outer"));
                overlay.components.push(PrefabComponentIntent::Properties {
                    target: direct("item"),
                    component: component.clone(),
                    version: ComponentSchemaVersion::ONE,
                    claims: vec![PrefabPropertyClaim {
                        path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new(
                            "value",
                        )),
                        intent: PrefabPropertyIntent::Absent,
                        bindings: vec![],
                    }],
                });
                if reference {
                    assert!(
                        scene(overlay.clone(), true)
                            .expand_prefabs(&registry, &resolver(base.clone()))
                            .candidate
                            .is_none()
                    );
                    let PrefabComponentIntent::Properties { claims, .. } =
                        &mut overlay.components[0]
                    else {
                        unreachable!()
                    };
                    claims[0]
                        .bindings
                        .push(binding(ComponentFieldPath::empty(), direct("socket")));
                }
                let source = scene(overlay, true);
                let before = (source.clone(), base.clone());
                let report = source.expand_prefabs(&registry, &resolver(base.clone()));
                assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
                assert_eq!(
                    record(&report, &projected(&report, "host_a", &direct("item"))).components
                        [&component]
                        .value,
                    ComponentValue::map([(
                        "settings",
                        ComponentValue::map([(
                            "wire_value",
                            if reference {
                                ComponentValue::EntityReference(EntityReference::SceneLocal {
                                    entity: projected(&report, "host_a", &direct("socket")),
                                })
                            } else {
                                ComponentValue::U64(7)
                            }
                        )])
                    )])
                );
                assert_eq!((source, base), before);
            }
        }
    }

    #[test]
    fn complete_addition_values_need_no_creation_template_adapter_or_edit_history() {
        let mut schema = schema();
        schema.capabilities.remove(&ComponentCapability::Edit);
        for field in &mut schema.fields {
            field.capabilities.remove(&ComponentCapability::Edit);
        }
        assert!(schema.authoring_initial_value.is_none());
        let registry = registry_with([schema.clone()]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.overlay-regressions").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let mut overlay = PrefabOverlay::new(key("outer"));
        let mut addition = PrefabAddition::new(id("socket"));
        addition.components.insert(
            ComponentTypeId::new("game.Link"),
            link_record("full value", local("socket")),
        );
        addition.bindings.insert(
            ComponentTypeId::new("game.Link"),
            vec![binding(
                ComponentFieldPath::from_fields(["target"]),
                direct("tail"),
            )],
        );
        overlay.additions.push(addition);
        overlay.placements.push(place(
            added("socket"),
            PrefabPlacementParent::Root,
            PrefabPlacementSlot::Last,
        ));
        let source = scene(overlay, true);
        let published = SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
            .unwrap()
            .admit_authoring(AuthoringDocumentSource::new(path("source")), &bundle, &[])
            .unwrap()
            .publish();
        let session = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
            published,
            crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
        );
        let before = (
            session.document().clone(),
            session.revision(),
            session.history_status(),
        );
        let report = session
            .document()
            .expand_prefabs(&registry, &resolver(outer(&["socket", "tail"])));
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert_link(&report, added("socket"), direct("tail"), "full value");
        let entity = projected(&report, "host_a", &added("socket"));
        assert!(
            matches!(report.provenance().unwrap()[&entity].projection_path().unwrap().steps(),
            [PrefabProjectionStep::InstanceAddition { entity }] if entity == &id("socket"))
        );
        assert_eq!(
            (
                session.document().clone(),
                session.revision(),
                session.history_status()
            ),
            before
        );
    }

    #[test]
    fn expansion_checks_aggregate_defaults_at_exact_and_under_value_budgets() {
        use nara_core::{ByteLimit, ItemLimit};

        let component = ComponentTypeId::new("game.AggregateDefaults");
        let schema = ComponentSchema::new(
            component.clone(),
            "Aggregate defaults",
            ComponentSchemaVersion(2),
        )
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["note"]),
            ComponentValueKind::String,
            ComponentValue::String("hi".into()),
        )
        .with_capabilities([ComponentCapability::Scene])]);
        let registry = registry_with([schema]);
        let base = PrefabDocument::from_ordered_roots(["first", "last"].map(|name| {
            SceneEntityRecord::new(id(name)).with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(2),
                    ComponentValue::map([] as [(&str, ComponentValue); 0]),
                ),
            )
        }));
        let base = crate::PrefabDocumentCandidate::decode_json_str(&base.to_json_string().unwrap())
            .unwrap()
            .canonicalize(&registry)
            .unwrap()
            .into_source_document()
            .0;
        let request = path("aggregate-defaults");
        let resolver = InMemoryPrefabSourceResolver::new().with_prefab(
            request.clone(),
            key("aggregate-defaults"),
            base.clone(),
        );
        let source = SceneDocument::from_ordered_roots([instance("host_a", request, None)]);
        let before = (source.clone(), resolver.clone());
        // The two effective maps contain four value nodes and twelve logical value bytes.
        // File admission keeps its separate default allowance for structural source bytes.
        let exact = crate::PrefabExpansionLimits::default()
            .with_materialized_value_nodes(ItemLimit::new(4).unwrap())
            .with_materialized_value_bytes(ByteLimit::new(12).unwrap());
        let report = source.expand_prefabs_with_options(
            &registry,
            &resolver,
            crate::PrefabExpansionOptions::default().with_limits(exact),
        );
        assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
        assert!(!report.diagnostics.has_errors());
        for name in ["first", "last"] {
            assert_eq!(
                record(&report, &projected(&report, "host_a", &direct(name))).components
                    [&component]
                    .value,
                ComponentValue::map([("note", ComponentValue::String("hi".into()))]),
            );
        }
        for (limits, kind, observed, maximum) in [
            (
                exact.with_materialized_value_nodes(ItemLimit::new(3).unwrap()),
                "materialized-value-nodes",
                2,
                1,
            ),
            (
                exact.with_materialized_value_bytes(ByteLimit::new(11).unwrap()),
                "materialized-value-bytes",
                6,
                5,
            ),
        ] {
            let report = source.expand_prefabs_with_options(
                &registry,
                &resolver,
                crate::PrefabExpansionOptions::default().with_limits(limits),
            );
            assert!(report.candidate.is_none());
            assert!(report.provenance().is_none());
            let diagnostic = report
                .diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
                })
                .expect("default materialization retains its typed budget refusal");
            let field = |key: &str| {
                diagnostic
                    .fields()
                    .iter()
                    .find(|field| field.key().as_str() == key)
                    .unwrap()
                    .value()
            };
            use nara_diagnostic::DiagnosticValueRef;
            assert_eq!(field("budget-kind"), DiagnosticValueRef::Identifier(kind));
            assert_eq!(field("observed"), DiagnosticValueRef::Unsigned(observed));
            assert_eq!(field("maximum"), DiagnosticValueRef::Unsigned(maximum));
            assert_eq!(field("entity-id"), DiagnosticValueRef::Identifier("last"));
            let failure = report.failure.as_ref().unwrap();
            assert_eq!(
                failure.reason(),
                &crate::PrefabExpansionFailureKind::WorkLimit,
            );
            assert!(failure.location().is_none());
            assert!(failure.slot().is_none());
            assert_eq!((&source, &resolver), (&before.0, &before.1));
        }
        assert!(base.entities.iter().all(|entity| {
            entity.components[&component].value
                == ComponentValue::map([] as [(&str, ComponentValue); 0])
        }));
        assert_eq!((source, resolver), before);
    }

    #[test]
    fn preserved_unavailable_source_intent_is_not_effective_projection_admission() {
        let component = ComponentTypeId::new("game.PreservedValue");
        let mut schema = ComponentSchema::new(
            component.clone(),
            "Preserved value",
            ComponentSchemaVersion(2),
        )
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["note"]),
            ComponentValueKind::String,
        )
        .with_capabilities([ComponentCapability::Scene])]);
        schema
            .field_tombstones
            .push(ComponentFieldId::new("retired"));
        let registry = registry_with([schema.clone()]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.preserved-values").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let empty = ComponentValue::map([] as [(&str, ComponentValue); 0]);
        let base = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("last"))
            .with_component(
                component.clone(),
                SceneComponentRecord::new(ComponentSchemaVersion(2), empty.clone()),
            )]);
        let replacement = |component, mode, record| PrefabComponentIntent::Set {
            target: direct("last"),
            component,
            mode,
            record,
            bindings: vec![],
        };
        let mut cases = [ComponentSchemaVersion::ONE, ComponentSchemaVersion(3)]
            .map(|version| {
                replacement(
                    component.clone(),
                    PrefabComponentSetMode::Replace,
                    SceneComponentRecord::new(version, empty.clone()),
                )
            })
            .to_vec();
        cases.extend([
            replacement(
                ComponentTypeId::new("game.Unknown"),
                PrefabComponentSetMode::Add,
                SceneComponentRecord::new(ComponentSchemaVersion(2), empty.clone()),
            ),
            replacement(
                component.clone(),
                PrefabComponentSetMode::Replace,
                SceneComponentRecord::new(ComponentSchemaVersion(2), empty.clone())
                    .with_owner(PersistentComponentSchemaOwnerId::try_new("game.other").unwrap()),
            ),
            replacement(
                component.clone(),
                PrefabComponentSetMode::Replace,
                SceneComponentRecord::new(
                    ComponentSchemaVersion(2),
                    ComponentValue::map([("undeclared", ComponentValue::Null)]),
                ),
            ),
            PrefabComponentIntent::Properties {
                target: direct("last"),
                component: component.clone(),
                version: ComponentSchemaVersion(2),
                claims: vec![PrefabPropertyClaim {
                    path: nara_reflect::ComponentPropertyPath::new(ComponentFieldId::new(
                        "retired",
                    )),
                    intent: PrefabPropertyIntent::Set(ComponentValue::String("retained".into())),
                    bindings: vec![],
                }],
            },
        ]);
        for intent in cases {
            let mut overlay = PrefabOverlay::new(key("outer"));
            overlay.components.push(intent);
            let source = scene(overlay.clone(), true);
            let admitted =
                SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring(
                        AuthoringDocumentSource::new(path("preserved-source")),
                        &bundle,
                        &[],
                    )
                    .unwrap();
            assert_eq!(
                admitted.readiness(),
                crate::AuthoringDocumentReadiness::Degraded
            );
            assert_eq!(admitted.document(), &source);
            let session =
                SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                    admitted.publish(),
                    crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
                );
            let before = (
                session.document().clone(),
                session.revision(),
                session.history_status(),
            );
            let report = session
                .document()
                .expand_prefabs(&registry, &resolver(base.clone()));
            assert!(report.candidate.is_none(), "{:?}", overlay);
            assert!(report.provenance().is_none());
            assert!(report.diagnostics.has_errors());
            assert_eq!(
                (
                    session.document().clone(),
                    session.revision(),
                    session.history_status()
                ),
                before
            );
            assert_eq!(
                session
                    .unavailable_record_manifest()
                    .unwrap()
                    .overlay_declarations()
                    .next()
                    .unwrap()
                    .1
                    .overrides
                    .as_ref(),
                Some(&overlay)
            );
            let reopened = SceneDocumentCandidate::decode_json_str(
                &session.document().to_json_string().unwrap(),
            )
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(path("preserved-source")),
                &bundle,
                &[],
            )
            .unwrap();
            assert_eq!(reopened.document(), &source);
            assert_eq!(
                reopened.readiness(),
                crate::AuthoringDocumentReadiness::Degraded
            );
        }
    }

    #[test]
    fn lawful_empty_and_absent_containers_expand_without_templates_adapters_or_history() {
        let component = ComponentTypeId::new("game.OptionalContainer");
        let schema = ComponentSchema::new(
            component.clone(),
            "Optional container",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities([ComponentCapability::Scene])
        .with_fields([ComponentFieldSchema::optional(
            ComponentFieldId::new("note"),
            "Note",
            ComponentFieldPath::from_fields(["group", "note"]),
            ComponentValueKind::String,
        )
        .with_capabilities([ComponentCapability::Scene])]);
        assert!(schema.authoring_initial_value.is_none());
        assert!(!schema.has_capability(ComponentCapability::Edit));
        let registry = registry_with([schema.clone()]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.optional-containers").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let mut effective_values = Vec::new();
        for value in [
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ComponentValue::map([(
                "group",
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            )]),
        ] {
            let mut overlay = PrefabOverlay::new(key("outer"));
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: direct("socket"),
                component: component.clone(),
                record: SceneComponentRecord::new(ComponentSchemaVersion::ONE, value.clone()),
                bindings: vec![],
            });
            let source = scene(overlay, true);
            let published =
                SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring(
                        AuthoringDocumentSource::new(path("optional-container-source")),
                        &bundle,
                        &[],
                    )
                    .unwrap()
                    .publish();
            let session =
                SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                    published,
                    crate::SceneAuthoringHistoryLimits::new(1, 1, 1, 1, 1, 1, 1).unwrap(),
                );
            let before = (
                session.document().clone(),
                session.revision(),
                session.history_status(),
            );
            let report = session
                .document()
                .expand_prefabs(&registry, &resolver(outer(&["socket"])));
            assert!(report.candidate.is_some(), "{:?}", report.diagnostics);
            let actual = &record(&report, &projected(&report, "host_a", &direct("socket")))
                .components[&component]
                .value;
            assert_eq!(actual, &value);
            effective_values.push(actual.clone());
            assert_eq!(
                (
                    session.document().clone(),
                    session.revision(),
                    session.history_status()
                ),
                before
            );
        }
        assert_ne!(effective_values[0], effective_values[1]);
    }
}
