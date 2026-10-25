#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericBound, ComponentNumericInterval,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringPropertyEdit,
    MAX_AUTHORING_FIELD_WRITES, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityRecord, ScenePatchOperation,
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
                ScenePatchOperation::SetField {
                    entity: id(),
                    component: ComponentTypeId::new("test.PropertyLimits"),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("data"),
                    value: ComponentValue::map([("text", ComponentValue::String("after".into()),)]),
                },
                ScenePatchOperation::SetField {
                    entity: id(),
                    component: ComponentTypeId::new("test.PropertyLimits"),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("weights"),
                    value: ComponentValue::map([("private-key", ComponentValue::U64(6))]),
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
            && field.value() == nara_diagnostic::DiagnosticValueRef::Unsigned(1)
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "property-error-kind"
            && field.value() == nara_diagnostic::DiagnosticValueRef::Identifier("member-not-found")
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "property-step-index"
            && field.value() == nara_diagnostic::DiagnosticValueRef::Unsigned(0)
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
                    && field.value() == nara_diagnostic::DiagnosticValueRef::Identifier(expected)
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
    let prefab = PrefabDocument::new(document.roots, document.entities);
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
    let prepared = session
        .prepare_authoring_property_edit(session.revision(), &[edit("middle"), edit("after")])
        .unwrap();
    assert_eq!(session.document(), &prefab);
    assert_eq!(prepared.patch().operations.len(), 1);
    assert_eq!(prepared.inverse().operations.len(), 1);
    assert!(session.commit_authoring_edit(prepared).applied);
    let accepted = session.document().clone();
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &accepted);
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
    let original = PrefabDocument::new(source.roots, source.entities);
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
                && field.value() == nara_diagnostic::DiagnosticValueRef::Identifier("weights"))
    );
    assert!(!format!("{:?}", report.diagnostics).contains("private-key"));
}
