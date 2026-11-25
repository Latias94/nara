use std::collections::BTreeMap;

use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
use nara_ecs::identity::SceneEntityId;

use super::*;
use crate::{
    AssetReferenceDemand, ComponentSchemaVersion, ComponentTypeId,
    ComponentValueMaterializationLimits, DeclaredAssetReferenceError,
    asset_reference::{
        AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
        collect_effective_references_with_admission,
    },
    effective::freeze_component_defaults,
    encode_asset_reference_value,
};

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.JointReferences"),
        "Joint references",
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

fn entity(target: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(target).unwrap(),
    })
}

fn asset(path: &str) -> ComponentValue {
    encode_asset_reference_value(&AssetRef::path(path).unwrap())
}

fn root_field(
    name: &str,
    shape: impl Into<ComponentValueShape>,
    capability: ComponentCapability,
) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
    .with_capability(capability)
}

fn mixed_fixture() -> (ComponentSchema, ComponentValue) {
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("stable-link-owner"),
        "Links",
        ComponentFieldPath::from_fields(["settings", "links"]),
        ComponentValueShape::map(ComponentValueShape::list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new(
                    "image",
                    ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
                ),
                ComponentShapeMember::new(
                    "target",
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ),
            ]),
        )),
    )
    .with_capabilities([
        ComponentCapability::AssetRef,
        ComponentCapability::EntityRef,
    ]);
    let missing = ComponentFieldSchema::optional(
        ComponentFieldId::new("missing"),
        "Missing optional field",
        ComponentFieldPath::from_fields(["missing"]),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef);
    let product = AssetRef::Product(
        AssetProductRef::new(
            StableAssetId::parse_str("31415926-5358-4979-8323-846264338327").unwrap(),
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
        )
        .unwrap(),
    );
    let value = ComponentValue::map([
        (
            "settings",
            ComponentValue::map([(
                "links",
                ComponentValue::map([(
                    "group",
                    ComponentValue::List(vec![
                        ComponentValue::map([
                            ("image", encode_asset_reference_value(&product)),
                            ("target", entity("first")),
                        ]),
                        ComponentValue::map([
                            ("image", asset("images/second.png")),
                            ("target", ComponentValue::Null),
                        ]),
                    ]),
                )]),
            )]),
        ),
        ("unknown", ComponentValue::String("kept".to_owned())),
    ]);
    (schema([missing, field]), value)
}

#[test]
fn mixed_references_share_asset_work_and_move_entity_paths_into_the_existing_plan() {
    let (schema, value) = mixed_fixture();
    let original = value.clone();
    let mut asset_work = (0, 0, 0);
    let expected_assets = collect_declared_effective_asset_references_with_admission(
        &schema,
        schema.version(),
        &value,
        Default::default(),
        |nodes, bytes, allocation| {
            asset_work.0 += nodes;
            asset_work.1 += bytes;
            asset_work.2 += allocation;
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    let mut joint_work = (0, 0, 0);
    let (assets, pending) = collect_effective_references_with_admission(
        &schema,
        schema.version(),
        &value,
        Default::default(),
        |nodes, bytes, allocation| {
            joint_work.0 += nodes;
            joint_work.1 += bytes;
            joint_work.2 += allocation;
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert_eq!(assets, expected_assets);
    assert_eq!(assets.len(), 2);
    assert!(matches!(assets[0].asset_ref(), AssetRef::Product(_)));
    assert!(assets.iter().all(|reference| {
        reference.field_id().as_str() == "stable-link-owner"
            && reference.demand() == AssetReferenceDemand::Deferred
    }));
    assert_eq!(
        joint_work,
        (
            asset_work.0,
            asset_work.1,
            asset_work.2 + 4 * size_of::<PresentReferenceLeaf<'_, '_>>(),
        ),
    );
    assert_eq!(pending.leaves.len(), 1);
    let path_storage = pending.leaves[0].path.segments().as_ptr();
    let mut finish_work = (0, 0, 0);
    let plan = pending
        .finish_with_admission::<(), _>(|nodes, bytes, allocation| {
            finish_work.0 += nodes;
            finish_work.1 += bytes;
            finish_work.2 += allocation;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        finish_work,
        (
            0,
            0,
            4 * size_of::<PlannedEntityReference>() + "stable-link-owner".len(),
        ),
    );
    assert_eq!(plan.references[0].path.segments().as_ptr(), path_storage);
    assert_eq!(
        plan,
        plan_declared_entity_references::<()>(&schema, &value, Default::default()).unwrap(),
    );
    let expected_path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("settings"),
        ComponentFieldPathSegment::field("links"),
        ComponentFieldPathSegment::field("group"),
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("target"),
    ]);
    plan.visit(&value, |path, reference| {
        assert_eq!(path, &expected_path);
        assert_eq!(
            plan.root_field_id(path).unwrap().as_str(),
            "stable-link-owner"
        );
        assert_eq!(reference, entity("first").as_entity_reference().unwrap());
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(value, original);
}

#[test]
fn reference_free_payloads_have_one_shared_raw_pass_with_or_without_an_asset() {
    for asset_backed in [false, true] {
        let mut fields = vec![ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueKind::List,
        )];
        if asset_backed {
            fields.push(root_field(
                "image",
                ComponentValueKind::AssetRef,
                ComponentCapability::AssetRef,
            ));
        }
        let schema = schema(fields);
        let measure = |population| {
            let mut fields = BTreeMap::from([(
                "payload".to_owned(),
                ComponentValue::List(vec![ComponentValue::String("row".to_owned()); population]),
            )]);
            if asset_backed {
                fields.insert("image".to_owned(), asset("tiles.png"));
            }
            let value = ComponentValue::Map(fields);
            let mut work = (0, 0, 0);
            let mut admit = |nodes, bytes, allocation| {
                work.0 += nodes;
                work.1 += bytes;
                work.2 += allocation;
                Ok::<_, ()>(())
            };
            let (assets, pending) = collect_effective_references_with_admission(
                &schema,
                schema.version(),
                &value,
                limits(8192, 64 * 1024, 8),
                &mut admit,
            )
            .unwrap();
            assert_eq!(assets.len(), usize::from(asset_backed));
            assert!(pending.leaves.is_empty());
            let plan = pending.finish_with_admission::<(), _>(&mut admit).unwrap();
            assert!(plan.references.is_empty());
            work
        };
        let small = measure(1);
        let large = measure(4097);
        assert_eq!(large.0 - small.0, 4096);
        assert_eq!(large.1 - small.1, 3 * 4096);
        assert_eq!(large.2, small.2, "opaque payload nodes are only borrowed");
    }
}

#[test]
fn joint_collection_keeps_asset_modes_tight_combined_raw_result_and_depth_limits() {
    let schema = schema([
        root_field(
            "a",
            ComponentValueKind::AssetRef,
            ComponentCapability::AssetRef,
        ),
        root_field(
            "e",
            ComponentValueShape::EntityRef,
            ComponentCapability::EntityRef,
        ),
    ]);
    let value = ComponentValue::map([("a", asset("a.png")), ("e", entity("t"))]);
    let (assets, pending) = collect_effective_references_with_admission(
        &schema,
        schema.version(),
        &value,
        limits(7, 30, 3),
        |_, _, _| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(assets.len(), 1);
    let plan = pending
        .finish_with_admission::<(), _>(|_, _, _| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(plan.references.len(), 1);
    for (finite, expected) in [
        (
            limits(6, 30, 3),
            ComponentEntityReferenceRewriteError::NodeLimit { maximum: 6 },
        ),
        (
            limits(7, 29, 3),
            ComponentEntityReferenceRewriteError::ByteLimit { maximum: 29 },
        ),
        (
            limits(7, 30, 2),
            ComponentEntityReferenceRewriteError::DepthLimit { maximum: 2 },
        ),
    ] {
        let joint_error = collect_effective_references_with_admission(
            &schema,
            schema.version(),
            &value,
            finite,
            |_, _, _| Ok::<_, ()>(()),
        )
        .unwrap_err();
        assert_eq!(
            joint_error,
            AssetReferenceCollectionError::Reference(DeclaredAssetReferenceError::Traversal {
                component: schema.id().clone(),
                source: expected,
            }),
        );
        assert_eq!(
            joint_error,
            collect_declared_effective_asset_references_with_admission(
                &schema,
                schema.version(),
                &value,
                finite,
                |_, _, _| Ok::<_, ()>(()),
            )
            .unwrap_err(),
        );
    }
}

#[test]
fn hidden_late_entity_references_precede_malformed_asset_decoding_and_scan_the_tail() {
    for declared_opaque in [false, true] {
        let mut fields = vec![root_field(
            "a",
            ComponentValueKind::AssetRef,
            ComponentCapability::AssetRef,
        )];
        if declared_opaque {
            fields.push(ComponentFieldSchema::required(
                ComponentFieldId::new("payload"),
                "Payload",
                ComponentFieldPath::from_fields(["payload"]),
                ComponentValueKind::List,
            ));
        }
        let schema = schema(fields);
        let mut payload = vec![ComponentValue::Null; 64];
        payload.extend([
            entity("hidden"),
            ComponentValue::String("tail".repeat(1024)),
        ]);
        let value = ComponentValue::map([
            (
                "a",
                ComponentValue::map([
                    ("kind", ComponentValue::String("unsupported".to_owned())),
                    ("value", ComponentValue::Null),
                ]),
            ),
            ("payload", ComponentValue::List(payload)),
        ]);
        let mut joint_work = (0, 0);
        let error = collect_effective_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |nodes, bytes, _| {
                joint_work.0 += nodes;
                joint_work.1 += bytes;
                Ok::<_, ()>(())
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            AssetReferenceCollectionError::Reference(DeclaredAssetReferenceError::Traversal {
                component: schema.id().clone(),
                source: ComponentEntityReferenceRewriteError::UndeclaredReference {
                    path: ComponentFieldPath::new([
                        ComponentFieldPathSegment::field("payload"),
                        ComponentFieldPathSegment::index(64),
                    ]),
                },
            }),
        );
        let mut asset_work = (0, 0);
        assert_eq!(
            error,
            collect_declared_effective_asset_references_with_admission(
                &schema,
                schema.version(),
                &value,
                Default::default(),
                |nodes, bytes, _| {
                    asset_work.0 += nodes;
                    asset_work.1 += bytes;
                    Ok::<_, ()>(())
                },
            )
            .unwrap_err(),
        );
        assert_eq!(joint_work, asset_work);
        assert!(joint_work.1 >= 2 * 4 * 1024);
    }
}

#[test]
fn malformed_assets_are_reported_before_any_pending_entity_plan_can_be_finished() {
    let schema = schema([
        root_field(
            "a",
            ComponentValueKind::AssetRef,
            ComponentCapability::AssetRef,
        ),
        root_field(
            "e",
            ComponentValueShape::EntityRef,
            ComponentCapability::EntityRef,
        ),
    ]);
    let value = ComponentValue::map([
        (
            "a",
            ComponentValue::map([
                ("kind", ComponentValue::String("unsupported".to_owned())),
                ("value", ComponentValue::Null),
            ]),
        ),
        ("e", entity("target")),
    ]);
    let error = collect_effective_references_with_admission(
        &schema,
        schema.version(),
        &value,
        Default::default(),
        |_, _, _| Ok::<_, ()>(()),
    )
    .unwrap_err();
    assert_eq!(
        error,
        AssetReferenceCollectionError::Reference(DeclaredAssetReferenceError::InvalidKind {
            component: schema.id().clone(),
            field: ComponentFieldId::new("a"),
            path: ComponentFieldPath::from_fields(["a"]),
        }),
    );
    assert_eq!(
        error,
        collect_declared_effective_asset_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |_, _, _| Ok::<_, ()>(()),
        )
        .unwrap_err(),
    );
}

#[test]
fn pending_capacity_and_plan_output_refusals_preserve_the_exact_caller_error() {
    let mut entities = root_field(
        "e",
        ComponentValueShape::list(ComponentValueShape::EntityRef),
        ComponentCapability::EntityRef,
    );
    entities.id = ComponentFieldId::new("entity-owner");
    let schema = schema([
        root_field(
            "a",
            ComponentValueKind::AssetRef,
            ComponentCapability::AssetRef,
        ),
        entities,
    ]);
    let value = ComponentValue::map([
        ("a", asset("a.png")),
        ("e", ComponentValue::List(vec![entity("target"); 5])),
    ]);
    let mut asset_work = (0, 0, 0);
    collect_declared_effective_asset_references_with_admission(
        &schema,
        schema.version(),
        &value,
        Default::default(),
        |nodes, bytes, allocation| {
            asset_work.0 += nodes;
            asset_work.1 += bytes;
            asset_work.2 += allocation;
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    let mut joint_work = (0, 0, 0);
    let (assets, pending) = collect_effective_references_with_admission(
        &schema,
        schema.version(),
        &value,
        Default::default(),
        |nodes, bytes, allocation| {
            joint_work.0 += nodes;
            joint_work.1 += bytes;
            joint_work.2 += allocation;
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert_eq!(assets[0].asset_ref(), &AssetRef::path("a.png").unwrap());
    assert_eq!(
        joint_work,
        (
            asset_work.0,
            asset_work.1,
            asset_work.2 + 12 * size_of::<PresentReferenceLeaf<'_, '_>>(),
        ),
    );
    assert_eq!(pending.leaves.len(), 5);
    drop(pending);

    let refusal = Box::new(("owner refusal", 17));
    for denied_call in [1, 2, 6] {
        let (assets, pending) = collect_effective_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |_, _, _| Ok::<_, ()>(()),
        )
        .unwrap();
        assert_eq!(assets[0].asset_ref(), &AssetRef::path("a.png").unwrap());
        let mut finish_calls = 0;
        let error = pending
            .finish_with_admission::<(), _>(|nodes, bytes, allocation| {
                assert_eq!((nodes, bytes), (0, 0));
                finish_calls += 1;
                if finish_calls == 1 {
                    assert_eq!(allocation, 5 * size_of::<PlannedEntityReference>());
                } else {
                    assert_eq!(allocation, "entity-owner".len());
                }
                if finish_calls == denied_call {
                    Err(&*refusal)
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        let EntityReferencePlanError::Admission(actual) = error else {
            panic!("expected exact caller refusal")
        };
        assert!(std::ptr::eq(actual, &*refusal));
        assert_eq!(finish_calls, denied_call);
    }

    for capacity in [4, 8] {
        let expected_leaves = if capacity == 4 { 1 } else { 5 };
        let mut recorded_leaves = 0;
        let error = collect_effective_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |nodes, bytes, allocation| {
                if (nodes, bytes, allocation) == (1, "entity-owner".len() + 1 + size_of::<u32>(), 0)
                {
                    recorded_leaves += 1;
                }
                if recorded_leaves == expected_leaves
                    && (nodes, bytes, allocation)
                        == (0, 0, capacity * size_of::<PresentReferenceLeaf<'_, '_>>())
                {
                    Err(&*refusal)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        let AssetReferenceCollectionError::Admission(actual) = error else {
            panic!("expected exact pending-capacity refusal")
        };
        assert!(std::ptr::eq(actual, &*refusal));
        assert_eq!(recorded_leaves, expected_leaves);
    }
}

#[test]
fn frozen_overlay_references_borrow_selected_defaults_and_keep_explicit_nulls() {
    let mut image = ComponentFieldSchema::required(
        ComponentFieldId::new("image-owner"),
        "Image",
        ComponentFieldPath::from_fields(["settings", "image"]),
        ComponentValueKind::AssetRef,
    )
    .with_capability(ComponentCapability::AssetRef);
    image.default_value = Some(asset("authored.png"));
    let mut target = ComponentFieldSchema::required(
        ComponentFieldId::new("target-owner"),
        "Target",
        ComponentFieldPath::from_fields(["settings", "target"]),
        ComponentValueShape::EntityRef,
    )
    .with_capability(ComponentCapability::EntityRef);
    target.default_value = Some(entity("authored"));
    let maybe = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("maybe"),
        "Maybe",
        ComponentFieldPath::from_fields(["maybe"]),
        ComponentValueShape::EntityRef,
        entity("not-selected"),
    )
    .with_capability(ComponentCapability::EntityRef);
    let schema = schema([target, image, maybe]);
    let source = ComponentValue::map([
        (
            "settings",
            ComponentValue::map([(
                "payload",
                ComponentValue::List(vec![ComponentValue::U64(7); 128]),
            )]),
        ),
        ("maybe", ComponentValue::Null),
    ]);
    let original = source.clone();
    let overrides = BTreeMap::from([
        (ComponentFieldId::new("image-owner"), asset("frozen.png")),
        (ComponentFieldId::new("target-owner"), entity("frozen")),
    ]);
    let frozen = freeze_component_defaults(
        &schema,
        &source,
        &overrides,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let read = frozen.read(&source);
    let (assets, pending) = collect_effective_references_with_admission(
        &schema,
        schema.version(),
        read,
        Default::default(),
        |_, _, _| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(
        assets[0].asset_ref(),
        &AssetRef::path("frozen.png").unwrap()
    );
    assert_eq!(assets[0].field_id().as_str(), "image-owner");
    assert_eq!(pending.leaves.len(), 1);
    assert!(std::ptr::eq(
        pending.leaves[0].value.as_entity_reference().unwrap(),
        read.get_path(&ComponentFieldPath::from_fields(["settings", "target"]))
            .unwrap()
            .as_entity_reference()
            .unwrap(),
    ));
    let plan = pending
        .finish_with_admission::<(), _>(|_, _, _| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(
        plan,
        plan_declared_entity_references::<()>(&schema, read, Default::default()).unwrap(),
    );
    plan.visit(read, |path, reference| {
        assert_eq!(
            path,
            &ComponentFieldPath::from_fields(["settings", "target"])
        );
        assert_eq!(plan.root_field_id(path).unwrap().as_str(), "target-owner");
        assert_eq!(reference, entity("frozen").as_entity_reference().unwrap());
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(read.get("maybe").unwrap().kind(), ComponentValueKind::Null);
    assert_eq!(source, original);
}
