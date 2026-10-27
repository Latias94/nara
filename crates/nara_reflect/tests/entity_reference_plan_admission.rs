use nara_identity::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    SceneEntityId,
};
use nara_reflect::__private::{
    EntityReferencePlanError, plan_declared_entity_references,
    plan_declared_entity_references_with_admission,
};
use nara_reflect::{
    ComponentCapability, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, EntityReferenceTraversalLimits, validate_component_value,
};

#[derive(Debug, PartialEq, Eq)]
struct AdmissionDenied {
    maximum: usize,
    attempted: usize,
}

fn local(entity: &str) -> EntityReference {
    EntityReference::SceneLocal {
        entity: SceneEntityId::new(entity).unwrap(),
    }
}

fn fixture() -> (ComponentSchema, ComponentValue) {
    let name = "links".repeat(4 * 1024);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.ReferencePlanAdmission"),
        "Reference plan admission",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("stable-links"),
            "Links",
            ComponentFieldPath::from_fields([name.clone()]),
            ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::EntityRef,
            )),
        )
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::required(
            ComponentFieldId::new("ordinary"),
            "Ordinary",
            ComponentFieldPath::from_fields(["ordinary"]),
            ComponentValueKind::String,
        ),
    ]);
    let persistent = EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.reference-plan").unwrap(),
            PersistentRuntimeId::parse_str("31415926-5358-4979-8323-846264338327").unwrap(),
        ),
    };
    let mut links = (0..32)
        .map(|_| ComponentValue::EntityReference(local("target")))
        .collect::<Vec<_>>();
    links.push(ComponentValue::EntityReference(persistent));
    links.push(ComponentValue::Null);
    let value = ComponentValue::map([
        (name, ComponentValue::List(links)),
        (
            "ordinary".to_owned(),
            ComponentValue::String("target".to_owned()),
        ),
    ]);
    validate_component_value(&schema, &value).unwrap();
    (schema, value)
}

#[test]
fn long_paths_and_multiple_references_respect_the_callers_aggregate_allocation_budget() {
    let (schema, value) = fixture();
    let original = value.clone();
    let maximum = 64 * 1024;
    let mut admitted = 0usize;
    let mut denied_at = None;
    let result = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_, _, allocation| {
            let attempted = admitted + allocation;
            if attempted > maximum {
                denied_at = Some(attempted);
                return Err(AdmissionDenied { maximum, attempted });
            }
            admitted = attempted;
            Ok(())
        },
    );
    assert!(admitted <= maximum);
    let attempted = denied_at.expect("long reference paths exceed the aggregate allowance");
    assert_eq!(
        result,
        Err(EntityReferencePlanError::Admission(AdmissionDenied {
            maximum,
            attempted,
        })),
    );
    assert_eq!(value, original);
}

#[test]
fn admitted_plan_matches_existing_slots_and_preserves_typed_rewrite_semantics() {
    let (schema, value) = fixture();
    let mut allocated = 0usize;
    let plan = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_, _, allocation| {
            allocated += allocation;
            Ok::<_, AdmissionDenied>(())
        },
    )
    .unwrap();
    assert_eq!(
        plan,
        plan_declared_entity_references::<()>(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default(),
        )
        .unwrap(),
    );
    assert!(allocated > 64 * 1024);
    let mut slots = 0u32;
    plan.visit(&value, |path, _| {
        assert_eq!(
            path,
            &ComponentFieldPath::new([
                schema.fields()[0].path().segments()[0].clone(),
                ComponentFieldPathSegment::index(slots),
            ]),
        );
        assert_eq!(plan.root_field_id(path), Some(schema.fields()[0].id()));
        slots += 1;
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(slots, 33);
    let mut rewritten = value.clone();
    plan.rewrite_in_place(&mut rewritten, |_, reference| {
        Ok::<_, ()>(match reference {
            EntityReference::SceneLocal { .. } => local("localized"),
            EntityReference::Persistent { .. } => reference.clone(),
        })
    })
    .unwrap();
    plan.visit(&value, |path, reference| {
        let expected = match reference {
            EntityReference::SceneLocal { .. } => local("localized"),
            EntityReference::Persistent { .. } => reference.clone(),
        };
        assert_eq!(
            rewritten.get_path(path).unwrap().as_entity_reference(),
            Some(&expected),
        );
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(rewritten.get("ordinary"), value.get("ordinary"));
    let nullable = ComponentFieldPath::new([
        schema.fields()[0].path().segments()[0].clone(),
        ComponentFieldPathSegment::index(slots),
    ]);
    assert_eq!(
        rewritten.get_path(&nullable).unwrap(),
        &ComponentValue::Null
    );
}

#[test]
fn final_output_capacity_and_field_id_copies_are_admitted_before_returning_a_plan() {
    let (schema, value) = fixture();
    let mut allocations = Vec::new();
    let plan = plan_declared_entity_references_with_admission::<(), _>(
        &schema,
        &value,
        EntityReferenceTraversalLimits::default(),
        |_, _, allocation| {
            if allocation != 0 {
                allocations.push(allocation);
            }
            Ok::<_, AdmissionDenied>(())
        },
    )
    .unwrap();
    let mut slots = 0usize;
    plan.visit(&value, |_, _| {
        slots += 1;
        Ok::<_, ()>(())
    })
    .unwrap();
    let capacity_index = allocations.len() - slots - 1;
    assert!(allocations[capacity_index] > slots * schema.fields()[0].id().as_str().len());
    assert!(
        allocations[capacity_index + 1..]
            .iter()
            .all(|bytes| *bytes == schema.fields()[0].id().as_str().len()),
    );
    for stop in [capacity_index, capacity_index + 1] {
        let maximum = allocations[..stop].iter().sum();
        let mut admitted = 0usize;
        let result = plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default(),
            |_, _, allocation| {
                let attempted = admitted + allocation;
                if attempted > maximum {
                    return Err(AdmissionDenied { maximum, attempted });
                }
                admitted = attempted;
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(EntityReferencePlanError::Admission(AdmissionDenied {
                maximum,
                attempted: maximum + allocations[stop],
            })),
        );
    }
}

#[test]
fn values_without_entity_refs_still_admit_structural_scan_and_temporary_allocations() {
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.ReferencePlanScalars"),
        "Reference plan scalars",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("values"),
        "Values",
        ComponentFieldPath::from_fields(["values"]),
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
    )]);
    let value = ComponentValue::map([(
        "values",
        ComponentValue::List(vec![ComponentValue::U64(7); 32]),
    )]);
    validate_component_value(&schema, &value).unwrap();
    for allocation_budget in [false, true] {
        let maximum = usize::from(!allocation_budget);
        let mut admitted = 0usize;
        let mut denied_at = None;
        let result = plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default(),
            |nodes, _, allocation| {
                let attempted = admitted + if allocation_budget { allocation } else { nodes };
                if attempted > maximum {
                    denied_at = Some(attempted);
                    return Err(AdmissionDenied { maximum, attempted });
                }
                admitted = attempted;
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(EntityReferencePlanError::Admission(AdmissionDenied {
                maximum,
                attempted: denied_at.expect("empty plans still perform admitted work"),
            })),
        );
    }
}

#[test]
fn reference_failures_stay_distinct_from_admission_failures() {
    let (schema, mut value) = fixture();
    value
        .set_path(
            &ComponentFieldPath::from_fields(["ordinary"]),
            ComponentValue::U64(7),
        )
        .unwrap();
    assert!(matches!(
        plan_declared_entity_references_with_admission::<(), _>(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default(),
            |_, _, _| Ok::<_, AdmissionDenied>(()),
        ),
        Err(EntityReferencePlanError::Reference(
            ComponentEntityReferenceRewriteError::InvalidReferenceValue {
                actual: ComponentValueKind::U64,
                ..
            },
        )),
    ));
}
