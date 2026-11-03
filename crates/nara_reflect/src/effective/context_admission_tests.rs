use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId, SourceDocumentKey, SourceDocumentKind,
};

#[test]
fn long_authored_default_refusal_preserves_node_budget_and_error_identity() {
    let base = SourceDocumentKey::named(
        "d632eb19-1ca0-4a54-8eb4-b652fdac811c".parse().unwrap(),
        "default-route",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let reference = AuthoredEntityReference {
        through: vec![
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("slot").unwrap()),
                expected_base: base,
            };
            4_096
        ],
        target: AuthoredReferenceTarget::Input(ReferenceInputId::new("u").unwrap()),
    };
    let expected_extra = reference.cost_nodes() - 1;
    let field = field(
        ComponentValueShape::EntityRef,
        Some(ComponentValue::EntityReference(EntityReference::Authored {
            reference,
        })),
    );
    let marker = Box::new(std::io::Error::other("route scan refused"));
    let pointer = std::ptr::from_ref(marker.as_ref());
    let mut marker = Some(marker);
    let mut nodes = 0usize;
    let mut refused = None;
    let result =
        component_field_default_requires_context_with_admission(&field, |delta, bytes, _| {
            assert_eq!(
                bytes, 0,
                "route bytes cannot be observed before its node gate"
            );
            if delta > 1 {
                refused = Some(delta);
                return Err(marker.take().unwrap());
            }
            nodes += delta;
            Ok(())
        });
    assert!(nodes > 0 && nodes < expected_extra); // Shape wrappers plus the raw root.
    assert_eq!(refused, Some(expected_extra));
    let Err(EffectiveValueAdmissionError::Admission(error)) = result else {
        panic!("the original caller refusal must stop the authored route scan");
    };
    assert!(std::ptr::eq(error.as_ref(), pointer));
}

use super::*;
use crate::{
    ComponentCapability, ComponentFieldPath, ComponentSchemaVersion, ComponentTypeId,
    ComponentValueKind, ComponentValueShape,
};

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    }
}

fn local() -> EntityReference {
    EntityReference::SceneLocal {
        entity: SceneEntityId::new("socket").unwrap(),
    }
}

fn authored() -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: Vec::new(),
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new("socket").unwrap()),
        },
    }
}

fn field(shape: ComponentValueShape, default: Option<ComponentValue>) -> ComponentFieldSchema {
    let mut field = ComponentFieldSchema::optional(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    field.default_value = default;
    field
}

fn nested_field(references: Vec<EntityReference>) -> ComponentFieldSchema {
    let mut values = vec![ComponentValue::Null];
    values.extend(references.into_iter().map(ComponentValue::EntityReference));
    let mut value = ComponentValue::List(values);
    let mut shape = ComponentValueShape::list(ComponentValueShape::optional(
        ComponentValueShape::EntityRef,
    ));
    for name in ["inner", "outer"] {
        value = ComponentValue::List(vec![ComponentValue::map([(name, value)])]);
        shape = ComponentValueShape::list(ComponentValueShape::map(shape));
    }
    field(shape, Some(value))
}

#[test]
fn admitted_default_summary_matches_canonical_shape_and_reference_semantics() {
    let cases = [
        (field(ComponentValueShape::EntityRef, None), false),
        (
            field(
                ComponentValueShape::scalar(ComponentValueKind::U64),
                Some(ComponentValue::U64(7)),
            ),
            false,
        ),
        (
            field(ComponentValueShape::EntityRef, Some(ComponentValue::Null)),
            false,
        ),
        (nested_field(vec![persistent()]), false),
        (nested_field(vec![local()]), true),
        (nested_field(vec![authored()]), true),
        (
            field(
                ComponentValueShape::opaque(ComponentValueKind::List),
                Some(ComponentValue::List(vec![ComponentValue::EntityReference(
                    local(),
                )])),
            ),
            false,
        ),
    ];
    for (field, expected) in cases {
        field.shape().validate().unwrap();
        if let Some(default) = field.default_value() {
            field.shape().validate_value(default).unwrap();
        }
        let original = field.default_value().cloned();
        let mut calls = 0;
        let actual = component_field_default_requires_context_with_admission(&field, |_, _, _| {
            calls += 1;
            Ok::<(), Infallible>(())
        });
        assert_eq!(
            component_field_default_requires_context(&field),
            Ok(expected)
        );
        assert_eq!(actual, Ok(expected));
        assert_eq!(field.default_value(), original.as_ref());
        if original.is_none() {
            assert_eq!(calls, 0);
        } else {
            assert!(calls > 0);
        }
    }
}

#[test]
fn explicit_values_do_not_mask_canonical_default_context_relevance() {
    let field = nested_field(vec![local(), authored()]);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("test.ContextAdmission"),
        "Context admission",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field.clone()]);
    let original = field.default_value().cloned();
    let persistent_default = nested_field(vec![persistent()]).default_value.unwrap();
    for explicit in [ComponentValue::Null, persistent_default] {
        let source = ComponentValue::map([("payload", explicit)]);
        let before = source.clone();
        let effective = effective_component_value(
            &schema,
            &source,
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap();
        assert!(matches!(effective, Cow::Borrowed(_)));
        assert_eq!(
            component_field_default_requires_context_with_admission(&field, |_, _, _| {
                Ok::<(), Infallible>(())
            }),
            Ok(true),
        );
        assert_eq!(source, before);
        assert_eq!(field.default_value(), original.as_ref());
    }
}

#[test]
fn refusal_retains_the_exact_box_before_scanning_and_after_visiting_defaults() {
    let field = nested_field(vec![persistent(), local(), authored()]);
    let original = field.default_value().cloned();
    let mut observations = Vec::new();
    component_field_default_requires_context_with_admission(&field, |nodes, bytes, allocated| {
        observations.push((nodes, bytes, allocated));
        Ok::<(), Infallible>(())
    })
    .unwrap();
    let after_visit = observations
        .iter()
        .position(|(nodes, bytes, allocated)| *nodes == 0 && *bytes > 0 && *allocated == 0)
        .unwrap();
    assert!(after_visit > 1);
    for denied in [0, 1, after_visit, observations.len() - 1] {
        let marker = Box::new(std::io::Error::other("context admission denied"));
        let pointer = std::ptr::from_ref(marker.as_ref());
        let mut refusal = Some(marker);
        let mut calls = 0;
        let result = component_field_default_requires_context_with_admission(&field, |_, _, _| {
            let current = calls;
            calls += 1;
            if current == denied {
                Err(refusal.take().unwrap())
            } else {
                Ok(())
            }
        });
        let Err(EffectiveValueAdmissionError::Admission(actual)) = result else {
            panic!("boundary {denied} must return the original refusal");
        };
        assert!(std::ptr::eq(actual.as_ref(), pointer));
        assert_eq!(calls, denied + 1);
        assert_eq!(field.default_value(), original.as_ref());
    }
}

#[test]
fn exact_cumulative_allowance_accepts_and_each_one_short_dimension_refuses() {
    let field = nested_field(vec![local(), persistent(), authored()]);
    let original = field.default_value().cloned();
    let mut total = [0usize; 3];
    let mut scratch_growths = 0;
    assert_eq!(
        component_field_default_requires_context_with_admission(
            &field,
            |nodes, bytes, allocated| {
                for (total, delta) in total.iter_mut().zip([nodes, bytes, allocated]) {
                    *total += delta;
                }
                if nodes == 0 && bytes == 0 && allocated > 0 {
                    scratch_growths += 1;
                }
                Ok::<(), Infallible>(())
            },
        ),
        Ok(true),
    );
    assert!(
        scratch_growths >= 3,
        "exercise shape scratch and iterator stack reallocation",
    );
    for short in [None, Some(0), Some(1), Some(2)] {
        let mut remaining = total;
        if let Some(dimension) = short {
            remaining[dimension] -= 1;
        }
        let result = component_field_default_requires_context_with_admission(
            &field,
            |nodes, bytes, allocated| {
                for (remaining, delta) in remaining.iter_mut().zip([nodes, bytes, allocated]) {
                    *remaining = remaining
                        .checked_sub(delta)
                        .ok_or("cumulative allowance exhausted")?;
                }
                Ok(())
            },
        );
        if short.is_none() {
            assert_eq!(result, Ok(true));
            assert_eq!(remaining, [0; 3]);
        } else {
            assert_eq!(
                result,
                Err(EffectiveValueAdmissionError::Admission(
                    "cumulative allowance exhausted",
                )),
            );
        }
        assert_eq!(field.default_value(), original.as_ref());
    }
}
