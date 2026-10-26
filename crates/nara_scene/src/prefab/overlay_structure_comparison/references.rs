//! Exact payload-slot binding for the private overlay comparison.

use std::collections::BTreeMap;

use nara_identity::EntityReference;
use nara_reflect::__private::{
    DeclaredEntityReferencePlan, plan_declared_entity_references,
    plan_declared_value_entity_references,
};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathSegment, ComponentFieldSchema, ComponentSchema,
    ComponentValue, ComponentValueShape, EntityReferenceTraversalLimits,
};
use serde::{Deserialize, Serialize};

use super::{Conflict, SceneEntityId, Target, TargetMap};

const MAX_REFERENCE_BINDINGS: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReferenceBinding {
    // This is a coordinate in the exact version-bound payload, not a durable selector.
    pub(super) slot: ComponentFieldPath,
    pub(super) expected: SceneEntityId,
    pub(super) target: Target,
}

pub(super) fn bind_references(
    schema: &ComponentSchema,
    value: &ComponentValue,
    bindings: &[ReferenceBinding],
    targets: &TargetMap,
) -> Result<ComponentValue, Conflict> {
    let limits = EntityReferenceTraversalLimits::default();
    let plan = plan_declared_entity_references::<Conflict>(schema, value, limits)
        .map_err(|_| Conflict::Reference)?;
    bind_plan(plan, value, bindings, targets, limits)
}

pub(super) fn bind_property_references(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    payload: &ComponentValue,
    bindings: &[ReferenceBinding],
    targets: &TargetMap,
) -> Result<ComponentValue, Conflict> {
    let limits = EntityReferenceTraversalLimits::default();
    let plan = plan_declared_value_entity_references::<Conflict>(field, shape, payload, limits)
        .map_err(|_| Conflict::Reference)?;
    bind_plan(plan, payload, bindings, targets, limits)
}

fn bind_plan(
    plan: DeclaredEntityReferencePlan,
    value: &ComponentValue,
    bindings: &[ReferenceBinding],
    targets: &TargetMap,
    limits: EntityReferenceTraversalLimits,
) -> Result<ComponentValue, Conflict> {
    if bindings.len() > MAX_REFERENCE_BINDINGS {
        return Err(Conflict::Reference);
    }
    let mut pending = BTreeMap::new();
    let mut binding_bytes = 0usize;
    for binding in bindings {
        if binding.slot.segments().len() > limits.depth().get() {
            return Err(Conflict::Reference);
        }
        binding_bytes = binding_bytes.saturating_add(binding.expected.as_str().len());
        for segment in binding.slot.segments() {
            binding_bytes = binding_bytes.saturating_add(match segment {
                ComponentFieldPathSegment::Field(name) => name.len(),
                ComponentFieldPathSegment::Index(_) => std::mem::size_of::<u32>(),
            });
        }
        binding_bytes = binding_bytes.saturating_add(
            binding
                .target
                .logical_bytes()
                .map_err(|_| Conflict::Reference)?,
        );
        if binding_bytes > limits.bytes().get() || pending.insert(&binding.slot, binding).is_some()
        {
            return Err(Conflict::Reference);
        }
    }

    let mut resolved = BTreeMap::new();
    let mut original_reference_bytes = 0usize;
    let mut resolved_reference_bytes = 0usize;
    plan.visit(value, |slot, reference| {
        if let EntityReference::SceneLocal { entity } = reference {
            let binding = pending.remove(slot).ok_or(Conflict::Reference)?;
            if entity != &binding.expected {
                return Err(Conflict::Reference);
            }
            let target = targets.get(&binding.target).ok_or(Conflict::Reference)?;
            original_reference_bytes =
                original_reference_bytes.saturating_add(entity.as_str().len());
            resolved_reference_bytes =
                resolved_reference_bytes.saturating_add(target.as_str().len());
            resolved.insert(slot.clone(), target);
        }
        Ok::<_, Conflict>(())
    })
    .map_err(|_| Conflict::Reference)?;
    // Null, Persistent, asset and non-reference slots cannot consume a binding.
    if !pending.is_empty() {
        return Err(Conflict::Reference);
    }
    let candidate_bytes = value
        .cost()
        .logical_bytes()
        .checked_sub(original_reference_bytes)
        .and_then(|bytes| bytes.checked_add(resolved_reference_bytes))
        .ok_or(Conflict::Reference)?;
    if candidate_bytes > limits.bytes().get() {
        return Err(Conflict::Reference);
    }

    let mut candidate = value.clone();
    plan.rewrite_in_place(&mut candidate, |slot, reference| {
        Ok::<_, Conflict>(match reference {
            EntityReference::SceneLocal { .. } => EntityReference::SceneLocal {
                entity: (*resolved.get(slot).ok_or(Conflict::Reference)?).clone(),
            },
            EntityReference::Persistent { .. } => reference.clone(),
        })
    })
    .map_err(|_| Conflict::Reference)?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use nara_asset::AssetRef;
    use nara_identity::{
        PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
    };
    use nara_reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldSchema, ComponentSchemaVersion,
        ComponentShapeMember, ComponentTypeId, ComponentValueKind, ComponentValueShape,
        collect_declared_asset_references_from_schema,
    };

    use super::super::{EntityOrigin, InstanceHop};
    use super::*;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn local(value: &str) -> ComponentValue {
        ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(value) })
    }

    fn source(name: &str) -> crate::SourceDocumentKey {
        crate::SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            name,
            crate::SourceDocumentKind::Prefab,
        )
        .unwrap()
    }

    fn inherited(anchor: &str) -> Target {
        Target {
            through: vec![InstanceHop {
                anchor: EntityOrigin::Source(id(anchor)),
                expected_base: source("inner"),
            }],
            entity: EntityOrigin::Source(id("socket")),
        }
    }

    fn schema(shape: ComponentValueShape) -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new("nara.test.OverlayReferences"),
            "Overlay references",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
        .with_capabilities([
            ComponentCapability::EntityRef,
            ComponentCapability::AssetRef,
        ])])
    }

    fn list_value() -> ComponentValue {
        ComponentValue::map([(
            "links",
            ComponentValue::List(vec![local("socket"), local("socket")]),
        )])
    }

    fn slot(index: u32) -> ComponentFieldPath {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("links"),
            ComponentFieldPathSegment::index(index),
        ])
    }

    fn binding(slot: ComponentFieldPath, target: Target) -> ReferenceBinding {
        ReferenceBinding {
            slot,
            expected: id("socket"),
            target,
        }
    }

    #[test]
    fn equal_local_spelling_binds_inherited_and_added_targets_per_slot() {
        let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let value = list_value();
        let targets = TargetMap::from([
            (inherited("left"), id("inherited-output")),
            (Target::added(id("socket")), id("added-output")),
        ]);
        let bindings = vec![
            binding(slot(0), inherited("left")),
            binding(slot(1), Target::added(id("socket"))),
        ];
        let wire = serde_json::to_vec(&bindings).unwrap();
        let reopened: Vec<ReferenceBinding> = serde_json::from_slice(&wire).unwrap();
        assert_eq!(reopened, bindings);

        let bound = bind_references(&schema, &value, &reopened, &targets).unwrap();

        assert_eq!(
            bound.get_path(&slot(0)).unwrap(),
            &local("inherited-output")
        );
        assert_eq!(bound.get_path(&slot(1)).unwrap(), &local("added-output"));
        assert_eq!(value, list_value());
    }

    #[test]
    fn repeated_nested_sources_require_the_complete_target_chain() {
        let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let targets = TargetMap::from([
            (inherited("left"), id("left-output")),
            (inherited("right"), id("right-output")),
        ]);
        let bindings = [
            binding(slot(0), inherited("left")),
            binding(slot(1), inherited("right")),
        ];

        let bound = bind_references(&schema, &list_value(), &bindings, &targets).unwrap();

        assert_eq!(bound.get_path(&slot(0)).unwrap(), &local("left-output"));
        assert_eq!(bound.get_path(&slot(1)).unwrap(), &local("right-output"));
        let mut rebound = targets.clone();
        let mut changed = inherited("right");
        changed.through[0].expected_base = source("replacement-inner");
        rebound.remove(&inherited("right"));
        rebound.insert(changed, id("right-output"));
        assert_eq!(
            bind_references(&schema, &list_value(), &bindings, &rebound),
            Err(Conflict::Reference),
        );
    }

    #[test]
    fn incomplete_duplicate_stray_stale_and_unresolved_bindings_are_atomic_conflicts() {
        let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let value = list_value();
        let targets = TargetMap::from([(Target::added(id("socket")), id("output"))]);
        let first = binding(slot(0), Target::added(id("socket")));
        let second = binding(slot(1), Target::added(id("socket")));
        let mut stale = second.clone();
        stale.expected = id("previous-local");
        let cases = [
            vec![],
            vec![first.clone()],
            vec![first.clone(), first.clone(), second.clone()],
            vec![
                first.clone(),
                second.clone(),
                binding(slot(2), Target::added(id("socket"))),
            ],
            vec![first.clone(), stale],
            vec![first, binding(slot(1), Target::added(id("missing")))],
        ];
        for bindings in cases {
            let original = bindings.clone();
            assert_eq!(
                bind_references(&schema, &value, &bindings, &targets),
                Err(Conflict::Reference),
            );
            let reopened: Vec<ReferenceBinding> =
                serde_json::from_slice(&serde_json::to_vec(&bindings).unwrap()).unwrap();
            assert_eq!(reopened, original);
            assert_eq!(value, list_value());
            assert_eq!(targets[&Target::added(id("socket"))], id("output"));
        }
    }

    #[test]
    fn nested_container_replacement_replans_slots_and_preserves_typed_external_references() {
        let schema = schema(ComponentValueShape::keyed_list(
            ComponentValueShape::record_members([
                ComponentShapeMember::new("id", ComponentValueKind::String)
                    .with_id(ComponentFieldId::new("row-id")),
                ComponentShapeMember::new(
                    "target",
                    ComponentValueShape::optional(ComponentValueShape::EntityRef),
                ),
                ComponentShapeMember::new(
                    "others",
                    ComponentValueShape::map(ComponentValueShape::EntityRef),
                ),
                ComponentShapeMember::new("portrait", ComponentValueShape::AssetRef),
            ]),
            ComponentFieldId::new("row-id"),
        ));
        let stable = AssetRef::stable_id("23456789-2345-4345-8345-234567890abc").unwrap();
        let portrait = ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".into())),
            (
                "value",
                ComponentValue::String(stable.as_stable_id().unwrap().to_string()),
            ),
        ]);
        let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
            entity: PersistentRuntimeReference::new(
                PersistentRuntimeNamespaceId::new("overlay.fixture").unwrap(),
                PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
            ),
        });
        let first = ComponentValue::map([
            ("id", ComponentValue::String("a".into())),
            ("target", persistent.clone()),
            (
                "others",
                ComponentValue::map([("name.with.dot", local("socket"))]),
            ),
            ("portrait", portrait.clone()),
        ]);
        let second = ComponentValue::map([
            ("id", ComponentValue::String("b".into())),
            ("target", ComponentValue::Null),
            (
                "others",
                ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ),
            ("portrait", portrait.clone()),
        ]);
        let value = ComponentValue::map([(
            "links",
            ComponentValue::List(vec![first.clone(), second.clone()]),
        )]);
        let nested_slot = |index, fields: &[&str]| {
            let mut segments = slot(index).into_segments();
            segments.extend(
                fields
                    .iter()
                    .map(|field| ComponentFieldPathSegment::field(*field)),
            );
            ComponentFieldPath::new(segments)
        };
        let targets = TargetMap::from([(Target::added(id("socket")), id("output"))]);
        let valid = binding(
            nested_slot(0, &["others", "name.with.dot"]),
            Target::added(id("socket")),
        );
        let bound =
            bind_references(&schema, &value, std::slice::from_ref(&valid), &targets).unwrap();
        assert_eq!(
            bound
                .get_path(&nested_slot(0, &["others", "name.with.dot"]))
                .unwrap(),
            &local("output"),
        );
        assert_eq!(
            bound.get_path(&nested_slot(0, &["target"])).unwrap(),
            &persistent,
        );
        assert_eq!(
            bound.get_path(&nested_slot(1, &["target"])).unwrap(),
            &ComponentValue::Null,
        );
        assert_eq!(
            bound.get_path(&nested_slot(0, &["portrait"])).unwrap(),
            &portrait,
        );
        let assets =
            collect_declared_asset_references_from_schema(&schema, schema.version(), &bound)
                .unwrap();
        assert_eq!(assets.len(), 2);
        assert!(
            assets
                .iter()
                .all(|reference| reference.asset_ref() == &stable)
        );
        for forbidden in [
            nested_slot(0, &["target"]),
            nested_slot(1, &["target"]),
            nested_slot(0, &["portrait"]),
            nested_slot(0, &["id"]),
        ] {
            assert_eq!(
                bind_references(
                    &schema,
                    &value,
                    &[
                        valid.clone(),
                        binding(forbidden, Target::added(id("socket")))
                    ],
                    &targets,
                ),
                Err(Conflict::Reference),
            );
        }
        let replacement =
            ComponentValue::map([("links", ComponentValue::List(vec![second, first]))]);
        assert_eq!(
            bind_references(&schema, &replacement, &[valid], &targets),
            Err(Conflict::Reference),
        );
        let rebound = bind_references(
            &schema,
            &replacement,
            &[binding(
                nested_slot(1, &["others", "name.with.dot"]),
                Target::added(id("socket")),
            )],
            &targets,
        )
        .unwrap();
        assert_eq!(
            rebound
                .get_path(&nested_slot(1, &["others", "name.with.dot"]))
                .unwrap(),
            &local("output"),
        );
    }

    #[test]
    fn unsupported_reference_shapes_and_bounded_work_return_reference_conflicts() {
        let targets = TargetMap::new();
        let opaque = schema(ComponentValueShape::from(ComponentValueKind::Map));
        let hidden =
            ComponentValue::map([("links", ComponentValue::map([("hidden", local("socket"))]))]);
        assert_eq!(
            bind_references(&opaque, &hidden, &[], &targets),
            Err(Conflict::Reference),
        );
        let required = schema(ComponentValueShape::EntityRef);
        for invalid in [
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
            ComponentValue::map([("links", ComponentValue::Null)]),
            ComponentValue::map([("links", ComponentValue::String("socket".into()))]),
        ] {
            assert_eq!(
                bind_references(&required, &invalid, &[], &targets),
                Err(Conflict::Reference),
            );
        }
        let list = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
        let targets = TargetMap::from([(Target::added(id("socket")), id("output"))]);
        let excessive = (0..=MAX_REFERENCE_BINDINGS)
            .map(|index| {
                binding(
                    slot(u32::try_from(index).unwrap()),
                    Target::added(id("socket")),
                )
            })
            .collect::<Vec<_>>();
        let excessive_value = ComponentValue::map([(
            "links",
            ComponentValue::List(vec![local("socket"); MAX_REFERENCE_BINDINGS + 1]),
        )]);
        assert_eq!(
            bind_references(&list, &excessive_value, &excessive, &targets),
            Err(Conflict::Reference),
        );
        let nullable_list = schema(ComponentValueShape::list(ComponentValueShape::optional(
            ComponentValueShape::EntityRef,
        )));
        let limit = EntityReferenceTraversalLimits::default().nodes().get();
        let oversized = ComponentValue::map([(
            "links",
            ComponentValue::List(vec![ComponentValue::Null; limit]),
        )]);
        assert_eq!(
            bind_references(&nullable_list, &oversized, &[], &targets),
            Err(Conflict::Reference),
        );
        let targets = TargetMap::from([(
            Target::added(id("socket")),
            id(&"x".repeat(EntityReferenceTraversalLimits::default().bytes().get() + 1)),
        )]);
        let bindings = [
            binding(slot(0), Target::added(id("socket"))),
            binding(slot(1), Target::added(id("socket"))),
        ];
        assert_eq!(
            bind_references(&list, &list_value(), &bindings, &targets),
            Err(Conflict::Reference),
        );
    }
}
