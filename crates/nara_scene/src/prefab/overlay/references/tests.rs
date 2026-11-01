use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPathSegment, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValueKind, effective_component_value,
};

use crate::{PrefabEntityOrigin, PrefabInstanceHop, SceneEntityId};

use super::*;

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn local(name: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(name) })
}

fn field(shape: ComponentValueShape) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("link"),
        "Link",
        ComponentFieldPath::from_fields(["wire_link"]),
        shape,
    )
    .with_capabilities([ComponentCapability::EntityRef])
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("game.BindingGrammar"),
        "Binding Grammar",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field(shape)])
}

fn binding(slot: ComponentFieldPath) -> PrefabReferenceBinding {
    PrefabReferenceBinding {
        slot,
        expected: id("socket"),
        target: PrefabEntityTarget::source(id("unresolved-target")),
    }
}

fn nested_target(anchor: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id(anchor)),
            expected_base: crate::tests::named_prefab_source_key("unresolved-base"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id("socket")),
    }
}

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes).unwrap(),
        ByteLimit::new(bytes).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
}

#[test]
fn component_plan_preserves_exact_paths_and_complete_authored_targets() {
    let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
    let value = ComponentValue::map([(
        "wire_link",
        ComponentValue::List(vec![local("socket"), local("socket"), local("socket")]),
    )]);
    let slot = |index| {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("wire_link"),
            ComponentFieldPathSegment::index(index),
        ])
    };
    let mut bindings = vec![binding(slot(2)), binding(slot(0)), binding(slot(1))];
    bindings[0].target = PrefabEntityTarget::addition(id("socket"));
    bindings[1].target = nested_target("left");
    bindings[2].target = nested_target("right");
    let expected = bindings
        .iter()
        .map(|binding| (binding.slot.clone(), binding.target.clone()))
        .collect();

    assert_eq!(
        plan_bindings(&schema, &value, &bindings, Default::default()),
        Ok(expected)
    );
    for index in 0..3 {
        assert_eq!(value.get_path(&slot(index)).unwrap(), &local("socket"));
    }
}

#[test]
fn scalar_property_plan_uses_only_the_empty_payload_path_and_selected_shape() {
    let field = field(ComponentValueShape::list(ComponentValueShape::EntityRef));
    let shape = ComponentValueShape::EntityRef;
    let payload = local("socket");
    let mut binding = binding(ComponentFieldPath::empty());
    binding.target = nested_target("unresolved-anchor");
    binding.target.entity = PrefabEntityOrigin::SourceEntity(id(&"x".repeat(512)));
    let exact = limits(1, "socket".len(), 1);
    assert_eq!(
        plan_property_bindings(
            &field,
            &shape,
            &payload,
            std::slice::from_ref(&binding),
            exact,
        ),
        Ok(BTreeMap::from([(
            ComponentFieldPath::empty(),
            binding.target.clone(),
        )]))
    );
    // Only the empty path addresses the scalar payload, regardless of owning field identity.
    for slot in [
        field.path().clone(),
        ComponentFieldPath::from_fields(["link"]),
    ] {
        binding.slot = slot;
        assert_eq!(
            plan_property_bindings(&field, &shape, &payload, &[binding.clone()], exact),
            Err(PrefabExpansionFailureKind::Reference)
        );
    }
}

#[test]
fn component_local_slots_reject_missing_stale_duplicate_and_orphan_bindings() {
    let schema = schema(ComponentValueShape::EntityRef);
    let value = ComponentValue::map([("wire_link", local("socket"))]);
    let valid = binding(ComponentFieldPath::from_fields(["wire_link"]));
    let mut stale = valid.clone();
    stale.expected = id("old-socket");
    let mut duplicate = valid.clone();
    duplicate.target = PrefabEntityTarget::addition(id("different-target"));
    let orphan = binding(ComponentFieldPath::from_fields(["unused"]));
    for bindings in [
        vec![],
        vec![stale],
        vec![valid.clone(), duplicate],
        vec![orphan.clone()],
        vec![valid, orphan],
    ] {
        assert_eq!(
            plan_bindings(&schema, &value, &bindings, Default::default()),
            Err(PrefabExpansionFailureKind::Reference)
        );
    }
}

#[test]
fn property_local_slots_reject_missing_stale_duplicate_and_orphan_bindings() {
    let field = field(ComponentValueShape::EntityRef);
    let payload = local("socket");
    let valid = binding(ComponentFieldPath::empty());
    let mut stale = valid.clone();
    stale.expected = id("old-socket");
    let orphan = binding(ComponentFieldPath::from_fields(["unused"]));
    for bindings in [
        vec![],
        vec![stale],
        vec![valid.clone(), valid.clone()],
        vec![valid, orphan],
    ] {
        assert_eq!(
            plan_property_bindings(
                &field,
                field.shape(),
                &payload,
                &bindings,
                Default::default(),
            ),
            Err(PrefabExpansionFailureKind::Reference)
        );
    }
}

#[test]
fn persistent_null_asset_and_scalar_leaves_never_consume_local_bindings() {
    let shape = ComponentValueShape::record_members([
        ComponentShapeMember::new("persistent", ComponentValueShape::EntityRef),
        ComponentShapeMember::new(
            "nullable",
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
        ),
        ComponentShapeMember::new("asset", ComponentValueKind::AssetRef),
        ComponentShapeMember::new("number", ComponentValueKind::U64),
    ]);
    let field = field(shape).with_capabilities([
        ComponentCapability::EntityRef,
        ComponentCapability::AssetRef,
    ]);
    let payload = ComponentValue::map([
        (
            "persistent",
            ComponentValue::EntityReference(EntityReference::Persistent {
                entity: PersistentRuntimeReference::new(
                    PersistentRuntimeNamespaceId::new("binding.grammar").unwrap(),
                    PersistentRuntimeId::parse_str("34567890-3456-4456-8456-345678901abc").unwrap(),
                ),
            }),
        ),
        ("nullable", ComponentValue::Null),
        (
            "asset",
            ComponentValue::map([
                ("kind", ComponentValue::String("path".into())),
                ("value", ComponentValue::String("portrait.png".into())),
            ]),
        ),
        ("number", ComponentValue::U64(1)),
    ]);
    assert_eq!(
        plan_property_bindings(&field, field.shape(), &payload, &[], Default::default()),
        Ok(BTreeMap::new())
    );
    for name in ["persistent", "nullable", "asset", "number"] {
        let extra = binding(ComponentFieldPath::from_fields([name]));
        assert_eq!(
            plan_property_bindings(
                &field,
                field.shape(),
                &payload,
                &[extra],
                Default::default(),
            ),
            Err(PrefabExpansionFailureKind::Reference)
        );
    }
}

#[test]
fn invalid_reference_shapes_map_to_reference_failure() {
    for (shape, payload) in [
        (ComponentValueShape::EntityRef, ComponentValue::Null),
        (ComponentValueShape::EntityRef, ComponentValue::U64(1)),
        (
            ComponentValueShape::list(ComponentValueShape::EntityRef),
            local("socket"),
        ),
    ] {
        let field = field(shape);
        assert_eq!(
            plan_property_bindings(&field, field.shape(), &payload, &[], Default::default()),
            Err(PrefabExpansionFailureKind::Reference)
        );
        let schema = schema(field.shape().clone());
        assert_eq!(
            plan_bindings(
                &schema,
                &ComponentValue::map([("wire_link", payload)]),
                &[],
                Default::default(),
            ),
            Err(PrefabExpansionFailureKind::Reference)
        );
    }
}

#[test]
fn undeclared_and_unmarked_entity_references_are_rejected() {
    let unmarked = ComponentFieldSchema::required(
        ComponentFieldId::new("link"),
        "Link",
        ComponentFieldPath::from_fields(["wire_link"]),
        ComponentValueShape::EntityRef,
    );
    assert_eq!(
        plan_property_bindings(
            &unmarked,
            unmarked.shape(),
            &local("socket"),
            &[binding(ComponentFieldPath::empty())],
            Default::default(),
        ),
        Err(PrefabExpansionFailureKind::Reference)
    );
    let schema = schema(ComponentValueShape::EntityRef);
    assert_eq!(
        plan_bindings(
            &schema,
            &ComponentValue::map([
                ("wire_link", local("socket")),
                ("undeclared", local("socket")),
            ]),
            &[binding(ComponentFieldPath::from_fields(["wire_link"]))],
            Default::default(),
        ),
        Err(PrefabExpansionFailureKind::Reference)
    );
}

#[test]
fn payload_depth_keeps_its_component_and_property_roots() {
    let shape =
        ComponentValueShape::list(ComponentValueShape::list(ComponentValueShape::EntityRef));
    let payload = ComponentValue::List(vec![ComponentValue::List(vec![local("socket")])]);
    let relative = ComponentFieldPath::new([
        ComponentFieldPathSegment::index(0),
        ComponentFieldPathSegment::index(0),
    ]);
    let field = field(shape.clone());
    let property_binding = binding(relative.clone());
    let plan_property = |depth| {
        plan_property_bindings(
            &field,
            field.shape(),
            &payload,
            std::slice::from_ref(&property_binding),
            limits(8, 128, depth),
        )
    };
    assert_eq!(
        plan_property(3),
        Ok(BTreeMap::from([(
            relative.clone(),
            property_binding.target.clone(),
        )]))
    );
    assert_eq!(plan_property(2), Err(PrefabExpansionFailureKind::WorkLimit));
    let schema = schema(shape);
    let slot = ComponentFieldPath::new(
        field
            .path()
            .segments()
            .iter()
            .chain(relative.segments())
            .cloned(),
    );
    let value = ComponentValue::map([("wire_link", payload)]);
    let component_binding = binding(slot.clone());
    let plan_component = |depth| {
        plan_bindings(
            &schema,
            &value,
            std::slice::from_ref(&component_binding),
            limits(8, 128, depth),
        )
    };
    assert_eq!(
        plan_component(4),
        Ok(BTreeMap::from([(slot, component_binding.target.clone())]))
    );
    assert_eq!(
        plan_component(3),
        Err(PrefabExpansionFailureKind::WorkLimit)
    );
}

#[test]
fn binding_slot_and_target_hop_depths_remain_bounded() {
    let field = field(ComponentValueShape::EntityRef);
    let excessive_slot = binding(ComponentFieldPath::from_fields(["one", "two"]));
    let mut excessive_target = binding(ComponentFieldPath::empty());
    excessive_target.target = nested_target("one");
    let hop = excessive_target.target.through[0].clone();
    excessive_target.target.through.push(hop);
    for binding in [excessive_slot, excessive_target] {
        assert_eq!(
            plan_property_bindings(
                &field,
                field.shape(),
                &local("socket"),
                &[binding],
                limits(8, 128, 1),
            ),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
    }
}

#[test]
fn property_payload_obeys_exact_node_and_byte_limits() {
    let field = field(ComponentValueShape::list(ComponentValueShape::EntityRef));
    let payload = ComponentValue::List(vec![local("socket"), local("socket")]);
    let bindings = [
        binding(ComponentFieldPath::new([ComponentFieldPathSegment::index(
            0,
        )])),
        binding(ComponentFieldPath::new([ComponentFieldPathSegment::index(
            1,
        )])),
    ];
    let expected = bindings
        .iter()
        .map(|binding| (binding.slot.clone(), binding.target.clone()))
        .collect();
    assert_eq!(
        plan_property_bindings(&field, field.shape(), &payload, &bindings, limits(3, 12, 2)),
        Ok(expected)
    );
    for limited in [limits(2, 12, 2), limits(3, 11, 2)] {
        assert_eq!(
            plan_property_bindings(&field, field.shape(), &payload, &bindings, limited),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
    }
}

#[test]
fn component_payload_obeys_exact_node_and_byte_limits() {
    let schema = schema(ComponentValueShape::list(ComponentValueShape::EntityRef));
    let value = ComponentValue::map([(
        "wire_link",
        ComponentValue::List(vec![local("socket"), local("socket")]),
    )]);
    let bindings = [0, 1].map(|index| {
        binding(ComponentFieldPath::new([
            ComponentFieldPathSegment::field("wire_link"),
            ComponentFieldPathSegment::index(index),
        ]))
    });
    let expected = bindings
        .iter()
        .map(|binding| (binding.slot.clone(), binding.target.clone()))
        .collect();
    assert_eq!(
        plan_bindings(&schema, &value, &bindings, limits(4, 21, 3)),
        Ok(expected)
    );
    for limited in [limits(3, 21, 3), limits(4, 20, 3)] {
        assert_eq!(
            plan_bindings(&schema, &value, &bindings, limited),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
    }
}

#[test]
fn overflowing_traversal_allowances_refuse_work() {
    let field = field(ComponentValueShape::EntityRef);
    for limited in [
        limits(usize::MAX, 128, 1),
        limits(8, usize::MAX, 1),
        limits(8, 128, usize::MAX),
        limits(1, usize::MAX / 3, 1),
    ] {
        assert_eq!(
            plan_property_bindings(
                &field,
                field.shape(),
                &local("socket"),
                &[binding(ComponentFieldPath::empty())],
                limited,
            ),
            Err(PrefabExpansionFailureKind::WorkLimit)
        );
    }
}

#[test]
fn default_reference_materialization_remains_the_callers_responsibility() {
    let field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("link"),
        "Link",
        ComponentFieldPath::from_fields(["wire_link"]),
        ComponentValueShape::EntityRef,
        local("socket"),
    )
    .with_capabilities([ComponentCapability::EntityRef]);
    let schema = ComponentSchema::new(
        ComponentTypeId::new("game.DefaultBindingGrammar"),
        "Default Binding Grammar",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([field]);
    let raw = ComponentValue::map([] as [(&str, ComponentValue); 0]);
    let binding = binding(ComponentFieldPath::from_fields(["wire_link"]));
    assert_eq!(
        plan_bindings(&schema, &raw, &[], Default::default()),
        Ok(BTreeMap::new())
    );
    assert_eq!(
        plan_bindings(
            &schema,
            &raw,
            std::slice::from_ref(&binding),
            Default::default(),
        ),
        Err(PrefabExpansionFailureKind::Reference)
    );
    let effective = effective_component_value(&schema, &raw, Default::default()).unwrap();
    assert_eq!(
        plan_bindings(
            &schema,
            &effective,
            std::slice::from_ref(&binding),
            Default::default(),
        ),
        Ok(BTreeMap::from([(
            binding.slot.clone(),
            binding.target.clone(),
        )]))
    );
}
