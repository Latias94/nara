use nara_scene::SchemaReclassificationKind::DeclarationExtension;
use std::{convert::Infallible, sync::Arc};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DocumentContentState, DocumentHistoryStatus};
use nara_diagnostic::{DiagnosticCode, SafeSummary};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringRecordReadiness, PrefabAuthoringSession, PrefabDocument, PrefabDocumentCandidate,
    SceneAuthoringHistoryUsage, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner, SchemaReclassificationLimits,
};

use super::*;
use crate::workspace::{
    EditorDocumentDigest, EditorDocumentKind, EditorDocumentTabModel, EditorExtensionDocument,
    EditorExtensionDocumentError, EditorPersistenceCommit, EditorSelectionSet,
    EditorWorkspaceCommand,
};
use crate::{
    EditorDocumentQueryRejection, EditorEntityQuery, EditorEntityQueryLimits,
    EditorEntityQueryPosition,
};

const RECOVERED: &str = "test.workspace.Recovered";
const RECOVERED_OWNER: &str = "test.workspace.recovered";
const UNKNOWN: &str = "missing.Unrelated";

mod creation_metadata;

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn empty_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap()
}

fn successor() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(RECOVERED),
        "Recovered value",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(RECOVERED_OWNER).unwrap(),
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

fn source_document() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")).with_component(
            ComponentTypeId::new(RECOVERED),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::U64(7))]),
            )
            .with_owner(PersistentComponentSchemaOwnerId::try_new(RECOVERED_OWNER).unwrap()),
        ),
        SceneEntityRecord::new(id("opaque")).with_component(
            ComponentTypeId::new(UNKNOWN),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve unrelated data".into()),
            ),
        ),
    ])
}

fn open(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    name: &str,
    source: &SceneDocument,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{name}.scene.json");
            workspace
                .open_authoring_scene_document(
                    name,
                    EditorDocumentSourceBinding::new(path.clone()),
                    SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                        .unwrap()
                        .admit_authoring_with_basis(
                            AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                            basis,
                        )
                        .unwrap()
                        .publish(),
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{name}.prefab.json");
            let source = PrefabDocument::new(source.roots.clone(), source.entities.clone());
            workspace
                .open_authoring_prefab_document(
                    name,
                    EditorDocumentSourceBinding::new(path.clone()),
                    PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                        .unwrap()
                        .admit_authoring_with_basis(
                            AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                            basis,
                        )
                        .unwrap()
                        .publish(),
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Extension(_) => unreachable!("use the extension owner fixture"),
    }
}

fn open_pair(
    workspace: &mut EditorWorkspace,
    basis: &AuthoringSchemaBasis,
) -> [EditorDocumentId; 2] {
    [EditorDocumentKind::Scene, EditorDocumentKind::Prefab]
        .map(|kind| open(workspace, kind, "reclassify", &source_document(), basis))
}

fn add(name: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(id(name)),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

fn apply_patch(workspace: &mut EditorWorkspace, document: EditorDocumentId, name: &str) {
    let base_revision = workspace.document_revision(document).unwrap();
    let patch = add(name);
    let command = match workspace.document_kind(document).unwrap() {
        EditorDocumentKind::Scene => EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Prefab => EditorWorkspaceCommand::ApplyPrefabPatch {
            document,
            base_revision,
            patch,
        },
        EditorDocumentKind::Extension(_) => unreachable!(),
    };
    let report = workspace.apply_authoring_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn replay(workspace: &mut EditorWorkspace, document: EditorDocumentId, undo: bool) {
    let base_revision = workspace.document_revision(document).unwrap();
    let command = if undo {
        EditorWorkspaceCommand::Undo {
            document,
            base_revision,
        }
    } else {
        EditorWorkspaceCommand::Redo {
            document,
            base_revision,
        }
    };
    let report = workspace.apply_authoring_command(command);
    assert!(report.applied, "{:?}", report.diagnostics);
}

#[derive(Debug, Clone, PartialEq)]
enum SourceSnapshot {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

#[derive(Debug, Clone, PartialEq)]
struct DocumentSnapshot {
    tab: EditorDocumentTabModel,
    source: SourceSnapshot,
    source_address: usize,
    authoring_source: Option<AuthoringDocumentSource>,
    content: DocumentContentState,
    history_usage: SceneAuthoringHistoryUsage,
    selection: EditorSelectionSet,
    saved_content: DocumentContentState,
    saved_digest: Option<EditorDocumentDigest>,
    upgrade_required: bool,
}

fn snapshot(workspace: &EditorWorkspace) -> Vec<DocumentSnapshot> {
    workspace
        .authoring_model()
        .documents
        .into_iter()
        .map(|tab| {
            let (
                source,
                source_address,
                authoring_source,
                content,
                history_usage,
                selection,
                saved_content,
                saved_digest,
                upgrade_required,
            ) = match &workspace.documents[&tab.document] {
                EditorDocumentSlot::Scene(slot) => {
                    let source = slot.session().shared_document();
                    (
                        SourceSnapshot::Scene(Arc::clone(&source)),
                        Arc::as_ptr(&source) as usize,
                        slot.session().authoring_source().cloned(),
                        slot.session().content_state(),
                        slot.session().history_usage(),
                        slot.selection().clone(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Prefab(slot) => {
                    let source = slot.session().shared_document();
                    (
                        SourceSnapshot::Prefab(Arc::clone(&source)),
                        Arc::as_ptr(&source) as usize,
                        slot.session().authoring_source().cloned(),
                        slot.session().content_state(),
                        slot.session().history_usage(),
                        slot.selection().clone(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Extension(_) => unreachable!("snapshot source documents"),
            };
            DocumentSnapshot {
                tab,
                source,
                source_address,
                authoring_source,
                content,
                history_usage,
                selection,
                saved_content,
                saved_digest,
                upgrade_required,
            }
        })
        .collect()
}

fn assert_refused(report: &EditorWorkspaceCommandReport) {
    assert!(!report.applied);
    assert!(report.diagnostics.has_errors(), "{report:?}");
}

fn assert_limit(report: &EditorWorkspaceCommandReport, kind: &str) {
    assert_refused(report);
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-limit"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "budget-kind" && field.display_value() == kind
                })
        }),
        "{:?}",
        report.diagnostics
    );
}

fn work_with_limits(work: usize, retained: usize) -> SchemaReclassificationWork {
    SchemaReclassificationWork::new(SchemaReclassificationLimits::new(
        ByteLimit::new(work).unwrap(),
        ByteLimit::new(retained).unwrap(),
    ))
}

#[test]
fn mixed_open_set_preserves_dirty_inactive_sources_navigation_checkpoints_and_real_history() {
    let previous = empty_basis();
    let next = successor();
    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let documents = [
        EditorDocumentKind::Scene,
        EditorDocumentKind::Prefab,
        EditorDocumentKind::Scene,
        EditorDocumentKind::Prefab,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| {
        open(
            &mut workspace,
            kind,
            &format!("source-{index}"),
            &source_document(),
            &previous,
        )
    })
    .collect::<Vec<_>>();
    for document in &documents {
        apply_patch(&mut workspace, *document, "saved");
        let checkpoint = authority.capture(&workspace, *document).unwrap();
        assert!(
            authority
                .commit(
                    &mut workspace,
                    EditorPersistenceCommit {
                        checkpoint,
                        digest: EditorDocumentDigest::new(19, [0x5a; 32]),
                    }
                )
                .applied
        );
        apply_patch(&mut workspace, *document, "dirty");
        apply_patch(&mut workspace, *document, "redo");
        replay(&mut workspace, *document, true);
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document: *document,
                    entity: Some(id("root")),
                })
                .applied
        );
    }
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::MarkExternalChanged {
                document: documents[3],
            })
            .applied
    );
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::SetActiveDocument {
                document: documents[0],
            })
            .applied
    );
    let before = snapshot(&workspace);
    for source in &before {
        assert!(source.tab.dirty);
        assert_eq!(source.tab.history.undo_depth, 2);
        assert_eq!(source.tab.history.redo_depth, 1);
        assert_eq!(source.tab.authoring_schema_basis.as_ref(), Some(&previous));
    }
    let mut work = SchemaReclassificationWork::default();
    let prepared = workspace
        .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
        .unwrap();
    assert_eq!(prepared.schema_basis(), &next);
    assert_eq!(prepared.documents().collect::<Vec<_>>(), documents);
    prepared.validate(&workspace).unwrap();
    for document in &documents {
        match &workspace.documents[document] {
            EditorDocumentSlot::Scene(slot) => {
                let view = prepared.scene_source(&workspace, *document).unwrap();
                assert!(Arc::ptr_eq(
                    &view.shared_document(),
                    &slot.session().shared_document()
                ));
                assert_eq!(view.document(), slot.session().document());
                assert_eq!(view.schema_basis(), next);
                assert_eq!(view.source(), slot.session().authoring_source().unwrap());
                assert_eq!(view.base_revision(), slot.revision());
                assert_eq!(view.unavailable_record_manifest().total_count(), 1);
            }
            EditorDocumentSlot::Prefab(slot) => {
                let view = prepared.prefab_source(&workspace, *document).unwrap();
                assert!(Arc::ptr_eq(
                    &view.shared_document(),
                    &slot.session().shared_document()
                ));
                assert_eq!(view.document(), slot.session().document());
                assert!(
                    view.document()
                        .entities
                        .iter()
                        .any(|entity| entity.id == id("dirty"))
                );
                assert_eq!(view.schema_basis(), next);
                assert_eq!(view.source(), slot.session().authoring_source().unwrap());
                assert_eq!(view.base_revision(), slot.revision());
                assert_eq!(view.unavailable_record_manifest().total_count(), 1);
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
    }
    assert_eq!(snapshot(&workspace), before);
    workspace
        .bind_schema_reclassification(prepared)
        .unwrap()
        .commit();
    let mut expected = before;
    for source in &mut expected {
        source.tab.authoring_schema_basis = Some(next.clone());
        source.tab.authoring_readiness = Some(AuthoringDocumentReadiness::Degraded);
    }
    assert_eq!(snapshot(&workspace), expected);
    assert_eq!(workspace.active_document(), Some(documents[0]));
    for document in documents {
        let facts = match &workspace.documents[&document] {
            EditorDocumentSlot::Scene(slot) => slot.session().authoring_record_facts().unwrap(),
            EditorDocumentSlot::Prefab(slot) => slot.session().authoring_record_facts().unwrap(),
            EditorDocumentSlot::Extension(_) => unreachable!(),
        };
        assert_eq!(
            facts
                .iter()
                .find(|fact| fact.component() == &ComponentTypeId::new(RECOVERED))
                .unwrap()
                .readiness(),
            AuthoringRecordReadiness::Available
        );
        assert_eq!(
            facts
                .iter()
                .find(|fact| fact.component() == &ComponentTypeId::new(UNKNOWN))
                .unwrap()
                .readiness(),
            AuthoringRecordReadiness::UnknownSchema
        );
        replay(&mut workspace, document, true);
        assert_eq!(workspace.document_is_dirty(document), Some(false));
        replay(&mut workspace, document, true);
        assert_eq!(workspace.document_is_dirty(document), Some(true));
        replay(&mut workspace, document, false);
        assert_eq!(workspace.document_is_dirty(document), Some(false));
        replay(&mut workspace, document, false);
        assert_eq!(workspace.document_is_dirty(document), Some(true));
        replay(&mut workspace, document, false);
        assert_eq!(
            workspace.documents[&document].history_status().redo_depth,
            0
        );
    }
}

#[test]
fn stale_final_source_refuses_the_whole_set_without_changing_earlier_classifications() {
    let previous = empty_basis();
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    let [scene, prefab] = open_pair(&mut workspace, &previous);
    let prepared = workspace
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    apply_patch(&mut workspace, prefab, "after-review");
    let before = snapshot(&workspace);
    assert_refused(&prepared.validate(&workspace).unwrap_err());
    assert!(prepared.scene_source(&workspace, scene).is_ok());
    assert_refused(&prepared.prefab_source(&workspace, prefab).unwrap_err());
    assert_refused(
        &workspace
            .bind_schema_reclassification(prepared)
            .unwrap_err(),
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn binding_and_dropping_all_guards_does_not_publish_any_classification() {
    let previous = empty_basis();
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    open_pair(&mut workspace, &previous);
    let before = snapshot(&workspace);
    let prepared = workspace
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    drop(workspace.bind_schema_reclassification(prepared).unwrap());
    assert_eq!(snapshot(&workspace), before);
    let prepared = workspace
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    workspace
        .bind_schema_reclassification(prepared)
        .unwrap()
        .commit();
    assert!(
        workspace
            .authoring_model()
            .documents
            .iter()
            .all(|document| document.authoring_schema_basis.as_ref() == Some(&next))
    );
}

#[test]
fn foreign_workspace_refuses_even_matching_raw_ids_and_the_exact_original_source_owners() {
    let previous = empty_basis();
    let next = successor();
    let mut original = EditorWorkspace::new();
    let mut foreign = EditorWorkspace::new();
    let original_ids = open_pair(&mut original, &previous);
    assert_eq!(original_ids, open_pair(&mut foreign, &previous));
    let prepared = original
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    let original_facts = snapshot(&original);
    // Move genuine source owners to isolate workspace identity from revision/incarnation checks.
    // No document or history is synthesized by this fixture.
    std::mem::swap(&mut original.documents, &mut foreign.documents);
    assert_eq!(snapshot(&foreign), original_facts);
    let before = snapshot(&foreign);
    assert_refused(&prepared.validate(&foreign).unwrap_err());
    assert_refused(
        &prepared
            .scene_source(&foreign, original_ids[0])
            .unwrap_err(),
    );
    assert_refused(
        &prepared
            .prefab_source(&foreign, original_ids[1])
            .unwrap_err(),
    );
    assert_refused(&foreign.bind_schema_reclassification(prepared).unwrap_err());
    assert_eq!(snapshot(&foreign), before);
}

#[test]
fn opening_closing_or_replacing_one_document_invalidates_the_exact_open_set() {
    for change in ["open", "close", "close-and-open"] {
        let previous = empty_basis();
        let next = successor();
        let mut workspace = EditorWorkspace::new();
        let [_, prefab] = open_pair(&mut workspace, &previous);
        let prepared = workspace
            .prepare_schema_reclassification(
                &previous,
                &next,
                &DeclarationExtension,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        if change != "open" {
            assert!(
                workspace
                    .apply_authoring_command(EditorWorkspaceCommand::CloseDocument {
                        document: prefab,
                    })
                    .applied
            );
        }
        if change != "close" {
            let replacement = open(
                &mut workspace,
                EditorDocumentKind::Prefab,
                "replacement",
                &source_document(),
                &previous,
            );
            assert_ne!(replacement, prefab);
            assert!(prepared.prefab_source(&workspace, replacement).is_err());
        }
        let before = snapshot(&workspace);
        assert_refused(&prepared.validate(&workspace).unwrap_err());
        assert_refused(
            &workspace
                .bind_schema_reclassification(prepared)
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn reopening_either_source_kind_invalidates_the_set_at_the_same_document_id() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let previous = empty_basis();
        let next = successor();
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let [scene, prefab] = open_pair(&mut workspace, &previous);
        let document = if kind == EditorDocumentKind::Scene {
            scene
        } else {
            prefab
        };
        let prepared = workspace
            .prepare_schema_reclassification(
                &previous,
                &next,
                &DeclarationExtension,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        let revision = workspace.document_revision(document).unwrap();
        let digest = EditorDocumentDigest::new(23, [0x31; 32]);
        match kind {
            EditorDocumentKind::Scene => {
                let session = workspace.scene(document).unwrap().session();
                let candidate = SceneDocumentCandidate::decode_json_str(
                    &session.document().to_json_string().unwrap(),
                )
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                authority
                    .publish_reopened_session(&mut workspace, document, revision, reopened, digest)
                    .unwrap();
                assert!(prepared.scene_source(&workspace, document).is_err());
            }
            EditorDocumentKind::Prefab => {
                let slot = workspace.prefab(document).unwrap();
                let candidate = PrefabDocumentCandidate::decode_json_str(
                    &slot.session().document().to_json_string().unwrap(),
                )
                .unwrap();
                let reopened = slot
                    .session()
                    .try_reopen_authoring_candidate(candidate)
                    .unwrap();
                let binding = slot.source_binding().clone();
                authority
                    .publish_reopened_prefab_session(
                        &mut workspace,
                        document,
                        revision,
                        binding,
                        reopened,
                        digest,
                    )
                    .unwrap();
                assert!(prepared.prefab_source(&workspace, document).is_err());
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
        assert_ne!(
            workspace.document_revision(document).unwrap().source_id(),
            revision.source_id()
        );
        let before = snapshot(&workspace);
        assert_refused(&prepared.validate(&workspace).unwrap_err());
        assert_refused(
            &workspace
                .bind_schema_reclassification(prepared)
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn changed_source_binding_refuses_views_and_commit_without_a_revision_change() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let previous = empty_basis();
        let next = successor();
        let mut workspace = EditorWorkspace::new();
        let document = open(
            &mut workspace,
            kind,
            "binding",
            &source_document(),
            &previous,
        );
        let prepared = workspace
            .prepare_schema_reclassification(
                &previous,
                &next,
                &DeclarationExtension,
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        let revision = workspace.document_revision(document).unwrap();
        // Host binding changes are independent of source edits; keep every source fact intact.
        let replacement = EditorDocumentSourceBinding::new("moved/source.json");
        match workspace.documents.get_mut(&document).unwrap() {
            EditorDocumentSlot::Scene(slot) => slot.source_binding = Some(replacement),
            EditorDocumentSlot::Prefab(slot) => slot.source_binding = replacement,
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
        assert_eq!(workspace.document_revision(document), Some(revision));
        let before = snapshot(&workspace);
        assert_refused(&prepared.validate(&workspace).unwrap_err());
        match kind {
            EditorDocumentKind::Scene => {
                assert!(prepared.scene_source(&workspace, document).is_err())
            }
            EditorDocumentKind::Prefab => {
                assert!(prepared.prefab_source(&workspace, document).is_err())
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
        assert_refused(
            &workspace
                .bind_schema_reclassification(prepared)
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn old_queries_continuations_and_edit_candidates_are_stale_at_the_same_revision() {
    let previous = empty_basis();
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    let documents = open_pair(&mut workspace, &previous);
    let observations = documents.map(|document| {
        let basis = workspace.document_query_basis(document).unwrap();
        let query = EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(basis.clone()),
            limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
        };
        let cursor = workspace
            .query_document_entities(query.clone())
            .unwrap()
            .next
            .unwrap();
        let candidate = workspace
            .prepare_authoring_edit(&basis, &add("old-candidate"))
            .unwrap();
        (document, basis, query, cursor, candidate)
    });
    let prepared = workspace
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    workspace
        .bind_schema_reclassification(prepared)
        .unwrap()
        .commit();
    let before = snapshot(&workspace);
    for (document, basis, query, cursor, candidate) in observations {
        assert_eq!(workspace.document_revision(document), Some(basis.revision));
        assert_eq!(
            workspace.validate_document_query_basis(&basis),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        assert_eq!(
            workspace.query_document_entities(query),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        assert_eq!(
            workspace.query_document_entities(EditorEntityQuery {
                position: EditorEntityQueryPosition::Continue(cursor),
                limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
            }),
            Err(EditorDocumentQueryRejection::StaleSchema)
        );
        assert_refused(
            &workspace
                .prepare_authoring_edit(&basis, &add("stale-basis"))
                .unwrap_err(),
        );
        assert_refused(&workspace.commit_authoring_edit(candidate));
        let current = workspace.document_query_basis(document).unwrap();
        assert_eq!(current.schema.as_ref(), Some(&next));
        assert_eq!(current.revision, basis.revision);
        assert!(
            workspace
                .query_document_entities(EditorEntityQuery {
                    position: EditorEntityQueryPosition::Start(current),
                    limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
                })
                .is_ok()
        );
    }
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn a_saved_checkpoint_captured_before_reclassification_remains_usable_after_adoption() {
    let previous = empty_basis();
    let next = successor();
    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let documents = open_pair(&mut workspace, &previous);
    let checkpoints = documents.map(|document| {
        apply_patch(&mut workspace, document, "captured");
        let checkpoint = authority.capture(&workspace, document).unwrap();
        apply_patch(&mut workspace, document, "later");
        (document, checkpoint)
    });
    let prepared = workspace
        .prepare_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    workspace
        .bind_schema_reclassification(prepared)
        .unwrap()
        .commit();
    for (document, checkpoint) in checkpoints {
        let digest = EditorDocumentDigest::new(29, [0x71; 32]);
        assert!(
            authority
                .commit(
                    &mut workspace,
                    EditorPersistenceCommit { checkpoint, digest }
                )
                .applied
        );
        assert_eq!(workspace.document_is_dirty(document), Some(true));
        replay(&mut workspace, document, true);
        assert_eq!(workspace.document_is_dirty(document), Some(false));
        assert_eq!(
            workspace
                .document_query_basis(document)
                .unwrap()
                .schema
                .as_ref(),
            Some(&next)
        );
    }
}

#[test]
fn independent_equal_predecessor_admission_is_refused_before_preparation_work() {
    let previous = empty_basis();
    let equal_but_independent = empty_basis();
    assert_ne!(previous, equal_but_independent);
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    open(
        &mut workspace,
        EditorDocumentKind::Scene,
        "first",
        &source_document(),
        &previous,
    );
    open(
        &mut workspace,
        EditorDocumentKind::Prefab,
        "last",
        &source_document(),
        &equal_but_independent,
    );
    let before = snapshot(&workspace);
    let mut work = SchemaReclassificationWork::default();
    let usage = work.usage();
    assert_refused(
        &workspace
            .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
            .unwrap_err(),
    );
    assert_eq!(work.usage(), usage);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn native_scene_and_prefab_owners_refuse_without_preparing_earlier_authoring_sources() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let previous = empty_basis();
        let next = successor();
        let mut workspace = EditorWorkspace::new();
        open_pair(&mut workspace, &previous);
        match kind {
            EditorDocumentKind::Scene => {
                workspace
                    .open_scene_session("native", SceneAuthoringSession::new(source_document()))
                    .unwrap();
            }
            EditorDocumentKind::Prefab => {
                workspace
                    .open_prefab_session(
                        "native",
                        EditorDocumentSourceBinding::new("prefabs/native.json"),
                        PrefabAuthoringSession::new(PrefabDocument::default()),
                    )
                    .unwrap();
            }
            EditorDocumentKind::Extension(_) => unreachable!(),
        }
        let before = snapshot(&workspace);
        let mut work = SchemaReclassificationWork::default();
        let usage = work.usage();
        assert_refused(
            &workspace
                .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
                .unwrap_err(),
        );
        assert_eq!(work.usage(), usage);
        assert_eq!(snapshot(&workspace), before);
    }
}

// This unsupported domain borrows real source/history identities rather than inventing a history.
struct UnsupportedExtension(SceneAuthoringSession);

impl EditorExtensionDocument for UnsupportedExtension {
    type Command = Infallible;
    type View = ();
    const TYPE_ID: &'static str = "test.schema-reclassification.unsupported";

    fn title(&self) -> &str {
        "Unsupported document"
    }
    fn content_state(&self) -> DocumentContentState {
        self.0.content_state()
    }
    fn history_status(&self) -> DocumentHistoryStatus {
        self.0.history_status()
    }
    fn view(&self) -> Self::View {}
    fn apply(&mut self, command: &Infallible) -> Result<(), EditorExtensionDocumentError> {
        match *command {}
    }
    fn undo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        Err(extension_error())
    }
    fn redo(&mut self) -> Result<(), EditorExtensionDocumentError> {
        Err(extension_error())
    }
    fn encode(&self) -> Result<Vec<u8>, EditorExtensionDocumentError> {
        Ok(Vec::new())
    }
    fn decode(_: &[u8]) -> Result<Self, EditorExtensionDocumentError> {
        Err(extension_error())
    }
}

fn extension_error() -> EditorExtensionDocumentError {
    EditorExtensionDocumentError::new(
        DiagnosticCode::new("test.unsupported-document-operation").unwrap(),
        SafeSummary::new("This fixture has no editable extension operations").unwrap(),
    )
}

#[test]
fn unsupported_extension_refuses_the_complete_set_before_any_preparation_work() {
    let previous = empty_basis();
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    let documents = open_pair(&mut workspace, &previous);
    let before = snapshot(&workspace);
    let extension = workspace
        .open_extension_document(
            EditorDocumentSourceBinding::new("extension/unsupported.json"),
            UnsupportedExtension(SceneAuthoringSession::new(SceneDocument::default())),
        )
        .unwrap();
    let extension_revision = workspace.document_revision(extension);
    let mut work = SchemaReclassificationWork::default();
    let usage = work.usage();
    assert_refused(
        &workspace
            .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
            .unwrap_err(),
    );
    assert_eq!(work.usage(), usage);
    assert_eq!(workspace.document_revision(extension), extension_revision);
    assert_eq!(
        workspace.document_kind(extension),
        Some(EditorDocumentKind::Extension(UnsupportedExtension::TYPE_ID))
    );
    // Compare the incumbent sources after closing only the untouched, clean extension fixture.
    assert!(
        workspace
            .apply_command(
                &nara_reflect::ComponentRegistry::new(),
                EditorWorkspaceCommand::CloseDocument {
                    document: extension
                }
            )
            .applied
    );
    assert_eq!(workspace.__document_ids(), documents);
    assert!(
        workspace
            .apply_authoring_command(EditorWorkspaceCommand::SetActiveDocument {
                document: documents[1],
            })
            .applied
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn work_and_retention_limits_are_shared_across_the_complete_open_set() {
    let previous = empty_basis();
    let next = successor();
    let mut single_usage = Vec::new();
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        open(
            &mut workspace,
            kind,
            "single",
            &source_document(),
            &previous,
        );
        let mut work = SchemaReclassificationWork::default();
        workspace
            .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
            .unwrap();
        single_usage.push(work.usage());
    }
    let work_cap = single_usage
        .iter()
        .map(|usage| usage.work_bytes)
        .max()
        .unwrap();
    let retention_cap = single_usage
        .iter()
        .map(|usage| usage.peak_retained_bytes)
        .max()
        .unwrap();
    let mut workspace = EditorWorkspace::new();
    open_pair(&mut workspace, &previous);
    let mut unlimited = SchemaReclassificationWork::default();
    workspace
        .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut unlimited)
        .unwrap();
    assert!(unlimited.usage().work_bytes > work_cap);
    assert!(unlimited.usage().peak_retained_bytes > retention_cap);
    let before = snapshot(&workspace);
    for (kind, mut work) in [
        ("work", work_with_limits(work_cap, 256 * 1024 * 1024)),
        (
            "retained",
            work_with_limits(512 * 1024 * 1024, retention_cap),
        ),
    ] {
        assert_limit(
            &workspace
                .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
                .unwrap_err(),
            kind,
        );
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn every_incumbent_is_admitted_before_the_first_malformed_successor_is_classified() {
    let previous = empty_basis();
    let next = successor();
    let mut workspace = EditorWorkspace::new();
    let mut malformed = source_document();
    malformed
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("root"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(RECOVERED))
        .unwrap()
        .value = ComponentValue::map([("value", ComponentValue::String("wrong kind".into()))]);
    let first = open(
        &mut workspace,
        EditorDocumentKind::Scene,
        "first",
        &malformed,
        &previous,
    );
    let mut large = source_document();
    large
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("opaque"))
        .unwrap()
        .components
        .get_mut(&ComponentTypeId::new(UNKNOWN))
        .unwrap()
        .value = ComponentValue::String("x".repeat(128 * 1024));
    let last = open(
        &mut workspace,
        EditorDocumentKind::Prefab,
        "last",
        &large,
        &previous,
    );

    // Calibrate against the source owner's actual admitted incumbent, without copying its model.
    let mut calibration = SchemaReclassificationWork::default();
    let ticket = workspace
        .scene(first)
        .unwrap()
        .session()
        .retain_schema_reclassification_source(&mut calibration)
        .unwrap();
    let first_peak = calibration.usage().peak_retained_bytes;
    drop(ticket);
    let mut first_only = SchemaReclassificationWork::default();
    let malformed_report = workspace
        .scene(first)
        .unwrap()
        .session()
        .prepare_schema_reclassification(&next, &DeclarationExtension, &mut first_only)
        .unwrap_err();
    assert!(malformed_report.diagnostics.has_errors());
    assert!(
        !malformed_report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.authoring-schema-reclassification-limit"
        }),
        "the first source must reach its semantic refusal: {:?}",
        malformed_report.diagnostics
    );
    let retention_cap = first_peak.max(first_only.usage().peak_retained_bytes) * 4;

    let mut all_incumbents = SchemaReclassificationWork::default();
    workspace
        .scene(first)
        .unwrap()
        .session()
        .retain_schema_reclassification_source(&mut all_incumbents)
        .unwrap();
    workspace
        .prefab(last)
        .unwrap()
        .session()
        .retain_schema_reclassification_source(&mut all_incumbents)
        .unwrap();
    assert!(all_incumbents.usage().peak_retained_bytes > retention_cap);
    let before = snapshot(&workspace);
    let mut work = work_with_limits(512 * 1024 * 1024, retention_cap);
    let report = workspace
        .prepare_schema_reclassification(&previous, &next, &DeclarationExtension, &mut work)
        .unwrap_err();
    assert_limit(&report, "retained");
    assert_eq!(report.document, Some(last));
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn finite_task_prepares_frozen_sources_and_only_the_original_live_set_can_adopt() {
    use nara_tasks::{
        TaskDomainKey, TaskPoolConfig, TaskPoolKind, TaskPools, TaskSpawnRequest, TaskTerminal,
    };
    use std::{
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    for change_after_capture in [false, true] {
        let previous = empty_basis();
        let next = successor();
        let mut workspace = EditorWorkspace::new();
        let documents = open_pair(&mut workspace, &previous);
        for document in documents {
            apply_patch(&mut workspace, document, "retained");
            apply_patch(&mut workspace, document, "redo");
            replay(&mut workspace, document, true);
        }
        let before = snapshot(&workspace);
        let original_scene = workspace
            .scene(documents[0])
            .unwrap()
            .session()
            .shared_document();
        let original_prefab = workspace
            .prefab(documents[1])
            .unwrap()
            .session()
            .shared_document();
        let request = workspace
            .capture_schema_reclassification(
                &previous,
                &next,
                &DeclarationExtension,
                Default::default(),
            )
            .unwrap();
        assert_eq!(snapshot(&workspace), before);
        let pools = TaskPools::try_new_lazy(
            TaskPoolConfig::threaded(
                nara_core::ItemLimit::ONE,
                nara_core::ItemLimit::ONE,
                nara_core::ItemLimit::ONE,
            )
            .unwrap(),
        )
        .unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut task = pools
            .spawn(
                TaskPoolKind::Compute,
                TaskSpawnRequest::new(1, TaskDomainKey::new(0x7363_6865_6d61)),
                move |_| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    request.prepare()
                },
            )
            .into_handle()
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if change_after_capture {
            apply_patch(&mut workspace, documents[1], "newer-source");
        }
        let live = snapshot(&workspace);
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let prepared = loop {
            if let Some(result) = task.try_take() {
                match result {
                    TaskTerminal::Completed(Ok(prepared)) => break prepared,
                    result => panic!("finite source preparation failed: {result:?}"),
                }
            }
            assert!(
                Instant::now() < deadline,
                "finite preparation did not return"
            );
            thread::yield_now();
        };
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        assert_eq!(snapshot(&workspace), live);
        assert_eq!(prepared.schema_basis(), &next);
        assert!(prepared.usage().work_bytes > 0);
        assert!(Arc::ptr_eq(
            &prepared
                .scene_snapshot(documents[0])
                .unwrap()
                .shared_document(),
            &original_scene
        ));
        assert!(Arc::ptr_eq(
            &prepared
                .prefab_snapshot(documents[1])
                .unwrap()
                .shared_document(),
            &original_prefab
        ));
        assert!(prepared.prefab_snapshot(documents[0]).is_none());
        assert!(prepared.scene_snapshot(documents[1]).is_none());
        if change_after_capture {
            assert_refused(&prepared.validate(&workspace).unwrap_err());
            assert_refused(
                &workspace
                    .bind_schema_reclassification(prepared)
                    .unwrap_err(),
            );
            assert_eq!(snapshot(&workspace), live);
        } else {
            prepared.validate(&workspace).unwrap();
            workspace
                .bind_schema_reclassification(prepared)
                .unwrap()
                .commit();
            for document in documents {
                assert_eq!(
                    workspace
                        .document_query_basis(document)
                        .unwrap()
                        .schema
                        .as_ref(),
                    Some(&next)
                );
                replay(&mut workspace, document, true);
                replay(&mut workspace, document, false);
                replay(&mut workspace, document, false);
            }
        }
    }
}

#[test]
fn owned_input_outlives_its_workspace_without_gaining_publication_authority() {
    let previous = empty_basis();
    let next = successor();
    let mut original = EditorWorkspace::new();
    let documents = open_pair(&mut original, &previous);
    let scene = original
        .scene(documents[0])
        .unwrap()
        .session()
        .shared_document();
    let prefab = original
        .prefab(documents[1])
        .unwrap()
        .session()
        .shared_document();
    let request = original
        .capture_schema_reclassification(
            &previous,
            &next,
            &DeclarationExtension,
            Default::default(),
        )
        .unwrap();
    drop(original);
    let prepared = request.prepare().unwrap();
    assert!(Arc::ptr_eq(
        &prepared
            .scene_snapshot(documents[0])
            .unwrap()
            .shared_document(),
        &scene
    ));
    assert!(Arc::ptr_eq(
        &prepared
            .prefab_snapshot(documents[1])
            .unwrap()
            .shared_document(),
        &prefab
    ));
    let mut replacement = EditorWorkspace::new();
    assert_eq!(documents, open_pair(&mut replacement, &previous));
    let before = snapshot(&replacement);
    assert_refused(
        &replacement
            .bind_schema_reclassification(prepared)
            .unwrap_err(),
    );
    assert_eq!(snapshot(&replacement), before);
}
