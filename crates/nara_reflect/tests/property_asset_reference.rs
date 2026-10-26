use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::SceneEntityId;
use nara_reflect::{
    ComponentCapability, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentMemberPath, ComponentMemberPathSegment,
    ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentPropertySchemaClaim, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, DeclaredAssetReferenceError, EntityReference,
    EntityReferenceTraversalLimits, collect_declared_value_asset_references_with_limits,
    resolve_component_property_in_root, resolve_component_property_schema_claim,
};

fn id(value: &str) -> ComponentFieldId {
    ComponentFieldId::new(value)
}

fn member(name: &str, stable: &str, shape: impl Into<ComponentValueShape>) -> ComponentShapeMember {
    ComponentShapeMember::new(name, shape).with_id(id(stable))
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.PropertyAssets"),
        "Property assets",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        id("root"),
        "Original metadata",
        ComponentFieldPath::from_fields(["settings", "wire_root"]),
        shape,
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef])])
}

fn path(steps: impl IntoIterator<Item = ComponentPropertyStep>) -> ComponentPropertyPath {
    ComponentPropertyPath::new(id("root")).with_steps(steps)
}

fn budget(entries: usize, bytes: usize) -> ComponentPropertyReadBudget {
    ComponentPropertyReadBudget::new(
        ItemLimit::new(entries).unwrap(),
        ByteLimit::new(bytes).unwrap(),
    )
}

fn claim<'a>(
    schema: &'a ComponentSchema,
    path: &ComponentPropertyPath,
) -> Result<ComponentPropertySchemaClaim<'a>, ComponentPropertyError> {
    resolve_component_property_schema_claim(
        schema,
        schema.version(),
        path,
        &mut budget(1024, 16 * 1024 * 1024),
    )
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn asset(value: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String(value.into())),
    ])
}

#[test]
fn root_and_renamed_nested_claims_keep_the_original_field_and_selected_payload_paths() {
    let selected_path = path([ComponentPropertyStep::Member(id("image"))]);
    for wire in ["old_image", "renamed_image"] {
        let schema = schema(ComponentValueShape::record_members([member(
            wire,
            "image",
            ComponentValueShape::AssetRef,
        )]));
        let original = schema.clone();
        let root = claim(&schema, &path([])).unwrap();
        let selected = claim(&schema, &selected_path).unwrap();
        assert!(std::ptr::eq(root.field, &schema.fields()[0]));
        assert!(std::ptr::eq(selected.field, root.field));
        assert!(std::ptr::eq(root.shape, root.field.shape()));
        assert_eq!(selected.shape, &ComponentValueShape::AssetRef);
        assert_eq!(selected.field.aliases(), ["Original metadata"]);
        assert!(selected.field.has_capability(ComponentCapability::Scene));

        let payload = asset("a.png");
        let scalar = collect_declared_value_asset_references_with_limits(
            schema.id(),
            selected.field,
            selected.shape,
            &payload,
            Default::default(),
        )
        .unwrap();
        assert_eq!(scalar.len(), 1);
        assert_eq!(scalar[0].field_id(), root.field.id());
        assert_eq!(scalar[0].path(), &ComponentFieldPath::empty());
        assert_eq!(scalar[0].asset_ref(), &AssetRef::path("a.png").unwrap());

        let record = collect_declared_value_asset_references_with_limits(
            schema.id(),
            root.field,
            root.shape,
            &ComponentValue::map([(wire, payload)]),
            Default::default(),
        )
        .unwrap();
        assert_eq!(record.len(), 1);
        assert_eq!(record[0].field_id(), root.field.id());
        assert_eq!(record[0].path(), &ComponentFieldPath::from_fields([wire]));
        assert_eq!(record[0].asset_ref(), scalar[0].asset_ref());
        assert_eq!(schema, original);
    }
}

#[test]
fn keyed_claims_check_the_declared_identity_policy_without_claiming_an_item_exists() {
    let row = ComponentValueShape::record_members([
        member("wire_id", "id", ComponentValueKind::String),
        member("wire_slug", "slug", ComponentValueKind::String),
        member("wire_image", "image", ComponentValueShape::AssetRef),
    ]);
    let keyed_schema = schema(ComponentValueShape::keyed_list(row.clone(), id("id")));
    let item = |by, key| ComponentPropertyStep::ListItem { by: id(by), key };
    let selected_path = path([
        item("id", ComponentPropertyKey::String("absent-item".into())),
        ComponentPropertyStep::Member(id("image")),
    ]);
    let selected = claim(&keyed_schema, &selected_path).unwrap();
    assert_eq!(selected.shape, &ComponentValueShape::AssetRef);
    let assets = collect_declared_value_asset_references_with_limits(
        keyed_schema.id(),
        selected.field,
        selected.shape,
        &asset("replacement.png"),
        Default::default(),
    )
    .unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].path(), &ComponentFieldPath::empty());
    assert!(matches!(
        resolve_component_property_in_root(
            selected.field,
            &ComponentValue::List(Vec::new()),
            &selected_path,
            &mut budget(1024, 4096),
        ),
        Err(ComponentPropertyError::ListItemNotFound { .. })
    ));
    assert_eq!(
        claim(
            &keyed_schema,
            &path([item("slug", ComponentPropertyKey::String("same".into()))]),
        )
        .unwrap_err(),
        ComponentPropertyError::ListIdentityMismatch {
            step: 0,
            expected: id("id"),
            actual: id("slug"),
        },
    );
    assert_eq!(
        claim(
            &keyed_schema,
            &path([item("id", ComponentPropertyKey::U64(1))])
        )
        .unwrap_err(),
        ComponentPropertyError::KeyTypeMismatch {
            step: 0,
            expected: ComponentValueKind::String,
            actual: ComponentValueKind::U64,
        },
    );
    let unkeyed = schema(ComponentValueShape::list(row));
    assert_eq!(
        claim(&unkeyed, &selected_path).unwrap_err(),
        ComponentPropertyError::UnkeyedList { step: 0 },
    );
}

#[test]
fn optional_map_claims_use_literal_keys_without_observing_missing_or_null_values() {
    let schema = schema(ComponentValueShape::optional(ComponentValueShape::map(
        ComponentValueShape::optional(ComponentValueShape::record_members([member(
            "wire_image",
            "image",
            ComponentValueShape::AssetRef,
        )])),
    )));
    let selected_path = path([
        ComponentPropertyStep::MapEntry("a.b".into()),
        ComponentPropertyStep::Member(id("image")),
    ]);
    let selected = claim(&schema, &selected_path).unwrap();
    assert_eq!(selected.shape, &ComponentValueShape::AssetRef);
    for (value, expected) in [
        (
            ComponentValue::Null,
            ComponentPropertyError::OptionalAbsent { step: 0 },
        ),
        (
            ComponentValue::Map(Default::default()),
            ComponentPropertyError::MapEntryNotFound {
                step: 0,
                key: "a.b".into(),
            },
        ),
        (
            ComponentValue::map([("a.b", ComponentValue::Null)]),
            ComponentPropertyError::OptionalAbsent { step: 1 },
        ),
    ] {
        assert_eq!(
            resolve_component_property_in_root(
                selected.field,
                &value,
                &selected_path,
                &mut budget(1024, 4096),
            )
            .unwrap_err(),
            expected,
        );
    }
    let entry = claim(
        &schema,
        &path([ComponentPropertyStep::MapEntry("a.b".into())]),
    )
    .unwrap();
    assert!(matches!(entry.shape, ComponentValueShape::Optional { .. }));
    let root = claim(&schema, &path([])).unwrap();
    let references = collect_declared_value_asset_references_with_limits(
        schema.id(),
        root.field,
        root.shape,
        &ComponentValue::map([
            ("a.b", ComponentValue::map([("wire_image", asset("a.png"))])),
            ("absent", ComponentValue::Null),
        ]),
        Default::default(),
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::from_fields(["a.b", "wire_image"]),
    );
}

#[test]
fn unknown_retired_anonymous_and_wire_name_selectors_never_acquire_a_schema_claim() {
    let mut schema = schema(ComponentValueShape::record_members([
        member("wire_image", "image", ComponentValueShape::AssetRef),
        ComponentShapeMember::new("anonymous", ComponentValueShape::AssetRef),
    ]))
    .with_field_tombstones([id("retired-root")]);
    schema.fields[0]
        .member_tombstones
        .push(ComponentMemberPath::new([
            ComponentMemberPathSegment::Member(id("retired-image")),
        ]));
    for (selector, expected) in [
        (
            ComponentPropertyPath::new(id("missing")),
            ComponentPropertyError::UnknownRoot(id("missing")),
        ),
        (
            ComponentPropertyPath::new(id("retired-root")),
            ComponentPropertyError::RetiredField(id("retired-root")),
        ),
        (
            ComponentPropertyPath::new(id("")),
            ComponentPropertyError::InvalidFieldId,
        ),
        (
            path([ComponentPropertyStep::Member(id("retired-image"))]),
            ComponentPropertyError::RetiredMember(ComponentMemberPath::new([
                ComponentMemberPathSegment::Member(id("retired-image")),
            ])),
        ),
        (
            path([ComponentPropertyStep::Member(id("wire_image"))]),
            ComponentPropertyError::MemberNotFound {
                step: 0,
                id: id("wire_image"),
            },
        ),
        (
            path([ComponentPropertyStep::Member(id("anonymous"))]),
            ComponentPropertyError::MemberNotFound {
                step: 0,
                id: id("anonymous"),
            },
        ),
        (
            path([ComponentPropertyStep::MapEntry("wire_image".into())]),
            ComponentPropertyError::ExpectedMap { step: 0 },
        ),
        (
            path([ComponentPropertyStep::ListItem {
                by: id("id"),
                key: ComponentPropertyKey::U64(1),
            }]),
            ComponentPropertyError::ExpectedList { step: 0 },
        ),
        (
            path([
                ComponentPropertyStep::Member(id("image")),
                ComponentPropertyStep::Member(id("child")),
            ]),
            ComponentPropertyError::ExpectedRecord { step: 1 },
        ),
    ] {
        assert_eq!(claim(&schema, &selector).unwrap_err(), expected);
    }
    assert_eq!(
        resolve_component_property_schema_claim(
            &schema,
            ComponentSchemaVersion(2),
            &path([]),
            &mut budget(1024, 4096),
        )
        .unwrap_err(),
        ComponentPropertyError::VersionMismatch {
            expected: ComponentSchemaVersion::ONE,
            actual: ComponentSchemaVersion(2),
        },
    );
}

#[test]
fn nested_tombstones_follow_keyed_item_and_map_declarations() {
    for (shape, first, segment) in [
        (
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([member(
                    "wire_id",
                    "id",
                    ComponentValueKind::String,
                )]),
                id("id"),
            ),
            ComponentPropertyStep::ListItem {
                by: id("id"),
                key: ComponentPropertyKey::String("absent".into()),
            },
            ComponentMemberPathSegment::ListItem,
        ),
        (
            ComponentValueShape::map(ComponentValueShape::record_members([])),
            ComponentPropertyStep::MapEntry("absent".into()),
            ComponentMemberPathSegment::MapEntry,
        ),
    ] {
        let mut schema = schema(shape);
        let retired =
            ComponentMemberPath::new([segment, ComponentMemberPathSegment::Member(id("retired"))]);
        schema.fields[0].member_tombstones.push(retired.clone());
        assert_eq!(
            claim(
                &schema,
                &path([first, ComponentPropertyStep::Member(id("retired"))])
            )
            .unwrap_err(),
            ComponentPropertyError::RetiredMember(retired),
        );
    }
}

#[test]
fn schema_claim_scan_limits_include_selector_roots_tombstones_and_optional_map_work() {
    let mut schema = schema(ComponentValueShape::optional(ComponentValueShape::map(
        ComponentValueShape::optional(ComponentValueShape::record_members([member(
            "wire_image",
            "image",
            ComponentValueShape::AssetRef,
        )])),
    )))
    .with_field_tombstones([id("gone")]);
    schema.fields[0]
        .member_tombstones
        .push(ComponentMemberPath::new([
            ComponentMemberPathSegment::MapEntry,
            ComponentMemberPathSegment::Member(id("gone")),
        ]));
    let selector = path([
        ComponentPropertyStep::MapEntry("a.b".into()),
        ComponentPropertyStep::Member(id("image")),
    ]);
    // Selector 12, retired root 4, root 4, map key 3, retired member 4, member name/ID 15.
    // Nine entries include two optional unwraps and two member-tombstone segments.
    for (entries, bytes, accepted) in [(9, 42, true), (8, 42, false), (9, 41, false)] {
        let mut budget = budget(entries, bytes);
        let result = resolve_component_property_schema_claim(
            &schema,
            schema.version(),
            &selector,
            &mut budget,
        );
        if accepted {
            assert_eq!(result.unwrap().shape, &ComponentValueShape::AssetRef);
            assert_eq!(budget.scanned_entries(), 9);
            assert_eq!(budget.scanned_logical_bytes(), 42);
            assert_eq!(
                resolve_component_property_schema_claim(
                    &schema,
                    schema.version(),
                    &selector,
                    &mut budget
                )
                .unwrap_err(),
                ComponentPropertyError::ReadBudgetExceeded,
            );
        } else {
            assert_eq!(
                result.unwrap_err(),
                ComponentPropertyError::ReadBudgetExceeded
            );
        }
    }
}

#[test]
fn schema_claim_path_and_selected_declaration_depth_have_inclusive_limits() {
    let mut shape = ComponentValueShape::AssetRef;
    for _ in 0..ComponentPropertyPath::MAX_STEPS {
        shape = ComponentValueShape::map(shape);
    }
    let mut nested_schema = schema(shape);
    nested_schema.fields[0].path = ComponentFieldPath::empty();
    let exact = path(std::iter::repeat_n(
        ComponentPropertyStep::MapEntry("x".into()),
        ComponentPropertyPath::MAX_STEPS,
    ));
    assert_eq!(
        claim(&nested_schema, &exact).unwrap().shape,
        &ComponentValueShape::AssetRef,
    );
    let mut too_deep = exact.clone();
    too_deep
        .steps
        .push(ComponentPropertyStep::MapEntry("x".into()));
    assert_eq!(
        claim(&nested_schema, &too_deep).unwrap_err(),
        ComponentPropertyError::PathDepthLimit,
    );

    // Optional wrappers count toward declaration depth without consuming selector steps.
    nested_schema.fields[0].shape =
        ComponentValueShape::optional(nested_schema.fields[0].shape.clone());
    assert_eq!(
        claim(&nested_schema, &exact).unwrap_err(),
        ComponentPropertyError::PathDepthLimit,
    );

    let map_schema = schema(ComponentValueShape::map(ComponentValueShape::AssetRef));
    let mut exact_bytes = path([ComponentPropertyStep::MapEntry(
        "x".repeat(4 * 1024 * 1024 - 4),
    )]);
    assert_eq!(exact_bytes.logical_bytes().unwrap(), 4 * 1024 * 1024);
    assert_eq!(
        claim(&map_schema, &exact_bytes).unwrap().shape,
        &ComponentValueShape::AssetRef
    );
    let ComponentPropertyStep::MapEntry(key) = &mut exact_bytes.steps[0] else {
        unreachable!()
    };
    key.push('x');
    assert_eq!(
        claim(&map_schema, &exact_bytes).unwrap_err(),
        ComponentPropertyError::PathByteLimit
    );
}

#[test]
fn scalar_asset_payload_limits_exclude_component_storage_and_include_returned_work() {
    let schema = schema(ComponentValueShape::record_members([member(
        "wire_image",
        "image",
        ComponentValueShape::AssetRef,
    )]));
    let selected = claim(&schema, &path([ComponentPropertyStep::Member(id("image"))])).unwrap();
    let payload = asset("a.png");
    // Three input nodes plus one result; 18 input bytes plus 5 asset and 4 field-ID bytes.
    // The wire asset's two strings reach depth two, independent of component storage depth.
    let references = collect_declared_value_asset_references_with_limits(
        schema.id(),
        selected.field,
        selected.shape,
        &payload,
        limits(4, 27, 2),
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].field_id(), selected.field.id());
    assert_eq!(references[0].path(), &ComponentFieldPath::empty());
    for (limits, source) in [
        (
            limits(3, 27, 2),
            ComponentEntityReferenceRewriteError::NodeLimit { maximum: 3 },
        ),
        (
            limits(4, 26, 2),
            ComponentEntityReferenceRewriteError::ByteLimit { maximum: 26 },
        ),
        (
            limits(4, 27, 1),
            ComponentEntityReferenceRewriteError::DepthLimit { maximum: 1 },
        ),
    ] {
        assert_eq!(
            collect_declared_value_asset_references_with_limits(
                schema.id(),
                selected.field,
                selected.shape,
                &payload,
                limits,
            ),
            Err(DeclaredAssetReferenceError::Traversal {
                component: schema.id().clone(),
                source
            }),
        );
    }
}

#[test]
fn payload_assets_keep_capabilities_null_and_parse_semantics_without_guessing_wire_dependencies() {
    let mut schema = schema(ComponentValueShape::AssetRef);
    schema.fields[0].default_value = Some(asset("default.png"));
    let field = &schema.fields()[0];
    assert_eq!(
        collect_declared_value_asset_references_with_limits(
            schema.id(),
            field,
            field.shape(),
            &ComponentValue::Null,
            Default::default(),
        ),
        Err(DeclaredAssetReferenceError::RequiredFieldIsNull {
            component: schema.id().clone(),
            field: field.id().clone(),
        }),
    );
    assert!(
        collect_declared_value_asset_references_with_limits(
            schema.id(),
            field,
            &ComponentValueShape::optional(ComponentValueShape::AssetRef),
            &ComponentValue::Null,
            Default::default(),
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        collect_declared_value_asset_references_with_limits(
            schema.id(),
            field,
            &ComponentValueShape::opaque(ComponentValueKind::Map),
            &asset("looks-like-a-dependency.png"),
            Default::default(),
        )
        .unwrap()
        .is_empty()
    );
    assert!(matches!(
        collect_declared_value_asset_references_with_limits(
            schema.id(), field, field.shape(), &ComponentValue::String("a.png".into()), Default::default(),
        ),
        Err(DeclaredAssetReferenceError::InvalidValueShape { path, .. }) if path == ComponentFieldPath::empty()
    ));
    assert!(matches!(
        collect_declared_value_asset_references_with_limits(
            schema.id(), field, field.shape(), &asset("../invalid.png"), Default::default(),
        ),
        Err(DeclaredAssetReferenceError::InvalidPath { path, .. }) if path == ComponentFieldPath::empty()
    ));
    let mut unmarked = field.clone();
    unmarked.capabilities.remove(&ComponentCapability::AssetRef);
    assert!(matches!(
        collect_declared_value_asset_references_with_limits(
            schema.id(), &unmarked, field.shape(), &asset("a.png"), Default::default(),
        ),
        Err(DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::MissingAssetRefCapability { path }, ..
        }) if path == *field.path()
    ));
}

#[test]
fn mixed_payloads_inventory_assets_without_guessing_entity_targets_or_defaults() {
    let shape = ComponentValueShape::record_members([
        member("wire_image", "image", ComponentValueShape::AssetRef),
        member("wire_target", "target", ComponentValueShape::EntityRef),
    ]);
    let mut schema = schema(shape);
    let payload = ComponentValue::map([
        ("wire_image", asset("not-checked-on-disk.png")),
        (
            "wire_target",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: SceneEntityId::new("not-in-any-source-namespace").unwrap(),
            }),
        ),
    ]);
    let original = payload.clone();
    let field = &schema.fields()[0];
    assert!(matches!(
        collect_declared_value_asset_references_with_limits(
            schema.id(),
            field,
            field.shape(),
            &payload,
            Default::default(),
        ),
        Err(DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::MissingEntityRefCapability { .. },
            ..
        })
    ));
    schema.fields[0]
        .capabilities
        .insert(ComponentCapability::EntityRef);
    let field = &schema.fields()[0];
    let references = collect_declared_value_asset_references_with_limits(
        schema.id(),
        field,
        field.shape(),
        &payload,
        Default::default(),
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::from_fields(["wire_image"])
    );
    assert_eq!(payload, original);
    assert!(matches!(
        collect_declared_value_asset_references_with_limits(
            schema.id(), field, &ComponentValueShape::opaque(ComponentValueKind::Map),
            &payload, Default::default(),
        ),
        Err(DeclaredAssetReferenceError::Traversal {
            source: ComponentEntityReferenceRewriteError::UndeclaredReference { path }, ..
        }) if path == ComponentFieldPath::from_fields(["wire_target"])
    ));
}

mod effective_collection_admission {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    use super::*;
    use nara_reflect::{
        __private::{
            AssetReferenceCollectionError,
            collect_declared_effective_asset_references_with_admission,
        },
        DeclaredAssetReference, collect_declared_asset_references_from_schema_with_limits,
    };

    struct CountingAllocator;

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    thread_local! {
        static ALLOCATION_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    fn record_allocation() {
        let _ = ALLOCATION_CALLS.try_with(|calls| calls.set(calls.get().saturating_add(1)));
    }

    // SAFETY: Every allocation operation is forwarded unchanged to System. The observation is
    // thread-local, nonallocating and independent of parallel tests in this executable.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: The caller-provided valid layout is forwarded to System.
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                record_allocation();
            }
            pointer
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: The caller-provided valid layout is forwarded to System.
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                record_allocation();
            }
            pointer
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: The caller's live allocation and requested size are forwarded to System.
            let pointer = unsafe { System.realloc(pointer, layout, new_size) };
            if !pointer.is_null() {
                record_allocation();
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: The caller's live allocation and its original layout are unchanged.
            unsafe { System.dealloc(pointer, layout) };
        }
    }

    fn allocation_calls() -> usize {
        ALLOCATION_CALLS.with(Cell::get)
    }

    fn document(with_assets: bool, fields: usize) -> (ComponentSchema, ComponentValue) {
        let schema = ComponentSchema::new(
            ComponentTypeId::new("test.EffectiveAssets"),
            "Effective assets",
            ComponentSchemaVersion::ONE,
        )
        .with_fields((0..fields).map(|index| {
            let name = format!("field_{index}");
            let shape = if with_assets {
                ComponentValueShape::AssetRef
            } else {
                ComponentValueShape::scalar(ComponentValueKind::U64)
            };
            ComponentFieldSchema::required(
                id(&name),
                &name,
                ComponentFieldPath::from_fields([&name]),
                shape,
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef])
        }));
        let value = ComponentValue::map((0..fields).map(|index| {
            let value = if with_assets {
                asset(&format!("images/{index}.png"))
            } else {
                ComponentValue::U64(index as u64)
            };
            (format!("field_{index}"), value)
        }));
        (schema, value)
    }

    fn observed_cost(
        schema: &ComponentSchema,
        value: &ComponentValue,
        limits: EntityReferenceTraversalLimits,
    ) -> (Vec<DeclaredAssetReference>, [usize; 3]) {
        let mut cost = [0; 3];
        let references = collect_declared_effective_asset_references_with_admission(
            schema,
            schema.version(),
            value,
            limits,
            |nodes, logical, allocation| {
                for (total, delta) in cost.iter_mut().zip([nodes, logical, allocation]) {
                    *total += delta;
                }
                Ok::<(), ()>(())
            },
        )
        .unwrap();
        (references, cost)
    }

    #[test]
    fn aggregate_caps_cover_many_asset_free_and_asset_bearing_fields() {
        for with_assets in [false, true] {
            let (schema, value) = document(with_assets, 48);
            let original = value.clone();
            let (expected, cost) = observed_cost(&schema, &value, Default::default());
            assert_eq!(expected.len(), if with_assets { 48 } else { 0 });
            assert!(cost.iter().all(|cost| *cost > 0));
            for dimension in 0..3 {
                let maximum = cost[dimension] * 2;
                let mut used = 0usize;
                for record in 0..3 {
                    let result = collect_declared_effective_asset_references_with_admission(
                        &schema,
                        schema.version(),
                        &value,
                        Default::default(),
                        |nodes, logical, allocation| {
                            let requested = used + [nodes, logical, allocation][dimension];
                            if requested > maximum {
                                return Err((dimension, requested, maximum));
                            }
                            used = requested;
                            Ok(())
                        },
                    );
                    if record < 2 {
                        assert_eq!(result.unwrap(), expected);
                    } else {
                        assert!(matches!(
                            result,
                            Err(AssetReferenceCollectionError::Admission((actual, observed, cap)))
                                if actual == dimension && observed > cap && cap == maximum
                        ));
                    }
                }
                assert!(used <= maximum);
            }
            assert_eq!(value, original);
        }
    }

    #[test]
    fn small_effective_values_pay_actual_work_independently_of_configured_limits() {
        let (schema, value) = document(true, 1);
        let (references, cost) = observed_cost(&schema, &value, Default::default());
        assert_eq!(references.len(), 1);
        assert!(cost[0] < 64);
        assert!(cost[1] < 1024);
        assert!(cost[2] < 16 * 1024);
        let (larger_limits_references, larger_limits_cost) =
            observed_cost(&schema, &value, limits(1_000_000, 128 * 1024 * 1024, 128));
        assert_eq!(larger_limits_references, references);
        assert_eq!(larger_limits_cost, cost);
        let ordinary = collect_declared_asset_references_from_schema_with_limits(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(references, ordinary);
    }

    #[test]
    fn immediate_denial_preserves_typed_owner_failure_without_any_allocation() {
        // No Clone, Display, Error or 'static bound is required of the owner's failure.
        struct OwnerFailure<'a>(&'a u8);
        let marker = 7;
        let (schema, value) = document(true, 1);
        let original = value.clone();
        let before = allocation_calls();
        let result = collect_declared_effective_asset_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |_, _, _| Err(OwnerFailure(&marker)),
        );
        let after = allocation_calls();
        assert_eq!(after, before);
        assert!(matches!(
            result,
            Err(AssetReferenceCollectionError::Admission(OwnerFailure(actual)))
                if std::ptr::eq(actual, &marker)
        ));
        assert_eq!(value, original);
    }

    #[test]
    fn first_denied_output_does_not_clone_its_path_field_or_parse_its_asset() {
        let (schema, _) = document(true, 1);
        let value = ComponentValue::map([("field_0", asset("../invalid.png"))]);
        let original = value.clone();
        let mut calls_at_denial = None;
        let result = collect_declared_effective_asset_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |nodes, _, allocation| {
                if nodes == 1 && allocation >= size_of::<DeclaredAssetReference>() {
                    calls_at_denial = Some(allocation_calls());
                    Err("owner-output-cap")
                } else {
                    Ok(())
                }
            },
        );
        let after = allocation_calls();
        assert!(matches!(
            result,
            Err(AssetReferenceCollectionError::Admission("owner-output-cap"))
        ));
        assert_eq!(Some(after), calls_at_denial);
        assert_eq!(value, original);
    }

    #[test]
    fn dynamic_path_name_is_admitted_before_copying_it() {
        let schema = schema(ComponentValueShape::map(ComponentValueShape::AssetRef));
        let name = "m".repeat(1003);
        let value = ComponentValue::map([(
            "settings",
            ComponentValue::map([(
                "wire_root",
                ComponentValue::map([(name.as_str(), asset("a.png"))]),
            )]),
        )]);
        let original = value.clone();
        let mut calls_at_denial = None;
        let result = collect_declared_effective_asset_references_with_admission(
            &schema,
            schema.version(),
            &value,
            Default::default(),
            |nodes, logical, allocation| {
                if (nodes, logical, allocation) == (0, 0, name.len()) {
                    calls_at_denial = Some(allocation_calls());
                    Err("owner-path-cap")
                } else {
                    Ok(())
                }
            },
        );
        let after = allocation_calls();
        assert!(matches!(
            result,
            Err(AssetReferenceCollectionError::Admission("owner-path-cap"))
        ));
        assert_eq!(Some(after), calls_at_denial);
        assert_eq!(value, original);
    }

    #[test]
    fn effective_borrow_does_not_read_or_materialize_missing_defaults() {
        let mut schema = document(false, 0).0;
        schema.fields.push(
            ComponentFieldSchema::optional_with_default(
                id("missing"),
                "Missing",
                ComponentFieldPath::from_fields(["missing"]),
                ComponentValueShape::AssetRef,
                asset(&"d".repeat(1024 * 1024)),
            )
            .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
        );
        let value = ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>());
        let (references, cost) = observed_cost(&schema, &value, Default::default());
        assert!(references.is_empty());
        assert!(cost[0] < 32 && cost[1] < 128 && cost[2] < 16 * 1024);
        assert_eq!(
            value,
            ComponentValue::map(std::iter::empty::<(&str, ComponentValue)>())
        );
    }

    #[test]
    fn semantic_failures_remain_concrete_and_leave_the_original_value_unchanged() {
        let (schema, _) = document(true, 1);
        for payload in [
            ComponentValue::Null,
            ComponentValue::String("invalid".into()),
            asset("../invalid.png"),
            ComponentValue::map([
                ("kind", ComponentValue::String("stable_id".into())),
                ("value", ComponentValue::String("invalid".into())),
            ]),
        ] {
            let value = ComponentValue::map([("field_0", payload)]);
            let original = value.clone();
            let expected = collect_declared_asset_references_from_schema_with_limits(
                &schema,
                schema.version(),
                &value,
                Default::default(),
                Default::default(),
            )
            .unwrap_err();
            let actual = collect_declared_effective_asset_references_with_admission(
                &schema,
                schema.version(),
                &value,
                Default::default(),
                |_, _, _| Ok::<(), ()>(()),
            )
            .unwrap_err();
            assert_eq!(actual, AssetReferenceCollectionError::Reference(expected));
            assert_eq!(value, original);
        }
    }
}
