use nara_asset::AssetRef;
use nara_identity::SceneEntityId;

use super::*;
use crate::__private::{
    AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
    collect_declared_field_asset_references_with_admission,
    collect_declared_value_asset_references_with_admission,
};
use crate::{ComponentSchemaVersion, ComponentTypeId, DeclaredAssetReferenceError};

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.ReferenceFreeTraversal"),
        "Reference-free traversal",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn scalar_payload() -> ComponentValue {
    ComponentValue::List(vec![
        ComponentValue::U64(7),
        ComponentValue::String("abc".to_owned()),
        ComponentValue::map([("tail", ComponentValue::Null)]),
    ])
}

fn local(target: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(target).unwrap(),
    })
}

#[test]
fn no_field_plans_and_asset_collection_debit_one_complete_raw_pass() {
    let schema = schema([]);
    let value = ComponentValue::map([("payload", scalar_payload())]);
    let original = value.clone();
    let mut observed = (0, 0, 0);
    let plan = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        limits(6, 14, 4),
        |nodes, bytes, allocation| {
            observed.0 += nodes;
            observed.1 += bytes;
            observed.2 += allocation;
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();
    assert_eq!((observed.0, observed.1), (6, 14));
    assert!(observed.2 > 0);
    plan.visit(&value, |_, _| -> Result<(), ()> {
        panic!("reference-free plans must not emit a reference")
    })
    .unwrap();
    assert_eq!(
        plan,
        plan_declared_entity_references::<()>(&schema, &value, limits(6, 14, 4)).unwrap(),
    );

    let mut asset_work = (0, 0, 0);
    let assets = collect_declared_effective_asset_references_with_admission(
        &schema,
        schema.version(),
        &value,
        limits(6, 14, 4),
        |nodes, bytes, allocation| {
            asset_work.0 += nodes;
            asset_work.1 += bytes;
            asset_work.2 += allocation;
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();
    assert!(assets.is_empty());
    assert_eq!(asset_work, observed);

    for (ceiling, succeeds) in [
        (observed, true),
        ((5, 14, observed.2), false),
        ((6, 13, observed.2), false),
        ((6, 14, observed.2 - 1), false),
    ] {
        let mut charged = (0, 0, 0);
        let result = plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            limits(6, 14, 4),
            |nodes, bytes, allocation| {
                let next = (charged.0 + nodes, charged.1 + bytes, charged.2 + allocation);
                if next.0 > ceiling.0 || next.1 > ceiling.1 || next.2 > ceiling.2 {
                    return Err("actual work refused");
                }
                charged = next;
                Ok(())
            },
        );
        if succeeds {
            assert_eq!(result.unwrap(), plan);
            assert_eq!(charged, observed);
        } else {
            assert_eq!(
                result,
                Err(EntityReferencePlanError::Admission("actual work refused")),
            );
        }
    }
    assert_eq!(value, original);
}

#[test]
fn selected_field_and_payload_collectors_keep_implicit_opaque_and_explicit_null_semantics() {
    let component = ComponentTypeId::new("test.ReferenceFreeTraversal");
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("data"),
        "Data",
        ComponentFieldPath::from_fields(["data"]),
        ComponentValueKind::List,
        ComponentValue::List(vec![ComponentValue::String("default-only".to_owned())]),
    );
    assert_eq!(
        field.shape(),
        &ComponentValueShape::optional(ComponentValueShape::opaque(ComponentValueKind::List)),
    );
    for payload_relative in [false, true] {
        for (value, expected) in [(scalar_payload(), (7, 7)), (ComponentValue::Null, (2, 0))] {
            let mut observed = (0, 0);
            let mut admit = |nodes, bytes, _| {
                observed.0 += nodes;
                observed.1 += bytes;
                Ok::<_, &'static str>(())
            };
            let references = if payload_relative {
                collect_declared_value_asset_references_with_admission(
                    &component,
                    &field,
                    field.shape(),
                    &value,
                    EntityReferenceTraversalLimits::default(),
                    &mut admit,
                )
            } else {
                collect_declared_field_asset_references_with_admission(
                    &component,
                    &field,
                    &value,
                    EntityReferenceTraversalLimits::default(),
                    &mut admit,
                )
            }
            .unwrap();
            assert!(references.is_empty());
            assert_eq!(observed, expected, "payload_relative={payload_relative}");
        }
    }
}

#[test]
fn reference_free_raw_limits_and_scratch_refusal_precede_schema_errors() {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("missing"),
        "Missing",
        ComponentFieldPath::from_fields(["missing"]),
        ComponentValueKind::U64,
    );
    let mut duplicate = field.clone();
    duplicate.id = ComponentFieldId::new("duplicate");
    let schema = schema([field, duplicate]);
    let value = ComponentValue::map([("payload", scalar_payload())]);
    for (finite, expected) in [
        (
            limits(5, 14, 4),
            ComponentEntityReferenceRewriteError::NodeLimit { maximum: 5 },
        ),
        (
            limits(6, 13, 4),
            ComponentEntityReferenceRewriteError::ByteLimit { maximum: 13 },
        ),
        (
            limits(6, 14, 3),
            ComponentEntityReferenceRewriteError::DepthLimit { maximum: 3 },
        ),
        (
            limits(6, 14, 4),
            ComponentEntityReferenceRewriteError::DuplicateDeclaredPath {
                path: ComponentFieldPath::from_fields(["missing"]),
            },
        ),
    ] {
        assert_eq!(
            plan_declared_entity_references_with_admission::<(), _>(
                &schema,
                &value,
                finite,
                |_, _, _| Ok::<_, &'static str>(()),
            ),
            Err(EntityReferencePlanError::Reference(expected)),
        );
    }
    let mut visited = 0;
    let result = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        limits(6, 14, 4),
        |nodes, _, allocation| {
            visited += nodes;
            assert!(
                allocation > 0,
                "initial scratch must be admitted before visiting"
            );
            Err("initial scratch refused")
        },
    );
    assert_eq!(
        result,
        Err(EntityReferencePlanError::Admission(
            "initial scratch refused"
        )),
    );
    assert_eq!(visited, 0);
}

#[test]
fn reference_free_values_still_validate_shapes_capabilities_and_required_references() {
    let path = ComponentFieldPath::from_fields(["data"]);
    let numeric = schema([ComponentFieldSchema::required(
        ComponentFieldId::new("data"),
        "Data",
        path.clone(),
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
    )]);
    let value = ComponentValue::map([(
        "data",
        ComponentValue::List(vec![ComponentValue::String("invalid".to_owned())]),
    )]);
    assert_eq!(
        plan_declared_entity_references_with_admission::<(), _>(
            &numeric,
            &value,
            Default::default(),
            |_, _, _| Ok::<_, &'static str>(()),
        ),
        Err(EntityReferencePlanError::Reference(
            ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                path: ComponentFieldPath::new([
                    ComponentFieldPathSegment::field("data"),
                    ComponentFieldPathSegment::index(0),
                ]),
                actual: ComponentValueKind::String,
            },
        )),
    );

    let optional = ComponentFieldSchema::optional(
        ComponentFieldId::new("data"),
        "Data",
        path.clone(),
        ComponentValueShape::EntityRef,
    );
    let null = ComponentValue::map([("data", ComponentValue::Null)]);
    assert_eq!(
        plan_declared_entity_references::<()>(&schema([optional]), &null, Default::default()),
        Err(
            ComponentEntityReferenceRewriteError::MissingEntityRefCapability { path: path.clone() }
        ),
    );
    let required = schema([ComponentFieldSchema::required(
        ComponentFieldId::new("data"),
        "Data",
        path.clone(),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef)]);
    assert_eq!(
        plan_declared_entity_references_with_admission::<(), _>(
            &required,
            &null,
            Default::default(),
            |_, _, _| Ok::<_, &'static str>(()),
        ),
        Err(EntityReferencePlanError::Reference(
            ComponentEntityReferenceRewriteError::RequiredReferenceMissing { path },
        )),
    );
}

#[test]
fn late_unmarked_references_keep_both_full_walks_and_the_first_undeclared_path() {
    let schema = schema([]);
    let mut items = vec![ComponentValue::Null; 64];
    items.extend([
        local("first"),
        local("second"),
        ComponentValue::String("tail".to_owned()),
    ]);
    let value = ComponentValue::List(items);
    let mut observed = (0, 0);
    let result = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        limits(68, 15, 2),
        |nodes, bytes, _| {
            observed.0 += nodes;
            observed.1 += bytes;
            Ok::<_, &'static str>(())
        },
    );
    assert_eq!(
        result,
        Err(EntityReferencePlanError::Reference(
            ComponentEntityReferenceRewriteError::UndeclaredReference {
                path: ComponentFieldPath::new([ComponentFieldPathSegment::index(64)]),
            },
        )),
    );
    assert_eq!(observed, (2 * 68, 2 * 15));
    assert_eq!(
        plan_declared_entity_references::<()>(&schema, &value, limits(67, 15, 2)),
        Err(ComponentEntityReferenceRewriteError::NodeLimit { maximum: 67 }),
    );
}

#[test]
fn marked_reference_plans_keep_combined_limits_and_public_rewriting() {
    let schema = schema([ComponentFieldSchema::required(
        ComponentFieldId::new("root"),
        "Root",
        ComponentFieldPath::empty(),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef)]);
    let value = local("target");
    let mut observed = (0, 0);
    let plan = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        limits(2, 10, 1),
        |nodes, bytes, _| {
            observed.0 += nodes;
            observed.1 += bytes;
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();
    assert_eq!(observed, (5, 20));
    let mut visited = 0;
    plan.visit(&value, |path, reference| {
        assert!(path.is_empty());
        assert_eq!(reference, value.as_entity_reference().unwrap());
        visited += 1;
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(visited, 1);
    let rewritten =
        rewrite_declared_entity_references(&schema, &value, limits(2, 10, 1), |_, _| {
            Ok::<_, ()>(local("replacement").as_entity_reference().unwrap().clone())
        })
        .unwrap();
    assert_eq!(rewritten, local("replacement"));
    assert_eq!(value, local("target"));
    for finite in [limits(1, 10, 1), limits(2, 9, 1)] {
        assert!(plan_declared_entity_references::<()>(&schema, &value, finite).is_err());
    }
}

#[test]
fn asset_only_outputs_keep_combined_limits_in_all_collectors_and_entity_planning() {
    let asset = AssetRef::path("a.png").unwrap();
    let payload = crate::encode_asset_reference_value(&asset);
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueKind::AssetRef,
    )
    .with_capability(ComponentCapability::AssetRef);
    let schema = schema([field.clone()]);
    let value = ComponentValue::map([("image", payload.clone())]);
    for (entry, nodes, bytes, depth) in [(0, 5, 38, 3), (1, 4, 33, 3), (2, 4, 28, 2)] {
        let collect = |finite| match entry {
            0 => collect_declared_effective_asset_references_with_admission(
                &schema,
                schema.version(),
                &value,
                finite,
                |_, _, _| Ok::<_, ()>(()),
            ),
            1 => collect_declared_field_asset_references_with_admission(
                schema.id(),
                &field,
                &payload,
                finite,
                |_, _, _| Ok::<_, ()>(()),
            ),
            _ => collect_declared_value_asset_references_with_admission(
                schema.id(),
                &field,
                field.shape(),
                &payload,
                finite,
                |_, _, _| Ok::<_, ()>(()),
            ),
        };
        let references = collect(limits(nodes, bytes, depth)).unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].asset_ref(), &asset);
        for (finite, source) in [
            (
                limits(nodes - 1, bytes, depth),
                ComponentEntityReferenceRewriteError::NodeLimit { maximum: nodes - 1 },
            ),
            (
                limits(nodes, bytes - 1, depth),
                ComponentEntityReferenceRewriteError::ByteLimit { maximum: bytes - 1 },
            ),
        ] {
            assert_eq!(
                collect(finite),
                Err(AssetReferenceCollectionError::Reference(
                    DeclaredAssetReferenceError::Traversal {
                        component: schema.id().clone(),
                        source,
                    },
                )),
            );
        }
    }
    let plan = plan_declared_entity_references::<()>(&schema, &value, limits(4, 28, 3)).unwrap();
    assert_eq!(
        plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            limits(4, 28, 3),
            |_, _, _| Ok::<_, &'static str>(()),
        )
        .unwrap(),
        plan,
    );
    assert_eq!(
        plan_declared_entity_references::<()>(&schema, &value, limits(4, 27, 3)),
        Err(ComponentEntityReferenceRewriteError::ByteLimit { maximum: 27 }),
    );
    assert_eq!(
        plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            limits(4, 27, 3),
            |_, _, _| Ok::<_, &'static str>(()),
        ),
        Err(EntityReferencePlanError::Reference(
            ComponentEntityReferenceRewriteError::ByteLimit { maximum: 27 },
        )),
    );
}

#[test]
fn asset_backed_opaque_lists_debit_one_payload_pass_when_combined_counts_fit() {
    let asset = AssetRef::path("tiles.png").unwrap();
    let schema = schema([
        ComponentFieldSchema::required(
            ComponentFieldId::new("cells"),
            "Cells",
            ComponentFieldPath::from_fields(["cells"]),
            ComponentValueKind::List,
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("tileset"),
            "Tileset",
            ComponentFieldPath::from_fields(["tileset"]),
            ComponentValueKind::AssetRef,
        )
        .with_capability(ComponentCapability::AssetRef),
    ]);
    for collect_assets in [false, true] {
        let measure = |population| {
            let value = ComponentValue::map([
                (
                    "cells",
                    ComponentValue::List(vec![ComponentValue::U64(7); population]),
                ),
                ("tileset", crate::encode_asset_reference_value(&asset)),
            ]);
            let mut observed = (0, 0);
            let mut admit = |nodes, bytes, _| {
                observed.0 += nodes;
                observed.1 += bytes;
                Ok::<_, ()>(())
            };
            if collect_assets {
                let references = collect_declared_effective_asset_references_with_admission(
                    &schema,
                    schema.version(),
                    &value,
                    limits(8192, 1024, 8),
                    &mut admit,
                )
                .unwrap();
                assert_eq!(references.len(), 1);
                assert_eq!(references[0].asset_ref(), &asset);
            } else {
                let plan = plan_declared_entity_references_with_admission::<(), _>(
                    &schema,
                    &value,
                    limits(8192, 1024, 8),
                    &mut admit,
                )
                .unwrap();
                plan.visit(&value, |_, _| -> Result<(), ()> {
                    panic!("the asset-backed scalar payload contains no EntityRef")
                })
                .unwrap();
            }
            observed
        };
        let small = measure(1);
        let large = measure(4097);
        assert_eq!(large.0 - small.0, 4096);
        assert_eq!(large.1, small.1);
    }
}
