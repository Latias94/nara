use super::*;

fn input() -> ReferenceInputId {
    ReferenceInputId::new("u").unwrap()
}

fn parameterized() -> PrefabDocument {
    let mut document = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("q-root"))
        .with_component(
            component("Link"),
            reference_record(EntityReference::Authored {
                reference: AuthoredEntityReference::input(input()),
            }),
        )]);
    document.inputs.insert(input());
    document
}

#[test]
fn parameterized_source_copy_needs_no_mount_arguments() {
    let document = parameterized();
    let request = PrefabCopyRequest::new(vec![mapped_source("q", document.clone())]);
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    assert_eq!(request.sources[0].document, document);
    assert_eq!(prepared.sources()[0].document(), &document);

    let mut mount = anchor("mount", "copied-q", None);
    let usage = SceneDocument::from_ordered_roots([mount.clone()]);
    let report = compile_use(prepared.sources(), &usage, &basis());
    assert!(report.candidate().is_none());
    assert!(
        report.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-input-argument-missing"
        })
    );

    mount.prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: key("copied-q"),
        values: [(input(), EntityReference::SceneLocal { entity: id("b") })].into(),
    });
    let usage = SceneDocument::from_ordered_roots([mount, SceneEntityRecord::new(id("b"))]);
    let report = compile_use(prepared.sources(), &usage, &basis());
    assert_use_succeeds(&report);
    let target = report
        .candidate()
        .unwrap()
        .document()
        .entities
        .iter()
        .find_map(|entity| entity.components.get(&component("Link")))
        .unwrap();
    assert_eq!(target.value, link("b").value);
}

#[test]
fn copied_parent_supplies_each_actual_child_occurrences_input() {
    let mut child = anchor("a", "q", None);
    child.prefab.as_mut().unwrap().arguments = Some(PrefabReferenceArguments {
        base: key("q"),
        values: [(input(), EntityReference::SceneLocal { entity: id("b") })].into(),
    });
    let request = PrefabCopyRequest::new(vec![
        mapped_source(
            "p",
            PrefabDocument::from_ordered_roots([child, SceneEntityRecord::new(id("b"))]),
        ),
        mapped_source("q", parameterized()),
    ]);
    let originals: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    for (source, original) in request.sources.iter().zip(originals) {
        assert_eq!(source.document, original);
    }
    assert_eq!(prepared.sources()[1].document(), &parameterized());
    let arguments = prepared.sources()[0].document().entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .arguments
        .as_ref()
        .unwrap();
    assert_eq!(arguments.base, key("copied-q"));
    assert_eq!(
        arguments.values[&input()],
        EntityReference::SceneLocal { entity: id("b") }
    );

    let usage = SceneDocument::from_ordered_roots([
        anchor("left", "copied-p", None),
        anchor("right", "copied-p", None),
    ]);
    let report = compile_use(prepared.sources(), &usage, &basis());
    assert_use_succeeds(&report);
    let candidate = report.candidate().unwrap();
    let origins = candidate.provenance();
    let targets: Vec<_> = candidate
        .document()
        .entities
        .iter()
        .filter_map(|entity| {
            let record = entity.components.get(&component("Link"))?;
            let Some(ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: target,
            })) = record.value.get("target")
            else {
                panic!("expected an actually lowered input")
            };
            assert_eq!(origins[target].local_entity_id(), &id("b"));
            assert_eq!(
                origins[&entity.id].projection_path().unwrap().root_anchor(),
                origins[target].projection_path().unwrap().root_anchor(),
            );
            Some(target.clone())
        })
        .collect();
    assert_eq!(targets.len(), 2);
    assert_ne!(targets[0], targets[1]);
}

#[test]
fn mapped_wrong_reference_hop_remains_a_constraint_for_actual_use() {
    let routed = reference_record(routed_reference(
        &[("a", "other")],
        AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
    ));
    let request = PrefabCopyRequest::new(vec![
        mapped_source(
            "p",
            PrefabDocument::from_ordered_roots([
                anchor("a", "q", None).with_component(component("Link"), routed)
            ]),
        ),
        mapped_source(
            "q",
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("b"))]),
        ),
        mapped_source("other", PrefabDocument::default()),
    ]);
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    let copied = &prepared.sources()[0].document().entities[0].components[&component("Link")];
    assert_eq!(
        copied,
        &reference_record(routed_reference(
            &[("a", "copied-other")],
            AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(id("b"))),
        ))
    );
    let usage = SceneDocument::from_ordered_roots([anchor("mount", "copied-p", None)]);
    let report = compile_use(prepared.sources(), &usage, &basis());
    assert!(report.candidate().is_none());
    assert_eq!(
        report.failure().unwrap().reason,
        PrefabExpansionFailureKind::SourceChanged
    );
}

#[test]
fn cyclic_source_closure_is_copied_and_only_the_chosen_mount_is_compiled() {
    let request = PrefabCopyRequest::new(vec![
        mapped_source(
            "p",
            PrefabDocument::from_ordered_roots([anchor("a", "q", None)]),
        ),
        mapped_source(
            "q",
            PrefabDocument::from_ordered_roots([anchor("b", "p", None)]),
        ),
    ]);
    let originals: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    for (source, original) in request.sources.iter().zip(originals) {
        assert_eq!(source.document, original);
    }
    let mut cut = PrefabOverlay::new(key("copied-p"));
    cut.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("a")),
                expected_base: key("copied-q"),
            }],
            entity: PrefabEntityOrigin::SourceEntity(id("b")),
        },
        value: None,
    });
    let usage = SceneDocument::from_ordered_roots([anchor("mount", "copied-p", Some(cut))]);
    assert_use_succeeds(&compile_use(prepared.sources(), &usage, &basis()));

    let usage = SceneDocument::from_ordered_roots([anchor("mount", "copied-p", None)]);
    let report = compile_use(prepared.sources(), &usage, &basis());
    assert!(report.candidate().is_none());
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "scene.prefab-cycle" })
    );
}

#[test]
fn source_structure_and_direct_local_reference_failures_still_refuse_copying() {
    for document in [
        PrefabDocument::new([id("missing")], [SceneEntityRecord::new(id("root"))]),
        PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(id("root")).with_component(component("Link"), link("missing"))
        ]),
    ] {
        let request = PrefabCopyRequest::new(vec![mapped_source("q", document)]);
        assert!(matches!(
            refusals::prepare_error(&request),
            PrefabCopyError::Source { source: 0, .. }
        ));
    }
}

#[test]
fn conditional_property_constraints_are_preserved_without_activation() {
    let schema = ComponentSchema::new(component("Nested"), "Nested", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::record_members([ComponentShapeMember::new(
                "value",
                ComponentValueKind::U64,
            )
            .with_id(ComponentFieldId::new("member"))]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let basis = basis_with([schema]);
    let mut overlay = PrefabOverlay::new(key("q"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("q-root")),
        component: component("Nested"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("payload")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("member")),
            ]),
            intent: PrefabPropertyIntent::Absent,
        }],
    });
    let request = PrefabCopyRequest::new(vec![
        mapped_source(
            "p",
            PrefabDocument::from_ordered_roots([anchor("a", "q", Some(overlay))]),
        ),
        mapped_source(
            "q",
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("q-root"))
                .with_component(
                    component("Nested"),
                    record(ComponentValue::map([(
                        "payload",
                        ComponentValue::map([("value", ComponentValue::U64(1))]),
                    )])),
                )]),
        ),
    ]);
    let before = request.sources[0].document.clone();
    let prepared = request
        .prepare(&basis, PrefabCopyLimits::default())
        .unwrap();
    assert_eq!(request.sources[0].document, before);
    let copied = prepared.sources()[0].document().entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    assert_eq!(copied.base, key("copied-q"));
    let original = before.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    assert_eq!(copied.components, original.components);
}
