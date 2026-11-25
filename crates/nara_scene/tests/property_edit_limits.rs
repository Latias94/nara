#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, SerdeShapeLimits};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericBound, ComponentNumericInterval,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringPropertyEdit, AuthoringSchemaBasis,
    MAX_AUTHORING_FIELD_WRITES, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, SceneFileBudgetKind, SceneFileLimits, SceneFormatError,
    ScenePatchOperation, SchemaReclassificationKind, SchemaReclassificationWork,
    authoring::AuthoringAdapterReceipt,
};

fn id() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}

fn bundle() -> AuthoringSchemaBundle {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.property-limits").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.PropertyLimits"),
                    "Property limits",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("data"),
                        "Data",
                        ComponentFieldPath::from_fields(["data"]),
                        ComponentValueShape::record_members([ComponentShapeMember::new(
                            "text",
                            ComponentValueKind::String,
                        )
                        .with_id(ComponentFieldId::new("text"))]),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("weights"),
                        "Weights",
                        ComponentFieldPath::from_fields(["weights"]),
                        ComponentValueShape::map(
                            ComponentValueShape::scalar(ComponentValueKind::U64)
                                .with_numeric_interval(Some(ComponentNumericInterval::U64 {
                                    min: None,
                                    max: Some(ComponentNumericBound::Included(10)),
                                })),
                        ),
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(id()).with_component(
        ComponentTypeId::new("test.PropertyLimits"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                (
                    "data",
                    ComponentValue::map([("text", ComponentValue::String("before".into()))]),
                ),
                (
                    "weights",
                    ComponentValue::map([("private-key", ComponentValue::U64(5))]),
                ),
            ]),
        ),
    )])
}

fn edit(text: impl Into<String>) -> AuthoringPropertyEdit {
    AuthoringPropertyEdit {
        entity: id(),
        component: ComponentTypeId::new("test.PropertyLimits"),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(ComponentFieldId::new("data"))
            .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("text"))]),
        value: ComponentValue::String(text.into()),
    }
}

fn open() -> SceneAuthoringSession {
    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes(document().to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/properties.json").unwrap()),
                &bundle,
                &[adapter],
            )
            .unwrap()
            .publish(),
    )
}

#[test]
fn raw_property_work_remains_bounded_even_if_it_would_coalesce_to_one_root() {
    let session = open();
    let before = session.document().clone();
    let history = session.history_status();
    let revision = session.revision();
    let edits = vec![edit("after"); MAX_AUTHORING_FIELD_WRITES + 1];
    assert!(
        session
            .prepare_authoring_property_edit(revision, &edits)
            .is_err()
    );
    assert!(
        session
            .prepare_authoring_property_edit(revision, &[])
            .is_err()
    );
    let edits = [edit("x".repeat(40 * 1024)), edit("y".repeat(40 * 1024))];
    assert!(
        session
            .prepare_authoring_property_edit(revision, &edits)
            .is_err()
    );
    assert_eq!(session.document(), &before);
    assert_eq!(session.history_status(), history);
    assert_eq!(session.revision(), revision);
}

#[test]
fn selector_bytes_are_admitted_before_private_candidate_work() {
    let session = open();
    let mut edit = edit("after");
    edit.property.steps = vec![ComponentPropertyStep::MapEntry("x".repeat(64 * 1024))];
    assert!(
        session
            .prepare_authoring_property_edit(session.revision(), &[edit])
            .is_err()
    );
    assert_eq!(session.history_status().undo_depth, 0);
}

#[test]
fn construction_rejects_oversized_component_ids_before_retaining_intent() {
    let session = open();
    for declaration in [false, true] {
        let mut draft = session
            .begin_authoring_construction(session.revision())
            .unwrap();
        draft
            .set_property(
                id(),
                ComponentTypeId::new("test.PropertyLimits"),
                ComponentPropertyPath::new(ComponentFieldId::new("weights"))
                    .with_steps([ComponentPropertyStep::MapEntry("private-key".into())]),
                ComponentValue::U64(6),
            )
            .unwrap();
        let huge = ComponentTypeId::new("private-unadmitted".repeat(8192));
        let result = if declaration {
            draft.add_component(id(), huge)
        } else {
            draft.set_property(
                id(),
                huge,
                edit("unused").property,
                ComponentValue::String("unused".into()),
            )
        };
        assert!(result.is_err());
        let valid = edit("after");
        draft
            .set_property(valid.entity, valid.component, valid.property, valid.value)
            .unwrap();
        let prepared = session.prepare_authoring_construction(&draft).unwrap();
        assert_eq!(
            prepared.patch().operations,
            vec![
                ScenePatchOperation::ReplaceProperty {
                    entity: id(),
                    component: ComponentTypeId::new("test.PropertyLimits"),
                    component_version: ComponentSchemaVersion::ONE,
                    property: ComponentPropertyPath::new(ComponentFieldId::new("weights"))
                        .with_steps([ComponentPropertyStep::MapEntry("private-key".into())]),
                    value: ComponentValue::U64(6),
                },
                ScenePatchOperation::ReplaceProperty {
                    entity: id(),
                    component: ComponentTypeId::new("test.PropertyLimits"),
                    component_version: ComponentSchemaVersion::ONE,
                    property: ComponentPropertyPath::new(ComponentFieldId::new("data"))
                        .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("text"))]),
                    value: ComponentValue::String("after".into()),
                },
            ]
        );
        assert_eq!(session.document(), &document());
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn failed_property_batch_identifies_the_rejected_operation_without_leaking_keys() {
    let session = open();
    let revision = session.revision();
    let history = session.history_status();
    let mut missing_member = edit("unused");
    missing_member.property =
        ComponentPropertyPath::new(ComponentFieldId::new("data")).with_steps([
            ComponentPropertyStep::Member(ComponentFieldId::new("missing")),
        ]);
    let report = session
        .prepare_authoring_property_edit(revision, &[edit("private-value"), missing_member])
        .unwrap_err();
    let diagnostic = report.diagnostics.iter().next().unwrap();
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "operation-index"
            && field.value() == nara_core::diagnostic::DiagnosticValueRef::Unsigned(1)
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "property-error-kind"
            && field.value()
                == nara_core::diagnostic::DiagnosticValueRef::Identifier("member-not-found")
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "property-step-index"
            && field.value() == nara_core::diagnostic::DiagnosticValueRef::Unsigned(0)
    }));
    assert_eq!(session.document(), &document());
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert!(!format!("{:?}", report.diagnostics).contains("private-value"));
}

#[test]
fn property_rejections_distinguish_version_and_missing_map_entry_without_private_content() {
    let session = open();
    let revision = session.revision();
    let mut wrong_version = edit("private-replacement");
    wrong_version.component_version = ComponentSchemaVersion::new(2).unwrap();
    let missing_entry = AuthoringPropertyEdit {
        property: ComponentPropertyPath::new(ComponentFieldId::new("weights")).with_steps([
            ComponentPropertyStep::MapEntry("private-missing-key".into()),
        ]),
        value: ComponentValue::U64(6),
        ..edit("unused")
    };
    for (edit, expected) in [
        (wrong_version, "version-mismatch"),
        (missing_entry, "map-entry-not-found"),
    ] {
        let report = session
            .prepare_authoring_property_edit(revision, &[edit])
            .unwrap_err();
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "property-error-kind"
                    && field.value()
                        == nara_core::diagnostic::DiagnosticValueRef::Identifier(expected)
            })
        }));
        let debug = format!("{:?}", report.diagnostics);
        assert!(!debug.contains("private-replacement"));
        assert!(!debug.contains("private-missing-key"));
        assert_eq!(session.document(), &document());
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn prefab_property_edits_reuse_the_nominal_owner_and_existing_inverse() {
    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let document = document();
    let prefab = PrefabDocument::new(document.placements, document.entities);
    let mut session = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/properties.json").unwrap()),
                &bundle,
                &[adapter],
            )
            .unwrap()
            .publish(),
    );
    let edits = [edit("middle"), edit("after")];
    let prepared = session
        .prepare_authoring_property_edit(session.revision(), &edits)
        .unwrap();
    assert_eq!(session.document(), &prefab);
    for (patch, expected) in [
        (prepared.patch(), ["middle", "after"]),
        (prepared.inverse(), ["middle", "before"]),
    ] {
        assert_eq!(patch.operations.len(), expected.len());
        for (operation, text) in patch.operations.iter().zip(expected) {
            assert_eq!(
                operation,
                &ScenePatchOperation::ReplaceProperty {
                    entity: id(),
                    component: edits[0].component.clone(),
                    component_version: ComponentSchemaVersion::ONE,
                    property: edits[0].property.clone(),
                    value: ComponentValue::String(text.into()),
                }
            );
        }
    }
    assert!(session.commit_authoring_edit(prepared).applied);
    let accepted = session.document().clone();
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
}

#[test]
fn scene_and_prefab_mixed_edits_enforce_file_limits_on_the_final_candidate() {
    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let old_text = "a".repeat(512);
    let component = ComponentTypeId::new("test.PropertyLimits");
    let weights_path = ComponentFieldPath::from_fields(["weights"]);
    let text_path = ComponentFieldPath::from_fields(["data", "text"]);
    let mut original = SceneDocument::from_ordered_roots(
        document()
            .entities
            .into_iter()
            .chain([SceneEntityRecord::new(SceneEntityId::new("spare").unwrap())]),
    );
    let value = original.entities[0]
        .components
        .get_mut(&component)
        .unwrap()
        .value_mut();
    value
        .set_path(&text_path, ComponentValue::String(old_text.clone()))
        .unwrap();
    let original_size = value.cost().logical_bytes();
    let extra_key = "b".repeat(256);
    let expanded_weights = ComponentValue::map([
        ("private-key", ComponentValue::U64(5)),
        (extra_key.as_str(), ComponentValue::U64(6)),
    ]);
    let growth = expanded_weights.cost().logical_bytes()
        - value
            .get_path(&weights_path)
            .unwrap()
            .cost()
            .logical_bytes();
    let new_text = &old_text[..old_text.len() - growth];
    let edits = [
        edit(new_text),
        AuthoringPropertyEdit {
            property: ComponentPropertyPath::new(ComponentFieldId::new("weights")),
            value: expanded_weights.clone(),
            ..edit("unused")
        },
    ];
    let overflow = [edit(format!("{new_text}x")), edits[1].clone()];
    let mut expected = original.clone();
    let value = expected.entities[0]
        .components
        .get_mut(&component)
        .unwrap()
        .value_mut();
    value
        .set_path(&text_path, ComponentValue::String(new_text.into()))
        .unwrap();
    value.set_path(&weights_path, expanded_weights).unwrap();
    assert_eq!(value.cost().logical_bytes(), original_size);
    let reordered = original
        .placements
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    let mut expected_reordered = expected.clone();
    expected_reordered.placements = reordered.clone();

    let exact_limits = |encode: &dyn Fn(SceneFileLimits) -> Result<String, SceneFormatError>| {
        let mut bytes = original_size;
        while bytes < original_size + growth {
            let limits = SceneFileLimits::default()
                .with_component_value_bytes(ByteLimit::new(bytes).unwrap());
            match encode(limits) {
                Ok(_) => return limits,
                Err(SceneFormatError::Budget(budget))
                    if budget.kind == SceneFileBudgetKind::ComponentValueBytes =>
                {
                    assert!(budget.observed > bytes);
                    bytes = budget.observed;
                }
                Err(error) => panic!("unexpected source budget error: {error}"),
            }
        }
        panic!("the fixture must fit only after balancing the property edits");
    };
    let scene_limits = exact_limits(&|limits| original.to_json_string_with_limits(limits));
    let mut scene = SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            original.to_json_string().unwrap().as_bytes(),
            scene_limits,
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/mixed-properties.json").unwrap()),
            &bundle,
            std::slice::from_ref(&adapter),
        )
        .unwrap()
        .publish(),
    );
    let prefab_original =
        PrefabDocument::new(original.placements.clone(), original.entities.clone());
    let prefab_expected =
        PrefabDocument::new(expected.placements.clone(), expected.entities.clone());
    let prefab_reordered = PrefabDocument::new(reordered.clone(), expected.entities.clone());
    let prefab_limits = exact_limits(&|limits| prefab_original.to_json_string_with_limits(limits));
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            prefab_original.to_json_string().unwrap().as_bytes(),
            prefab_limits,
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/mixed-properties.json").unwrap()),
            &bundle,
            std::slice::from_ref(&adapter),
        )
        .unwrap()
        .publish(),
    );
    macro_rules! readmit {
        ($session:ident, $expected:ident) => {{
            let before = ($session.revision(), $session.history_status());
            let next =
                AuthoringSchemaBasis::admit(&bundle, std::slice::from_ref(&adapter)).unwrap();
            assert_ne!($session.authoring_schema_basis(), Some(next.clone()));
            let prepared = $session
                .prepare_schema_reclassification(
                    &next,
                    &SchemaReclassificationKind::DeclarationExtension,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap();
            $session
                .bind_schema_reclassification(prepared)
                .unwrap()
                .commit();
            assert_eq!($session.authoring_schema_basis(), Some(next));
            assert_eq!(($session.revision(), $session.history_status()), before);
            assert_eq!($session.document(), &$expected);
        }};
    }
    macro_rules! assert_history {
        ($session:ident, $original:ident, $expected:ident, $reordered:ident) => {{
            let revision = $session.revision();
            let report = $session
                .prepare_authoring_property_edit(revision, &overflow)
                .unwrap_err();
            assert_eq!(
                report.diagnostics.iter().next().unwrap().code().as_str(),
                "scene.authoring-field-edit-format-invalid",
            );
            assert_eq!($session.document(), &$original);
            assert_eq!($session.revision(), revision);
            assert_eq!($session.history_status().undo_depth, 0);

            let prepared = $session
                .prepare_authoring_property_edit(revision, &edits)
                .unwrap();
            assert!(matches!(
                prepared.patch().operations.as_slice(),
                [
                    ScenePatchOperation::SetField { .. },
                    ScenePatchOperation::ReplaceProperty { .. }
                ]
            ));
            assert!(matches!(
                prepared.inverse().operations.as_slice(),
                [
                    ScenePatchOperation::ReplaceProperty { .. },
                    ScenePatchOperation::SetField { .. }
                ]
            ));
            let mut mixed = prepared.patch().clone();
            mixed.operations.insert(
                1,
                ScenePatchOperation::SetPlacements {
                    expected: $original.placements.clone(),
                    value: reordered.clone(),
                },
            );
            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!($session.document(), &$expected);
            assert_eq!($session.history_status().undo_depth, 1);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &$original);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &$expected);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &$original);

            let prepared = $session
                .prepare_authoring_edit($session.revision(), &mixed)
                .unwrap();
            assert!($session.commit_authoring_edit(prepared).applied);
            assert_eq!($session.document(), &$reordered);
            readmit!($session, $reordered);
            assert!($session.undo_authoring().applied);
            assert_eq!($session.document(), &$original);
            readmit!($session, $original);
            assert!($session.redo_authoring().applied);
            assert_eq!($session.document(), &$reordered);
        }};
    }
    assert_history!(scene, original, expected, expected_reordered);
    assert_history!(prefab, prefab_original, prefab_expected, prefab_reordered);
}

#[test]
fn a_coalesced_root_can_discard_a_deep_intermediate_value_before_publication() {
    let component = ComponentTypeId::new("test.DeepProperty");
    let root = ComponentFieldId::new("settings");
    let shape = (0..34).fold(ComponentValueKind::U64.into(), |item, _| {
        ComponentValueShape::list(item)
    });
    let schema = ComponentSchema::new(
        component.clone(),
        "Deep property",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        root.clone(),
        "Settings",
        ComponentFieldPath::from_fields(["settings"]),
        ComponentValueShape::record_members([
            ComponentShapeMember::new("tree", shape).with_id(ComponentFieldId::new("tree")),
            ComponentShapeMember::new("label", ComponentValueKind::String)
                .with_id(ComponentFieldId::new("label")),
        ]),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.deep-property").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let settings = |label: &str| {
        ComponentValue::map([
            ("tree", ComponentValue::List(Vec::new())),
            ("label", ComponentValue::String(label.into())),
        ])
    };
    let document = |label| {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id()).with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("settings", settings(label))]),
            ),
        )])
    };
    let original = document("before");
    let limits = SceneFileLimits::default();
    let shape = limits.shape();
    let limits = limits.with_shape(SerdeShapeLimits::new(
        DepthLimit::new(32).unwrap(),
        shape.nodes(),
        shape.container_items(),
        shape.string_bytes(),
        shape.total_string_bytes(),
    ));
    let mut session = SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            original.to_json_string().unwrap().as_bytes(),
            limits,
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/deep-property.json").unwrap()),
            &bundle,
            &[adapter],
        )
        .unwrap()
        .publish(),
    );
    let insert_deep = AuthoringPropertyEdit {
        entity: id(),
        component: component.clone(),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(root.clone())
            .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new("tree"))]),
        value: (0..34).fold(ComponentValue::U64(1), |item, _| {
            ComponentValue::List(vec![item])
        }),
    };
    let revision = session.revision();
    let overflow = session
        .prepare_authoring_property_edit(revision, std::slice::from_ref(&insert_deep))
        .unwrap_err();
    assert_eq!(
        overflow.diagnostics.iter().next().unwrap().code().as_str(),
        "scene.authoring-field-edit-format-invalid"
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.document(), &original);
    let reset = AuthoringPropertyEdit {
        property: ComponentPropertyPath::new(root),
        value: settings("after"),
        ..insert_deep.clone()
    };
    let prepared = session
        .prepare_authoring_property_edit(revision, &[insert_deep, reset])
        .unwrap();
    assert!(matches!(
        prepared.patch().operations.as_slice(),
        [ScenePatchOperation::SetField { .. }]
    ));
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &document("after"));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &original);
}

#[test]
fn scene_and_prefab_range_rejections_retain_safe_root_diagnostics_without_map_keys() {
    let invalid = AuthoringPropertyEdit {
        property: ComponentPropertyPath::new(ComponentFieldId::new("weights"))
            .with_steps([ComponentPropertyStep::MapEntry("private-key".into())]),
        value: ComponentValue::U64(11),
        ..edit("unused")
    };
    let scene = open();
    let revision = scene.revision();
    let report = scene
        .prepare_authoring_property_edit(revision, std::slice::from_ref(&invalid))
        .unwrap_err();
    assert_range_report(&report);
    assert_eq!(scene.document(), &document());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.history_status().undo_depth, 0);

    let bundle = bundle();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let source = document();
    let original = PrefabDocument::new(source.placements, source.entities);
    let prefab = PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_bytes(original.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("prefabs/properties.json").unwrap()),
                &bundle,
                &[adapter],
            )
            .unwrap()
            .publish(),
    );
    let revision = prefab.revision();
    let report = prefab
        .prepare_authoring_property_edit(revision, &[invalid])
        .unwrap_err();
    assert_range_report(&report);
    assert_eq!(prefab.document(), &original);
    assert_eq!(prefab.revision(), revision);
    assert_eq!(prefab.history_status().undo_depth, 0);
}

fn assert_range_report(report: &nara_scene::ScenePatchReport) {
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|item| item.code().as_str() == "nara.reflect.numeric-range")
        .unwrap();
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "component-field-id"
                && field.value()
                    == nara_core::diagnostic::DiagnosticValueRef::Identifier("weights"))
    );
    assert!(!format!("{:?}", report.diagnostics).contains("private-key"));
}
