#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    AuthoringDocumentSource, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    PrefabInstance, SceneAuthoringHistoryLimits, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn instance(name: &str, additions: usize) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap(),
        overrides: ScenePatchDocument::new((0..additions).map(|index| {
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(id(&format!("addition_{index}"))),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }
        })),
    }
}

fn set(
    entity: &str,
    expected: Option<PrefabInstance>,
    value: Option<PrefabInstance>,
) -> ScenePatchOperation {
    ScenePatchOperation::SetPrefabInstance {
        entity: id(entity),
        expected,
        value,
    }
}

fn open(document: &SceneDocument, limits: SceneAuthoringHistoryLimits) -> SceneAuthoringSession {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/instance.scene.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    SceneAuthoringSession::from_published_authoring_document_with_history_limits(published, limits)
}

fn document(declaration: Option<PrefabInstance>) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    anchor.prefab = declaration;
    anchor.children.push(id("local_child"));
    // Unrelated unavailable data remains a preservation obligation during instance editing.
    anchor.components.insert(
        ComponentTypeId::new("missing.Opaque"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("payload", ComponentValue::String("keep".into()))]),
        ),
    );
    SceneDocument::new(
        [id("anchor")],
        [anchor, SceneEntityRecord::new(id("local_child"))],
    )
}

#[test]
fn prefab_source_preparation_retains_exact_instance_inverse_and_local_records() {
    let original = instance("unresolved", 2);
    let document = document(Some(original.clone()));
    let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let published =
        PrefabDocumentCandidate::decode_json_bytes(prefab.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/container.prefab.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
    let revision = session.revision();
    let content = session.content_state();
    let patch = ScenePatchDocument::new([set("anchor", Some(original.clone()), None)]);
    let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
    assert_eq!(session.document(), &prefab);
    let mut detached = prefab.clone();
    detached
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("anchor"))
        .unwrap()
        .prefab = None;
    assert_eq!(prepared.document(), &detached);
    assert_eq!(
        prepared.inverse(),
        &ScenePatchDocument::new([set("anchor", None, Some(original))])
    );
    assert!(session.commit_authoring_edit(prepared).applied);
    assert_eq!(session.document(), &detached);
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo_authoring().applied);
    assert_eq!(session.document(), &prefab);
    assert_eq!(session.content_state(), content);
    assert!(session.redo_authoring().applied);
    assert_eq!(session.document(), &detached);
}

#[test]
fn complete_prior_intent_is_checked_and_the_final_mismatch_publishes_nothing() {
    let original = instance("base", 1);
    let document = document(Some(original.clone()));
    let session = open(&document, Default::default());
    let revision = session.revision();
    let content = session.content_state();
    let history = session.history_status();
    let replacement = instance("successor", 0);
    // The lookup matches the first replacement, but its full retained intent does not.
    let incorrect = instance("successor", 1);
    let patch = ScenePatchDocument::new([
        set("anchor", Some(original), Some(replacement)),
        set("anchor", Some(incorrect), None),
    ]);
    let rejected = session
        .prepare_authoring_edit(revision, &patch)
        .unwrap_err();
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "scene.patch-prefab-instance-context-mismatch")
    );
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.content_state(), content);
    assert_eq!(session.history_status(), history);
}

#[test]
fn instance_payload_work_and_both_way_history_are_not_hidden_in_one_operation() {
    let original = instance("base", 8);
    let document = document(Some(original.clone()));
    let limits = SceneAuthoringHistoryLimits::new(8, 32, 1024, 4096, 8, 1024, 4096).unwrap();
    let session = open(&document, limits);
    let history = session.history_status();
    let revision = session.revision();
    let patch = ScenePatchDocument::new([set("anchor", Some(original), None)]);
    assert!(session.prepare_authoring_edit(revision, &patch).is_err());
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
}

#[test]
fn declaration_replacement_does_not_grant_unavailable_record_deletion_or_repair() {
    let mut original = instance("base", 0);
    let mut added = SceneEntityRecord::new(id("unavailable"));
    added.components.insert(
        ComponentTypeId::new("missing.Override"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("preserve".into()),
        ),
    );
    original
        .overrides
        .operations
        .push(ScenePatchOperation::AddEntity {
            entity: added,
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        });
    let document = document(Some(original.clone()));
    let session = open(&document, Default::default());
    let revision = session.revision();
    let history = session.history_status();
    for replacement in [None, Some(instance("base", 0))] {
        assert!(
            session
                .prepare_authoring_edit(
                    revision,
                    &ScenePatchDocument::new([set("anchor", Some(original.clone()), replacement)])
                )
                .is_err()
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
    }
}

#[test]
fn cancelling_or_staling_an_instance_edit_leaves_the_current_owner_intact() {
    let document = document(None);
    let mut session = open(&document, Default::default());
    let patch = ScenePatchDocument::new([set("anchor", None, Some(instance("base", 1)))]);
    let revision = session.revision();
    let history = session.history_status();
    drop(session.prepare_authoring_edit(revision, &patch).unwrap());
    assert_eq!(session.document(), &document);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
    let stale = session.prepare_authoring_edit(revision, &patch).unwrap();
    let accepted = ScenePatchDocument::new([set("anchor", None, Some(instance("other", 0)))]);
    assert!(session.apply_authoring_patch(&accepted).applied);
    let current = session.document().clone();
    let revision = session.revision();
    let history = session.history_status();
    assert!(!session.commit_authoring_edit(stale).applied);
    assert_eq!(session.document(), &current);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.history_status(), history);
}

#[test]
fn complete_snapshot_owner_bytes_have_an_inclusive_history_ceiling() {
    let declaration = PrefabInstance {
        source: AssetRef::path("p").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
            entity: id("e"),
            component: ComponentTypeId::new("c"),
            value: SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null)
                .with_owner(
                    nara_reflect::PersistentComponentSchemaOwnerId::try_new("test.owner").unwrap(),
                ),
        }]),
    };
    let mut anchor = SceneEntityRecord::new(id("a"));
    anchor.prefab = Some(declaration.clone());
    let document = SceneDocument::from_ordered_roots([anchor]);
    let patch = ScenePatchDocument::new([set("a", Some(declaration.clone()), Some(declaration))]);
    // Target "a" plus two snapshots of source's canonical value (14), target "e" (1),
    // component "c" (1), owner "test.owner" (10); Null has no logical payload bytes.
    for bytes in [53, 52] {
        let limits = SceneAuthoringHistoryLimits::new(8, 32, 64, 1024, 16, 64, bytes).unwrap();
        let session = open(&document, limits);
        let revision = session.revision();
        let result = session.prepare_authoring_edit(revision, &patch);
        if bytes == 53 {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}
