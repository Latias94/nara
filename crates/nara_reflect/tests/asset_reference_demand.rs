use std::collections::BTreeMap;

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
use nara_core::{ByteLimit, ItemLimit, Vec2};
use nara_reflect::{
    __macro_support::PersistentFieldCodec,
    __private::{
        collect_declared_effective_asset_references_with_admission,
        collect_declared_field_asset_references_with_admission,
        collect_declared_value_asset_references_with_admission,
    },
    AssetReferenceDemand, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeErrorKind, ComponentShapeMember, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, EntityReference,
    collect_declared_asset_references_from_schema, collect_declared_field_asset_references,
    collect_declared_value_asset_references_with_limits, effective_component_value,
    encode_asset_reference_value, resolve_component_property_schema_claim,
    rewrite_declared_asset_references, validate_component_value,
};

const IMMEDIATE: ComponentValueShape =
    ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate);
const DEFERRED: ComponentValueShape =
    ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred);

type NestedAssets = Option<Vec<BTreeMap<String, AssetRef>>>;
type NestedEntities = Option<Vec<BTreeMap<String, EntityReference>>>;

const _: () = {
    assert!(<NestedAssets as PersistentFieldCodec>::HAS_ASSET_REFERENCES);
    assert!(!<NestedEntities as PersistentFieldCodec>::HAS_ASSET_REFERENCES);
    let _: nara_asset::AssetReferenceDemand =
        nara_reflect::__macro_support::AssetReferenceDemand::Deferred;
};

fn product(source: &str) -> AssetRef {
    AssetRef::product(
        AssetProductRef::new(
            StableAssetId::parse_str(source).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap(),
    )
}

fn shared_product() -> AssetRef {
    product("919617ea-c89c-4f83-9cce-b084de435a83")
}

fn mixed_shape() -> ComponentValueShape {
    ComponentValueShape::record_members([
        ComponentShapeMember::new("immediate", IMMEDIATE).with_id(ComponentFieldId::new("current")),
        ComponentShapeMember::new(
            "deferred",
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
                DEFERRED,
            ))),
        )
        .with_id(ComponentFieldId::new("future")),
        ComponentShapeMember::new(
            "opaque",
            ComponentValueShape::opaque(ComponentValueKind::Map),
        ),
    ])
}

fn mixed_payload() -> ComponentValue {
    let reference = encode_asset_reference_value(&shared_product());
    ComponentValue::map([
        ("immediate", reference.clone()),
        (
            "deferred",
            ComponentValue::map([(
                "name.with.dot",
                ComponentValue::List(vec![ComponentValue::Null, reference.clone()]),
            )]),
        ),
        ("opaque", ComponentValue::map([("undeclared", reference)])),
    ])
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.ReferenceDemand"),
        "Reference demand",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::from_fields(["settings", "wire_root"]),
        mixed_shape(),
        mixed_payload(),
    )
    .with_capability(ComponentCapability::AssetRef)])
}

#[test]
fn nested_defaults_and_all_effective_collectors_preserve_each_leaf_demand() {
    let schema = schema();
    let absent = ComponentValue::Map(BTreeMap::new());
    let effective = effective_component_value(&schema, &absent, Default::default()).unwrap();
    validate_component_value(&schema, effective.as_ref()).unwrap();
    let references =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &absent).unwrap();
    assert_eq!(absent, ComponentValue::Map(BTreeMap::new()));
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].demand(), AssetReferenceDemand::Immediate);
    assert_eq!(references[1].demand(), AssetReferenceDemand::Deferred);
    for reference in &references {
        assert_eq!(reference.field_id(), schema.fields()[0].id());
        assert_eq!(reference.asset_ref(), &shared_product());
    }
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::from_fields(["settings", "wire_root", "immediate"]),
    );
    assert_eq!(
        references[1].path(),
        &ComponentFieldPath::new([
            ComponentFieldPathSegment::field("settings"),
            ComponentFieldPathSegment::field("wire_root"),
            ComponentFieldPathSegment::field("deferred"),
            ComponentFieldPathSegment::field("name.with.dot"),
            ComponentFieldPathSegment::index(1),
        ]),
    );
    assert_eq!(
        collect_declared_effective_asset_references_with_admission(
            &schema,
            schema.version(),
            effective.as_ref(),
            Default::default(),
            |_, _, _| Ok::<(), ()>(()),
        )
        .unwrap(),
        references,
    );
    let field = &schema.fields()[0];
    let payload = effective.get_path(field.path()).unwrap();
    assert_eq!(
        collect_declared_field_asset_references(schema.id(), field, payload).unwrap(),
        references,
    );
    assert_eq!(
        collect_declared_field_asset_references_with_admission(
            schema.id(),
            field,
            payload,
            Default::default(),
            |_, _, _| Ok::<(), ()>(()),
        )
        .unwrap(),
        references,
    );
    let explicit_null = ComponentValue::map([(
        "settings",
        ComponentValue::map([("wire_root", ComponentValue::Null)]),
    )]);
    assert!(
        collect_declared_asset_references_from_schema(&schema, schema.version(), &explicit_null)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn selected_property_inventories_use_the_selected_leaf_instead_of_the_owning_field() {
    let schema = schema();
    for (member, payload, demand) in [
        (
            "current",
            encode_asset_reference_value(&shared_product()),
            AssetReferenceDemand::Immediate,
        ),
        (
            "future",
            mixed_payload().field("deferred").unwrap().clone(),
            AssetReferenceDemand::Deferred,
        ),
    ] {
        let mut budget = ComponentPropertyReadBudget::new(
            ItemLimit::new(128).unwrap(),
            ByteLimit::new(4096).unwrap(),
        );
        let property = ComponentPropertyPath::new(ComponentFieldId::new("root"))
            .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(member))]);
        let selected = resolve_component_property_schema_claim(
            &schema,
            schema.version(),
            &property,
            &mut budget,
        )
        .unwrap();
        assert!(std::ptr::eq(selected.field, &schema.fields()[0]));
        let references = collect_declared_value_asset_references_with_limits(
            schema.id(),
            selected.field,
            selected.shape,
            &payload,
            Default::default(),
        )
        .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].asset_ref(), &shared_product());
        assert_eq!(references[0].demand(), demand);
        let expected_path = if demand == AssetReferenceDemand::Immediate {
            ComponentFieldPath::empty()
        } else {
            ComponentFieldPath::new([
                ComponentFieldPathSegment::field("name.with.dot"),
                ComponentFieldPathSegment::index(1),
            ])
        };
        assert_eq!(references[0].path(), &expected_path);
        assert_eq!(
            collect_declared_value_asset_references_with_admission(
                schema.id(),
                selected.field,
                selected.shape,
                &payload,
                Default::default(),
                |_, _, _| Ok::<(), ()>(()),
            )
            .unwrap(),
            references,
        );
    }
}

#[test]
fn identity_rewriting_observes_and_preserves_mixed_default_demand() {
    let schema = schema();
    let absent = ComponentValue::Map(BTreeMap::new());
    let mut value = effective_component_value(&schema, &absent, Default::default())
        .unwrap()
        .into_owned();
    let references =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &value).unwrap();
    let replacement = product("7214561b-ae28-454f-888b-fd049235d212");
    let mut observed = Vec::new();
    rewrite_declared_asset_references(&mut value, &references, |reference| {
        observed.push(reference.demand());
        Ok::<_, ()>(replacement.clone())
    })
    .unwrap();
    assert_eq!(
        observed,
        [
            AssetReferenceDemand::Immediate,
            AssetReferenceDemand::Deferred
        ],
    );
    let rewritten =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &value).unwrap();
    assert_eq!(rewritten.len(), references.len());
    for (before, after) in references.iter().zip(rewritten) {
        assert_eq!(after.field_id(), before.field_id());
        assert_eq!(after.path(), before.path());
        assert_eq!(after.demand(), before.demand());
        assert_eq!(after.asset_ref(), &replacement);
    }
    assert_eq!(
        value
            .get_path(&ComponentFieldPath::from_fields([
                "settings",
                "wire_root",
                "opaque",
                "undeclared",
            ]))
            .unwrap(),
        &encode_asset_reference_value(&shared_product()),
    );
}

#[test]
fn recursive_override_updates_all_declared_leaves_and_preserves_opaque_shapes() {
    let shape = ComponentValueShape::keyed_list(
        ComponentValueShape::record_members([
            ComponentShapeMember::new("id", ComponentValueKind::String)
                .with_id(ComponentFieldId::new("key")),
            ComponentShapeMember::new("assets", mixed_shape()),
        ]),
        ComponentFieldId::new("key"),
    );
    let value = ComponentValue::List(vec![ComponentValue::map([
        ("id", ComponentValue::String("row".into())),
        ("assets", mixed_payload()),
    ])]);
    for demand in [
        AssetReferenceDemand::Deferred,
        AssetReferenceDemand::Immediate,
    ] {
        let overridden = shape.clone().with_asset_reference_demand(demand).unwrap();
        overridden.validate_value(&value).unwrap();
        let field = ComponentFieldSchema::required(
            ComponentFieldId::new("rows"),
            "Rows",
            ComponentFieldPath::from_fields(["rows"]),
            overridden,
        )
        .with_capability(ComponentCapability::AssetRef);
        let references = collect_declared_value_asset_references_with_limits(
            &ComponentTypeId::new("test.ReferenceDemand"),
            &field,
            field.shape(),
            &value,
            Default::default(),
        )
        .unwrap();
        assert_eq!(references.len(), 2);
        assert!(
            references
                .iter()
                .all(|reference| reference.demand() == demand)
        );
        let ComponentValueShape::KeyedList { item, .. } = field.shape() else {
            unreachable!()
        };
        let ComponentValueShape::Record { members } = item.as_ref() else {
            unreachable!()
        };
        let ComponentValueShape::Record { members } = &members[1].shape else {
            unreachable!()
        };
        assert_eq!(
            members[2].shape,
            ComponentValueShape::opaque(ComponentValueKind::Map),
        );
    }
}

#[test]
fn recursive_override_rejects_no_leaf_and_preserves_the_shape_owners_limits() {
    for shape in [
        ComponentValueShape::scalar(ComponentValueKind::String),
        ComponentValueShape::EntityRef,
        ComponentValueShape::opaque(ComponentValueKind::Map),
        ComponentValueShape::opaque(ComponentValueKind::List),
        ComponentValueShape::optional(ComponentValueShape::map(ComponentValueShape::list(
            ComponentValueShape::EntityRef,
        ))),
    ] {
        let error = shape
            .with_asset_reference_demand(AssetReferenceDemand::Deferred)
            .unwrap_err();
        assert_eq!(
            error.kind,
            ComponentShapeErrorKind::MissingAssetReferenceLeaf
        );
        assert_eq!(error.path, ComponentFieldPath::empty());
    }
    let mut deep = IMMEDIATE;
    for _ in 0..65 {
        deep = ComponentValueShape::list(deep);
    }
    let wide = ComponentValueShape::record((0..16_384).map(|index| {
        (
            format!("field_{index}"),
            ComponentValueShape::scalar(ComponentValueKind::Bool),
        )
    }));
    for shape in [
        deep,
        wide,
        ComponentValueShape::opaque(ComponentValueKind::AssetRef),
    ] {
        let expected = shape.validate().unwrap_err();
        assert_eq!(
            shape
                .with_asset_reference_demand(AssetReferenceDemand::Deferred)
                .unwrap_err(),
            expected,
        );
    }
}

#[test]
fn codec_asset_leaf_predicates_agree_with_their_actual_shapes_and_aliases() {
    fn check<T: PersistentFieldCodec>(expected: bool) {
        assert_eq!(T::HAS_ASSET_REFERENCES, expected);
        assert_eq!(
            T::value_shape().contains_kind(ComponentValueKind::AssetRef),
            expected,
        );
    }
    check::<bool>(false);
    check::<String>(false);
    check::<i8>(false);
    check::<i16>(false);
    check::<i32>(false);
    check::<i64>(false);
    check::<u8>(false);
    check::<u16>(false);
    check::<u32>(false);
    check::<u64>(false);
    check::<f32>(false);
    check::<f64>(false);
    check::<Vec2>(false);
    check::<EntityReference>(false);
    check::<AssetRef>(true);
    check::<NestedAssets>(true);
    check::<NestedEntities>(false);
    assert_eq!(<AssetRef as PersistentFieldCodec>::value_shape(), IMMEDIATE);
    assert_eq!(
        ComponentValueShape::from(ComponentValueKind::AssetRef),
        IMMEDIATE,
    );
}

#[test]
fn demand_changes_catalog_fingerprints_without_changing_reference_identity() {
    let first = ComponentSchemaCatalog {
        components: vec![schema()],
        ..Default::default()
    };
    let mut reordered = first.clone();
    let ComponentValueShape::Optional { value } = &mut reordered.components[0].fields[0].shape
    else {
        unreachable!()
    };
    let ComponentValueShape::Record { members } = value.as_mut() else {
        unreachable!()
    };
    members.reverse();
    assert_eq!(first.fingerprint(), reordered.fingerprint());

    let mut swapped = first.clone();
    let ComponentValueShape::Optional { value } = &mut swapped.components[0].fields[0].shape else {
        unreachable!()
    };
    let ComponentValueShape::Record { members } = value.as_mut() else {
        unreachable!()
    };
    members[0].shape = DEFERRED;
    members[1].shape = members[1]
        .shape
        .clone()
        .with_asset_reference_demand(AssetReferenceDemand::Immediate)
        .unwrap();
    // Equal counts of each demand still describe different leaf semantics.
    assert_ne!(first.fingerprint(), swapped.fingerprint());

    let mut changed = first.clone();
    let field = &mut changed.components[0].fields[0];
    field.shape = field
        .shape
        .clone()
        .with_asset_reference_demand(AssetReferenceDemand::Deferred)
        .unwrap();
    assert_eq!(
        first.components[0].fields[0].default_value(),
        field.default_value(),
    );
    assert_ne!(first.fingerprint(), changed.fingerprint());
}

#[cfg(feature = "serde")]
#[test]
fn wire_demand_is_explicit_in_canonical_shapes_and_missing_demand_is_rejected() {
    for (shape, spelling) in [(IMMEDIATE, "immediate"), (DEFERRED, "deferred")] {
        let wire = serde_json::to_value(&shape).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({ "shape": "asset_ref", "demand": spelling }),
        );
        assert_eq!(
            serde_json::from_value::<ComponentValueShape>(wire).unwrap(),
            shape,
        );
        let ron = ron::ser::to_string(&shape).unwrap();
        assert_eq!(ron::from_str::<ComponentValueShape>(&ron).unwrap(), shape);
    }
    let json_error =
        serde_json::from_str::<ComponentValueShape>(r#"{"shape":"asset_ref"}"#).unwrap_err();
    assert!(json_error.to_string().contains("demand"));
    let ron_error = ron::from_str::<ComponentValueShape>(r#"(shape:"asset_ref")"#).unwrap_err();
    assert!(ron_error.to_string().contains("demand"));

    let mut catalog = ComponentSchemaCatalog {
        components: vec![schema()],
        ..Default::default()
    };
    catalog.canonicalize();
    let json = catalog.to_json_string().unwrap();
    let ron = catalog.to_ron_string().unwrap();
    assert_eq!(
        ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap(),
        catalog,
    );
    assert_eq!(
        ComponentSchemaCatalog::from_ron_bytes(ron.as_bytes()).unwrap(),
        catalog,
    );
    let mut wire: serde_json::Value = serde_json::from_str(&json).unwrap();
    let members = wire["payload"]["components"][0]["fields"][0]["shape"]["value"]["members"]
        .as_array_mut()
        .unwrap();
    let immediate = members
        .iter_mut()
        .find(|member| member["name"] == "immediate")
        .unwrap();
    immediate["shape"].as_object_mut().unwrap().remove("demand");
    assert!(ComponentSchemaCatalog::from_json_bytes(&serde_json::to_vec(&wire).unwrap()).is_err());
}
