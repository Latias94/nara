use super::*;

fn input() -> ReferenceInputId {
    ReferenceInputId::new("socket").unwrap()
}

fn default_schema(
    name: &str,
    shape: ComponentValueShape,
    default: ComponentValue,
) -> ComponentSchema {
    let entity_refs = shape.contains_kind(ComponentValueKind::EntityRef);
    let mut field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::from_fields(["target"]),
        shape,
        default,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if entity_refs {
        field = field.with_capability(ComponentCapability::EntityRef);
    }
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field])
}

fn reference_shape() -> ComponentValueShape {
    ComponentValueShape::from(ComponentValueKind::EntityRef)
}

fn route_list(source: &str, leaf: &str) -> ComponentValue {
    ComponentValue::List(vec![ComponentValue::map([
        (
            "source",
            ComponentValue::EntityReference(routed_reference(
                &[("host", source), ("nested", leaf)],
                AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
            )),
        ),
        (
            "addition",
            ComponentValue::EntityReference(routed_reference(
                &[("host", source)],
                AuthoredReferenceTarget::Entity(PrefabEntityOrigin::InstanceAddition(id(
                    "before-added",
                ))),
            )),
        ),
    ])])
}

fn hidden_default(value: ComponentValue) -> SceneComponentRecord {
    record(ComponentValue::map([("target", ComponentValue::Null)]))
        .with_contextual_default(ComponentFieldId::new("target"), value)
}

fn persistent() -> EntityReference {
    EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference {
            namespace: nara_identity::PersistentRuntimeNamespaceId::new("test.external").unwrap(),
            entity: nara_identity::PersistentRuntimeId::parse_str(
                "00000000-0000-4000-8000-000000000033",
            )
            .unwrap(),
        },
    }
}

fn arguments(base: &str, value: EntityReference) -> PrefabReferenceArguments {
    PrefabReferenceArguments {
        base: key(base),
        values: [(input(), value)].into(),
    }
}

fn input_reference(hops: &[(&str, &str)]) -> EntityReference {
    routed_reference(hops, AuthoredReferenceTarget::Input(input()))
}

#[test]
fn whole_closure_copy_retains_hidden_defaults_inputs_and_lexical_argument_routes() {
    let basis = basis_with([
        default_schema(
            "DefaultRoutes",
            ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
                reference_shape(),
            ))),
            route_list("middle", "leaf"),
        ),
        default_schema(
            "DefaultInput",
            reference_shape(),
            ComponentValue::EntityReference(input_reference(&[])),
        ),
        default_schema(
            "DefaultLocal",
            reference_shape(),
            ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("b") }),
        ),
        default_schema(
            "DefaultScalar",
            ComponentValueKind::U64.into(),
            ComponentValue::U64(7),
        ),
        default_schema("DefaultNull", reference_shape(), ComponentValue::Null),
        default_schema(
            "DefaultPersistent",
            reference_shape(),
            ComponentValue::EntityReference(persistent()),
        ),
    ]);
    let mut request = nested_fixture();
    request.sources[1].document.inputs.insert(input());
    request.sources[2].document.inputs.insert(input());
    let host = &mut request.sources[0].document.entities[0];
    host.components.insert(component("Link"), outer_link("b"));
    host.components.insert(
        component("DefaultRoutes"),
        hidden_default(route_list("middle", "leaf")),
    );
    host.components.insert(
        component("DefaultInput"),
        hidden_default(ComponentValue::EntityReference(input_reference(&[
            ("host", "middle"),
            ("nested", "leaf"),
        ]))),
    );
    for name in ["DefaultScalar", "DefaultNull", "DefaultPersistent"] {
        host.components.insert(
            component(name),
            record(ComponentValue::Map(Default::default())),
        );
    }
    let middle = host.prefab.as_mut().unwrap();
    middle.arguments = Some(arguments(
        "middle",
        input_reference(&[("host", "middle"), ("nested", "leaf")]),
    ));
    let overlay = middle.overrides.as_mut().unwrap();
    overlay.additions[0].prefab.as_mut().unwrap().arguments = Some(arguments(
        "leaf",
        input_reference(&[("host", "middle"), ("nested", "leaf")]),
    ));
    overlay.instances[0].value.as_mut().unwrap().arguments = Some(arguments(
        "leaf",
        input_reference(&[("host", "middle"), ("nested", "leaf")]),
    ));
    request.sources[1].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .arguments = Some(arguments("leaf", input_reference(&[])));
    let local = request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("c"))
        .unwrap();
    local.components.insert(
        component("DefaultLocal"),
        hidden_default(ComponentValue::EntityReference(
            EntityReference::SceneLocal { entity: id("b") },
        )),
    );
    let originals: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let mut prepared = request
        .prepare(&basis, PrefabCopyLimits::default())
        .unwrap();
    for ((original, supplied), copied) in originals
        .iter()
        .zip(&request.sources)
        .zip(prepared.sources())
    {
        assert_eq!(&supplied.document, original);
        assert_eq!(copied.document.inputs, original.inputs);
    }
    let host = &prepared.sources()[0].document.entities[0];
    assert_eq!(
        host.components[&component("Link")],
        reference_record(routed_reference(
            &[("host", "copied-middle"), ("nested", "copied-leaf")],
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
        ))
    );
    let copied = &host.components[&component("DefaultRoutes")];
    assert_eq!(
        copied.value,
        ComponentValue::map([("target", ComponentValue::Null)])
    );
    assert_eq!(
        copied.contextual_defaults[&ComponentFieldId::new("target")],
        route_list("copied-middle", "copied-leaf")
    );
    assert_eq!(
        host.components[&component("DefaultInput")].contextual_defaults
            [&ComponentFieldId::new("target")],
        ComponentValue::EntityReference(input_reference(&[
            ("host", "copied-middle"),
            ("nested", "copied-leaf")
        ]))
    );
    for name in ["DefaultScalar", "DefaultNull", "DefaultPersistent"] {
        assert_eq!(
            host.components[&component(name)],
            originals[0].entities[0].components[&component(name)]
        );
        assert!(
            host.components[&component(name)]
                .value
                .get("target")
                .is_none()
        );
    }
    let middle = host.prefab.as_ref().unwrap();
    assert_eq!(
        middle.arguments,
        Some(arguments(
            "copied-middle",
            input_reference(&[("host", "copied-middle"), ("nested", "copied-leaf"),])
        ))
    );
    let overlay = middle.overrides.as_ref().unwrap();
    for nested in [
        overlay.additions[0].prefab.as_ref().unwrap(),
        overlay.instances[0].value.as_ref().unwrap(),
    ] {
        assert_eq!(
            nested.arguments,
            Some(arguments(
                "copied-leaf",
                input_reference(&[("host", "copied-middle"), ("nested", "copied-leaf"),])
            ))
        );
    }
    assert_eq!(
        prepared.sources()[1].document.entities[0]
            .prefab
            .as_ref()
            .unwrap()
            .arguments,
        Some(arguments("copied-leaf", input_reference(&[])))
    );
    let copied_local = prepared.sources()[2]
        .document
        .entities
        .iter()
        .find(|entity| entity.id == id("c"))
        .unwrap();
    assert_eq!(
        &copied_local.components[&component("DefaultLocal")],
        local_record(&originals[2])
    );

    // Expose the retained default after copying. Its routes still start in the original
    // declaration's outer source, including its explicit instance-addition endpoint.
    let record = prepared.sources[0].document.entities[0]
        .components
        .get_mut(&component("DefaultRoutes"))
        .unwrap();
    let ComponentValue::Map(fields) = &mut record.value else {
        panic!("record map")
    };
    fields.remove("target");
    let usage = SceneDocument::from_ordered_roots([anchor("mount", "copied-outer", None)]);
    assert_use_succeeds(&compile_use(prepared.sources(), &usage, &basis));
}

fn local_record(document: &PrefabDocument) -> &SceneComponentRecord {
    &document
        .entities
        .iter()
        .find(|entity| entity.id == id("c"))
        .unwrap()
        .components[&component("DefaultLocal")]
}

#[test]
fn omitted_contextual_default_remains_a_declaration_instead_of_a_value_pin() {
    let default = ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("b") });
    let basis = basis_with([default_schema(
        "DefaultLink",
        reference_shape(),
        default.clone(),
    )]);
    let original = record(ComponentValue::Map(Default::default()))
        .with_contextual_default(ComponentFieldId::new("target"), default);
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b"))
            .with_component(component("DefaultLink"), original.clone())]),
    )]);
    let prepared = request
        .prepare(&basis, PrefabCopyLimits::default())
        .unwrap();
    assert_eq!(
        prepared.sources()[0].document.entities[0].components[&component("DefaultLink")],
        original
    );
    assert!(
        prepared.sources()[0].document.entities[0].components[&component("DefaultLink")]
            .value
            .get("target")
            .is_none()
    );
}

#[test]
fn property_absence_keeps_the_source_contextual_default_environment() {
    let default = ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id("b") });
    let basis = basis_with([default_schema(
        "DefaultLink",
        reference_shape(),
        default.clone(),
    )]);
    let mut request = nested_fixture();
    request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("a"))
        .unwrap()
        .components
        .insert(component("DefaultLink"), hidden_default(default.clone()));
    let overlay = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    overlay.components.push(PrefabComponentIntent::Properties {
        target: through("a"),
        component: component("DefaultLink"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Absent,
        }],
    });
    let prepared = request
        .prepare(&basis, PrefabCopyLimits::default())
        .unwrap();
    let a = prepared.sources()[2]
        .document
        .entities
        .iter()
        .find(|entity| entity.id == id("a"))
        .unwrap();
    assert_eq!(
        a.components[&component("DefaultLink")],
        hidden_default(default)
    );
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = overlay.components.last().unwrap()
    else {
        panic!("property");
    };
    assert_eq!(claims[0].intent, PrefabPropertyIntent::Absent);
}

#[test]
fn selected_collection_payload_rewrites_inline_routes_without_changing_its_shape() {
    let shape = ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
        reference_shape(),
    )));
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("payload"),
        "Payload",
        ComponentFieldPath::from_fields(["payload"]),
        ComponentValueShape::record_members([ComponentShapeMember::new("targets", shape)
            .with_id(ComponentFieldId::new("targets-member"))]),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef);
    let schema = ComponentSchema::new(
        component("References"),
        "References",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let basis = basis_with([schema]);
    let mut request = nested_fixture();
    request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("a"))
        .unwrap()
        .components
        .insert(
            component("References"),
            record(ComponentValue::map([(
                "payload",
                ComponentValue::map([("targets", ComponentValue::List(Vec::new()))]),
            )])),
        );
    let overlay = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let path = ComponentPropertyPath::new(ComponentFieldId::new("payload")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("targets-member")),
    ]);
    overlay.components.push(PrefabComponentIntent::Properties {
        target: through("a"),
        component: component("References"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: path.clone(),
            intent: PrefabPropertyIntent::Set(route_list("middle", "leaf")),
        }],
    });
    let prepared = request
        .prepare(&basis, PrefabCopyLimits::default())
        .unwrap();
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = overlay.components.last().unwrap()
    else {
        panic!("property")
    };
    assert_eq!(claims[0].path, path);
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(route_list("copied-middle", "copied-leaf"))
    );
}

#[test]
fn hidden_source_defaults_require_explicit_current_contextual_declarations() {
    for template in [
        EntityReference::SceneLocal { entity: id("b") },
        routed_reference(
            &[("nested", "leaf")],
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
        ),
    ] {
        let basis = basis_with([default_schema(
            "DefaultLink",
            reference_shape(),
            ComponentValue::EntityReference(template),
        )]);
        for explicit in [false, true] {
            let value = if explicit {
                ComponentValue::map([("target", ComponentValue::Null)])
            } else {
                ComponentValue::Map(Default::default())
            };
            let request = PrefabCopyRequest::new(vec![mapped_source(
                "leaf",
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b"))
                    .with_component(component("DefaultLink"), record(value))]),
            )]);
            assert!(matches!(
                request.prepare(&basis, PrefabCopyLimits::default()),
                Err(PrefabCopyError::Value { source: 0, .. })
            ));
        }
    }
}

#[test]
fn hidden_default_and_unused_argument_routes_require_complete_canonical_mapping() {
    let basis = basis_with([default_schema(
        "DefaultLink",
        reference_shape(),
        ComponentValue::EntityReference(input_reference(&[])),
    )]);
    for site in 0..3 {
        let mut request = nested_fixture();
        request.sources[1].document.inputs.insert(input());
        let host = &mut request.sources[0].document.entities[0];
        let unmapped = input_reference(&[("host", "external")]);
        if site == 0 {
            host.components.insert(
                component("DefaultLink"),
                hidden_default(ComponentValue::EntityReference(unmapped)),
            );
        } else {
            host.prefab.as_mut().unwrap().arguments = Some(arguments(
                if site == 1 { "external" } else { "middle" },
                unmapped,
            ));
        }
        let before = request.sources[0].document.clone();
        assert!(matches!(
            request.prepare(&basis, PrefabCopyLimits::default()),
            Err(PrefabCopyError::UnmappedSourceKey { source: 0 })
        ));
        assert_eq!(request.sources[0].document, before);
    }
}

#[test]
fn omitted_asset_default_refuses_instead_of_creating_a_value_pin() {
    let request = PrefabCopyRequest::new(vec![mapped_source(
        "leaf",
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b")).with_component(
            component("Assets"),
            record(ComponentValue::map([(
                "plain",
                ComponentValue::String("unchanged".into()),
            )])),
        )]),
    )]);
    assert!(matches!(
        refusals::prepare_error(&request),
        PrefabCopyError::ReferenceDefaultAbsent { source: 0 }
    ));
    assert!(
        request.sources[0].document.entities[0].components[&component("Assets")]
            .value
            .get("asset")
            .is_none()
    );
}
