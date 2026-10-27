use std::sync::Arc;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, EntityReference,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentSource, AuthoringSchemaBasis,
    PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate, PrefabInstance,
    SceneAuthoringHistoryLimits, SceneAuthoringSession, SceneComponentRecord,
    SceneDocumentCandidate, SceneEntityRecord, authoring::AuthoringHistoryScope,
};

pub(super) fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

pub(super) fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ConversionHistory")
}

pub(super) fn schema_owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.conversion-history").unwrap()
}

pub(super) fn basis() -> AuthoringSchemaBasis {
    // Copying an admitted record is not creation from an editable initial-value template.
    let schema = ComponentSchema::new(component(), "History", ComponentSchemaVersion::ONE)
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::Inspect])
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
        )
        .with_capability(ComponentCapability::Scene)
        .with_capability(ComponentCapability::Inspect)
        .with_capability(ComponentCapability::EntityRef)]);
    admit_schema(schema)
}

pub(super) fn admit_schema(schema: ComponentSchema) -> AuthoringSchemaBasis {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        schema_owner(),
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

pub(super) fn instance(name: &str) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap(),
        overrides: None,
    }
}

pub(super) fn original() -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor")).with_children([id("local-child")]);
    anchor.prefab = Some(instance("base"));
    anchor.components.insert(
        ComponentTypeId::new("test.Unavailable"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(73)),
    );
    let mut child = SceneEntityRecord::new(id("local-child"));
    child.prefab = Some(instance("missing-child"));
    let mut other = SceneEntityRecord::new(id("other"));
    other.prefab = Some(instance("missing-unrelated"));
    SceneDocument::new([id("anchor"), id("other")], [anchor, child, other])
}

fn patch() -> ScenePatchDocument {
    let value = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("localized-leaf"),
            }),
        )]),
    )
    .with_owner(schema_owner());
    ScenePatchDocument::new([
        ScenePatchOperation::SetPrefabInstance {
            entity: id("anchor"),
            expected: Some(instance("base")),
            value: None,
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id("localized-root")).with_component(component(), value),
            placement: ScenePlacement::before(
                ScenePlacementOwner::Parent {
                    parent: id("anchor"),
                },
                id("local-child"),
            ),
        },
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id("localized-leaf")),
            placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                parent: id("localized-root"),
            }),
        },
    ])
}

fn scope() -> AuthoringHistoryScope {
    AuthoringHistoryScope::PrefabConversion(
        PrefabConversionFootprint::new(
            &id("anchor"),
            &[id("localized-root"), id("localized-leaf")],
            SceneAuthoringHistoryLimits::default().max_entry_logical_bytes(),
        )
        .unwrap(),
    )
}

fn scene(limits: SceneAuthoringHistoryLimits) -> SceneAuthoringSession {
    let published = SceneDocumentCandidate::decode_json_str(&original().to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/main.scene.json").unwrap()),
            &basis(),
        )
        .unwrap()
        .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(published, limits)
}

#[test]
fn source_owned_footprint_replays_complete_records_with_unrelated_links_and_unavailable_data() {
    let mut session = scene(Default::default());
    let before = session.shared_document();
    let before_state = session.content_state();
    let manifest = session.unavailable_record_manifest().unwrap().clone();
    let patch = patch();
    assert!(
        session
            .prepare_authoring_edit(session.revision(), &patch)
            .is_err()
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    let prepared = session
        .prepare_scoped_authoring_edit(session.revision(), &patch, scope())
        .unwrap();
    assert_eq!(session.document(), before.as_ref());
    let reviewed = prepared.document().clone();
    let report = session.commit_authoring_edit(prepared);
    assert!(report.applied, "{:?}", report.diagnostics);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(session.history_status().undo_depth, 1);
    let converted_manifest = session.unavailable_record_manifest().unwrap().clone();
    assert_eq!(converted_manifest.entries(), manifest.entries());
    assert_eq!(
        converted_manifest
            .overlay_declarations()
            .collect::<Vec<_>>(),
        manifest
            .overlay_declarations()
            .filter(|(anchor, _)| *anchor != &id("anchor"))
            .collect::<Vec<_>>()
    );
    for original in &before.entities {
        let retained = reviewed
            .entities
            .iter()
            .find(|record| record.id == original.id)
            .unwrap();
        if original.id == id("anchor") {
            assert_eq!(retained.components, original.components);
        } else {
            assert_eq!(retained, original);
        }
    }
    assert_eq!(reviewed.roots, before.roots);
    let anchor = session
        .document()
        .entities
        .iter()
        .find(|record| record.id == id("anchor"))
        .unwrap();
    assert_eq!(anchor.children, [id("localized-root"), id("local-child")]);
    assert!(anchor.prefab.is_none());
    let accepted_state = session.content_state();
    let admitted_usage = session.history_usage();
    assert!(
        session
            .prepare_authoring_edit(session.revision(), report.inverse.as_ref().unwrap())
            .is_err(),
        "an exposed inverse does not transfer the private history footprint"
    );
    assert_eq!(session.document(), &reviewed);
    assert_eq!(session.history_usage(), admitted_usage);

    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), before.as_ref());
    assert_eq!(session.content_state(), before_state);
    assert_eq!(session.history_status().redo_depth, 1);
    assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
    assert_eq!(session.history_usage(), admitted_usage);

    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(session.content_state(), accepted_state);
    assert_eq!(session.history_status().undo_depth, 1);
    assert_eq!(session.history_usage(), admitted_usage);
    assert_eq!(
        session.unavailable_record_manifest(),
        Some(&converted_manifest)
    );
}

#[test]
fn prefab_source_uses_the_same_scoped_history_without_a_scene_publication() {
    let source = original();
    let document = PrefabDocument::new(source.roots, source.entities);
    let published = PrefabDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/main.prefab.json").unwrap()),
            &basis(),
        )
        .unwrap()
        .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let before = session.shared_document();
    let prepared = session
        .prepare_scoped_authoring_edit(session.revision(), &patch(), scope())
        .unwrap();
    let reviewed = prepared.document().clone();
    let committed = session.commit_authoring_edit(prepared);
    assert!(committed.applied, "{:?}", committed.diagnostics);
    assert_eq!(session.document(), &reviewed);
    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), before.as_ref());
    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
    assert_eq!(session.history_status().undo_depth, 1);
}

#[test]
fn scope_refuses_foreign_records_mixed_directions_and_partial_entity_sets() {
    let session = scene(Default::default());
    let before = session.shared_document();
    let revision = session.revision();
    let mut foreign = patch();
    if let ScenePatchOperation::AddEntity { entity, .. } = &mut foreign.operations[1] {
        entity.id = id("local-child");
    }
    let mut partial = patch();
    partial.operations.pop();
    let mut mixed = patch();
    mixed.operations[1] = ScenePatchOperation::RemoveEntity {
        entity: id("localized-root"),
        from: ScenePlacement::last(ScenePlacementOwner::Parent {
            parent: id("anchor"),
        }),
    };
    for invalid in [foreign, partial, mixed] {
        let rejected = session
            .prepare_scoped_authoring_edit(revision, &invalid, scope())
            .unwrap_err();
        assert!(!rejected.applied);
        assert!(Arc::ptr_eq(&before, &session.shared_document()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}

#[test]
fn footprint_retention_is_part_of_the_preparation_and_history_byte_budget() {
    let patch = patch();
    let bytes = super::super::SceneAuthoringHistoryCost::for_patch(&patch).logical_bytes;
    let defaults = SceneAuthoringHistoryLimits::default();
    let limits = SceneAuthoringHistoryLimits::new(
        defaults.max_records(),
        defaults.max_operations(),
        defaults.max_value_nodes(),
        defaults.max_logical_bytes(),
        defaults.max_entry_operations(),
        defaults.max_entry_value_nodes(),
        bytes,
    )
    .unwrap();
    let session = scene(limits);
    let before = session.shared_document();
    let rejected = session
        .prepare_scoped_authoring_edit(session.revision(), &patch, scope())
        .unwrap_err();
    assert!(!rejected.applied);
    assert!(
        rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-history-entry-limit-exceeded"
        }),
        "{:?}",
        rejected.diagnostics,
    );
    assert!(Arc::ptr_eq(&before, &session.shared_document()));
    assert_eq!(session.history_usage().records, 0);
}

#[test]
fn an_invalid_final_reference_or_unavailable_insert_never_publishes() {
    let session = scene(Default::default());
    let before = session.shared_document();
    let revision = session.revision();
    let mut bad_reference = patch();
    if let ScenePatchOperation::AddEntity { entity, .. } = &mut bad_reference.operations[1] {
        entity.components.get_mut(&component()).unwrap().value = ComponentValue::map([(
            "target",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: id("missing-target"),
            }),
        )]);
    }
    let mut future = patch();
    if let ScenePatchOperation::AddEntity { entity, .. } = &mut future.operations[1] {
        entity.components.get_mut(&component()).unwrap().version = ComponentSchemaVersion(2);
    }
    for invalid in [bad_reference, future] {
        let refused = session
            .prepare_scoped_authoring_edit(revision, &invalid, scope())
            .unwrap_err();
        assert!(!refused.applied);
        assert!(Arc::ptr_eq(&before, &session.shared_document()));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_usage().records, 0);
    }
}

#[test]
fn an_empty_localized_projection_still_has_one_exact_source_history_entry() {
    let mut session = scene(Default::default());
    let before = session.shared_document();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
        entity: id("anchor"),
        expected: Some(instance("base")),
        value: None,
    }]);
    let scope = AuthoringHistoryScope::PrefabConversion(
        PrefabConversionFootprint::new(
            &id("anchor"),
            &[],
            session.history_limits().max_entry_logical_bytes(),
        )
        .unwrap(),
    );
    let prepared = session
        .prepare_scoped_authoring_edit(session.revision(), &patch, scope)
        .unwrap();
    let reviewed = prepared.document().clone();
    let commit = session.commit_authoring_edit(prepared);
    assert!(commit.applied, "{:?}", commit.diagnostics);
    assert_eq!(session.history_status().undo_depth, 1);
    let undo = session.undo_authoring();
    assert!(undo.applied, "{:?}", undo.diagnostics);
    assert_eq!(session.document(), before.as_ref());
    let redo = session.redo_authoring();
    assert!(redo.applied, "{:?}", redo.diagnostics);
    assert_eq!(session.document(), &reviewed);
}
