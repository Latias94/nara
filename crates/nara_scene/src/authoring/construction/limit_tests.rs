use std::sync::Arc;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};

use super::{
    AuthoringConstructionDraft, MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_LOGICAL_BYTES,
    MAX_AUTHORING_FIELD_VALUE_NODES,
};
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringPropertyEdit,
    AuthoringSchemaBasis, MAX_AUTHORING_FIELD_WRITES, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

const FIELDS: [&str; 10] = [
    "field_0", "field_1", "field_2", "field_3", "field_4", "field_5", "field_6", "field_7",
    "field_8", "field_9",
];

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ConstructionLimits")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.construction-limits").unwrap()
}

fn existing() -> SceneEntityId {
    SceneEntityId::new("existing").unwrap()
}

fn values(base: u64) -> ComponentValue {
    ComponentValue::map(
        FIELDS
            .iter()
            .enumerate()
            .map(|(index, name)| (*name, ComponentValue::U64(base + index as u64))),
    )
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        component(),
        "Construction limits",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(FIELDS.map(|name| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name),
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    }))
    .with_authoring_initial_value(values(0));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
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
    assert!(adapter.contribution().is_none());
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn document(with_component: bool) -> SceneDocument {
    let mut entity = SceneEntityRecord::new(existing());
    if with_component {
        entity = entity.with_component(
            component(),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, values(0)).with_owner(owner()),
        );
    }
    SceneDocument::from_ordered_roots([entity])
}

fn scene(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> SceneAuthoringSession {
    SceneAuthoringSession::from_published_authoring_document(
        SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/construction-limits.scene.json").unwrap(),
                ),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

fn prefab(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> PrefabAuthoringSession {
    let document = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    PrefabAuthoringSession::from_published_authoring_document(
        PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/construction-limits.prefab.json").unwrap(),
                ),
                basis,
            )
            .unwrap()
            .publish(),
    )
}

macro_rules! each_source {
    ($document:expr, $basis:expr, |$session:ident| $body:block) => {{
        {
            let mut $session = scene($document, $basis);
            $body
        }
        {
            let mut $session = prefab($document, $basis);
            $body
        }
    }};
}

#[test]
fn ten_field_construction_preserves_templates_and_one_scene_and_prefab_history_entry() {
    let basis = basis();
    each_source!(&document(false), &basis, |session| {
        let before = session.shared_document();
        let revision = session.revision();
        let content = session.content_state();
        let history = session.history_status();
        let source = session.authoring_source().cloned();
        let template = session.authoring_initial_component(&component()).unwrap();
        assert_eq!(template.value, values(0));
        assert_eq!(template.owner(), Some(&owner()));
        let mut draft = session.begin_authoring_construction(revision).unwrap();
        let local = draft
            .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
            .unwrap();
        draft.add_component(&local, component()).unwrap();
        for (index, name) in FIELDS.iter().enumerate() {
            draft
                .set_property(
                    &local,
                    component(),
                    ComponentPropertyPath::new(ComponentFieldId::new(*name)),
                    ComponentValue::U64(100 + index as u64),
                )
                .unwrap();
        }

        let prepared = session.prepare_authoring_construction(&draft).unwrap();
        let created = prepared.entity(&local).unwrap().clone();
        let expected = prepared.document().clone();
        assert_eq!(prepared.patch().operations.len(), FIELDS.len() + 2);
        assert!(matches!(
            &prepared.patch().operations[1],
            ScenePatchOperation::AddComponent { entity, value, .. }
                if entity == &created && value == &template
        ));
        let written: Vec<_> = prepared
            .patch()
            .operations
            .iter()
            .filter_map(|operation| match operation {
                ScenePatchOperation::SetField { field, .. } => Some(field.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(written, FIELDS);
        let record = &expected
            .entities
            .iter()
            .find(|entity| entity.id == created)
            .unwrap()
            .components[&component()];
        assert_eq!(record.value, values(100));
        assert_eq!(record.owner(), Some(&owner()));
        assert_eq!(expected.roots, [existing(), created.clone()]);
        assert!(Arc::ptr_eq(&before, &session.shared_document()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.history_status(), history);

        let committed = session.commit_authoring_construction(prepared).unwrap();
        assert!(committed.report().applied);
        assert_eq!(committed.entity(&local), Some(&created));
        assert_eq!(session.document(), &expected);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_eq!(session.history_status().redo_depth, 0);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), before.as_ref());
        assert_eq!(session.content_state(), content);
        assert_eq!(session.history_status().undo_depth, 0);
        assert_eq!(session.history_status().redo_depth, 1);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &expected);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_eq!(session.history_status().redo_depth, 0);
        assert_eq!(committed.entity(&local), Some(&created));
        assert_eq!(session.authoring_source(), source.as_ref());
        assert_eq!(session.authoring_schema_basis(), Some(basis.clone()));
    });
}

#[test]
fn ordinary_scene_and_prefab_property_edits_keep_the_eight_write_limit() {
    assert_eq!(MAX_AUTHORING_FIELD_WRITES, 8);
    let basis = basis();
    each_source!(&document(true), &basis, |session| {
        let before = session.shared_document();
        let revision = session.revision();
        let content = session.content_state();
        let history = session.history_status();
        let edits: Vec<_> = FIELDS
            .iter()
            .enumerate()
            .map(|(index, name)| AuthoringPropertyEdit {
                entity: existing(),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                property: ComponentPropertyPath::new(ComponentFieldId::new(*name)),
                value: ComponentValue::U64(100 + index as u64),
            })
            .collect();

        for count in [9, 10] {
            let edits = &edits[..count];
            assert!(
                session
                    .prepare_authoring_property_edit(revision, edits)
                    .is_err()
            );
            let patch =
                ScenePatchDocument::new(edits.iter().map(|edit| ScenePatchOperation::SetField {
                    entity: edit.entity.clone(),
                    component: edit.component.clone(),
                    component_version: edit.component_version,
                    field: edit.property.root.clone(),
                    value: edit.value.clone(),
                }));
            assert!(session.prepare_authoring_edit(revision, &patch).is_err());
            let refused = session.apply_authoring_patch(&patch);
            assert!(!refused.applied);
            assert!(refused.inverse.is_none());
            assert!(Arc::ptr_eq(&before, &session.shared_document()));
            assert_eq!(session.document(), before.as_ref());
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
        }

        let prepared = session
            .prepare_authoring_property_edit(revision, &edits[..8])
            .unwrap();
        assert_eq!(prepared.patch().operations.len(), 8);
        let expected = prepared.document().clone();
        assert!(session.commit_authoring_edit(prepared).applied);
        assert_eq!(session.document(), &expected);
        assert_eq!(session.history_status().undo_depth, 1);
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), before.as_ref());
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &expected);
    });
}

#[test]
fn declaration_free_construction_rejects_nine_repeated_writes_before_coalescing() {
    let basis = basis();
    each_source!(&document(true), &basis, |session| {
        let original = session.shared_document();
        let revision = session.revision();
        let mut draft = session.begin_authoring_construction(revision).unwrap();
        for value in 100..109 {
            draft
                .set_property(
                    existing(),
                    component(),
                    ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
                    ComponentValue::U64(value),
                )
                .unwrap();
        }
        assert!(session.prepare_authoring_construction(&draft).is_err());
        assert!(Arc::ptr_eq(&original, &session.shared_document()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
        let mut bounded = session.begin_authoring_construction(revision).unwrap();
        for value in 100..108 {
            bounded
                .set_property(
                    existing(),
                    component(),
                    ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
                    ComponentValue::U64(value),
                )
                .unwrap();
        }
        let prepared = session.prepare_authoring_construction(&bounded).unwrap();
        assert_eq!(prepared.patch().operations.len(), 1);
        assert!(session.commit_authoring_construction(prepared).is_ok());
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), original.as_ref());
    });
}

#[test]
fn mixed_nine_field_transaction_admits_history_cost_before_copying_a_large_component() {
    use crate::authoring::{AuthoringHistory, AuthoringHistoryScope, SceneAuthoringHistoryLimits};

    let large = ComponentTypeId::new("test.ConstructionLimits.Large");
    let value = ComponentValue::String("x".repeat(60_000));
    let large_schema =
        ComponentSchema::new(large.clone(), "Large template", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::empty(),
                ComponentValueKind::String,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
            .with_authoring_initial_value(value.clone());
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![basis().schema(&component()).unwrap().clone(), large_schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let patch = ScenePatchDocument::new(
        std::iter::once(ScenePatchOperation::AddComponent {
            entity: existing(),
            component: large,
            value: SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
                .with_owner(owner()),
        })
        .chain(
            FIELDS[..9]
                .iter()
                .map(|field| ScenePatchOperation::SetField {
                    entity: existing(),
                    component: component(),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new(*field),
                    value: ComponentValue::U64(100),
                }),
        ),
    );
    let ordinary = scene(&document(true), &basis);
    assert!(
        ordinary
            .prepare_authoring_edit(ordinary.revision(), &patch)
            .is_ok()
    );
    let limits =
        SceneAuthoringHistoryLimits::new(8, 2048, 8192, 400_000, 256, 1024, 50_000).unwrap();
    let refusal = AuthoringHistory::admit_input(limits, &patch, &AuthoringHistoryScope::Ordinary)
        .expect_err("valid mixed input must cross the history budget before source copying");
    assert!(
        refusal.diagnostics.iter().any(|entry| {
            entry.code().as_str() == "scene.authoring-history-entry-limit-exceeded"
        })
    );
}

fn usage(draft: &AuthoringConstructionDraft) -> (usize, usize, usize, usize, usize) {
    (
        draft.entities.len(),
        draft.components.len(),
        draft.writes.len(),
        draft.write_nodes,
        draft.write_bytes,
    )
}

#[test]
fn declaration_append_admits_exactly_256_entities_components_or_writes_in_total() {
    assert_eq!(MAX_AUTHORING_EDIT_OPERATIONS, 256);
    let session = scene(&document(true), &basis());
    for (entities, components, writes) in [(256, 0, 0), (0, 256, 0), (0, 0, 256), (85, 85, 86)] {
        let mut draft = session
            .begin_authoring_construction(session.revision())
            .unwrap();
        for _ in 0..entities {
            draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .unwrap();
        }
        for _ in 0..components {
            draft.add_component(existing(), component()).unwrap();
        }
        for _ in 0..writes {
            draft
                .set_property(
                    existing(),
                    component(),
                    ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
                    ComponentValue::U64(1),
                )
                .unwrap();
        }
        let accepted = usage(&draft);
        assert_eq!(accepted.0 + accepted.1 + accepted.2, 256);
        assert_eq!(accepted.3, writes);
        assert!(accepted.4 < MAX_AUTHORING_FIELD_LOGICAL_BYTES);
        assert!(
            draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                .is_err()
        );
        assert_eq!(usage(&draft), accepted);
        assert!(draft.add_component(existing(), component()).is_err());
        assert_eq!(usage(&draft), accepted);
        assert!(
            draft
                .set_property(
                    existing(),
                    component(),
                    ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
                    ComponentValue::U64(1),
                )
                .is_err()
        );
        assert_eq!(usage(&draft), accepted);
    }
}

#[test]
fn construction_write_value_nodes_remain_bounded_at_256() {
    assert_eq!(MAX_AUTHORING_FIELD_VALUE_NODES, 256);
    let session = scene(&document(true), &basis());
    let mut draft = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    // Append checks retained value cost; Schema compatibility is a separate preparation gate.
    draft
        .set_property(
            existing(),
            component(),
            ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
            ComponentValue::List(vec![ComponentValue::U64(1); 255]),
        )
        .unwrap();
    assert_eq!(draft.write_nodes, 256);
    let accepted = usage(&draft);
    assert!(
        draft
            .set_property(
                existing(),
                component(),
                ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0])),
                ComponentValue::U64(1),
            )
            .is_err()
    );
    assert_eq!(usage(&draft), accepted);
    draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    draft.add_component(existing(), component()).unwrap();
}

#[test]
fn construction_write_logical_bytes_remain_bounded_at_64_kib() {
    assert_eq!(MAX_AUTHORING_FIELD_LOGICAL_BYTES, 64 * 1024);
    let session = scene(&document(true), &basis());
    let mut draft = session
        .begin_authoring_construction(session.revision())
        .unwrap();
    let property = ComponentPropertyPath::new(ComponentFieldId::new(FIELDS[0]));
    let selector_bytes = property.logical_bytes().unwrap();
    draft
        .set_property(
            existing(),
            component(),
            property.clone(),
            ComponentValue::String("x".repeat(64 * 1024 - selector_bytes)),
        )
        .unwrap();
    assert_eq!(draft.write_bytes, 64 * 1024);
    let accepted = usage(&draft);
    assert!(
        draft
            .set_property(existing(), component(), property, ComponentValue::U64(1))
            .is_err()
    );
    assert_eq!(usage(&draft), accepted);
    draft
        .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
        .unwrap();
    draft.add_component(existing(), component()).unwrap();
}
