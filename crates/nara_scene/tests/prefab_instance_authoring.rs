#![cfg(feature = "serde")]

#[path = "support/prefab_source.rs"]
mod prefab_source;

use nara_asset::AssetRef;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringRecordLocation, PrefabAddition, PrefabAuthoringSession,
    PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument, PrefabDocumentCandidate,
    PrefabEntityTarget, PrefabInstance, PrefabOverlay, SceneAuthoringHistoryLimits,
    SceneAuthoringSession, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
    SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementParent, UnavailableRecordManifestEntry,
};

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn instance(name: &str, additions: usize) -> PrefabInstance {
    let mut overlay = PrefabOverlay::new(prefab_source::key(name));
    for index in 0..additions {
        let entity = id(&format!("addition_{index}"));
        overlay.additions.push(PrefabAddition::new(entity.clone()));
        overlay.placements.push(SceneEntityPlacement::new(
            PrefabEntityTarget::addition(entity),
            ScenePlacement::last(ScenePlacementParent::Root),
        ));
    }
    PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap(),
        arguments: None,
        overrides: Some(overlay),
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
    // Unrelated unavailable data remains a preservation obligation during instance editing.
    anchor.components.insert(
        ComponentTypeId::new("missing.Opaque"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("payload", ComponentValue::String("keep".into()))]),
        ),
    );
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("anchor")),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(id("local_child")),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    id("anchor"),
                ))),
            ),
        ],
        [anchor, SceneEntityRecord::new(id("local_child"))],
    )
}

#[test]
fn prefab_source_preparation_retains_exact_instance_inverse_and_local_records() {
    let original = instance("unresolved", 2);
    let document = document(Some(original.clone()));
    let prefab = PrefabDocument::new(document.placements.clone(), document.entities.clone());
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
fn declaration_replacement_repairs_only_its_overlay_and_restores_the_exact_manifest() {
    let mut original = instance("base", 0);
    let mut added = PrefabAddition::new(id("unavailable"));
    added.components.insert(
        ComponentTypeId::new("missing.Override"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("preserve".into()),
        ),
    );
    let overlay = original.overrides.as_mut().unwrap();
    overlay.additions.push(added);
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("unavailable")),
        ScenePlacement::last(ScenePlacementParent::Root),
    ));
    let mut document = document(Some(original.clone()));
    let mut other = SceneEntityRecord::new(id("other-anchor"));
    other.prefab = Some(original.clone());
    document
        .placements
        .push(SceneEntityPlacement::root(other.id.clone()));
    document.entities.push(other);
    for replacement in [None, Some(instance("base", 0))] {
        let mut session = open(&document, Default::default());
        let revision = session.revision();
        let content = session.content_state();
        let history = session.history_status();
        let manifest = session.unavailable_record_manifest().unwrap().clone();
        assert_eq!(manifest.record_count(), 3);
        let readmission = session.authoring_readmission().unwrap();
        let protected = manifest
            .entries()
            .iter()
            .filter(|entry| {
                !matches!(entry, UnavailableRecordManifestEntry::Record(record)
                    if matches!(record.location(), AuthoringRecordLocation::OverlayComponent {
                        anchor, ..
                    } if anchor == &id("anchor")))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(protected.len(), 2);
        let patch =
            ScenePatchDocument::new([set("anchor", Some(original.clone()), replacement.clone())]);
        // Repairing this declaration grants no authority over its direct opaque component
        // or the other anchor's unavailable subtree, even inside the same transaction.
        for operation in [
            ScenePatchOperation::SetField {
                entity: id("anchor"),
                component: ComponentTypeId::new("missing.Opaque"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("payload"),
                value: ComponentValue::String("changed".into()),
            },
            ScenePatchOperation::RemoveComponent {
                entity: id("anchor"),
                component: ComponentTypeId::new("missing.Opaque"),
            },
            ScenePatchOperation::RemoveEntity {
                entity: id("other-anchor"),
            },
        ] {
            let mut guarded = patch.clone();
            let removes_entity = matches!(&operation, ScenePatchOperation::RemoveEntity { .. });
            guarded.operations.push(operation);
            if removes_entity {
                guarded.operations.push(ScenePatchOperation::SetPlacements {
                    expected: document.placements.clone(),
                    value: document
                        .placements
                        .iter()
                        .filter(|entry| {
                            entry.target != PrefabEntityTarget::source(id("other-anchor"))
                        })
                        .cloned()
                        .collect(),
                });
            }
            let rejected = session
                .prepare_authoring_edit(revision, &guarded)
                .unwrap_err();
            assert!(!rejected.applied);
            assert!(rejected.inverse.is_none());
            assert_eq!(session.document(), &document);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        }
        let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
        let mut repaired = document.clone();
        repaired.entities[0].prefab = replacement.clone();
        assert_eq!(prepared.document(), &repaired);
        assert_eq!(
            prepared.inverse(),
            &ScenePatchDocument::new([set("anchor", replacement, Some(original.clone()))])
        );
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.history_status(), history);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        assert!(session.commit_authoring_edit(prepared).applied);
        assert_eq!(session.document(), &repaired);
        assert_eq!(session.history_status().undo_depth, 1);
        assert_eq!(
            session.unavailable_record_manifest().unwrap().entries(),
            protected
        );
        assert_eq!(
            session
                .unavailable_record_manifest()
                .unwrap()
                .overlay_declarations()
                .find(|(anchor, _)| *anchor == &id("other-anchor"))
                .map(|(_, declaration)| declaration),
            Some(&original)
        );
        let repaired_readmission = session.authoring_readmission().unwrap();
        let reopened = repaired_readmission
            .reopen_scene_candidate(
                SceneDocumentCandidate::decode_ron_bytes(
                    session.document().to_ron_string().unwrap().as_bytes(),
                )
                .unwrap(),
            )
            .unwrap();
        assert!(repaired_readmission.preserves_scene(&reopened));
        assert!(!readmission.preserves_scene(&reopened));
        assert_eq!(reopened.document(), &repaired);
        // A saved repair establishes a new manifest only for its own exact result.
        for direct in [false, true] {
            let mut drifted = repaired.clone();
            if direct {
                drifted.entities[0]
                    .components
                    .get_mut(&ComponentTypeId::new("missing.Opaque"))
                    .unwrap()
                    .value = ComponentValue::String("changed".into());
            } else {
                drifted
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == id("other-anchor"))
                    .unwrap()
                    .prefab = None;
            }
            let reopened = repaired_readmission
                .reopen_scene_candidate(
                    SceneDocumentCandidate::decode_json_bytes(
                        drifted.to_json_string().unwrap().as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(
                !repaired_readmission.preserves_scene(&reopened),
                "direct={direct}"
            );
        }
        assert!(session.undo_authoring().applied);
        assert_eq!(session.document(), &document);
        assert_eq!(session.content_state(), content);
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        let restored = readmission
            .reopen_scene_candidate(
                SceneDocumentCandidate::decode_json_bytes(
                    session.document().to_json_string().unwrap().as_bytes(),
                )
                .unwrap(),
            )
            .unwrap();
        assert!(readmission.preserves_scene(&restored));
        assert_eq!(restored.document(), &document);
        assert!(session.redo_authoring().applied);
        assert_eq!(session.document(), &repaired);
        assert!(repaired_readmission.preserves_scene(&session));
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
    let base = prefab_source::key("history");
    let mut overlay = PrefabOverlay::new(base);
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("e")),
        component: ComponentTypeId::new("c"),
        record: SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null)
            .with_owner(
                nara_reflect::PersistentComponentSchemaOwnerId::try_new("test.owner").unwrap(),
            ),
    });
    let declaration = PrefabInstance {
        binding: nara_scene::PrefabInstanceBinding::Forest,
        source: AssetRef::path("p").unwrap(),
        arguments: None,
        overrides: Some(overlay),
    };
    let mut anchor = SceneEntityRecord::new(id("a"));
    anchor.prefab = Some(declaration.clone());
    let document = SceneDocument::from_ordered_roots([anchor]);
    let patch = ScenePatchDocument::new([set("a", Some(declaration.clone()), Some(declaration))]);
    // Each snapshot owns instance/binding/overlay/base/Set/target/record/value: eight work nodes.
    // Its bytes are source wire value (13 + path), Forest tag (1), Named base (37 + name),
    // intent/mode (2), target origin/ID (2), component ID (1), version/owner flag (5),
    // and owner text (10). The Null value has no payload bytes.
    let snapshot_bytes = (13 + "p".len())
        + 1
        + (37 + "history".len())
        + 2
        + (1 + "e".len())
        + "c".len()
        + 5
        + "test.owner".len();
    let exact_bytes = "a".len() + 2 * snapshot_bytes;
    let exact_operations = 1 + 2 * 8;
    assert_eq!((exact_operations, exact_bytes), (17, 159));
    for (operations, bytes, accepted) in [
        (exact_operations, exact_bytes, true),
        (exact_operations - 1, exact_bytes, false),
        (exact_operations, exact_bytes - 1, false),
    ] {
        let limits =
            SceneAuthoringHistoryLimits::new(8, 32, 64, 1024, operations, 64, bytes).unwrap();
        let session = open(&document, limits);
        let revision = session.revision();
        let result = session.prepare_authoring_edit(revision, &patch);
        if accepted {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert!(result.is_err());
        }
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status().undo_depth, 0);
    }
}
