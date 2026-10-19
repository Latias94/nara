#![cfg(feature = "serde")]

use std::collections::BTreeSet;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
    effective_component_value,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringFieldEdit,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation,
};

fn component() -> ComponentTypeId {
    ComponentTypeId::new("game.Overrides")
}

fn entity() -> SceneEntityId {
    SceneEntityId::new("actor").unwrap()
}

fn source() -> AuthoringDocumentSource {
    AuthoringDocumentSource::new(AssetRef::path("scenes/overrides.scene.json").unwrap())
}

fn fixture() -> (
    AuthoringSchemaBundle,
    AuthoringAdapterReceipt,
    SceneDocument,
) {
    let required = |name: &str| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    };
    let optional = |name: &str, default: Option<i64>| {
        let mut field = ComponentFieldSchema::optional(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING);
        field.default_value = default.map(ComponentValue::I64);
        field
    };
    let mut required_default = required("required_default");
    required_default.default_value = Some(ComponentValue::I64(10));
    let mut readonly = optional("readonly", None);
    readonly.capabilities.remove(&ComponentCapability::Edit);
    let schema = ComponentSchema::new(component(), "Overrides", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            required_default,
            optional("optional_default", Some(20)),
            optional("optional_absent", None),
            optional("explicit_null", Some(40)),
            required("required"),
            readonly,
            ComponentFieldSchema::optional(
                ComponentFieldId::new("text"),
                "Text",
                ComponentFieldPath::from_fields(["text"]),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("game.overrides").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    assert!(adapter.contribution().is_none());
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity())
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("required_default", ComponentValue::I64(1)),
                    ("optional_default", ComponentValue::I64(2)),
                    ("optional_absent", ComponentValue::I64(3)),
                    ("explicit_null", ComponentValue::Null),
                    ("required", ComponentValue::I64(7)),
                    ("readonly", ComponentValue::I64(8)),
                ]),
            ),
        )
        .with_component(
            ComponentTypeId::new("game.Unavailable"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve unknown record".into()),
            ),
        )]);
    (bundle, adapter, document)
}

fn reset(field: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: entity(),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(field),
    }])
}

fn raw_value(entities: &[SceneEntityRecord]) -> &ComponentValue {
    &entities[0].components[&component()].value
}

fn assert_effective(bundle: &AuthoringSchemaBundle, value: &ComponentValue, field: &str) {
    let effective = effective_component_value(
        &bundle.owners()[0].current().components[0],
        value,
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap();
    let expected = match field {
        "required_default" => Some(ComponentValue::I64(10)),
        "optional_default" => Some(ComponentValue::I64(20)),
        "explicit_null" => Some(ComponentValue::I64(40)),
        "optional_absent" => None,
        _ => unreachable!("only resettable fields are checked"),
    };
    assert_eq!(value.get(field), None);
    assert_eq!(effective.get(field), expected.as_ref());
}

#[test]
fn data_only_scene_reset_preserves_raw_presence_history_and_reopen() {
    let (bundle, adapter, document) = fixture();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    for field in [
        "required_default",
        "optional_default",
        "optional_absent",
        "explicit_null",
    ] {
        let before = session.document().clone();
        let history = session.history_status();
        let revision = session.revision();
        let report = session.apply_authoring_patch(&reset(field));
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(session.revision().generation(), revision.generation() + 1);
        assert_eq!(session.history_status().undo_depth, history.undo_depth + 1);
        assert_effective(&bundle, raw_value(&session.document().entities), field);
        let accepted = session.document().clone();
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &before);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &accepted);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    }
    let encoded = session.document().to_json_string().unwrap();
    let reopened = session
        .try_reopen_authoring_candidate(
            SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
        )
        .unwrap();
    assert_eq!(reopened.document(), session.document());
    assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn data_only_prefab_reset_preserves_raw_presence_history_and_reopen() {
    let (bundle, adapter, document) = fixture();
    let document = PrefabDocument::new(document.roots, document.entities);
    let published =
        PrefabDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    for field in [
        "required_default",
        "optional_default",
        "optional_absent",
        "explicit_null",
    ] {
        let before = session.document().clone();
        let history = session.history_status();
        let revision = session.revision();
        let report = session.apply_authoring_patch(&reset(field));
        assert!(report.applied, "{:?}", report.diagnostics);
        assert_eq!(session.revision().generation(), revision.generation() + 1);
        assert_eq!(session.history_status().undo_depth, history.undo_depth + 1);
        assert_effective(&bundle, raw_value(&session.document().entities), field);
        let accepted = session.document().clone();
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &before);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &accepted);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    }
    let encoded = session.document().to_json_string().unwrap();
    let reopened = session
        .try_reopen_authoring_candidate(
            PrefabDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
        )
        .unwrap();
    assert_eq!(reopened.document(), session.document());
    assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn data_only_field_write_inserts_absent_override_and_undo_removes_it() {
    let (bundle, adapter, mut document) = fixture();
    document.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value
        .remove_path(&ComponentFieldPath::from_fields(["required_default"]))
        .unwrap();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter.clone()])
            .unwrap()
            .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let edit = AuthoringFieldEdit::single(
        entity(),
        component(),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("required_default"),
        ComponentValue::I64(99),
    )
    .unwrap();
    let report = session.apply_authoring_field_edit(edit.clone());
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.inverse, Some(reset("required_default")));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &document);
    assert!(session.redo_authoring().applied);
    assert_eq!(
        raw_value(&session.document().entities).get("required_default"),
        Some(&ComponentValue::I64(99))
    );

    let prefab = PrefabDocument::new(document.roots, document.entities);
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let report = session.apply_authoring_field_edit(edit);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(report.inverse, Some(reset("required_default")));
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
    assert!(session.redo_authoring().applied);
    assert_eq!(
        raw_value(&session.document().entities).get("required_default"),
        Some(&ComponentValue::I64(99))
    );
}

#[test]
fn data_only_reset_rejects_required_unavailable_and_readonly_fields_atomically() {
    let (bundle, adapter, document) = fixture();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let revision = session.revision();
    let history = session.history_status();
    let content = session.content_state();
    for field in ["required", "readonly", "missing"] {
        let mut patch = reset("required_default");
        patch.operations.extend(reset(field).operations);
        let report = session.apply_authoring_patch(&patch);
        assert!(!report.applied);
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    }
    let mut patch = reset("required_default");
    patch.operations.push(ScenePatchOperation::RemoveField {
        entity: entity(),
        component: ComponentTypeId::new("game.Unavailable"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
    });
    assert!(!session.apply_authoring_patch(&patch).applied);
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
}

#[test]
fn data_only_reset_bounds_inverse_values_before_publication() {
    let (bundle, adapter, mut document) = fixture();
    document.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value
        .set_path(
            &ComponentFieldPath::from_fields(["text"]),
            ComponentValue::String("x".repeat(70 * 1024)),
        )
        .unwrap();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(source(), &bundle, &[adapter])
            .unwrap()
            .publish();
    let mut session = SceneAuthoringSession::from_published_authoring_document(published);
    let revision = session.revision();
    let report = session.apply_authoring_patch(&reset("text"));
    assert!(!report.applied);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.authoring-field-patch-unavailable")
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status().undo_depth, 0);
}
