use std::cell::Cell;

use nara_core::{ByteLimit, DepthLimit, ItemLimit};

use super::*;
use crate::{
    ComponentCapability, ComponentFieldId, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValueKind,
    ComponentValueShape, collect_declared_asset_references_from_schema,
    collect_declared_field_asset_references, collect_declared_value_asset_references_with_limits,
};

fn path_value(path: &str) -> ComponentValue {
    AssetRef::path(path).unwrap().encode().unwrap()
}

fn field(shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("images"),
        "Images",
        ComponentFieldPath::from_fields(["settings", "images"]),
        shape,
    )
    .with_capability(ComponentCapability::AssetRef)
}

fn collect_payload(
    shape: ComponentValueShape,
    value: &ComponentValue,
) -> Vec<DeclaredAssetReference> {
    let field = field(shape);
    collect_declared_value_asset_references_with_limits(
        &ComponentTypeId::new("test.TemplateAssets"),
        &field,
        field.shape(),
        value,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap()
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> EntityReferenceTraversalLimits {
    EntityReferenceTraversalLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn rewrites_nested_list_map_and_optional_leaves_without_matching_ordinary_text() {
    let shape = ComponentValueShape::record_members([
        ComponentShapeMember::new(
            "images",
            ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
                ComponentValueShape::AssetRef,
            ))),
        ),
        ComponentShapeMember::new("caption", ComponentValueKind::String),
        ComponentShapeMember::new(
            "opaque",
            ComponentValueShape::opaque(ComponentValueKind::Map),
        ),
    ]);
    let mut value = ComponentValue::map([
        (
            "images",
            ComponentValue::List(vec![
                ComponentValue::map([
                    ("first", path_value("images/same.png")),
                    ("missing", ComponentValue::Null),
                ]),
                ComponentValue::map([("last", path_value("images/other.png"))]),
            ]),
        ),
        ("caption", ComponentValue::String("images/same.png".into())),
        ("opaque", path_value("images/same.png")),
    ]);
    let references = collect_payload(shape, &value);
    assert_eq!(references.len(), 2);
    let mut visited = Vec::new();
    rewrite_declared_asset_references(&mut value, &references, |reference| {
        visited.push(reference.path().clone());
        Ok::<_, ()>(
            AssetRef::path(format!(
                "copied/{}",
                reference.asset_ref().as_path().unwrap()
            ))
            .unwrap(),
        )
    })
    .unwrap();

    assert_eq!(
        visited,
        references
            .iter()
            .map(|reference| reference.path().clone())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        value,
        ComponentValue::map([
            (
                "images",
                ComponentValue::List(vec![
                    ComponentValue::map([
                        ("first", path_value("copied/images/same.png")),
                        ("missing", ComponentValue::Null),
                    ]),
                    ComponentValue::map([("last", path_value("copied/images/other.png"))]),
                ]),
            ),
            ("caption", ComponentValue::String("images/same.png".into())),
            ("opaque", path_value("images/same.png")),
        ]),
    );
}

#[test]
fn complete_and_selected_field_records_rewrite_the_complete_component() {
    let field = field(ComponentValueShape::list(ComponentValueShape::AssetRef));
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.TemplateAssets"),
        "Template assets",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field.clone()]);
    let selected = ComponentValue::List(vec![path_value("a.png")]);
    let original = ComponentValue::map([(
        "settings",
        ComponentValue::map([("images", selected.clone())]),
    )]);
    let whole = collect_declared_asset_references_from_schema(&schema, schema.version(), &original)
        .unwrap();
    let selected = collect_declared_field_asset_references(schema.id(), &field, &selected).unwrap();
    assert_eq!(whole, selected);
    for references in [whole, selected] {
        let mut value = original.clone();
        rewrite_declared_asset_references(&mut value, &references, |_| {
            Ok::<_, ()>(AssetRef::path("copied/a.png").unwrap())
        })
        .unwrap();
        assert_eq!(
            value,
            ComponentValue::map([(
                "settings",
                ComponentValue::map([(
                    "images",
                    ComponentValue::List(vec![path_value("copied/a.png")]),
                )]),
            )]),
        );
    }
}

#[test]
fn empty_payload_path_supports_longer_paths_and_both_wire_variants() {
    let path = AssetRef::path("a.png").unwrap();
    let stable = AssetRef::stable_id("12345678-1234-5678-90ab-123456789abc").unwrap();
    let longer = AssetRef::path(format!("copied/{}/a.png", "long".repeat(1024))).unwrap();
    for (original, replacement) in [
        (path.clone(), longer.clone()),
        (path, stable.clone()),
        (stable, longer),
    ] {
        let mut value = original.encode().unwrap();
        let references = collect_payload(ComponentValueShape::AssetRef, &value);
        assert_eq!(references.len(), 1);
        assert!(references[0].path().is_empty());
        let expected = replacement.encode().unwrap();
        let mut replacement = Some(replacement);
        rewrite_declared_asset_references(&mut value, &references, |reference| {
            assert_eq!(reference.asset_ref(), &original);
            Ok::<_, ()>(replacement.take().unwrap())
        })
        .unwrap();
        assert_eq!(value, expected);
    }
}

#[test]
fn decoded_identity_is_compared_using_the_existing_canonical_decoder() {
    let stable = AssetRef::stable_id("12345678-1234-5678-90ab-123456789abc").unwrap();
    let mut value = stable.encode().unwrap();
    let references = collect_payload(ComponentValueShape::AssetRef, &value);
    let ComponentValue::Map(fields) = &mut value else {
        unreachable!()
    };
    // The shared stable-ID decoder accepts this spelling as the same identity.
    fields.insert(
        "value".into(),
        ComponentValue::String("123456781234567890AB123456789ABC".into()),
    );
    rewrite_declared_asset_references(&mut value, &references, |_| {
        Ok::<_, ()>(AssetRef::path("copied/a.png").unwrap())
    })
    .unwrap();
    assert_eq!(value, path_value("copied/a.png"));
}

#[test]
fn stale_later_reference_refuses_before_any_callback_or_replacement() {
    let mut value = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &value,
    );
    *value.get_path_mut(references[1].path()).unwrap() = path_value("changed.png");
    let stale = value.clone();
    let mut calls = 0;
    let result = rewrite_declared_asset_references(&mut value, &references, |_| {
        calls += 1;
        Ok::<_, ()>(AssetRef::path("copied/a.png").unwrap())
    });
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::ChangedReference {
            path: references[1].path().clone(),
        })
    );
    assert_eq!(calls, 0);
    assert_eq!(value, stale);
}

#[test]
fn missing_paths_and_invalid_later_values_refuse_before_any_callback() {
    let original = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &original,
    );
    for invalid in [
        ComponentValue::Null,
        ComponentValue::String("b.png".into()),
        ComponentValue::map([
            ("kind", ComponentValue::String("unsupported".into())),
            ("value", ComponentValue::String("b.png".into())),
        ]),
        ComponentValue::map([
            ("kind", ComponentValue::String("path".into())),
            ("value", ComponentValue::String("../outside.png".into())),
        ]),
        ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".into())),
            ("value", ComponentValue::String("not-an-id".into())),
        ]),
    ] {
        let mut value = original.clone();
        *value.get_path_mut(references[1].path()).unwrap() = invalid;
        let before = value.clone();
        let result = rewrite_declared_asset_references(
            &mut value,
            &references,
            |_| -> Result<AssetRef, ()> { panic!("all leaves must validate before callbacks") },
        );
        assert_eq!(
            result,
            Err(AssetReferenceRewriteError::InvalidReferenceValue {
                path: references[1].path().clone(),
            })
        );
        assert_eq!(value, before);
    }
    for mut value in [
        ComponentValue::List(vec![path_value("a.png")]),
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
    ] {
        let before = value.clone();
        let result = rewrite_declared_asset_references(&mut value, &references, |_| {
            Err::<AssetRef, _>("callback must not run")
        });
        assert!(matches!(
            result,
            Err(AssetReferenceRewriteError::InvalidPath { .. })
        ));
        assert_eq!(value, before);
    }
}

#[test]
fn nonadjacent_duplicates_and_overlapping_paths_refuse_before_callbacks() {
    let original = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &original,
    );
    let mut descendant = references[0].clone();
    descendant.path = ComponentFieldPath::new([
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("value"),
    ]);
    for records in [
        vec![
            references[0].clone(),
            references[1].clone(),
            references[0].clone(),
        ],
        vec![descendant, references[1].clone(), references[0].clone()],
    ] {
        let mut value = original.clone();
        let result = rewrite_declared_asset_references(&mut value, &records, |_| {
            Err::<AssetRef, _>("callback must not run")
        });
        assert!(matches!(
            result,
            Err(AssetReferenceRewriteError::ConflictingPaths { .. })
        ));
        assert_eq!(value, original);
    }
    let mut value = path_value("a.png");
    let references = collect_payload(ComponentValueShape::AssetRef, &value);
    let mut descendant = references[0].clone();
    descendant.path = ComponentFieldPath::from_fields(["value"]);
    let result =
        rewrite_declared_asset_references(&mut value, &[descendant, references[0].clone()], |_| {
            Err::<AssetRef, _>("callback must not run")
        });
    assert!(matches!(
        result,
        Err(AssetReferenceRewriteError::ConflictingPaths { path, .. }) if path.is_empty()
    ));
    assert_eq!(value, path_value("a.png"));
}

#[test]
fn callback_failure_retains_actual_earlier_changes_in_supplied_order() {
    let mut value = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let mut references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &value,
    );
    references.reverse();
    let mut calls = 0;
    let result = rewrite_declared_asset_references(&mut value, &references, |reference| {
        calls += 1;
        if calls == 2 {
            return Err("owner-rewrite-failure");
        }
        assert_eq!(reference.asset_ref(), &AssetRef::path("b.png").unwrap());
        Ok(AssetRef::stable_id("12345678-1234-5678-90ab-123456789abc").unwrap())
    });
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::Rewrite {
            path: references[1].path().clone(),
            error: "owner-rewrite-failure",
        })
    );
    assert_eq!(calls, 2);
    assert_eq!(
        value,
        ComponentValue::List(vec![
            path_value("a.png"),
            AssetRef::stable_id("12345678-1234-5678-90ab-123456789abc")
                .unwrap()
                .encode()
                .unwrap(),
        ]),
    );
}

#[test]
fn cumulative_allocation_refusal_precedes_the_next_replacement() {
    let mut value = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &value,
    );
    let first = AssetRef::path(format!("copied/{}.png", "a".repeat(700))).unwrap();
    let second = AssetRef::path(format!("copied/{}.png", "b".repeat(700))).unwrap();
    let expected = ComponentValue::List(vec![first.encode().unwrap(), path_value("b.png")]);
    // Prebuilt callback results exclude caller allocations from the helper's admission.
    let mut replacements = [first, second].into_iter();
    let calls = Cell::new(0);
    let mut allocated = 0usize;
    let mut denied_at = None;
    let result = rewrite_declared_asset_references_with_admission(
        &mut value,
        &references,
        EntityReferenceTraversalLimits::default(),
        |_| {
            calls.set(calls.get() + 1);
            Ok::<_, ()>(replacements.next().unwrap())
        },
        |_, _, delta| {
            assert!(
                delta < 2_000,
                "each individual request fits the owner budget"
            );
            if allocated + delta > 2_000 {
                denied_at = Some(calls.get());
                return Err("owner-cumulative-allocation-limit");
            }
            allocated += delta;
            Ok(())
        },
    );
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::Admission(
            "owner-cumulative-allocation-limit"
        )),
    );
    assert_eq!(denied_at, Some(2));
    assert_eq!(calls.get(), 2);
    assert_eq!(value, expected);
}

#[test]
fn admission_refusal_keeps_the_exact_borrowed_error_without_trait_bounds() {
    struct OwnerRefusal<'a>(&'a u8);
    let token = 7;
    let mut value = path_value("a.png");
    let original = value.clone();
    let references = collect_payload(ComponentValueShape::AssetRef, &value);
    let result = rewrite_declared_asset_references_with_admission(
        &mut value,
        &references,
        EntityReferenceTraversalLimits::default(),
        |_| Err::<AssetRef, _>("callback must not run"),
        |_, _, _| Err(OwnerRefusal(&token)),
    );
    let Err(AssetReferenceRewriteError::Admission(OwnerRefusal(actual))) = result else {
        panic!("the exact owner refusal must be returned");
    };
    assert!(std::ptr::eq(actual, &token));
    assert_eq!(value, original);
}

#[test]
fn work_refusal_before_error_path_copy_stays_distinct_from_rewrite_failure() {
    let mut value = ComponentValue::List(vec![path_value("a.png")]);
    let original = value.clone();
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &value,
    );
    let called = Cell::new(false);
    let result = rewrite_declared_asset_references_with_admission(
        &mut value,
        &references,
        EntityReferenceTraversalLimits::default(),
        |_| {
            called.set(true);
            Err::<AssetRef, _>("rewrite-failed")
        },
        |_, _, allocated| {
            if called.get() && allocated > 0 {
                Err("owner-error-path-refusal")
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::Admission(
            "owner-error-path-refusal"
        ))
    );
    assert_eq!(value, original);
}

#[test]
fn invalid_stable_id_text_is_admitted_before_the_shared_decoder_can_own_it() {
    let mut value = path_value("a.png");
    let references = collect_payload(ComponentValueShape::AssetRef, &value);
    let invalid = "x".repeat(64 * 1024);
    value = ComponentValue::map([
        ("kind", ComponentValue::String("stable_id".into())),
        ("value", ComponentValue::String(invalid.clone())),
    ]);
    let before = value.clone();
    let mut refused = false;
    let result = rewrite_declared_asset_references_with_admission(
        &mut value,
        &references,
        EntityReferenceTraversalLimits::default(),
        |_| Err::<AssetRef, _>("callback must not run"),
        |_, _, allocated| {
            if allocated == invalid.len() {
                refused = true;
                Err("owner-decode-allocation-refusal")
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::Admission(
            "owner-decode-allocation-refusal"
        )),
    );
    assert!(refused);
    assert_eq!(value, before);
}

#[test]
fn node_byte_and_path_depth_limits_are_bounded_before_callbacks() {
    let original = ComponentValue::List(vec![path_value("a.png"), path_value("b.png")]);
    let references = collect_payload(
        ComponentValueShape::list(ComponentValueShape::AssetRef),
        &original,
    );
    for (limits, expected) in [
        (
            limits(1, 1024, 64),
            AssetReferenceRewriteError::NodeLimit { maximum: 1 },
        ),
        (
            limits(128, 1, 64),
            AssetReferenceRewriteError::ByteLimit { maximum: 1 },
        ),
    ] {
        let mut value = original.clone();
        let result = rewrite_declared_asset_references_with_admission(
            &mut value,
            &references,
            limits,
            |_| Err::<AssetRef, _>("callback must not run"),
            |_, _, _| Ok::<_, ()>(()),
        );
        assert_eq!(result, Err(expected));
        assert_eq!(value, original);
    }
    let field = field(ComponentValueShape::AssetRef);
    let selected = path_value("a.png");
    let references = collect_declared_field_asset_references(
        &ComponentTypeId::new("test.TemplateAssets"),
        &field,
        &selected,
    )
    .unwrap();
    let mut value =
        ComponentValue::map([("settings", ComponentValue::map([("images", selected)]))]);
    let original = value.clone();
    let result = rewrite_declared_asset_references_with_admission(
        &mut value,
        &references,
        limits(128, 1024, 1),
        |_| Err::<AssetRef, _>("callback must not run"),
        |_, _, _| Ok::<_, ()>(()),
    );
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::DepthLimit { maximum: 1 })
    );
    assert_eq!(value, original);
}

#[test]
fn collected_effective_default_does_not_materialize_a_missing_candidate_leaf() {
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueShape::AssetRef,
        path_value("fallback.png"),
    )
    .with_capability(ComponentCapability::AssetRef);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.DefaultAsset"),
        "Default asset",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field]);
    let mut value = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let references =
        collect_declared_asset_references_from_schema(&schema, schema.version(), &value).unwrap();
    assert_eq!(references.len(), 1);
    let original = value.clone();
    let result = rewrite_declared_asset_references(&mut value, &references, |_| {
        Err::<AssetRef, _>("callback must not run")
    });
    assert_eq!(
        result,
        Err(AssetReferenceRewriteError::InvalidPath {
            path: references[0].path().clone(),
        })
    );
    assert_eq!(value, original);
}
