use nara_ecs::World;
use nara_identity::{
    EntityLookup, EntityReference, PersistentRuntimeId, PersistentRuntimeNamespaceId,
    PersistentRuntimeReference, TombstoneCause, WorldIdentityDomain, spawn_identity_entity,
};
use nara_reflect::__private::plan_declared_entity_references;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldPathSegment,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueMaterializationLimits, ComponentValueShape, EntityReferenceTraversalLimits,
    effective_component_value, validate_component_value,
};

fn reference() -> EntityReference {
    EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("durable-visitor-test").unwrap(),
            PersistentRuntimeId::parse_str("31415926-5358-4979-8323-846264338327").unwrap(),
        ),
    }
}

fn schema(required: bool) -> ComponentSchema {
    let shape = ComponentValueShape::list(ComponentValueShape::record([
        ("required".to_owned(), ComponentValueShape::EntityRef),
        (
            "nullable".to_owned(),
            ComponentValueShape::optional(ComponentValueShape::EntityRef),
        ),
    ]));
    let mut field = if required {
        ComponentFieldSchema::required(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
    } else {
        ComponentFieldSchema::optional(
            ComponentFieldId::new("links"),
            "Links",
            ComponentFieldPath::from_fields(["links"]),
            shape,
        )
    }
    .with_capability(ComponentCapability::EntityRef);
    field.default_value = Some(links());
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.DurableReference"),
        "Durable reference",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::EntityRef])
    .with_fields([field])
}

fn links() -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::map([
        ("required", ComponentValue::EntityReference(reference())),
        ("nullable", ComponentValue::Null),
    ])])
}

#[test]
fn nested_durable_values_do_not_require_live_residency_in_the_schema_visitor() {
    let mut world = World::new();
    let domain = WorldIdentityDomain::new(&world, Default::default()).unwrap();
    world.insert_resource(domain);
    let token = spawn_identity_entity(&mut world).unwrap();
    let EntityReference::Persistent { entity: logical } = reference() else {
        unreachable!()
    };
    world.resource_scope(
        |world, mut domain: nara_ecs::change_detection::Mut<WorldIdentityDomain>| {
            domain
                .register_persistent(world, token, logical.clone())
                .unwrap();
            domain
                .retire_entity(world, token, TombstoneCause::Unloaded)
                .unwrap();
        },
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .resolve_entity_reference(&world, &reference(), None),
        EntityLookup::Unloaded
    );
    for required in [true, false] {
        let schema = schema(required);
        for omitted in [true, false] {
            let raw = if omitted {
                ComponentValue::map([] as [(&str, ComponentValue); 0])
            } else {
                ComponentValue::map([("links", links())])
            };
            let effective = effective_component_value(
                &schema,
                &raw,
                ComponentValueMaterializationLimits::default(),
            )
            .unwrap();
            validate_component_value(&schema, &raw).unwrap();
            let plan = plan_declared_entity_references::<()>(
                &schema,
                effective.as_ref(),
                EntityReferenceTraversalLimits::default(),
            )
            .unwrap();
            let mut visited = Vec::new();
            plan.visit(effective.as_ref(), |path, reference| {
                visited.push((path.clone(), reference.clone()));
                Ok::<_, ()>(())
            })
            .unwrap();
            assert_eq!(
                visited,
                vec![(
                    ComponentFieldPath::new([
                        ComponentFieldPathSegment::field("links"),
                        ComponentFieldPathSegment::index(0),
                        ComponentFieldPathSegment::field("required"),
                    ]),
                    reference(),
                )]
            );
            assert_eq!(raw.get("links").is_none(), omitted);
        }
    }
}

#[test]
fn optional_null_durable_field_creates_no_resolution_edge() {
    let schema = schema(false);
    let raw = ComponentValue::map([("links", ComponentValue::Null)]);
    let effective = effective_component_value(
        &schema,
        &raw,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    validate_component_value(&schema, &raw).unwrap();
    let plan = plan_declared_entity_references::<()>(
        &schema,
        effective.as_ref(),
        EntityReferenceTraversalLimits::default(),
    )
    .unwrap();
    let mut visits = 0;
    plan.visit(effective.as_ref(), |_, _| {
        visits += 1;
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(visits, 0);
    assert_eq!(effective.as_ref(), &raw);
}

#[test]
fn typed_deferred_codec_preserves_durable_reference_and_reads_encode_context() {
    use nara_ecs::{Component, Resource};
    use nara_reflect::{ComponentCodecError, ComponentRegistry, PreparedComponentCandidate};

    #[derive(Component)]
    struct DurableLink(EntityReference);

    #[derive(Resource)]
    struct SavedReference(EntityReference);

    let component_id = ComponentTypeId::new("nara.test.TypedDurableLink");
    let schema = ComponentSchema::new(
        component_id.clone(),
        "Durable link",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        ComponentValueShape::EntityRef,
    )
    .with_capabilities([ComponentCapability::Scene, ComponentCapability::EntityRef])]);
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_codec_with_context::<DurableLink, _, _>(
            schema,
            |value, _context| {
                let target = value.field_entity_reference("target")?.clone();
                Ok(PreparedComponentCandidate::deferred(move || {
                    Ok(DurableLink(target))
                }))
            },
            |link, world, _context| {
                if link.0 != world.resource::<SavedReference>().0 {
                    return Err(ComponentCodecError::invalid_field(
                        "target",
                        "saved reference",
                    ));
                }
                Ok(ComponentValue::map([(
                    "target",
                    ComponentValue::EntityReference(link.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    let input = ComponentValue::map([("target", ComponentValue::EntityReference(reference()))]);
    let prepared = registry
        .preflight_component(&component_id, &input)
        .unwrap()
        .unwrap();
    drop(input);
    let mut world = World::new();
    world.insert_resource(SavedReference(reference()));
    let target = world.spawn_empty().id();
    prepared.apply(&mut world, target).unwrap();
    assert_eq!(world.get::<DurableLink>(target).unwrap().0, reference());
    assert_eq!(
        registry.encode_component(&component_id, &world, target),
        Some(Ok(Some(ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(reference())
        ),])))),
    );
}
