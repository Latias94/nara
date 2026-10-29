use nara_asset::{
    AssetProductIdentityError, AssetProductRef, AssetRef, ImportedProductId, StableAssetId,
};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    __macro_support::{PersistentFieldCodec, encode_persistent_field},
    __private::{
        AssetReferenceCollectionError, collect_declared_value_asset_references_with_admission,
    },
    AssetReferenceRewriteError, ComponentCapability, ComponentCodecError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueMaterializationLimits,
    ComponentValueShape, DeclaredAssetReferenceError,
    collect_declared_asset_references_from_schema, collect_declared_field_asset_references,
    collect_declared_value_asset_references_with_limits, effective_component_value,
    resolve_component_property_schema_claim, rewrite_declared_asset_references,
};

const SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";
const COPIED_SOURCE: &str = "7214561b-ae28-454f-888b-fd049235d212";
const PRODUCT: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";
const OTHER_PRODUCT: &str = "08f508de-5ce8-442a-adc9-fef61fb9a75f";

fn product(source: &str, id: Option<&str>) -> AssetRef {
    AssetRef::product(
        AssetProductRef::new(
            StableAssetId::parse_str(source).unwrap(),
            id.map_or(ImportedProductId::PRIMARY, |id| {
                ImportedProductId::parse_subproduct(id).unwrap()
            }),
        )
        .unwrap(),
    )
}

fn wire(source: &str, id: Option<&str>) -> ComponentValue {
    let selector = match id {
        None => ComponentValue::map([("kind", ComponentValue::String("primary".into()))]),
        Some(id) => ComponentValue::map([
            ("kind", ComponentValue::String("subproduct".into())),
            ("id", ComponentValue::String(id.into())),
        ]),
    };
    ComponentValue::map([
        ("kind", ComponentValue::String("product".into())),
        (
            "value",
            ComponentValue::map([
                ("source", ComponentValue::String(source.into())),
                ("product", selector),
            ]),
        ),
    ])
}

fn field(shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::from_fields(["root"]),
        shape,
    )
    .with_capability(ComponentCapability::AssetRef)
}

fn schema(field: ComponentFieldSchema) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.ProductReferences"),
        "Product references",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field])
}

fn collect(
    value: &ComponentValue,
) -> Result<Vec<nara_reflect::DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let field = field(ComponentValueShape::AssetRef);
    collect_declared_value_asset_references_with_limits(
        &ComponentTypeId::new("test.ProductReferences"),
        &field,
        field.shape(),
        value,
        Default::default(),
    )
}

#[test]
fn product_asset_leaf_codec_preserves_primary_and_subproduct_wire() {
    assert_eq!(
        <AssetRef as PersistentFieldCodec>::value_shape(),
        ComponentValueShape::AssetRef
    );
    for id in [None, Some(PRODUCT)] {
        let reference = product(SOURCE, id);
        let expected = wire(SOURCE, id);
        assert_eq!(encode_persistent_field(&reference).unwrap(), expected);
        assert_eq!(
            <AssetRef as PersistentFieldCodec>::decode(&expected, "caption").unwrap(),
            reference
        );
        assert!(ComponentValueShape::AssetRef.accepts_value(&expected));
        assert_eq!(collect(&expected).unwrap()[0].asset_ref(), &reference);
    }
    assert_ne!(product(SOURCE, None), AssetRef::stable_id(SOURCE).unwrap());
}

#[test]
fn nested_products_share_field_and_property_payload_traversal() {
    let selected_shape = ComponentValueShape::map(ComponentValueShape::list(
        ComponentValueShape::optional(ComponentValueShape::AssetRef),
    ));
    let schema = schema(field(ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_captions", selected_shape)
            .with_id(ComponentFieldId::new("captions")),
        ComponentShapeMember::new("ordinary", ComponentValueKind::String),
        ComponentShapeMember::new(
            "opaque",
            ComponentValueShape::opaque(ComponentValueKind::Map),
        ),
    ])));
    let payload = ComponentValue::map([(
        "a.b",
        ComponentValue::List(vec![
            wire(SOURCE, Some(PRODUCT)),
            ComponentValue::Null,
            wire(SOURCE, None),
        ]),
    )]);
    let root = ComponentValue::map([
        ("wire_captions", payload.clone()),
        ("ordinary", ComponentValue::String(PRODUCT.into())),
        ("opaque", wire(SOURCE, Some(OTHER_PRODUCT))),
    ]);
    let component = ComponentValue::map([("root", root.clone())]);
    let original = component.clone();
    let whole =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &component)
            .unwrap();
    let selected_field =
        collect_declared_field_asset_references(schema.id(), &schema.fields()[0], &root).unwrap();
    assert_eq!(whole, selected_field);
    assert_eq!(whole.len(), 2);
    let claim = resolve_component_property_schema_claim(
        &schema,
        schema.version(),
        &ComponentPropertyPath::new(ComponentFieldId::new("root")).with_steps([
            ComponentPropertyStep::Member(ComponentFieldId::new("captions")),
        ]),
        &mut ComponentPropertyReadBudget::new(
            ItemLimit::new(128).unwrap(),
            ByteLimit::new(4096).unwrap(),
        ),
    )
    .unwrap();
    let selected = collect_declared_value_asset_references_with_limits(
        schema.id(),
        claim.field,
        claim.shape,
        &payload,
        Default::default(),
    )
    .unwrap();
    assert_eq!(selected.len(), 2);
    for (index, id) in [(0, Some(PRODUCT)), (1, None)] {
        assert_eq!(selected[index].asset_ref(), &product(SOURCE, id));
        assert_eq!(selected[index].field_id(), whole[index].field_id());
        assert_eq!(
            &whole[index].path().segments()[2..],
            selected[index].path().segments()
        );
    }
    assert_eq!(component, original);
}

#[test]
fn product_default_materialization_and_source_rebind_keep_the_product_id() {
    let schema = schema(
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("root"),
            "Root",
            ComponentFieldPath::from_fields(["root"]),
            ComponentValueShape::AssetRef,
            wire(SOURCE, Some(PRODUCT)),
        )
        .with_capability(ComponentCapability::AssetRef),
    );
    let authored = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let references =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &authored)
            .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].asset_ref(), &product(SOURCE, Some(PRODUCT)));
    // The complete expanded component is eight nodes, 121 logical bytes and five levels.
    let limits = |nodes, bytes, depth| {
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(depth).unwrap(),
        )
    };
    assert!(effective_component_value(&schema, &authored, limits(8, 121, 5)).is_ok());
    assert!(matches!(
        effective_component_value(&schema, &authored, limits(7, 121, 5)),
        Err(ComponentCodecError::MaterializationNodeLimit { maximum: 7, .. })
    ));
    assert!(matches!(
        effective_component_value(&schema, &authored, limits(8, 120, 5)),
        Err(ComponentCodecError::MaterializationByteLimit { maximum: 120, .. })
    ));
    assert!(matches!(
        effective_component_value(&schema, &authored, limits(8, 121, 4)),
        Err(ComponentCodecError::MaterializationDepthLimit { maximum: 4, .. })
    ));
    let mut candidate = effective_component_value(&schema, &authored, Default::default())
        .unwrap()
        .into_owned();
    rewrite_declared_asset_references(&mut candidate, &references, |reference| {
        let old = reference.asset_ref().as_product().unwrap();
        Ok::<_, ()>(AssetRef::product(
            AssetProductRef::new(
                StableAssetId::parse_str(COPIED_SOURCE).unwrap(),
                old.product(),
            )
            .unwrap(),
        ))
    })
    .unwrap();
    assert_eq!(
        candidate,
        ComponentValue::map([("root", wire(COPIED_SOURCE, Some(PRODUCT)))])
    );
    assert_eq!(
        authored,
        ComponentValue::map([] as [(&str, ComponentValue); 0])
    );
    assert!(matches!(
        collect_declared_asset_references_from_schema(
            &schema,
            schema.version(),
            &ComponentValue::map([("root", ComponentValue::Null)])
        )
        .unwrap()
        .as_slice(),
        []
    ));
}

#[test]
fn product_rewrite_checks_the_complete_old_identity_before_any_callback() {
    for id in [None, Some(PRODUCT)] {
        let original = wire(SOURCE, id);
        let references = collect(&original).unwrap();
        for mut stale in [
            wire(COPIED_SOURCE, id),
            wire(SOURCE, Some(OTHER_PRODUCT)),
            encode_persistent_field(&AssetRef::stable_id(SOURCE).unwrap()).unwrap(),
        ] {
            let before = stale.clone();
            let result = rewrite_declared_asset_references(
                &mut stale,
                &references,
                |_| -> Result<AssetRef, ()> {
                    panic!("stale identities must refuse before callbacks");
                },
            );
            assert_eq!(
                result,
                Err(AssetReferenceRewriteError::ChangedReference {
                    path: ComponentFieldPath::empty()
                })
            );
            assert_eq!(stale, before);
        }
        let mut candidate = original;
        rewrite_declared_asset_references(&mut candidate, &references, |_| {
            Ok::<_, ()>(product(COPIED_SOURCE, id))
        })
        .unwrap();
        assert_eq!(candidate, wire(COPIED_SOURCE, id));
    }
}

#[test]
fn product_references_reject_missing_null_wrong_shapes_and_unknown_fields() {
    let mut cases = vec![ComponentValue::Null, ComponentValue::Bool(false)];
    for (path, invalid) in [
        (vec!["value"], ComponentValue::String(SOURCE.into())),
        (vec!["value", "source"], ComponentValue::Null),
        (vec!["value", "product"], ComponentValue::Null),
        (vec!["value", "product", "kind"], ComponentValue::Bool(true)),
        (
            vec!["value", "product", "kind"],
            ComponentValue::String("unknown".into()),
        ),
        (vec!["value", "product", "id"], ComponentValue::U64(1)),
    ] {
        let mut value = wire(SOURCE, Some(PRODUCT));
        *value
            .get_path_mut(&ComponentFieldPath::from_fields(path))
            .unwrap() = invalid;
        cases.push(value);
    }
    for path in [vec![], vec!["value"], vec!["value", "product"]] {
        let mut value = wire(SOURCE, Some(PRODUCT));
        let ComponentValue::Map(fields) = value
            .get_path_mut(&ComponentFieldPath::from_fields(path))
            .unwrap()
        else {
            unreachable!()
        };
        fields.insert("unknown".into(), ComponentValue::Null);
        cases.push(value);
    }
    for (path, name) in [
        (vec![], "value"),
        (vec!["value"], "source"),
        (vec!["value"], "product"),
        (vec!["value", "product"], "id"),
        (vec!["value", "product"], "kind"),
    ] {
        let mut value = wire(SOURCE, Some(PRODUCT));
        let ComponentValue::Map(fields) = value
            .get_path_mut(&ComponentFieldPath::from_fields(path))
            .unwrap()
        else {
            unreachable!()
        };
        fields.remove(name);
        cases.push(value);
    }
    let mut primary_with_id = wire(SOURCE, Some(PRODUCT));
    *primary_with_id
        .get_path_mut(&ComponentFieldPath::from_fields([
            "value", "product", "kind",
        ]))
        .unwrap() = ComponentValue::String("primary".into());
    cases.push(primary_with_id);
    for value in cases {
        assert!(!ComponentValueShape::AssetRef.accepts_value(&value));
        assert!(<AssetRef as PersistentFieldCodec>::decode(&value, "caption").is_err());
        let expected = collect(&value).unwrap_err();
        let field = field(ComponentValueShape::AssetRef);
        assert_eq!(
            collect_declared_value_asset_references_with_admission(
                &ComponentTypeId::new("test.ProductReferences"),
                &field,
                field.shape(),
                &value,
                Default::default(),
                |_, _, _| Ok::<(), ()>(()),
            ),
            Err(AssetReferenceCollectionError::Reference(expected)),
        );
    }
    let schema = schema(field(ComponentValueShape::AssetRef));
    assert!(matches!(
        collect_declared_asset_references_from_schema(
            &schema,
            schema.version(),
            &ComponentValue::map([] as [(&str, ComponentValue); 0])
        ),
        Err(DeclaredAssetReferenceError::Traversal { .. })
    ));
}

#[test]
fn product_identity_failures_are_typed_canonical_and_do_not_retain_asset_text() {
    for invalid in [
        "private-invalid-product",
        "00000000-0000-0000-0000-000000000000",
        "011A8CC0-343A-4B95-8ADB-3AA6D81977ED",
        "011a8cc0343a4b958adb3aa6d81977ed",
    ] {
        let value = wire(SOURCE, Some(invalid));
        let error = collect(&value).unwrap_err();
        assert!(matches!(
            error,
            DeclaredAssetReferenceError::InvalidProductIdentity {
                source: AssetProductIdentityError::InvalidProductText
                    | AssetProductIdentityError::NilProduct,
                ..
            }
        ));
        assert!(!format!("{error:?} {error}").contains(invalid));
        assert!(!ComponentValueShape::AssetRef.accepts_value(&value));
    }
    for invalid in [
        "private-invalid-source",
        "919617EA-C89C-4F83-9CCE-B084DE435A83",
        "919617eac89c4f839cceb084de435a83",
    ] {
        let value = wire(invalid, Some(PRODUCT));
        let error = collect(&value).unwrap_err();
        assert!(matches!(
            error,
            DeclaredAssetReferenceError::InvalidProductSource { .. }
        ));
        assert!(!format!("{error:?} {error}").contains(invalid));
    }
    assert!(matches!(
        collect(&wire("00000000-0000-0000-0000-000000000000", None)),
        Err(DeclaredAssetReferenceError::InvalidProductIdentity {
            source: AssetProductIdentityError::NilSource,
            ..
        })
    ));
}
