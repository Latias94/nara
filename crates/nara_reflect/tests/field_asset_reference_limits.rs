use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_reflect::{
    ComponentCapability, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentTypeId,
    ComponentValue, ComponentValueShape, DeclaredAssetReferenceError,
    EntityReferenceTraversalLimits, collect_declared_field_asset_references,
    collect_declared_field_asset_references_with_limits,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn asset(kind: &str, value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_owned())),
        ("value", ComponentValue::String(value.to_owned())),
    ])
}

fn field(shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["settings", "image"]),
        shape,
    )
    .with_capability(ComponentCapability::AssetRef)
}

#[test]
fn field_asset_limits_include_input_and_returned_reference_work_at_the_exact_ceiling() {
    let component = ComponentTypeId::new("test.FieldAssets");
    let field = field(ComponentValueShape::AssetRef);
    let value = asset("path", "a.png");
    // Three input nodes plus one result; 18 input bytes plus 23 result bytes.
    // The two-segment component path places the asset payload strings at depth four.
    let references = collect_declared_field_asset_references_with_limits(
        &component,
        &field,
        &value,
        limits(4, 41, 4),
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].field_id(), field.id());
    assert_eq!(references[0].path(), field.path());
    assert_eq!(references[0].asset_ref(), &AssetRef::path("a.png").unwrap());
    assert_eq!(
        references,
        collect_declared_field_asset_references(&component, &field, &value).unwrap(),
    );
    for (limits, expected) in [
        (
            limits(3, 41, 4),
            ComponentEntityReferenceRewriteError::NodeLimit { maximum: 3 },
        ),
        (
            limits(4, 40, 4),
            ComponentEntityReferenceRewriteError::ByteLimit { maximum: 40 },
        ),
        (
            limits(4, 41, 3),
            ComponentEntityReferenceRewriteError::DepthLimit { maximum: 3 },
        ),
    ] {
        assert_eq!(
            collect_declared_field_asset_references_with_limits(&component, &field, &value, limits,),
            Err(DeclaredAssetReferenceError::Traversal {
                component: component.clone(),
                source: expected,
            }),
        );
    }
}

#[test]
fn bounded_nested_map_list_assets_keep_component_paths_and_skip_optional_nulls() {
    let component = ComponentTypeId::new("test.FieldAssets");
    let field = field(ComponentValueShape::map(ComponentValueShape::list(
        ComponentValueShape::optional(ComponentValueShape::AssetRef),
    )));
    let stable_id = "3c2d4005-63bd-4e2a-9792-bb2f48a12821";
    let value = ComponentValue::map([(
        "portraits",
        ComponentValue::List(vec![
            asset("path", "a.png"),
            ComponentValue::Null,
            asset("stable_id", stable_id),
        ]),
    )]);
    let references = collect_declared_field_asset_references_with_limits(
        &component,
        &field,
        &value,
        limits(16, 512, 6),
    )
    .unwrap();
    assert_eq!(references.len(), 2);
    for (reference, index, expected) in [
        (&references[0], 0, AssetRef::path("a.png").unwrap()),
        (&references[1], 2, AssetRef::stable_id(stable_id).unwrap()),
    ] {
        assert_eq!(reference.field_id(), field.id());
        assert_eq!(reference.asset_ref(), &expected);
        assert_eq!(
            reference.path(),
            &ComponentFieldPath::new([
                ComponentFieldPathSegment::field("settings"),
                ComponentFieldPathSegment::field("image"),
                ComponentFieldPathSegment::field("portraits"),
                ComponentFieldPathSegment::index(index),
            ]),
        );
    }
    assert_eq!(
        references,
        collect_declared_field_asset_references(&component, &field, &value).unwrap(),
    );
    assert!(matches!(
        collect_declared_field_asset_references_with_limits(
            &component,
            &field,
            &value,
            limits(16, 512, 5),
        ),
        Err(DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::DepthLimit { maximum: 5 },
            ..
        }),
    ));
}

#[test]
fn field_limits_preserve_required_optional_null_and_capability_semantics() {
    let component = ComponentTypeId::new("test.FieldAssets");
    let required = field(ComponentValueShape::AssetRef);
    let optional = ComponentFieldSchema::optional_with_default(
        required.id().clone(),
        "Image",
        required.path().clone(),
        ComponentValueShape::AssetRef,
        asset("path", "default.png"),
    )
    .with_capability(ComponentCapability::AssetRef);
    let finite = limits(16, 512, 4);
    assert_eq!(
        collect_declared_field_asset_references_with_limits(
            &component,
            &required,
            &ComponentValue::Null,
            finite,
        ),
        Err(DeclaredAssetReferenceError::RequiredFieldIsNull {
            component: component.clone(),
            field: required.id().clone(),
        }),
    );
    assert!(
        collect_declared_field_asset_references_with_limits(
            &component,
            &optional,
            &ComponentValue::Null,
            finite,
        )
        .unwrap()
        .is_empty(),
    );
    let explicit = asset("path", "a.png");
    for field in [&required, &optional] {
        for value in [&explicit, &ComponentValue::Null] {
            assert_eq!(
                collect_declared_field_asset_references_with_limits(
                    &component, field, value, finite,
                ),
                collect_declared_field_asset_references(&component, field, value),
            );
            let mut unmarked = field.clone();
            unmarked.capabilities.remove(&ComponentCapability::AssetRef);
            assert!(matches!(
                collect_declared_field_asset_references_with_limits(
                    &component, &unmarked, value, finite,
                ),
                Err(DeclaredAssetReferenceError::Traversal {
                    source: ComponentEntityReferenceRewriteError::MissingAssetRefCapability {
                        path,
                    },
                    ..
                }) if path == *field.path(),
            ));
        }
    }
}
