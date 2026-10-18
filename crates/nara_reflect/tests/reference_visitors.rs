use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{EntityReference, SceneEntityId};
use nara_reflect::__private::plan_declared_entity_references;
use nara_reflect::{
    ComponentCapability, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, DeclaredAssetReferenceError, EntityReferenceTraversalLimits,
    collect_declared_asset_references_from_schema, collect_declared_field_asset_references,
    rewrite_declared_entity_references,
};
use std::cell::Cell;

fn entity(value: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(value).unwrap(),
    })
}

fn asset(value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".to_owned())),
        ("value", ComponentValue::String(value.to_owned())),
    ])
}

fn schema(field: ComponentFieldSchema) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.ReferenceVisitor"),
        "Reference visitor",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field])
}

#[test]
fn nested_present_entity_references_keep_root_identity_and_concrete_paths() {
    let schema = schema(
        ComponentFieldSchema::required(
            ComponentFieldId::new("stable-links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            ComponentValueShape::list(ComponentValueShape::record([
                (
                    "target".to_owned(),
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ),
                (
                    "others".to_owned(),
                    ComponentValueShape::map(ComponentValueShape::EntityRef),
                ),
            ])),
        )
        .with_capability(ComponentCapability::EntityRef),
    );
    let mut value = ComponentValue::map([(
        "links",
        ComponentValue::List(vec![ComponentValue::map([
            ("target", ComponentValue::Null),
            (
                "others",
                ComponentValue::map([("name.with.dot", entity("enemy"))]),
            ),
        ])]),
    )]);
    let plan = plan_declared_entity_references::<()>(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    let expected = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("links"),
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("others"),
        ComponentFieldPathSegment::field("name.with.dot"),
    ]);
    let mut observed = Vec::new();
    plan.visit(&value, |path, _| {
        observed.push(path.clone());
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(observed, [expected.clone()]);
    assert_eq!(
        plan.root_field_id(&expected).unwrap().as_str(),
        "stable-links"
    );
    plan.rewrite_in_place(&mut value, |_, _| {
        Ok::<_, ()>(EntityReference::SceneLocal {
            entity: SceneEntityId::new("rewritten").unwrap(),
        })
    })
    .unwrap();
    assert_eq!(value.get_path(&expected).unwrap(), &entity("rewritten"));
}

#[test]
fn nested_asset_leaves_keep_distinct_paths_under_one_root_field() {
    let schema = schema(
        ComponentFieldSchema::required(
            ComponentFieldId::new("stable-assets"),
            "Assets",
            ComponentFieldPath::from_fields(["assets"]),
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::AssetRef,
            ))),
        )
        .with_capability(ComponentCapability::AssetRef),
    );
    let value = ComponentValue::map([(
        "assets",
        ComponentValue::map([(
            "portrait",
            ComponentValue::List(vec![
                asset("textures/a.png"),
                ComponentValue::Null,
                asset("textures/b.png"),
            ]),
        )]),
    )]);
    let references =
        collect_declared_asset_references_from_schema(&schema, ComponentSchemaVersion::ONE, &value)
            .unwrap();
    assert_eq!(references.len(), 2);
    for (reference, index, path) in [
        (&references[0], 0, "textures/a.png"),
        (&references[1], 2, "textures/b.png"),
    ] {
        assert_eq!(reference.field_id().as_str(), "stable-assets");
        assert_eq!(
            reference.path(),
            &ComponentFieldPath::new([
                ComponentFieldPathSegment::field("assets"),
                ComponentFieldPathSegment::field("portrait"),
                ComponentFieldPathSegment::index(index),
            ])
        );
        assert_eq!(reference.asset_ref(), &AssetRef::path(path).unwrap());
    }
    let mut invalid = value.clone();
    invalid
        .replace_path(
            references[1].path(),
            ComponentValue::map([
                ("kind", ComponentValue::String("path".to_owned())),
                ("value", ComponentValue::String("textures/b.png".to_owned())),
                ("extra", ComponentValue::Bool(true)),
            ]),
        )
        .unwrap();
    assert!(matches!(
        collect_declared_asset_references_from_schema(&schema, ComponentSchemaVersion::ONE, &invalid),
        Err(DeclaredAssetReferenceError::InvalidValueShape { path, .. })
            if path == *references[1].path()
    ));
}

#[test]
fn set_field_asset_traversal_keeps_nested_stable_ids_and_root_paths() {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("stable-assets"),
        "Assets",
        ComponentFieldPath::from_fields(["settings", "assets"]),
        ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::AssetRef)),
    )
    .with_capability(ComponentCapability::AssetRef);
    let stable_id = "3c2d4005-63bd-4e2a-9792-bb2f48a12821";
    let value = ComponentValue::map([(
        "portraits",
        ComponentValue::List(vec![ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(stable_id.to_owned())),
        ])]),
    )]);
    let references = collect_declared_field_asset_references(
        &ComponentTypeId::new("nara.test.ReferenceVisitor"),
        &field,
        &value,
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].field_id(), field.id());
    assert_eq!(
        references[0].asset_ref(),
        &AssetRef::stable_id(stable_id).unwrap()
    );
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::new([
            ComponentFieldPathSegment::field("settings"),
            ComponentFieldPathSegment::field("assets"),
            ComponentFieldPathSegment::field("portraits"),
            ComponentFieldPathSegment::index(0),
        ])
    );
    let component = ComponentValue::map([("settings", ComponentValue::map([("assets", value)]))]);
    let whole = collect_declared_asset_references_from_schema(
        &schema(field),
        ComponentSchemaVersion::ONE,
        &component,
    )
    .unwrap();
    assert_eq!(references, whole);
}

#[test]
fn reference_collection_leaves_unrelated_field_validation_to_the_component_owner() {
    let mut schema = schema(
        ComponentFieldSchema::required(
            ComponentFieldId::new("image"),
            "Image",
            ComponentFieldPath::from_fields(["image"]),
            ComponentValueShape::AssetRef,
        )
        .with_capability(ComponentCapability::AssetRef),
    );
    schema.fields.push(ComponentFieldSchema::required(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::U64,
    ));
    let value = ComponentValue::map([("image", asset("textures/portrait.png"))]);
    assert_eq!(
        collect_declared_asset_references_from_schema(&schema, schema.version(), &value)
            .unwrap()
            .len(),
        1,
    );
    assert!(nara_reflect::validate_component_value(&schema, &value).is_err());
}

#[test]
fn optional_record_members_must_exist_and_opaque_entity_references_reject() {
    let record = schema(
        ComponentFieldSchema::required(
            ComponentFieldId::new("record"),
            "Record",
            ComponentFieldPath::from_fields(["record"]),
            ComponentValueShape::record([(
                "target".to_owned(),
                ComponentValueShape::optional(ComponentValueShape::EntityRef),
            )]),
        )
        .with_capability(ComponentCapability::EntityRef),
    );
    let missing = ComponentValue::map([(
        "record",
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
    )]);
    assert!(matches!(
        plan_declared_entity_references::<()>(
            &record,
            &missing,
            EntityReferenceTraversalLimits::default()
        ),
        Err(ComponentEntityReferenceRewriteError::InvalidPath { .. })
    ));

    let opaque = schema(ComponentFieldSchema::required(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueKind::Map,
    ));
    let value = ComponentValue::map([(
        "payload",
        ComponentValue::map([("hidden", entity("enemy"))]),
    )]);
    assert!(matches!(
        plan_declared_entity_references::<()>(
            &opaque,
            &value,
            EntityReferenceTraversalLimits::default()
        ),
        Err(ComponentEntityReferenceRewriteError::UndeclaredReference { .. })
    ));
    assert!(matches!(
        collect_declared_asset_references_from_schema(&opaque, ComponentSchemaVersion::ONE, &value),
        Err(DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::UndeclaredReference { .. },
            ..
        })
    ));
}

#[test]
fn reference_result_paths_share_the_node_and_byte_work_limits() {
    let schema = schema(
        ComponentFieldSchema::required(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            ComponentValueShape::list(ComponentValueShape::EntityRef),
        )
        .with_capability(ComponentCapability::EntityRef),
    );
    let value = ComponentValue::map([(
        "links",
        ComponentValue::List(vec![entity("a"), entity("b")]),
    )]);
    for limits in [
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(5).unwrap(),
            ByteLimit::new(512).unwrap(),
            DepthLimit::new(8).unwrap(),
        ),
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(32).unwrap(),
            ByteLimit::new(8).unwrap(),
            DepthLimit::new(8).unwrap(),
        ),
    ] {
        let callbacks = Cell::new(0);
        assert!(
            rewrite_declared_entity_references(&schema, &value, limits, |_, reference| {
                callbacks.set(callbacks.get() + 1);
                Ok::<_, ()>(reference.clone())
            })
            .is_err()
        );
        assert_eq!(callbacks.get(), 0);
    }
    assert!(
        plan_declared_entity_references::<()>(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default()
        )
        .is_ok()
    );
}
