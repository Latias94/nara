use std::sync::Arc;

use nara_reflect::ComponentRegistry;
use nara_scene::{
    PrefabAuthoringSession, PrefabDocument, SceneAuthoringHistoryLimits, SceneAuthoringSession,
    SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
    ScenePlacement, ScenePlacementOwner,
};

#[cfg(feature = "serde")]
use nara_asset::AssetRef;
#[cfg(feature = "serde")]
use nara_reflect::AuthoringSchemaBundle;
#[cfg(feature = "serde")]
use nara_scene::{
    AuthoringDocumentReadiness, AuthoringDocumentSource, ConstructionPlacement,
    PrefabDocumentCandidate, SceneDocumentCandidate,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn add(names: &[&str]) -> ScenePatchDocument {
    ScenePatchDocument::new(names.iter().map(|name| ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }))
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    registry
}

macro_rules! source_snapshots {
    ($module:ident, $session:ty, $document:ty, $candidate:ty, $kind:literal, $source:literal) => {
        mod $module {
            use super::*;

            fn document() -> $document {
                <$document>::new(
                    [id("z-root"), id("tail")],
                    [
                        SceneEntityRecord::new(id("z-root")).with_children([id("a-child")]),
                        SceneEntityRecord::new(id("a-child")),
                        SceneEntityRecord::new(id("tail")),
                    ],
                )
            }

            #[cfg(feature = "serde")]
            fn candidate() -> $candidate {
                <$candidate>::decode_json_str(concat!(
                    "{\"kind\":\"",
                    $kind,
                    "\",",
                    r#"
                    "format_version": 1,
                    "engine_min_version": "0.1.0",
                    "generator": {"name": "nara", "version": "0.1.0"},
                    "payload": {
                        "roots": ["z-root", "tail"],
                        "entities": [
                            {"id": "a-child"},
                            {"id": "tail"},
                            {
                                "id": "z-root",
                                "children": ["a-child"],
                                "components": {
                                    "game.Missing": {
                                        "version": 1,
                                        "value": {"type": "string", "value": "retain exactly"}
                                    }
                                }
                            }
                        ]
                    }}
                    "#
                ))
                .unwrap()
            }

            #[cfg(feature = "serde")]
            fn open(limits: SceneAuthoringHistoryLimits) -> $session {
                let bundle = AuthoringSchemaBundle::new([]).unwrap();
                let published = candidate()
                    .admit_authoring(
                        AuthoringDocumentSource::new(AssetRef::path($source).unwrap()),
                        &bundle,
                        &[],
                    )
                    .unwrap()
                    .publish();
                <$session>::from_published_authoring_document_with_history_limits(
                    published, limits,
                )
            }

            #[test]
            fn native_snapshots_survive_edits_history_and_source_replacement() {
                let registry = registry();
                let original = document();
                let mut session = <$session>::new(original.clone());
                let before = session.shared_document();
                let initial_revision = session.revision();
                let initial_content = session.content_state();
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert!(std::ptr::eq(before.as_ref(), session.document()));
                assert_eq!(before.as_ref(), &original);

                assert!(!session.undo(&registry).applied);
                assert!(!session.redo(&registry).applied);
                assert!(!session.apply_patch(&add(&["temporary", "z-root"]), &registry).applied);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert_eq!(session.revision(), initial_revision);
                assert_eq!(session.content_state(), initial_content);
                assert_eq!(session.history_status().undo_depth, 0);

                let report = session.apply_patch(&add(&["accepted"]), &registry);
                assert!(report.applied, "{:?}", report.diagnostics);
                let mut expected = original.clone();
                expected.roots.push(id("accepted"));
                expected.entities.push(SceneEntityRecord::new(id("accepted")));
                expected.canonicalize();
                let accepted = session.shared_document();
                let accepted_revision = session.revision();
                let accepted_content = session.content_state();
                assert!(!Arc::ptr_eq(&before, &accepted));
                assert!(Arc::ptr_eq(&accepted, &session.shared_document()));
                assert!(std::ptr::eq(accepted.as_ref(), session.document()));
                assert_eq!(accepted.as_ref(), &expected);
                assert_eq!(before.as_ref(), &original);
                assert_eq!(accepted_revision.source_id(), initial_revision.source_id());
                assert_eq!(session.history_status().undo_depth, 1);

                assert!(session.undo(&registry).applied);
                let undone = session.shared_document();
                assert!(!Arc::ptr_eq(&accepted, &undone));
                assert_eq!(undone.as_ref(), &original);
                assert_eq!(session.content_state(), initial_content);
                assert_eq!(accepted.as_ref(), &expected);
                assert!(session.redo(&registry).applied);
                let redone = session.shared_document();
                assert!(!Arc::ptr_eq(&undone, &redone));
                assert_eq!(redone.as_ref(), &expected);
                assert_eq!(session.content_state(), accepted_content);
                assert!(session.revision().generation() > accepted_revision.generation());

                let replacement =
                    <$document>::from_ordered_roots([SceneEntityRecord::new(id("replacement"))]);
                session.replace_document(replacement.clone()).unwrap();
                let replaced = session.shared_document();
                assert!(!Arc::ptr_eq(&redone, &replaced));
                assert!(Arc::ptr_eq(&replaced, &session.shared_document()));
                assert_eq!(replaced.as_ref(), &replacement);
                assert_ne!(session.revision().source_id(), initial_revision.source_id());
                assert_eq!(session.revision().generation(), 0);
                assert!(!session.owns_content_state(initial_content));
                assert!(!session.acknowledge_source_saved(accepted_content));
                assert_eq!(session.history_status().undo_depth, 0);
                assert_eq!(session.history_status().redo_depth, 0);
                assert_eq!(session.history_usage().records, 0);
                assert!(!session.undo(&registry).applied);
                assert!(Arc::ptr_eq(&replaced, &session.shared_document()));
                drop(session);
                assert_eq!(before.as_ref(), &original);
                assert_eq!(accepted.as_ref(), &expected);
                assert_eq!(undone.as_ref(), &original);
                assert_eq!(redone.as_ref(), &expected);
                assert_eq!(replaced.as_ref(), &replacement);
            }

            #[test]
            fn history_admission_refusal_keeps_the_exact_shared_source() {
                let limits =
                    SceneAuthoringHistoryLimits::new(8, 64, 128, 4096, 64, 128, 1).unwrap();
                let mut session = <$session>::new_with_history_limits(document(), limits);
                let snapshot = session.shared_document();
                let revision = session.revision();
                let content = session.content_state();
                let history = session.history_status();
                let usage = session.history_usage();
                assert!(!session.apply_patch(&add(&["too-large"]), &registry()).applied);
                assert!(Arc::ptr_eq(&snapshot, &session.shared_document()));
                assert_eq!(snapshot.as_ref(), &document());
                assert_eq!(session.revision(), revision);
                assert_eq!(session.content_state(), content);
                assert_eq!(session.history_status(), history);
                assert_eq!(session.history_usage(), usage);

                #[cfg(feature = "serde")]
                {
                    let session = open(limits);
                    let snapshot = session.shared_document();
                    let revision = session.revision();
                    let history = session.history_status();
                    assert!(
                        session
                            .prepare_authoring_edit(revision, &add(&["too-large"]))
                            .is_err()
                    );
                    assert!(Arc::ptr_eq(&snapshot, &session.shared_document()));
                    assert_eq!(session.revision(), revision);
                    assert_eq!(session.history_status(), history);
                }
            }

            #[cfg(feature = "serde")]
            #[test]
            fn published_snapshots_survive_prepared_commit_history_and_reopen() {
                let mut session = open(SceneAuthoringHistoryLimits::default());
                let before = session.shared_document();
                let original = before.as_ref().clone();
                let revision = session.revision();
                let content = session.content_state();
                let history = session.history_status();
                let basis = session.authoring_schema_basis();
                let manifest = session.unavailable_record_manifest().unwrap().clone();
                assert_eq!(
                    session.authoring_readiness(),
                    Some(AuthoringDocumentReadiness::Degraded)
                );
                assert_eq!(manifest.record_count(), 1);
                assert!(!session.undo_authoring().applied);
                assert!(!session.redo_authoring().applied);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));

                let prepared = session
                    .prepare_authoring_edit(revision, &add(&["accepted"]))
                    .unwrap();
                let mut expected = original.clone();
                expected.roots.push(id("accepted"));
                expected.entities.push(SceneEntityRecord::new(id("accepted")));
                expected.canonicalize();
                assert_eq!(prepared.document(), &expected);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert!(std::ptr::eq(before.as_ref(), session.document()));
                assert_eq!(session.revision(), revision);
                assert_eq!(session.content_state(), content);
                assert_eq!(session.history_status(), history);
                let report = session.commit_authoring_edit(prepared);
                assert!(report.applied, "{:?}", report.diagnostics);
                let accepted = session.shared_document();
                let accepted_content = session.content_state();
                assert!(!Arc::ptr_eq(&before, &accepted));
                assert!(Arc::ptr_eq(&accepted, &session.shared_document()));
                assert!(std::ptr::eq(accepted.as_ref(), session.document()));
                assert_eq!(accepted.as_ref(), &expected);
                assert_eq!(before.as_ref(), &original);
                assert_eq!(session.history_status().undo_depth, 1);

                assert!(session.undo_authoring().applied);
                let undone = session.shared_document();
                assert!(!Arc::ptr_eq(&accepted, &undone));
                assert_eq!(undone.as_ref(), &original);
                assert_eq!(session.content_state(), content);
                assert_eq!(accepted.as_ref(), &expected);
                assert!(session.redo_authoring().applied);
                let redone = session.shared_document();
                assert!(!Arc::ptr_eq(&undone, &redone));
                assert_eq!(redone.as_ref(), &expected);
                assert_eq!(session.content_state(), accepted_content);
                assert_eq!(session.authoring_schema_basis(), basis);
                assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

                let prepared = session
                    .prepare_authoring_edit(session.revision(), &add(&["old-lineage"]))
                    .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate()).unwrap();
                assert!(Arc::ptr_eq(&redone, &session.shared_document()));
                assert_eq!(session.document(), &expected);
                assert_eq!(reopened.document(), &original);
                assert_eq!(reopened.authoring_schema_basis(), basis);
                assert_eq!(reopened.authoring_readiness(), session.authoring_readiness());
                assert_eq!(reopened.unavailable_record_manifest(), Some(&manifest));
                assert_eq!(reopened.history_limits(), session.history_limits());
                assert_ne!(reopened.revision().source_id(), revision.source_id());
                assert_eq!(reopened.revision().generation(), 0);
                assert!(!reopened.owns_content_state(accepted_content));
                assert_eq!(reopened.history_status().undo_depth, 0);
                assert_eq!(reopened.history_status().redo_depth, 0);
                session = reopened;
                let reopened_snapshot = session.shared_document();
                assert!(!Arc::ptr_eq(&redone, &reopened_snapshot));
                assert!(!Arc::ptr_eq(&before, &reopened_snapshot));
                assert!(!session.commit_authoring_edit(prepared).applied);
                assert!(Arc::ptr_eq(&reopened_snapshot, &session.shared_document()));
                assert!(session.apply_authoring_patch(&add(&["new-lineage"])).applied);
                drop(session);
                assert_eq!(before.as_ref(), &original);
                assert_eq!(accepted.as_ref(), &expected);
                assert_eq!(undone.as_ref(), &original);
                assert_eq!(redone.as_ref(), &expected);
                assert_eq!(reopened_snapshot.as_ref(), &original);
            }

            #[cfg(feature = "serde")]
            #[test]
            fn cancelled_rejected_foreign_and_stale_prepared_edits_preserve_snapshots() {
                let mut session = open(SceneAuthoringHistoryLimits::default());
                let before = session.shared_document();
                let original = before.as_ref().clone();
                let revision = session.revision();
                let content = session.content_state();
                let history = session.history_status();
                let usage = session.history_usage();
                let basis = session.authoring_schema_basis();
                let readiness = session.authoring_readiness();
                let manifest = session.unavailable_record_manifest().unwrap().clone();
                drop(
                    session
                        .prepare_authoring_edit(revision, &add(&["cancelled"]))
                        .unwrap(),
                );
                assert!(
                    session
                        .prepare_authoring_edit(revision, &add(&["temporary", "z-root"]))
                        .is_err()
                );
                assert!(session.replace_document(document()).is_err());
                assert!(!session.apply_patch(&add(&["wrong-authority"]), &registry()).applied);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert_eq!(before.as_ref(), &original);
                assert_eq!(session.revision(), revision);
                assert_eq!(session.content_state(), content);
                assert_eq!(session.history_status(), history);
                assert_eq!(session.history_usage(), usage);
                assert_eq!(session.authoring_schema_basis(), basis);
                assert_eq!(session.authoring_readiness(), readiness);
                assert_eq!(session.unavailable_record_manifest(), Some(&manifest));

                let mut other = open(SceneAuthoringHistoryLimits::default());
                let other_snapshot = other.shared_document();
                let other_revision = other.revision();
                let foreign = session
                    .prepare_authoring_edit(revision, &add(&["foreign"]))
                    .unwrap();
                assert!(!other.commit_authoring_edit(foreign).applied);
                assert!(Arc::ptr_eq(&other_snapshot, &other.shared_document()));
                assert_eq!(other.revision(), other_revision);
                assert_eq!(other.history_status().undo_depth, 0);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));

                let stale = session
                    .prepare_authoring_edit(revision, &add(&["superseded"]))
                    .unwrap();
                assert!(session.apply_authoring_patch(&add(&["accepted"])).applied);
                let accepted = session.shared_document();
                let accepted_revision = session.revision();
                let accepted_content = session.content_state();
                let accepted_history = session.history_status();
                let accepted_usage = session.history_usage();
                assert!(!session.commit_authoring_edit(stale).applied);
                assert!(
                    session
                        .prepare_authoring_edit(revision, &add(&["old-revision"]))
                        .is_err()
                );
                assert!(Arc::ptr_eq(&accepted, &session.shared_document()));
                assert_eq!(session.revision(), accepted_revision);
                assert_eq!(session.content_state(), accepted_content);
                assert_eq!(session.history_status(), accepted_history);
                assert_eq!(session.history_usage(), accepted_usage);
                assert_eq!(session.authoring_schema_basis(), basis);
                assert_eq!(session.authoring_readiness(), readiness);
                assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
                assert_eq!(before.as_ref(), &original);
            }

            #[cfg(feature = "serde")]
            #[test]
            fn construction_retains_source_snapshots_through_cancellation_commit_and_history() {
                let mut session = open(SceneAuthoringHistoryLimits::default());
                let before = session.shared_document();
                let original = before.as_ref().clone();
                let revision = session.revision();
                let content = session.content_state();
                let mut draft = session.begin_authoring_construction(revision).unwrap();
                let root = draft
                    .create_entity(ScenePlacement::last(ScenePlacementOwner::Root))
                    .unwrap();
                let child = draft
                    .create_entity(ConstructionPlacement::ChildOf((&root).into()))
                    .unwrap();
                drop(session.prepare_authoring_construction(&draft).unwrap());
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                assert_eq!(session.revision(), revision);
                assert_eq!(session.content_state(), content);
                assert_eq!(session.history_status().undo_depth, 0);

                let prepared = session.prepare_authoring_construction(&draft).unwrap();
                let root_id = prepared.entity(&root).unwrap().clone();
                let child_id = prepared.entity(&child).unwrap().clone();
                let mut expected = original.clone();
                expected.roots.push(root_id.clone());
                expected.entities.extend([
                    SceneEntityRecord::new(root_id.clone()).with_children([child_id.clone()]),
                    SceneEntityRecord::new(child_id.clone()),
                ]);
                expected.canonicalize();
                assert_eq!(prepared.document(), &expected);
                assert!(Arc::ptr_eq(&before, &session.shared_document()));
                let committed = session.commit_authoring_construction(prepared).unwrap();
                assert_eq!(committed.entity(&root), Some(&root_id));
                assert_eq!(committed.entity(&child), Some(&child_id));
                let accepted = session.shared_document();
                assert!(!Arc::ptr_eq(&before, &accepted));
                assert!(Arc::ptr_eq(&accepted, &session.shared_document()));
                assert_eq!(accepted.as_ref(), &expected);
                assert_eq!(before.as_ref(), &original);
                assert_eq!(session.history_status().undo_depth, 1);
                assert!(session.undo_authoring().applied);
                assert_eq!(session.document(), &original);
                assert_eq!(session.content_state(), content);
                assert_eq!(accepted.as_ref(), &expected);
                assert!(session.redo_authoring().applied);
                assert_eq!(session.document(), &expected);
                assert_eq!(before.as_ref(), &original);
            }
        }
    };
}

source_snapshots!(
    scene,
    SceneAuthoringSession,
    SceneDocument,
    SceneDocumentCandidate,
    "scene",
    "scenes/snapshots.scene.json"
);
source_snapshots!(
    prefab,
    PrefabAuthoringSession,
    PrefabDocument,
    PrefabDocumentCandidate,
    "prefab",
    "prefabs/snapshots.prefab.json"
);
