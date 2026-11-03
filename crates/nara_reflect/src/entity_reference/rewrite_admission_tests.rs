use std::collections::BTreeMap;

use nara_identity::SceneEntityId;

use super::*;
use crate::{ComponentSchemaVersion, ComponentTypeId, validate_component_value};

#[derive(Debug)]
struct Denied;

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal {
        entity: SceneEntityId::new(entity).unwrap(),
    }
}

fn schema(field: ComponentFieldSchema) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.ReferenceRewriteAdmission"),
        "Reference rewrite admission",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field])
}

fn reference_field(path: ComponentFieldPath, shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(ComponentFieldId::new("links"), "Links", path, shape)
        .with_capability(ComponentCapability::EntityRef)
}

fn plan(schema: &ComponentSchema, value: &ComponentValue) -> DeclaredEntityReferencePlan {
    validate_component_value(schema, value).unwrap();
    plan_declared_entity_references::<()>(schema, value, EntityReferenceTraversalLimits::default())
        .unwrap()
}

fn bill(plan: &DeclaredEntityReferencePlan, cost: ComponentValueCost) -> (usize, usize, usize) {
    let mut total = (0usize, 0usize, 0usize);
    admit_declared_entity_reference_plan_rewrite(plan, cost, |nodes, bytes, allocation| {
        total.0 = total.0.checked_add(nodes).unwrap();
        total.1 = total.1.checked_add(bytes).unwrap();
        total.2 = total.2.checked_add(allocation.unwrap()).unwrap();
        Ok::<_, Denied>(())
    })
    .unwrap();
    total
}

fn sparse_fixture() -> (ComponentSchema, ComponentValue) {
    let field = reference_field(
        ComponentFieldPath::from_fields(["links"]),
        ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
            ComponentValueShape::EntityRef,
        ))),
    );
    let schema = schema(field).with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("notes"),
        "Notes",
        ComponentFieldPath::from_fields(["notes"]),
        ComponentValueKind::String,
    )]);
    let mut entries = (0..1023)
        .map(|index| (format!("unused-{index:04}"), ComponentValue::Null))
        .collect::<BTreeMap<_, _>>();
    entries.insert(
        "target".to_owned(),
        ComponentValue::EntityReference(local("original")),
    );
    let mut list = vec![ComponentValue::Map(BTreeMap::new()); 1024];
    list[0] = ComponentValue::Map(entries);
    let value = ComponentValue::map([
        ("links", ComponentValue::List(list)),
        ("notes", ComponentValue::String(String::new())),
    ]);
    (schema, value)
}

#[test]
fn empty_plan_for_large_scalar_list_has_no_rewrite_debit() {
    let schema = schema(ComponentFieldSchema::required(
        ComponentFieldId::new("values"),
        "Values",
        ComponentFieldPath::from_fields(["values"]),
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
    ));
    let value = ComponentValue::map([(
        "values",
        ComponentValue::List(vec![ComponentValue::U64(7); 1024]),
    )]);
    let plan = plan(&schema, &value);
    assert!(plan.references.is_empty());
    admit_declared_entity_reference_plan_rewrite(&plan, value.cost(), |_, _, _| {
        panic!("an empty plan must not call admission");
        #[allow(unreachable_code)]
        Ok::<_, Denied>(())
    })
    .unwrap();
}

#[test]
fn sparse_list_and_map_paths_have_a_bounded_bill_independent_of_unrelated_payload_bytes() {
    let (schema, value) = sparse_fixture();
    let plan = plan(&schema, &value);
    assert_eq!(plan.references.len(), 1);
    let total = bill(&plan, value.cost());
    assert!(total.0 > 0 && total.0 < 2048, "{total:?}");
    assert!(total.1 > 0 && total.1 < 16 * 1024, "{total:?}");
    assert!(total.2 > 0 && total.2 < 4096, "{total:?}");

    let mut larger = value.clone();
    larger
        .set_path(
            &ComponentFieldPath::from_fields(["notes"]),
            ComponentValue::String("x".repeat(64 * 1024)),
        )
        .unwrap();
    validate_component_value(&schema, &larger).unwrap();
    assert_eq!(larger.cost().nodes(), value.cost().nodes());
    assert!(larger.cost().logical_bytes() > value.cost().logical_bytes());
    assert_eq!(bill(&plan, larger.cost()), total);
}

#[test]
fn admission_refusal_preserves_error_identity_and_prevents_the_callers_rewrite() {
    let (schema, original) = sparse_fixture();
    let plan = plan(&schema, &original);
    for deny_at in [0, 1, 3] {
        let denied = Denied;
        let mut candidate = original.clone();
        let mut admissions = 0;
        let mut rewrites = 0;
        let result = admit_declared_entity_reference_plan_rewrite(
            &plan,
            candidate.cost(),
            |_, _, allocation| {
                assert!(allocation.is_some());
                let current = admissions;
                admissions += 1;
                if current == deny_at {
                    Err(&denied)
                } else {
                    Ok(())
                }
            },
        );
        if result.is_ok() {
            plan.rewrite_in_place(&mut candidate, |_, _| {
                rewrites += 1;
                Ok::<_, ()>(local("rewritten"))
            })
            .unwrap();
        }
        assert!(std::ptr::eq(result.unwrap_err(), &denied));
        assert_eq!(admissions, deny_at + 1);
        assert_eq!(rewrites, 0);
        assert_eq!(candidate, original);
    }
}

#[test]
fn successful_admission_uses_the_existing_rewrite_and_typed_callback_error() {
    let (schema, mut candidate) = sparse_fixture();
    let plan = plan(&schema, &candidate);
    let cost = candidate.cost();
    assert!(bill(&plan, cost).2 > 0);
    let path = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("links"),
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::field("target"),
    ]);
    assert_eq!(
        candidate.get_path(&path).unwrap().as_entity_reference(),
        Some(&local("original")),
    );
    let denied = Denied;
    let error = plan
        .rewrite_in_place(&mut candidate, |_, _| Err(&denied))
        .unwrap_err();
    match error {
        ComponentEntityReferenceRewriteError::Rewrite {
            path: actual,
            error,
        } => {
            assert_eq!(actual, path);
            assert!(std::ptr::eq(error, &denied));
        }
        other => panic!("unexpected rewrite error: {other:?}"),
    }
    // A second rewrite requires its own admission, including already discarded scratch.
    bill(&plan, candidate.cost());
    plan.rewrite_in_place(&mut candidate, |_, _| Ok::<_, ()>(local("rewritten")))
        .unwrap();
    assert_eq!(
        candidate.get_path(&path).unwrap().as_entity_reference(),
        Some(&local("rewritten")),
    );
}

#[test]
fn scalar_payload_reference_charges_both_accesses_and_inline_diagnostic_storage() {
    let schema = schema(reference_field(
        ComponentFieldPath::from_fields(["links"]),
        ComponentValueShape::EntityRef,
    ));
    let field = &schema.fields()[0];
    let mut value = ComponentValue::EntityReference(local("original"));
    let plan = plan_declared_value_entity_references::<()>(
        field,
        &field.shape,
        &value,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    assert_eq!(plan.references.len(), 1);
    assert!(plan.references[0].path.is_empty());
    assert_eq!(
        bill(&plan, value.cost()),
        (2, 0, size_of::<ComponentEntityReferenceRewriteError<()>>()),
    );
    plan.rewrite_in_place(&mut value, |path, _| {
        assert!(path.is_empty());
        Ok::<_, ()>(local("rewritten"))
    })
    .unwrap();
    assert_eq!(value, ComponentValue::EntityReference(local("rewritten")));
}

#[test]
fn long_route_is_admitted_before_its_key_and_prefix_copy_bill() {
    const NESTING: usize = 48;
    let name = "route".repeat(2048);
    let mut shape = ComponentValueShape::EntityRef;
    let mut payload = ComponentValue::EntityReference(local("original"));
    for _ in 0..NESTING {
        shape = ComponentValueShape::list(shape);
        payload = ComponentValue::List(vec![payload]);
    }
    let schema = schema(reference_field(
        ComponentFieldPath::from_fields([name.as_str()]),
        shape,
    ));
    let value = ComponentValue::map([(name.as_str(), payload)]);
    let plan = plan(&schema, &value);
    let denied = Denied;
    let mut calls = 0;
    let error = admit_declared_entity_reference_plan_rewrite(
        &plan,
        value.cost(),
        |nodes, bytes, allocation| {
            calls += 1;
            assert_eq!(nodes, NESTING + 3);
            assert_eq!(bytes, 0);
            assert_eq!(
                allocation,
                Some(size_of::<ComponentEntityReferenceRewriteError<()>>())
            );
            Err(&denied)
        },
    )
    .unwrap_err();
    assert!(std::ptr::eq(error, &denied));
    assert_eq!(calls, 1);

    let total = bill(&plan, value.cost());
    assert!(total.2 > 2 * (NESTING + 1) * name.len());
    let mut allocation = 0usize;
    let error = admit_declared_entity_reference_plan_rewrite(&plan, value.cost(), |_, _, next| {
        allocation = allocation
            .checked_add(next.ok_or(&denied)?)
            .ok_or(&denied)?;
        if allocation > 128 * 1024 {
            Err(&denied)
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(std::ptr::eq(error, &denied));
}

#[test]
fn unrepresentable_segment_bills_forward_none_and_the_exact_refusal() {
    // Synthetic lengths exercise arithmetic that would require unallocatable real paths.
    // The same billing boundary is used after the production helper admits each real route.
    for (prefixes, field_bytes, comparisons) in [
        (usize::MAX, None, 0),
        (usize::MAX / size_of::<ComponentFieldPathSegment>(), None, 0),
        (1, Some(usize::MAX), 1),
        (1, Some(1), usize::MAX),
    ] {
        let denied = Denied;
        let mut calls = 0;
        let error = admit_reference_rewrite_segment(
            prefixes,
            field_bytes,
            comparisons,
            &mut |nodes, bytes, allocation| {
                calls += 1;
                assert_eq!((nodes, bytes, allocation), (0, 0, None));
                Err(&denied)
            },
        )
        .unwrap_err();
        assert!(std::ptr::eq(error, &denied));
        assert_eq!(calls, 1);
    }
}
