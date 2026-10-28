use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};

use super::*;
use crate::{ComponentCapability, ComponentFieldPathSegment, ComponentShapeMember};

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

thread_local! {
    static ALLOCATION_CALLS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = ALLOCATION_CALLS.try_with(|calls| calls.set(calls.get().saturating_add(1)));
}

// SAFETY: Operations forward the caller's allocation and layout unchanged to System. The
// thread-local counter does not allocate and excludes allocations from other test threads.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The valid caller-provided layout is forwarded unchanged.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation();
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The valid caller-provided layout is forwarded unchanged.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation();
        }
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The live allocation, original layout and requested size are unchanged.
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !pointer.is_null() {
            record_allocation();
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The live allocation and original layout are forwarded unchanged.
        unsafe { System.dealloc(pointer, layout) };
    }
}

fn allocation_calls() -> usize {
    ALLOCATION_CALLS.with(Cell::get)
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

fn asset(path: &str) -> ComponentValue {
    ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String(path.into())),
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

fn collect<E>(
    payload_relative: bool,
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    if payload_relative {
        collect_declared_value_asset_references_with_admission(
            component,
            field,
            field.shape(),
            value,
            limits,
            admit,
        )
    } else {
        collect_declared_field_asset_references_with_admission(
            component, field, value, limits, admit,
        )
    }
}

#[test]
fn selected_collectors_admit_actual_long_asset_paths_before_parsing() {
    let component = ComponentTypeId::new("test.SelectedAssets");
    let field = field(ComponentValueShape::AssetRef);
    for payload_relative in [false, true] {
        for length in [5, 64 * 1024] {
            let path = "a".repeat(length);
            let value = asset(&path);
            let original = value.clone();
            let output_bytes =
                size_of::<DeclaredAssetReference>() + field.id().as_str().len() + path.len();
            let mut output_charge = None;
            let references = collect(
                payload_relative,
                &component,
                &field,
                &value,
                Default::default(),
                |nodes, logical, allocation| {
                    if nodes == 1 && allocation >= size_of::<DeclaredAssetReference>() {
                        output_charge = Some((logical, allocation));
                    }
                    Ok::<(), ()>(())
                },
            )
            .unwrap();
            let expected_path = if payload_relative {
                ComponentFieldPath::empty()
            } else {
                field.path().clone()
            };
            let path_bytes = expected_path
                .segments()
                .iter()
                .map(|segment| match segment {
                    ComponentFieldPathSegment::Field(name) => name.len(),
                    ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
                })
                .sum::<usize>();
            assert_eq!(
                output_charge,
                Some((
                    field.id().as_str().len() + path_bytes + path.len(),
                    output_bytes,
                )),
            );
            assert_eq!(references.len(), 1);
            assert_eq!(references[0].path(), &expected_path);
            assert_eq!(references[0].field_id(), field.id());
            assert_eq!(
                references[0].asset_ref(),
                &AssetRef::path(path.as_str()).unwrap()
            );

            let mut calls_at_denial = None;
            let result = collect(
                payload_relative,
                &component,
                &field,
                &value,
                Default::default(),
                |nodes, _, allocation| {
                    if nodes == 1 && allocation == output_bytes {
                        calls_at_denial = Some(allocation_calls());
                        Err("owner-output-cap")
                    } else {
                        Ok(())
                    }
                },
            );
            let after = allocation_calls();
            assert_eq!(
                result,
                Err(AssetReferenceCollectionError::Admission("owner-output-cap")),
            );
            assert_eq!(calls_at_denial, Some(after));
            assert_eq!(value, original);
        }
    }
}

#[test]
fn selected_refusal_precedes_dynamic_name_path_and_leaf_vector_growth() {
    #[derive(Clone, Copy)]
    enum Deny {
        Name,
        PathGrowth,
        OutputPath,
        LeafGrowth,
    }

    let component = ComponentTypeId::new("test.SelectedAssets");
    let field = field(ComponentValueShape::map(ComponentValueShape::list(
        ComponentValueShape::AssetRef,
    )));
    let name = "m".repeat(1003);
    let value = ComponentValue::map([(
        name.as_str(),
        ComponentValue::List((0..5).map(|_| asset("a.png")).collect()),
    )]);
    let original = value.clone();
    for payload_relative in [false, true] {
        let path_segments = if payload_relative { 2 } else { 4 };
        let path_names = name.len() + if payload_relative { 0 } else { 13 };
        let output_path_bytes = path_segments * size_of::<ComponentFieldPathSegment>() + path_names;
        for deny in [
            Deny::Name,
            Deny::PathGrowth,
            Deny::OutputPath,
            Deny::LeafGrowth,
        ] {
            let mut copying_name = false;
            let mut recorded_outputs = 0;
            let mut calls_at_denial = None;
            let result = collect(
                payload_relative,
                &component,
                &field,
                &value,
                Default::default(),
                |nodes, logical, allocation| {
                    let name_copy = (nodes, logical, allocation) == (0, 0, name.len());
                    let refuse = match deny {
                        Deny::Name => name_copy,
                        Deny::PathGrowth => {
                            copying_name && allocation == 4 * size_of::<ComponentFieldPathSegment>()
                        }
                        Deny::OutputPath => recorded_outputs > 0 && allocation == output_path_bytes,
                        Deny::LeafGrowth => {
                            recorded_outputs == 5
                                && allocation == 8 * size_of::<PresentReferenceLeaf<'_, '_>>()
                        }
                    };
                    if refuse {
                        calls_at_denial = Some(allocation_calls());
                        return Err("owner-growth-cap");
                    }
                    copying_name |= name_copy;
                    if nodes == 1 && allocation >= size_of::<DeclaredAssetReference>() {
                        recorded_outputs += 1;
                    }
                    Ok(())
                },
            );
            let after = allocation_calls();
            assert_eq!(
                result,
                Err(AssetReferenceCollectionError::Admission("owner-growth-cap")),
            );
            assert_eq!(calls_at_denial, Some(after));
            assert_eq!(value, original);
        }
    }
}

#[test]
fn selected_immediate_refusal_preserves_borrowed_owner_error_without_allocating() {
    // The caller's error needs neither Clone, Debug, Display, Error nor 'static.
    struct OwnerFailure<'a>(&'a u8);
    let marker = 7;
    let component = ComponentTypeId::new("test.SelectedAssets");
    let field = field(ComponentValueShape::AssetRef);
    let value = asset("a.png");
    let original = value.clone();
    for payload_relative in [false, true] {
        let before = allocation_calls();
        let result = collect(
            payload_relative,
            &component,
            &field,
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
}

#[test]
fn nested_selected_shape_borrows_its_owner_and_fits_a_small_actual_work_cap() {
    let component = ComponentTypeId::new("test.SelectedAssets");
    let payload = ComponentValue::map([("images", ComponentValue::List(vec![asset("a.png")]))]);
    let default = ComponentValue::map([
        ("selected", payload.clone()),
        ("unused", ComponentValue::String("u".repeat(1024 * 1024))),
    ]);
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["settings", "image"]),
        ComponentValueShape::record_members([
            ComponentShapeMember::new(
                "selected",
                ComponentValueShape::record_members([ComponentShapeMember::new(
                    "images",
                    ComponentValueShape::list(ComponentValueShape::AssetRef),
                )]),
            ),
            ComponentShapeMember::new("unused", ComponentValueKind::String),
        ]),
        default,
    )
    .with_capability(ComponentCapability::AssetRef);
    let original_field = field.clone();
    let original_payload = payload.clone();
    let ComponentValueShape::Optional { value: root } = field.shape() else {
        unreachable!()
    };
    let ComponentValueShape::Record { members } = root.as_ref() else {
        unreachable!()
    };
    let shape = &members[0].shape;
    let mut previous = None;
    for traversal_limits in [
        limits(16, 256, 4),
        limits(1_000_000, 128 * 1024 * 1024, 128),
    ] {
        let mut cost = [0usize; 3];
        let references = collect_declared_value_asset_references_with_admission(
            &component,
            &field,
            shape,
            &payload,
            traversal_limits,
            |nodes, logical, allocation| {
                for (used, delta) in cost.iter_mut().zip([nodes, logical, allocation]) {
                    *used += delta;
                }
                if cost[0] > 64 || cost[1] > 1024 || cost[2] > 8 * 1024 {
                    return Err("owner-small-cap");
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].field_id(), field.id());
        assert_eq!(
            references[0].path(),
            &ComponentFieldPath::new([
                ComponentFieldPathSegment::field("images"),
                ComponentFieldPathSegment::index(0),
            ]),
        );
        assert_eq!(
            references,
            collect_declared_value_asset_references_with_limits(
                &component,
                &field,
                shape,
                &payload,
                traversal_limits,
            )
            .unwrap(),
        );
        if let Some(previous) = previous {
            assert_eq!(cost, previous);
        }
        previous = Some(cost);
    }
    let before = allocation_calls();
    let result = collect_declared_value_asset_references_with_admission(
        &component,
        &field,
        shape,
        &payload,
        Default::default(),
        |_, _, allocation| {
            if allocation > 1 {
                Err("owner-tiny-cap")
            } else {
                Ok(())
            }
        },
    );
    let after = allocation_calls();
    assert_eq!(
        result,
        Err(AssetReferenceCollectionError::Admission("owner-tiny-cap")),
    );
    assert_eq!(after, before);
    assert_eq!(field, original_field);
    assert_eq!(payload, original_payload);
}

#[test]
fn selected_collectors_preserve_reference_errors_and_source_values() {
    let component = ComponentTypeId::new("test.SelectedAssets");
    let field = field(ComponentValueShape::AssetRef);
    for value in [
        ComponentValue::Null,
        ComponentValue::String("invalid".into()),
        asset("../invalid.png"),
        ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".into())),
            ("value", ComponentValue::String("invalid".into())),
        ]),
        ComponentValue::map([
            ("kind", ComponentValue::String("unsupported".into())),
            ("value", ComponentValue::String("a.png".into())),
        ]),
    ] {
        let original = value.clone();
        for payload_relative in [false, true] {
            let expected = if payload_relative {
                collect_declared_value_asset_references_with_limits(
                    &component,
                    &field,
                    field.shape(),
                    &value,
                    Default::default(),
                )
            } else {
                collect_declared_field_asset_references_with_limits(
                    &component,
                    &field,
                    &value,
                    Default::default(),
                )
            }
            .unwrap_err();
            assert_eq!(
                collect(
                    payload_relative,
                    &component,
                    &field,
                    &value,
                    Default::default(),
                    |_, _, _| Ok::<(), ()>(()),
                ),
                Err(AssetReferenceCollectionError::Reference(expected)),
            );
            assert_eq!(value, original);
        }
    }
}

#[test]
fn selected_collectors_keep_exact_input_and_output_traversal_limits() {
    let component = ComponentTypeId::new("test.SelectedAssets");
    let field = field(ComponentValueShape::AssetRef);
    let value = asset("a.png");
    let original = value.clone();
    for (payload_relative, bytes, depth) in [(false, 41, 4), (true, 28, 2)] {
        assert!(
            collect(
                payload_relative,
                &component,
                &field,
                &value,
                limits(4, bytes, depth),
                |_, _, _| Ok::<(), ()>(()),
            )
            .is_ok()
        );
        for (limits, source) in [
            (
                limits(3, bytes, depth),
                ComponentEntityReferenceRewriteError::NodeLimit { maximum: 3 },
            ),
            (
                limits(4, bytes - 1, depth),
                ComponentEntityReferenceRewriteError::ByteLimit { maximum: bytes - 1 },
            ),
            (
                limits(4, bytes, depth - 1),
                ComponentEntityReferenceRewriteError::DepthLimit { maximum: depth - 1 },
            ),
        ] {
            assert_eq!(
                collect(
                    payload_relative,
                    &component,
                    &field,
                    &value,
                    limits,
                    |_, _, _| Ok::<(), ()>(()),
                ),
                Err(AssetReferenceCollectionError::Reference(
                    DeclaredAssetReferenceError::Traversal {
                        component: component.clone(),
                        source,
                    },
                )),
            );
        }
        assert_eq!(value, original);
    }
}
