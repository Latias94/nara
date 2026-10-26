use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_identity::SceneEntityId;
use nara_reflect::{
    __private::plan_declared_value_entity_references, ComponentCapability,
    ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentValue, ComponentValueShape, EntityReference,
    EntityReferenceTraversalLimits,
};

fn local(name: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(name).unwrap(),
    })
}

fn field() -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new("links"),
        "Links",
        ComponentFieldPath::from_fields(["nested", "links"]),
        ComponentValueShape::list(ComponentValueShape::EntityRef),
    )
    .with_capability(ComponentCapability::EntityRef)
}

#[test]
fn scalar_property_reference_is_bound_at_its_payload_root_not_the_component_field_path() {
    let field = field();
    let mut payload = local("original");
    let plan = plan_declared_value_entity_references::<()>(
        &field,
        &ComponentValueShape::EntityRef,
        &payload,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    let mut slots = Vec::new();
    plan.visit(&payload, |slot, _| {
        slots.push(slot.clone());
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(slots, [ComponentFieldPath::empty()]);
    assert_eq!(plan.root_field_id(&slots[0]), Some(field.id()));
    plan.rewrite_in_place(&mut payload, |slot, _| {
        assert_eq!(slot, &ComponentFieldPath::empty());
        Ok::<_, ()>(EntityReference::SceneLocal {
            entity: SceneEntityId::new("resolved").unwrap(),
        })
    })
    .unwrap();
    assert_eq!(payload, local("resolved"));
    let empty = plan_declared_value_entity_references::<()>(
        &field,
        &ComponentValueShape::optional(ComponentValueShape::EntityRef),
        &ComponentValue::Null,
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    let mut visits = 0;
    empty
        .visit(&ComponentValue::Null, |_, _| {
            visits += 1;
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(visits, 0);
}

#[test]
fn replacement_reference_plans_keep_root_capabilities_shape_and_work_limits() {
    let mut unmarked = field();
    unmarked.capabilities.clear();
    assert!(matches!(
        plan_declared_value_entity_references::<()>(
            &unmarked,
            &ComponentValueShape::EntityRef,
            &local("target"),
            Default::default(),
        ),
        Err(ComponentEntityReferenceRewriteError::MissingEntityRefCapability { .. })
    ));
    let field = field();
    let payload = ComponentValue::List(vec![local("target")]);
    let shallow = EntityReferenceTraversalLimits::new(
        ItemLimit::new(16).unwrap(),
        ByteLimit::new(1024).unwrap(),
        DepthLimit::new(1).unwrap(),
    );
    assert!(matches!(
        plan_declared_value_entity_references::<()>(&field, &field.shape, &payload, shallow,),
        Err(ComponentEntityReferenceRewriteError::DepthLimit { maximum: 1 })
    ));
    assert!(
        plan_declared_value_entity_references::<()>(
            &field,
            &field.shape,
            &payload,
            Default::default(),
        )
        .is_ok()
    );
    assert!(
        plan_declared_value_entity_references::<()>(
            &field,
            &ComponentValueShape::EntityRef,
            &ComponentValue::String("target".into()),
            Default::default(),
        )
        .is_err()
    );
    assert!(matches!(plan_declared_value_entity_references::<()>(
        &field, &ComponentValueShape::opaque(nara_reflect::ComponentValueKind::Map),
        &ComponentValue::map([("hidden", local("target"))]), Default::default(),
    ), Err(ComponentEntityReferenceRewriteError::UndeclaredReference { path }) if path == ComponentFieldPath::from_fields(["hidden"])));
}

#[test]
fn payload_reference_budget_excludes_component_path_but_retains_field_identity() {
    let field = field();
    let limits = |nodes, bytes| {
        EntityReferenceTraversalLimits::new(
            ItemLimit::new(nodes).unwrap(),
            ByteLimit::new(bytes).unwrap(),
            DepthLimit::new(1).unwrap(),
        )
    };
    let value = local("target");
    plan_declared_value_entity_references::<()>(
        &field,
        &ComponentValueShape::EntityRef,
        &value,
        limits(2, 11),
    )
    .unwrap();
    assert!(matches!(
        plan_declared_value_entity_references::<()>(
            &field,
            &ComponentValueShape::EntityRef,
            &value,
            limits(1, 11)
        ),
        Err(ComponentEntityReferenceRewriteError::NodeLimit { maximum: 1 })
    ));
    assert!(matches!(
        plan_declared_value_entity_references::<()>(
            &field,
            &ComponentValueShape::EntityRef,
            &value,
            limits(2, 10)
        ),
        Err(ComponentEntityReferenceRewriteError::ByteLimit { maximum: 10 })
    ));
}

#[test]
fn mixed_payload_plans_rewrite_relative_paths_and_preserve_admitted_asset_values() {
    use nara_reflect::ComponentShapeMember;
    let mut field = field().with_capability(ComponentCapability::AssetRef);
    let shape = ComponentValueShape::record_members([
        ComponentShapeMember::new(
            "targets",
            ComponentValueShape::list(ComponentValueShape::EntityRef),
        ),
        ComponentShapeMember::new("portrait", ComponentValueShape::AssetRef),
    ]);
    let asset = ComponentValue::map([
        ("kind", ComponentValue::String("path".into())),
        ("value", ComponentValue::String("portrait.png".into())),
    ]);
    let mut payload = ComponentValue::map([
        ("targets", ComponentValue::List(vec![local("target")])),
        ("portrait", asset.clone()),
    ]);
    let plan =
        plan_declared_value_entity_references::<()>(&field, &shape, &payload, Default::default())
            .unwrap();
    let mut slots = Vec::new();
    plan.rewrite_in_place(&mut payload, |slot, _| {
        slots.push(slot.clone());
        Ok::<_, ()>(EntityReference::SceneLocal {
            entity: SceneEntityId::new("resolved").unwrap(),
        })
    })
    .unwrap();
    assert_eq!(
        slots,
        [ComponentFieldPath::new([
            nara_reflect::ComponentFieldPathSegment::field("targets"),
            nara_reflect::ComponentFieldPathSegment::index(0),
        ])]
    );
    assert_eq!(payload.get_path(&slots[0]).unwrap(), &local("resolved"));
    assert_eq!(payload.get("portrait"), Some(&asset));
    field.capabilities.remove(&ComponentCapability::AssetRef);
    assert!(matches!(
        plan_declared_value_entity_references::<()>(&field, &shape, &payload, Default::default()),
        Err(ComponentEntityReferenceRewriteError::MissingAssetRefCapability { .. })
    ));
    field.capabilities.insert(ComponentCapability::AssetRef);
    payload
        .set_path(
            &ComponentFieldPath::from_fields(["portrait"]),
            ComponentValue::String("invalid".into()),
        )
        .unwrap();
    assert!(
        plan_declared_value_entity_references::<()>(&field, &shape, &payload, Default::default())
            .is_err()
    );
}
