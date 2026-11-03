use std::convert::Infallible;

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, DocumentContentState, DocumentHistoryStatus, ItemLimit};
use nara_diagnostic::{DiagnosticCode, SafeSummary};
use nara_identity::ReferenceInputId;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistryBuilder, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, CapturedExtractionSource, PrefabExtractionBinding,
        PrefabExtractionDestination, PrefabExtractionEnvironment, PrefabExtractionRequest,
    },
    AuthoringFieldEdit, InMemoryPrefabSourceResolver, PrefabAuthoringSession,
    PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding,
    PrefabOverlay, ResolvedPrefabSource, SceneAuthoringContentState, SceneAuthoringHistoryLimits,
    SceneAuthoringHistoryUsage, SceneAuthoringSession, SceneCompilationLimits,
    SceneComponentRecord, SceneDocumentCandidate, SceneEntityPlacement, SceneEntityRecord,
    SceneFileLimits, SceneOccurrenceKey, ScenePlacement, ScenePlacementParent, SourceDocumentKey,
    SourceDocumentKind,
};

use super::*;
use crate::workspace::{
    EditorDocumentDigest, EditorDocumentTabModel, EditorExtensionDocument,
    EditorExtensionDocumentError, EditorExternalReloadState, EditorPersistenceAuthority,
    EditorSelectionSet, EditorWorkspaceCommand,
};

const COMPONENT: &str = "test.extraction.Counter";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.extraction").unwrap()
}

fn basis() -> AuthoringSchemaBasis {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Counter",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::I64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
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
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn source_document() -> SceneDocument {
    let mut mount = SceneEntityRecord::new(id("mount"));
    mount.prefab = Some(PrefabInstance::forest(
        AssetRef::path("prefabs/unopened.prefab.json").unwrap(),
    ));
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a-first")),
        SceneEntityRecord::new(id("branch")).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::I64(7))]),
            )
            .with_owner(owner()),
        ),
        mount,
        SceneEntityRecord::new(id("unavailable")).with_component(
            ComponentTypeId::new("missing.Opaque"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("retain this complete unknown record".into()),
            ),
        ),
    ])
}

fn open(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    let name = format!("source-{}", workspace.len());
    let source = source_document();
    match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{name}.scene.json");
            let published =
                SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring_with_basis(
                        AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                        basis,
                    )
                    .unwrap()
                    .publish();
            workspace
                .open_authoring_scene_document(
                    name,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{name}.prefab.json");
            let mut source = PrefabDocument::new(source.placements, source.entities);
            source
                .inputs
                .insert(ReferenceInputId::new("retained.input").unwrap());
            let published =
                PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
                    .unwrap()
                    .admit_authoring_with_basis(
                        AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                        basis,
                    )
                    .unwrap()
                    .publish();
            workspace
                .open_authoring_prefab_document(
                    name,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
                .opened_document
                .unwrap()
        }
        EditorDocumentKind::Extension(_) => unreachable!("source fixture kinds"),
    }
}

fn set_value(workspace: &mut EditorWorkspace, document: EditorDocumentId, value: i64) {
    let query = workspace.document_query_basis(document).unwrap();
    let patch = AuthoringFieldEdit::single(
        id("branch"),
        ComponentTypeId::new(COMPONENT),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("value"),
        ComponentValue::I64(value),
    )
    .unwrap()
    .to_patch();
    let prepared = workspace.prepare_authoring_edit(&query, &patch).unwrap();
    let report = workspace.commit_authoring_edit(prepared);
    assert!(report.applied, "{report:?}");
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
    assert!(report.applied, "{report:?}");
}

fn select(workspace: &mut EditorWorkspace, document: EditorDocumentId, entity: &str) {
    let report = workspace.apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
        document,
        entity: Some(id(entity)),
    });
    assert!(report.applied, "{report:?}");
}

fn fixture(
    basis: &AuthoringSchemaBasis,
) -> (
    EditorWorkspace,
    EditorPersistenceAuthority,
    [EditorDocumentId; 3],
) {
    let (mut workspace, authority) = EditorWorkspace::new_hosted();
    let documents = [
        EditorDocumentKind::Scene,
        EditorDocumentKind::Scene,
        EditorDocumentKind::Prefab,
    ]
    .map(|kind| open(&mut workspace, kind, basis));
    for document in documents {
        set_value(&mut workspace, document, 8);
        set_value(&mut workspace, document, 9);
        replay(&mut workspace, document, true);
        // The requested source-owned root intentionally differs from panel selection.
        select(&mut workspace, document, "a-first");
    }
    assert!(workspace.activate_document(documents[0]).applied);
    (workspace, authority, documents)
}

fn work() -> PrefabExtractionWork {
    PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap()
}

#[derive(Debug, PartialEq)]
enum SourceSnapshot {
    Scene(Arc<SceneDocument>),
    Prefab(Arc<PrefabDocument>),
}

#[derive(Debug, PartialEq)]
struct SlotSnapshot {
    document: EditorDocumentId,
    source: SourceSnapshot,
    address: usize,
    content: SceneAuthoringContentState,
    saved_content: SceneAuthoringContentState,
    digest: Option<EditorDocumentDigest>,
    usage: SceneAuthoringHistoryUsage,
    selection: EditorSelectionSet,
    upgrade_required: bool,
}

#[derive(Debug, PartialEq)]
struct WorkspaceSnapshot {
    active: Option<EditorDocumentId>,
    tabs: Vec<EditorDocumentTabModel>,
    sources: Vec<SlotSnapshot>,
}

fn snapshot(workspace: &EditorWorkspace) -> WorkspaceSnapshot {
    let sources = workspace
        .documents
        .iter()
        .filter_map(|(&document, slot)| {
            let (
                source,
                address,
                content,
                saved_content,
                digest,
                usage,
                selection,
                upgrade_required,
            ) = match slot {
                EditorDocumentSlot::Scene(slot) => {
                    let source = slot.session().shared_document();
                    let address = Arc::as_ptr(&source) as usize;
                    (
                        SourceSnapshot::Scene(source),
                        address,
                        slot.session().content_state(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.session().history_usage(),
                        slot.selection().clone(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Prefab(slot) => {
                    let source = slot.session().shared_document();
                    let address = Arc::as_ptr(&source) as usize;
                    (
                        SourceSnapshot::Prefab(source),
                        address,
                        slot.session().content_state(),
                        slot.saved_content_state(),
                        slot.saved_digest(),
                        slot.session().history_usage(),
                        slot.selection().clone(),
                        slot.session().source_upgrade_required(),
                    )
                }
                EditorDocumentSlot::Extension(_) => return None,
            };
            Some(SlotSnapshot {
                document,
                source,
                address,
                content,
                saved_content,
                digest,
                usage,
                selection,
                upgrade_required,
            })
        })
        .collect();
    WorkspaceSnapshot {
        active: workspace.active_document(),
        tabs: workspace.authoring_model().documents,
        sources,
    }
}

fn assert_refused(report: &EditorWorkspaceCommandReport) {
    assert!(!report.applied, "{report:?}");
    assert!(report.diagnostics.has_errors(), "{report:?}");
}

fn value(entities: &[SceneEntityRecord]) -> i64 {
    entities
        .iter()
        .find(|entity| entity.id == id("branch"))
        .unwrap()
        .components[&ComponentTypeId::new(COMPONENT)]
        .value
        .field_i64("value")
        .unwrap()
}

#[test]
fn complete_dirty_input_shares_actual_sources_moves_to_worker_and_allows_navigation() {
    let basis = basis();
    let (mut workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    assert!(
        before
            .tabs
            .iter()
            .all(|tab| tab.dirty && tab.history.undo_depth == 1 && tab.history.redo_depth == 1)
    );
    let mut work = work();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
        .unwrap();
    let captured = work.usage();
    assert_eq!(input.documents().collect::<Vec<_>>(), documents);
    let mut membership = input.documents();
    assert_eq!(membership.len(), 3);
    assert_eq!(membership.next(), Some(documents[0]));
    assert_eq!(membership.len(), 2);
    drop(membership);
    assert_eq!(input.selected_document(), documents[2]);
    assert_eq!(input.root(), &id("branch"));
    assert_eq!(input.schema_basis(), &basis);
    for document in documents {
        assert_eq!(
            input.document_revision(document),
            workspace.document_revision(document)
        );
        assert_eq!(
            input.document_kind(document),
            workspace.document_kind(document)
        );
        assert_eq!(
            input.document_source_binding(document),
            workspace.document_source_binding(document)
        );
        match &workspace.documents[&document] {
            EditorDocumentSlot::Scene(slot) => {
                let source = input.scene_input(document).unwrap();
                assert!(std::ptr::eq(source.document(), slot.session().document()));
                assert!(std::ptr::eq(
                    input.scene_document(document).unwrap(),
                    slot.session().document()
                ));
                assert!(Arc::ptr_eq(
                    &source.shared_document(),
                    &slot.session().shared_document()
                ));
                assert!(std::ptr::eq(
                    input.document_source(document).unwrap(),
                    slot.session().authoring_source().unwrap()
                ));
                assert_eq!(source.content_state(), slot.session().content_state());
                assert_eq!(source.history_limits(), slot.session().history_limits());
                assert_eq!(source.file_limits(), SceneFileLimits::default());
                assert_eq!(source.schema_basis(), basis);
                assert!(input.prefab_input(document).is_none());
                assert!(input.prefab_document(document).is_none());
            }
            EditorDocumentSlot::Prefab(slot) => {
                let source = input.prefab_input(document).unwrap();
                assert!(std::ptr::eq(source.document(), slot.session().document()));
                assert!(std::ptr::eq(
                    input.prefab_document(document).unwrap(),
                    slot.session().document()
                ));
                assert!(Arc::ptr_eq(
                    &source.shared_document(),
                    &slot.session().shared_document()
                ));
                assert!(std::ptr::eq(
                    input.document_source(document).unwrap(),
                    slot.session().authoring_source().unwrap()
                ));
                assert_eq!(source.content_state(), slot.session().content_state());
                assert_eq!(source.history_limits(), slot.session().history_limits());
                assert_eq!(source.file_limits(), SceneFileLimits::default());
                assert_eq!(source.schema_basis(), basis);
                assert!(
                    source
                        .document()
                        .inputs
                        .contains(&ReferenceInputId::new("retained.input").unwrap())
                );
                assert!(input.scene_input(document).is_none());
                assert!(input.scene_document(document).is_none());
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
    }
    let missing = EditorDocumentId::from_raw(9001);
    assert!(input.scene_document(missing).is_none());
    assert!(input.prefab_document(missing).is_none());
    assert!(input.scene_input(missing).is_none());
    assert!(input.prefab_input(missing).is_none());
    assert!(input.document_kind(missing).is_none());
    assert!(input.document_revision(missing).is_none());
    assert!(input.document_source_binding(missing).is_none());
    assert!(input.document_source(missing).is_none());

    let (input, mut work) = std::thread::spawn(move || {
        assert_eq!(
            value(&input.scene_document(documents[1]).unwrap().entities),
            8
        );
        assert_eq!(
            value(&input.prefab_document(documents[2]).unwrap().entities),
            8
        );
        (input, work)
    })
    .join()
    .unwrap();
    assert_eq!(work.usage(), captured);
    assert_eq!(snapshot(&workspace), before);
    input.validate(&workspace, &mut work).unwrap();
    assert!(work.usage().work_bytes > captured.work_bytes);
    assert_eq!(snapshot(&workspace), before);

    assert!(workspace.activate_document(documents[1]).applied);
    select(&mut workspace, documents[2], "branch");
    let navigated = snapshot(&workspace);
    input.validate(&workspace, &mut work).unwrap();
    assert_eq!(snapshot(&workspace), navigated);
    assert_eq!(input.documents().len(), 3);
    // Capture and validation did not clear either branch in any inactive or active owner.
    for document in documents {
        replay(&mut workspace, document, false);
        replay(&mut workspace, document, true);
    }
    assert!(
        workspace
            .authoring_model()
            .documents
            .iter()
            .all(|tab| tab.history.undo_depth == 1 && tab.history.redo_depth == 1)
    );
}

#[test]
fn real_undo_in_every_owner_invalidates_input_even_after_redo_restores_original_content() {
    let basis = basis();
    for index in 0..3 {
        let (mut workspace, _, documents) = fixture(&basis);
        let mut work = work();
        let input = workspace
            .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
            .unwrap();
        replay(&mut workspace, documents[index], true);
        let undone = snapshot(&workspace);
        assert_refused(&input.validate(&workspace, &mut work).unwrap_err());
        assert_eq!(snapshot(&workspace), undone);
        replay(&mut workspace, documents[index], false);
        let redone = snapshot(&workspace);
        assert_refused(&input.validate(&workspace, &mut work).unwrap_err());
        assert_eq!(snapshot(&workspace), redone);
        assert_ne!(
            input.document_revision(documents[index]),
            workspace.document_revision(documents[index])
        );
        assert_eq!(
            value(&input.prefab_document(documents[2]).unwrap().entities),
            8
        );
        assert_eq!(
            value(&input.scene_document(documents[1]).unwrap().entities),
            8
        );
    }
}

#[test]
fn new_closed_replaced_rebound_retyped_and_reopened_slots_refuse_without_mutation() {
    let basis = basis();
    for change in [
        "open",
        "close",
        "replace",
        "scene-binding",
        "prefab-binding",
        "kind",
        "reopen",
    ] {
        let (mut workspace, mut authority, documents) = fixture(&basis);
        let mut work = work();
        let input = workspace
            .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
            .unwrap();
        match change {
            "open" => {
                open(&mut workspace, EditorDocumentKind::Scene, &basis);
            }
            "close" => {
                assert!(workspace.__discard_document(documents[1]).applied);
            }
            "replace" => {
                assert!(workspace.__discard_document(documents[1]).applied);
                open(&mut workspace, EditorDocumentKind::Scene, &basis);
            }
            "scene-binding" => {
                if let EditorDocumentSlot::Scene(slot) =
                    workspace.documents.get_mut(&documents[1]).unwrap()
                {
                    slot.source_binding = Some(EditorDocumentSourceBinding::new(
                        "scenes/rebound.scene.json",
                    ));
                }
            }
            "prefab-binding" => {
                if let EditorDocumentSlot::Prefab(slot) =
                    workspace.documents.get_mut(&documents[2]).unwrap()
                {
                    slot.source_binding =
                        EditorDocumentSourceBinding::new("prefabs/rebound.prefab.json");
                }
            }
            "kind" => {
                let mut other = EditorWorkspace::new();
                let replacement = open(&mut other, EditorDocumentKind::Prefab, &basis);
                let mut replacement = other.documents.remove(&replacement).unwrap();
                if let EditorDocumentSlot::Prefab(slot) = &mut replacement {
                    slot.source_binding = workspace
                        .document_source_binding(documents[1])
                        .unwrap()
                        .clone();
                }
                workspace.documents.insert(documents[1], replacement);
            }
            "reopen" => {
                let slot = workspace.scene(documents[1]).unwrap();
                let candidate = SceneDocumentCandidate::decode_json_str(
                    &slot.session().document().to_json_string().unwrap(),
                )
                .unwrap();
                let session = slot
                    .session()
                    .try_reopen_authoring_candidate(candidate)
                    .unwrap();
                let revision = slot.revision();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        documents[1],
                        revision,
                        session,
                        EditorDocumentDigest::new(19, [0x31; 32]),
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        assert_refused(&input.validate(&workspace, &mut work).unwrap_err());
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn identical_raw_document_ids_cannot_replace_actual_workspace_or_work_identity() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let (foreign, _, foreign_documents) = fixture(&basis);
    assert_eq!(documents, foreign_documents);
    let before = snapshot(&workspace);
    let foreign_before = snapshot(&foreign);
    let mut work = work();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
        .unwrap();
    assert_refused(&input.validate(&foreign, &mut work).unwrap_err());
    let retained = work.usage();
    let mut foreign_work = PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap();
    let report = input.validate(&workspace, &mut foreign_work).unwrap_err();
    assert_refused(&report);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.prefab-extraction-stale")
    );
    assert_eq!(work.usage(), retained);
    input.validate(&workspace, &mut work).unwrap();
    assert_eq!(snapshot(&workspace), before);
    assert_eq!(snapshot(&foreign), foreign_before);
}

#[test]
fn selection_requires_open_prefab_direct_record_and_actual_unchanged_basis() {
    let basis = basis();
    let foreign_basis = self::basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut work = work();
    for (selected, root, selected_basis) in [
        (documents[0], id("branch"), &basis),
        (EditorDocumentId::from_raw(9001), id("branch"), &basis),
        (documents[2], id("missing-root"), &basis),
        (documents[2], id("branch"), &foreign_basis),
    ] {
        let used = work.usage();
        let report = workspace
            .capture_prefab_extraction_input(selected, &root, selected_basis, &mut work)
            .unwrap_err();
        assert_refused(&report);
        assert!(work.usage().work_bytes > used.work_bytes);
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn actual_compiled_projection_membership_cannot_authorize_an_inherited_extraction_root() {
    let basis = basis();
    let inherited_path = AssetRef::path("prefabs/inherited.prefab.json").unwrap();
    let inherited_key = SourceDocumentKey::named(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        "inherited",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let inherited =
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("inherited-only"))]);
    let mut mount = SceneEntityRecord::new(id("mount"));
    mount.prefab = Some(PrefabInstance::forest(inherited_path.clone()));
    let source = PrefabDocument::from_ordered_roots([mount]);
    let resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(inherited_path, inherited_key, inherited);
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let report = source.compile(&registry, &resolver);
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let compiled = report.candidate.unwrap();
    let projected_root = compiled
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| origin.local_entity_id() == &id("inherited-only"))
        .unwrap()
        .0
        .clone();
    assert!(
        compiled
            .entities()
            .iter()
            .any(|entity| entity.id() == &projected_root)
    );
    assert!(
        source
            .entities
            .iter()
            .all(|entity| entity.id != projected_root)
    );

    let published = PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("prefabs/selected.prefab.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let mut workspace = EditorWorkspace::new();
    let selected = workspace
        .open_authoring_prefab_document(
            "Selected",
            EditorDocumentSourceBinding::new("prefabs/selected.prefab.json"),
            published,
        )
        .unwrap()
        .opened_document
        .unwrap();
    let before = snapshot(&workspace);
    let mut work = work();
    for root in [&projected_root, &id("inherited-only")] {
        assert_refused(
            &workspace
                .capture_prefab_extraction_input(selected, root, &basis, &mut work)
                .unwrap_err(),
        );
    }
    let input = workspace
        .capture_prefab_extraction_input(selected, &id("mount"), &basis, &mut work)
        .unwrap();
    assert_eq!(input.root(), &id("mount"));
    input.validate(&workspace, &mut work).unwrap();
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn complete_set_refuses_unbound_native_extension_and_foreign_basis_sources() {
    let basis = basis();
    for change in [
        "unbound",
        "native-scene",
        "native-prefab",
        "extension",
        "basis",
    ] {
        let (mut workspace, _, documents) = fixture(&basis);
        match change {
            "unbound" => {
                let published = SceneDocumentCandidate::decode_json_str(
                    &SceneDocument::default().to_json_string().unwrap(),
                )
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/unbound.scene.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish();
                workspace
                    .open_scene_session(
                        "Unbound",
                        SceneAuthoringSession::from_published_authoring_document(published),
                    )
                    .unwrap();
            }
            "native-scene" => {
                workspace
                    .open_scene_session_with_source_binding(
                        "Native",
                        Some(EditorDocumentSourceBinding::new("scenes/native.scene.json")),
                        SceneAuthoringSession::new(SceneDocument::default()),
                    )
                    .unwrap();
            }
            "native-prefab" => {
                workspace
                    .open_prefab_session(
                        "Native",
                        EditorDocumentSourceBinding::new("prefabs/native.prefab.json"),
                        PrefabAuthoringSession::new(PrefabDocument::default()),
                    )
                    .unwrap();
            }
            "extension" => {
                workspace
                    .open_extension_document(
                        EditorDocumentSourceBinding::new("extension/unsupported.json"),
                        UnsupportedExtension(SceneAuthoringSession::new(SceneDocument::default())),
                    )
                    .unwrap();
            }
            "basis" => {
                open(&mut workspace, EditorDocumentKind::Scene, &self::basis());
            }
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        let mut work = work();
        assert_refused(
            &workspace
                .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before, "{change}");
    }
}

#[test]
fn actual_live_scene_is_refused_without_retiring_its_world() {
    let basis = basis();
    let (mut workspace, _, documents) = fixture(&basis);
    let native = workspace
        .open_scene_session_with_source_binding(
            "Native",
            Some(EditorDocumentSourceBinding::new("scenes/live.scene.json")),
            SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
                SceneEntityRecord::new(id("live")),
            ])),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let mut world = nara_ecs::World::new();
    if let EditorDocumentSlot::Scene(slot) = workspace.documents.get_mut(&native).unwrap() {
        let report = slot
            .session
            .sync_world(
                &mut world,
                &registry,
                &SourceDocumentKey::named(
                    "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
                    "live",
                    SourceDocumentKind::Scene,
                )
                .unwrap(),
                &SceneOccurrenceKey::new("tooling.extraction.tests", "live").unwrap(),
            )
            .unwrap();
        assert!(report.synced);
    }
    let before = snapshot(&workspace);
    let mut work = work();
    assert_refused(
        &workspace
            .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
            .unwrap_err(),
    );
    assert!(
        workspace
            .scene(native)
            .unwrap()
            .session()
            .live_instance()
            .is_some()
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn exact_and_one_short_allowances_include_work_itself_and_all_preexisting_debits() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut measured = work();
    // An earlier consumer's debit is part of the same operation, not another capture allowance.
    measured.admit_capture_storage(Some(4096)).unwrap();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut measured)
        .unwrap();
    let required = measured.usage();
    drop(input);
    assert_eq!(measured.usage(), required);
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(required.work_bytes).unwrap())
        .with_reference_work_nodes(ItemLimit::new(required.work_nodes).unwrap());
    let mut exact_work = PrefabExtractionWork::new(exact).unwrap();
    exact_work.admit_capture_storage(Some(4096)).unwrap();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut exact_work)
        .unwrap();
    assert_eq!(exact_work.usage(), required);
    assert_refused(&input.validate(&workspace, &mut exact_work).unwrap_err());
    assert_eq!(exact_work.usage(), required);
    for limits in [
        exact.with_working_bytes(ByteLimit::new(required.work_bytes - 1).unwrap()),
        exact.with_reference_work_nodes(ItemLimit::new(required.work_nodes - 1).unwrap()),
    ] {
        let mut short = PrefabExtractionWork::new(limits).unwrap();
        short.admit_capture_storage(Some(4096)).unwrap();
        let initial = short.usage();
        let report = workspace
            .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut short)
            .unwrap_err();
        assert_refused(&report);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|entry| entry.code().as_str() == "scene.prefab-extraction-capacity")
        );
        assert!(short.usage().work_bytes > initial.work_bytes);
        let spent = short.usage();
        assert_refused(
            &workspace
                .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut short)
                .unwrap_err(),
        );
        assert!(short.usage().work_bytes >= spent.work_bytes);
        assert_eq!(snapshot(&workspace), before);
    }
    assert_eq!(snapshot(&workspace), before);
}

const P_PATH: &str = "prefabs/source-2.prefab.json";
const Q_PATH: &str = "prefabs/extracted.prefab.json";
const P_ASSET: &str = "63d8475a-1b07-4a22-aea7-3dc24991861f";
const Q_ASSET: &str = "626c4cba-1c8c-4880-bb54-b0575f1860c6";

fn corpus_key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn corpus_metadata(path: &str, asset: &str) -> AssetRecord {
    AssetRecord::new(
        asset.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Prefab,
    )
}

// The capture fixture intentionally has an unresolved nested source. This separate fixture
// publishes real instance consumers and one unrelated dirty Scene, retaining both history branches.
fn corpus_fixture(basis: &AuthoringSchemaBasis) -> (EditorWorkspace, [EditorDocumentId; 3]) {
    let (workspace, _, documents) = source_install_fixture(basis, false);
    (workspace, documents)
}

fn source_install_fixture(
    basis: &AuthoringSchemaBasis,
    rewrite_consumers: bool,
) -> (
    EditorWorkspace,
    EditorPersistenceAuthority,
    [EditorDocumentId; 3],
) {
    let (mut workspace, authority) = EditorWorkspace::new_hosted();
    let file_limits = if rewrite_consumers {
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(1024 * 1024).unwrap())
    } else {
        SceneFileLimits::default()
    };
    let history_limits = if rewrite_consumers {
        SceneAuthoringHistoryLimits::new(32, 4096, 4096, 1024 * 1024, 1024, 1024, 64 * 1024)
            .unwrap()
    } else {
        SceneAuthoringHistoryLimits::default()
    };
    let mut prefab =
        PrefabDocument::from_ordered_roots(source_document().entities.into_iter().take(2));
    if rewrite_consumers {
        prefab.entities.push(SceneEntityRecord::new(id("inside")));
        prefab.placements.push(SceneEntityPlacement::new(
            PrefabEntityTarget::source(id("inside")),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id("branch"),
            ))),
        ));
    }
    prefab
        .inputs
        .insert(ReferenceInputId::new("retained.input").unwrap());
    let scenes = std::array::from_fn::<_, 2, _>(|index| {
        let mut records = prefab.entities.clone();
        if index == 0 {
            for (anchor, reference) in [
                ("path-instance", AssetRef::path(P_PATH).unwrap()),
                (
                    "stable-instance",
                    AssetRef::StableId(P_ASSET.parse().unwrap()),
                ),
            ] {
                let mut mount = SceneEntityRecord::new(id(anchor));
                mount.prefab = Some(if rewrite_consumers {
                    let mut instance = PrefabInstance::root(
                        reference,
                        corpus_key(P_PATH, SourceDocumentKind::Prefab),
                        PrefabEntityTarget::source(id("inside")),
                    );
                    // Root binding requires one actual selected root. Promote the moving
                    // descendant before suppressing its parent and the independent root.
                    let mut overlay =
                        PrefabOverlay::new(corpus_key(P_PATH, SourceDocumentKind::Prefab));
                    overlay
                        .placements
                        .push(SceneEntityPlacement::root(id("inside")));
                    overlay.suppressions.extend([
                        PrefabEntityTarget::source(id("a-first")),
                        PrefabEntityTarget::source(id("branch")),
                    ]);
                    instance.overrides = Some(overlay);
                    instance
                } else {
                    PrefabInstance::forest(reference)
                });
                records.push(mount);
            }
        }
        let source = SceneDocument::from_ordered_roots(records);
        let path = format!("scenes/source-{index}.scene.json");
        let published = SceneDocumentCandidate::decode_json_bytes_with_limits(
            source.to_json_string().unwrap().as_bytes(),
            file_limits,
        )
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
            basis,
        )
        .unwrap()
        .publish();
        workspace
            .open_scene_session_with_source_binding(
                format!("source-{index}"),
                Some(EditorDocumentSourceBinding::new(path)),
                SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                    published,
                    history_limits,
                ),
            )
            .unwrap()
            .opened_document
            .unwrap()
    });
    let published = PrefabDocumentCandidate::decode_json_bytes_with_limits(
        prefab.to_json_string().unwrap().as_bytes(),
        file_limits,
    )
    .unwrap()
    .admit_authoring_with_basis(
        AuthoringDocumentSource::new(AssetRef::path(P_PATH).unwrap()),
        basis,
    )
    .unwrap()
    .publish();
    let selected = workspace
        .open_prefab_session(
            "source-2",
            EditorDocumentSourceBinding::new(P_PATH),
            PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
                published,
                history_limits,
            ),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let documents = [scenes[0], scenes[1], selected];
    for document in documents {
        set_value(&mut workspace, document, 8);
        set_value(&mut workspace, document, 9);
        replay(&mut workspace, document, true);
        select(&mut workspace, document, "a-first");
    }
    assert!(workspace.activate_document(documents[0]).applied);
    (workspace, authority, documents)
}

fn captured_corpus(
    input: &EditorWorkspacePrefabExtractionInput,
    work: &mut PrefabExtractionWork,
) -> Vec<CapturedExtractionSource> {
    // Admit the request, destination, physical sources and actual environment before construction.
    // Actual incumbent bodies and history were already admitted by Workspace capture.
    let environment_bytes = size_of::<PrefabExtractionEnvironment>()
        + "default".len()
        + input.documents().len() * size_of::<usize>()
        + 5 * size_of::<PrefabExtractionBinding>()
        + 4 * (P_PATH.len() + 2 * size_of::<usize>())
        + Q_PATH.len()
        + 2 * size_of::<usize>()
        + 2 * P_PATH.len();
    let mut bytes = size_of::<PrefabExtractionRequest>()
        + size_of::<PrefabExtractionDestination>()
        + input.documents().len() * size_of::<CapturedExtractionSource>()
        + 2 * size_of::<AssetRecord>()
        + 2 * (size_of::<SourceDocumentKey>() + 2 * size_of::<usize>())
        + 2 * P_PATH.len()
        + 2 * Q_PATH.len()
        + 6 * size_of::<usize>()
        + input.root().as_str().len()
        + environment_bytes;
    for document in input.documents() {
        bytes += size_of::<SourceDocumentKey>()
            + 2 * size_of::<usize>()
            + input
                .document_source_binding(document)
                .unwrap()
                .locator()
                .len();
    }
    work.admit_capture_storage(Some(bytes)).unwrap();
    let mut sources = Vec::with_capacity(input.documents().len());
    for document in input.documents() {
        let path = input.document_source_binding(document).unwrap().locator();
        let source = if let Some(scene) = input.scene_input(document) {
            CapturedExtractionSource::Scene {
                key: corpus_key(path, SourceDocumentKind::Scene),
                document: scene.shared_document(),
                basis: scene.schema_basis(),
            }
        } else {
            let prefab = input.prefab_input(document).unwrap();
            CapturedExtractionSource::Prefab {
                source: ResolvedPrefabSource::bound_authoring_source(
                    prefab.shared_document(),
                    prefab.schema_basis(),
                    corpus_key(path, SourceDocumentKind::Prefab),
                )
                .with_source_asset(&corpus_metadata(P_PATH, P_ASSET))
                .unwrap(),
            }
        };
        sources.push(source);
    }
    // Corpus ordering is independent of Workspace ordering; bindings must carry actual indices.
    sources.reverse();
    sources
}

// One actual environment contains every physical source, independent of Workspace ordering.
// Its resolver tables retain both P spellings; only the after table additionally resolves Q.
fn corpus_environment(source_count: usize) -> PrefabExtractionEnvironment {
    PrefabExtractionEnvironment {
        name: "default".into(),
        sources: (0..source_count).collect(),
        before: vec![
            PrefabExtractionBinding {
                reference: AssetRef::path(P_PATH).unwrap(),
                source: corpus_key(P_PATH, SourceDocumentKind::Prefab),
            },
            PrefabExtractionBinding {
                reference: AssetRef::StableId(P_ASSET.parse().unwrap()),
                source: corpus_key(P_PATH, SourceDocumentKind::Prefab),
            },
        ],
        after: vec![
            PrefabExtractionBinding {
                reference: AssetRef::path(P_PATH).unwrap(),
                source: corpus_key(P_PATH, SourceDocumentKind::Prefab),
            },
            PrefabExtractionBinding {
                reference: AssetRef::StableId(P_ASSET.parse().unwrap()),
                source: corpus_key(P_PATH, SourceDocumentKind::Prefab),
            },
            PrefabExtractionBinding {
                reference: AssetRef::StableId(Q_ASSET.parse().unwrap()),
                source: corpus_key(Q_PATH, SourceDocumentKind::Prefab),
            },
        ],
        assets_before: None,
        assets_after: None,
    }
}

fn corpus_request(
    input: &EditorWorkspacePrefabExtractionInput,
    work: &mut PrefabExtractionWork,
) -> PrefabExtractionRequest {
    let sources = captured_corpus(input, work);
    let environment = corpus_environment(sources.len());
    PrefabExtractionRequest::new(
        sources,
        vec![environment],
        0,
        input.schema_basis().clone(),
        corpus_key(P_PATH, SourceDocumentKind::Prefab),
        input.root().clone(),
        PrefabExtractionDestination {
            key: corpus_key(Q_PATH, SourceDocumentKind::Prefab),
            reference: AssetRef::StableId(Q_ASSET.parse().unwrap()),
            metadata: corpus_metadata(Q_PATH, Q_ASSET),
        },
    )
}

fn prepare_corpus(
    workspace: &EditorWorkspace,
    basis: &AuthoringSchemaBasis,
    documents: [EditorDocumentId; 3],
    limits: SceneCompilationLimits,
) -> (
    EditorWorkspacePrefabExtractionInput,
    PreparedPrefabExtraction,
) {
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    work.admit_capture_storage(Some(4096)).unwrap();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), basis, &mut work)
        .unwrap();
    let request = corpus_request(&input, &mut work);
    let prepared = std::thread::spawn(move || request.prepare(work))
        .join()
        .unwrap()
        .unwrap();
    (input, prepared)
}

fn corpus_bindings(documents: [EditorDocumentId; 3]) -> Vec<EditorPrefabExtractionCorpusBinding> {
    documents
        .into_iter()
        .enumerate()
        .map(|(index, document)| EditorPrefabExtractionCorpusBinding {
            document,
            source: 2 - index,
        })
        .collect()
}

fn corpus_owners(
    prepared: &PreparedPrefabExtraction,
) -> (
    *const CapturedExtractionSource,
    *const CapturedExtractionSource,
    *const PrefabDocument,
) {
    (
        prepared.original_sources().as_ptr(),
        prepared.sources().as_ptr(),
        prepared.extracted().document(),
    )
}

#[test]
fn actual_worker_corpus_binds_original_members_and_validates_navigation_without_installing() {
    let basis = basis();
    let (mut workspace, documents) = corpus_fixture(&basis);
    let before = snapshot(&workspace);
    let (input, corpus) = prepare_corpus(&workspace, &basis, documents, Default::default());
    let owners = corpus_owners(&corpus);
    let usage = corpus.usage();
    assert!(
        matches!(&corpus.original_sources()[0], CapturedExtractionSource::Prefab { source }
        if std::ptr::eq(source.document(), input.prefab_document(documents[2]).unwrap())
            && source.key() == &corpus_key(P_PATH, SourceDocumentKind::Prefab))
    );
    assert!(
        matches!(&corpus.original_sources()[2], CapturedExtractionSource::Scene { document, .. }
        if Arc::ptr_eq(document, &input.scene_input(documents[0]).unwrap().shared_document()))
    );
    let CapturedExtractionSource::Scene { document, .. } = &corpus.sources()[2] else {
        unreachable!()
    };
    // Both before bindings were resolved in the actual environment and retain their spellings.
    for (anchor, reference) in [
        ("path-instance", AssetRef::path(P_PATH).unwrap()),
        (
            "stable-instance",
            AssetRef::StableId(P_ASSET.parse().unwrap()),
        ),
    ] {
        assert_eq!(
            document
                .entities
                .iter()
                .find(|entity| entity.id == id(anchor))
                .unwrap()
                .prefab
                .as_ref()
                .unwrap()
                .source,
            reference
        );
    }
    assert_eq!(
        corpus.extracted().key(),
        &corpus_key(Q_PATH, SourceDocumentKind::Prefab)
    );
    assert_eq!(value(&corpus.extracted().document().entities), 8);
    let CapturedExtractionSource::Prefab { source } = &corpus.sources()[0] else {
        unreachable!()
    };
    assert!(
        source
            .document()
            .entities
            .iter()
            .any(|entity| entity.id == id("branch")
                && entity.prefab.as_ref().is_some_and(
                    |instance| instance.source == AssetRef::StableId(Q_ASSET.parse().unwrap())
                ))
    );
    let CapturedExtractionSource::Scene { document, .. } = &corpus.sources()[1] else {
        unreachable!()
    };
    assert_eq!(
        document.as_ref(),
        input.scene_document(documents[1]).unwrap()
    );

    let bindings = corpus_bindings(documents);
    let allocation = bindings.as_ptr();
    let mut prepared = input.bind_corpus(corpus, bindings).unwrap();
    assert_eq!(prepared.bindings(), corpus_bindings(documents));
    assert_eq!(prepared.bindings().as_ptr(), allocation);
    assert_eq!(corpus_owners(prepared.corpus()), owners);
    assert!(prepared.usage().work_bytes > usage.work_bytes);
    assert_eq!(prepared.input().schema_basis(), &basis);
    prepared.validate(&workspace).unwrap();
    assert_eq!(snapshot(&workspace), before);
    assert_eq!(workspace.len(), 3); // Q remains unopened and no saved/history checkpoint is installed.

    assert!(workspace.activate_document(documents[1]).applied);
    select(&mut workspace, documents[2], "branch");
    let navigated = snapshot(&workspace);
    let spent = prepared.usage();
    prepared.validate(&workspace).unwrap();
    assert!(prepared.usage().work_bytes > spent.work_bytes);
    assert_eq!(snapshot(&workspace), navigated);
    assert_eq!(corpus_owners(prepared.corpus()), owners);
    // In particular the unrelated guard still owns both real history branches.
    replay(&mut workspace, documents[1], false);
    replay(&mut workspace, documents[1], true);
}

#[test]
fn live_undo_after_worker_return_rejects_bound_corpus_even_after_redo() {
    let basis = basis();
    let (mut workspace, documents) = corpus_fixture(&basis);
    let (input, corpus) = prepare_corpus(&workspace, &basis, documents, Default::default());
    let owners = corpus_owners(&corpus);
    let output = corpus.extracted().document().to_json_string().unwrap();
    // The worker is finished while the actual unrelated guard owner executes Undo.
    replay(&mut workspace, documents[1], true);
    let mut prepared = input
        .bind_corpus(corpus, corpus_bindings(documents))
        .unwrap();
    let undone = snapshot(&workspace);
    let spent = prepared.usage();
    assert_refused(&prepared.validate(&workspace).unwrap_err());
    assert!(prepared.usage().work_bytes > spent.work_bytes);
    assert_eq!(snapshot(&workspace), undone);
    replay(&mut workspace, documents[1], false);
    let redone = snapshot(&workspace);
    assert_refused(&prepared.validate(&workspace).unwrap_err());
    assert_eq!(snapshot(&workspace), redone);
    assert_eq!(corpus_owners(prepared.corpus()), owners);
    assert_eq!(
        prepared
            .corpus()
            .extracted()
            .document()
            .to_json_string()
            .unwrap(),
        output
    );
}

#[test]
fn refused_mappings_return_exact_owners_and_corrected_retry_keeps_spent_work_and_output() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let before = snapshot(&workspace);
    let (mut input, mut corpus) = prepare_corpus(&workspace, &basis, documents, Default::default());
    let input_allocation = input.sources.as_ptr();
    let owners = corpus_owners(&corpus);
    let output = corpus.extracted().document().to_json_string().unwrap();
    for change in [
        "missing",
        "duplicate",
        "order",
        "foreign-document",
        "foreign-source",
        "kind",
        "out-of-range",
    ] {
        let mut bindings = corpus_bindings(documents);
        match change {
            "missing" => {
                bindings.pop();
            }
            "duplicate" => {
                bindings[1].source = bindings[0].source;
            }
            "order" => bindings.swap(0, 1),
            "foreign-document" => {
                bindings[0].document = EditorDocumentId::from_raw(9001);
            }
            "foreign-source" => {
                bindings[0].source = 1;
                bindings[1].source = 2;
            }
            "kind" => {
                bindings[0].source = 0;
            }
            "out-of-range" => {
                bindings[0].source = usize::MAX;
            }
            _ => unreachable!(),
        }
        let expected = bindings.clone();
        let allocation = bindings.as_ptr();
        let capacity = bindings.capacity();
        let spent = corpus.usage();
        let failure = input.bind_corpus(corpus, bindings).unwrap_err();
        assert_refused(failure.report());
        let (returned_input, returned_corpus, returned_bindings, report) = failure.into_parts();
        assert_refused(&report);
        assert_eq!(
            returned_input.sources.as_ptr(),
            input_allocation,
            "{change}"
        );
        assert_eq!(corpus_owners(&returned_corpus), owners, "{change}");
        assert_eq!(returned_bindings.as_ptr(), allocation, "{change}");
        assert_eq!(returned_bindings.capacity(), capacity);
        assert_eq!(returned_bindings, expected);
        assert!(returned_corpus.usage().work_bytes > spent.work_bytes);
        assert!(returned_corpus.usage().work_nodes > spent.work_nodes);
        assert_eq!(
            returned_corpus
                .extracted()
                .document()
                .to_json_string()
                .unwrap(),
            output
        );
        input = returned_input;
        corpus = returned_corpus;
    }
    let spent = corpus.usage();
    let mut prepared = input
        .bind_corpus(corpus, corpus_bindings(documents))
        .unwrap();
    assert!(prepared.usage().work_bytes > spent.work_bytes);
    prepared.validate(&workspace).unwrap();
    assert_eq!(corpus_owners(prepared.corpus()), owners);
    assert_eq!(
        prepared
            .corpus()
            .extracted()
            .document()
            .to_json_string()
            .unwrap(),
        output
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn equal_documents_from_another_actual_capture_cannot_bind_foreign_allocations() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let (foreign, foreign_documents) = corpus_fixture(&basis);
    let before = snapshot(&workspace);
    let foreign_before = snapshot(&foreign);
    let mut work = work();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
        .unwrap();
    let foreign_input = foreign
        .capture_prefab_extraction_input(foreign_documents[2], &id("branch"), &basis, &mut work)
        .unwrap();
    assert_eq!(
        input.scene_document(documents[0]),
        foreign_input.scene_document(foreign_documents[0])
    );
    assert!(!Arc::ptr_eq(
        &input.scene_input(documents[0]).unwrap().shared_document(),
        &foreign_input
            .scene_input(foreign_documents[0])
            .unwrap()
            .shared_document(),
    ));
    let request = corpus_request(&foreign_input, &mut work);
    let corpus = std::thread::spawn(move || request.prepare(work))
        .join()
        .unwrap()
        .unwrap();
    let owners = corpus_owners(&corpus);
    let input_allocation = input.sources.as_ptr();
    let usage = corpus.usage();
    let bindings = corpus_bindings(documents);
    let allocation = bindings.as_ptr();
    let failure = input.bind_corpus(corpus, bindings).unwrap_err();
    assert!(
        failure
            .report()
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.prefab-extraction-stale")
    );
    let (input, corpus, bindings, _) = failure.into_parts();
    assert_eq!(input.sources.as_ptr(), input_allocation);
    assert_eq!(corpus_owners(&corpus), owners);
    assert_eq!(bindings.as_ptr(), allocation);
    assert!(corpus.usage().work_bytes > usage.work_bytes);
    assert_eq!(snapshot(&workspace), before);
    assert_eq!(snapshot(&foreign), foreign_before);
}

#[test]
fn the_same_actual_document_arcs_under_foreign_work_cannot_bind_the_corpus() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let mut original_work = work();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut original_work)
        .unwrap();
    let spent = original_work.usage();
    let (foreign_input, corpus) = prepare_corpus(&workspace, &basis, documents, Default::default());
    assert!(Arc::ptr_eq(
        &input.scene_input(documents[0]).unwrap().shared_document(),
        &foreign_input
            .scene_input(documents[0])
            .unwrap()
            .shared_document(),
    ));
    let owners = corpus_owners(&corpus);
    let usage = corpus.usage();
    let failure = input
        .bind_corpus(corpus, corpus_bindings(documents))
        .unwrap_err();
    assert!(
        failure
            .report()
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.prefab-extraction-stale")
    );
    let (input, corpus, bindings, _) = failure.into_parts();
    assert_eq!(original_work.usage(), spent);
    assert_eq!(corpus_owners(&corpus), owners);
    assert!(corpus.usage().work_bytes > usage.work_bytes);
    assert_eq!(bindings, corpus_bindings(documents));
    input.validate(&workspace, &mut original_work).unwrap();
}

#[test]
fn binding_refuses_a_different_actual_cut_or_nominal_basis_and_retains_all_supplied_owners() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let foreign_basis = self::basis();
    let (foreign, foreign_documents) = corpus_fixture(&foreign_basis);
    for wrong_basis in [false, true] {
        let mut work = work();
        let input = workspace
            .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
            .unwrap();
        let other = if wrong_basis {
            foreign
                .capture_prefab_extraction_input(
                    foreign_documents[2],
                    &id("branch"),
                    &foreign_basis,
                    &mut work,
                )
                .unwrap()
        } else {
            workspace
                .capture_prefab_extraction_input(documents[2], &id("a-first"), &basis, &mut work)
                .unwrap()
        };
        let request = corpus_request(&other, &mut work);
        let corpus = std::thread::spawn(move || request.prepare(work))
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(input.schema_basis() == corpus.schema_basis(), !wrong_basis);
        assert_eq!(input.root() == corpus.root(), wrong_basis);
        let owners = corpus_owners(&corpus);
        let input_allocation = input.sources.as_ptr();
        let usage = corpus.usage();
        let bindings = corpus_bindings(documents);
        let allocation = bindings.as_ptr();
        let failure = input.bind_corpus(corpus, bindings).unwrap_err();
        assert_refused(failure.report());
        let (input, corpus, bindings, _) = failure.into_parts();
        assert_eq!(input.sources.as_ptr(), input_allocation);
        assert_eq!(corpus_owners(&corpus), owners);
        assert_eq!(bindings.as_ptr(), allocation);
        assert!(corpus.usage().work_bytes > usage.work_bytes);
    }
}

#[test]
fn the_selected_binding_must_name_the_corpus_selected_key_even_for_the_same_original_arc() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let mut work = work();
    let input = workspace
        .capture_prefab_extraction_input(documents[2], &id("branch"), &basis, &mut work)
        .unwrap();
    let mut sources = captured_corpus(&input, &mut work);
    // Admit the extra physical source, its keys and its environment membership before growth.
    work.admit_capture_storage(Some(
        4 * size_of::<CapturedExtractionSource>()
            + 2 * size_of::<SourceDocumentKey>()
            + 5 * size_of::<usize>()
            + 2 * "closed-decoy".len(),
    ))
    .unwrap();
    sources.reserve_exact(1);
    let selected = SourceDocumentKey::named(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        "closed-decoy",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let prefab = input.prefab_input(documents[2]).unwrap();
    sources.push(CapturedExtractionSource::Prefab {
        source: ResolvedPrefabSource::bound_authoring_source(
            prefab.shared_document(),
            prefab.schema_basis(),
            selected.clone(),
        ),
    });
    let environment = corpus_environment(sources.len());
    let request = PrefabExtractionRequest::new(
        sources,
        vec![environment],
        0,
        basis.clone(),
        selected,
        input.root().clone(),
        PrefabExtractionDestination {
            key: corpus_key(Q_PATH, SourceDocumentKind::Prefab),
            reference: AssetRef::StableId(Q_ASSET.parse().unwrap()),
            metadata: corpus_metadata(Q_PATH, Q_ASSET),
        },
    );
    let mut corpus = std::thread::spawn(move || request.prepare(work))
        .join()
        .unwrap()
        .unwrap();
    // Both indices contain the actual captured allocation/basis/Work; only one has the selected key.
    corpus.require_prefab_input(0, prefab).unwrap();
    corpus.require_prefab_input(3, prefab).unwrap();
    let owners = corpus_owners(&corpus);
    let spent = corpus.usage();
    let failure = input
        .bind_corpus(corpus, corpus_bindings(documents))
        .unwrap_err();
    assert_refused(failure.report());
    let (input, corpus, mut bindings, _) = failure.into_parts();
    assert_eq!(corpus_owners(&corpus), owners);
    assert!(corpus.usage().work_bytes > spent.work_bytes);
    bindings[2].source = 3;
    let mut prepared = input.bind_corpus(corpus, bindings).unwrap();
    prepared.validate(&workspace).unwrap();
    assert_eq!(corpus_owners(prepared.corpus()), owners);
}

#[test]
fn exact_and_one_short_binding_allowances_retain_preparation_and_refuse_without_a_fresh_work() {
    let basis = basis();
    let (workspace, documents) = corpus_fixture(&basis);
    let before = snapshot(&workspace);
    let (input, mut corpus) = prepare_corpus(&workspace, &basis, documents, Default::default());
    // Keep the final cumulative bound above the compiler's transient suspended working set.
    corpus.admit_binding_storage(Some(64 * 1024)).unwrap();
    let mut bindings = Vec::with_capacity(32);
    bindings.extend(corpus_bindings(documents));
    let prepared = input.bind_corpus(corpus, bindings).unwrap();
    let required = prepared.usage();
    let exact = SceneCompilationLimits::default()
        .with_working_bytes(ByteLimit::new(required.work_bytes).unwrap());
    let (input, mut corpus) = prepare_corpus(&workspace, &basis, documents, exact);
    corpus.admit_binding_storage(Some(64 * 1024)).unwrap();
    let mut bindings = Vec::with_capacity(32);
    bindings.extend(corpus_bindings(documents));
    let mut prepared = input.bind_corpus(corpus, bindings).unwrap();
    assert_eq!(prepared.usage(), required);
    assert_refused(&prepared.validate(&workspace).unwrap_err());
    assert_eq!(prepared.usage(), required);

    let short = exact.with_working_bytes(ByteLimit::new(required.work_bytes - 1).unwrap());
    let (input, mut corpus) = prepare_corpus(&workspace, &basis, documents, short);
    corpus.admit_binding_storage(Some(64 * 1024)).unwrap();
    let owners = corpus_owners(&corpus);
    let input_allocation = input.sources.as_ptr();
    let captured = corpus.usage();
    let mut bindings = Vec::with_capacity(32);
    bindings.extend(corpus_bindings(documents));
    let allocation = bindings.as_ptr();
    let failure = input.bind_corpus(corpus, bindings).unwrap_err();
    assert!(
        failure
            .report()
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "scene.prefab-extraction-capacity")
    );
    let (input, corpus, bindings, _) = failure.into_parts();
    assert_eq!(input.sources.as_ptr(), input_allocation);
    assert_eq!(corpus_owners(&corpus), owners);
    assert_eq!(bindings.as_ptr(), allocation);
    let spent = corpus.usage();
    assert!(spent.work_bytes > captured.work_bytes);
    let failure = input.bind_corpus(corpus, bindings).unwrap_err();
    let (input, corpus, bindings, _) = failure.into_parts();
    assert_eq!(input.sources.as_ptr(), input_allocation);
    assert_eq!(corpus_owners(&corpus), owners);
    assert_eq!(bindings.as_ptr(), allocation);
    assert!(corpus.usage().work_bytes >= spent.work_bytes);
    assert_eq!(snapshot(&workspace), before);
}

fn source_corpus(
    workspace: &EditorWorkspace,
    basis: &AuthoringSchemaBasis,
    documents: [EditorDocumentId; 3],
    limits: SceneCompilationLimits,
) -> PreparedEditorWorkspacePrefabExtraction {
    let (input, mut corpus) = prepare_corpus(workspace, basis, documents, limits);
    // Keep final cumulative admission above the compiler's suspended transient working set.
    corpus.admit_binding_storage(Some(64 * 1024)).unwrap();
    input
        .bind_corpus(corpus, corpus_bindings(documents))
        .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct TokenSnapshot {
    output: usize,
    classification: usize,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
}

fn token_snapshot(
    prepared: &PreparedEditorWorkspaceExtractionSources,
) -> Vec<Option<TokenSnapshot>> {
    prepared
        .sources
        .iter()
        .map(|source| {
            Some(match source.as_ref()? {
                PreparedSourceKind::Scene(source) => TokenSnapshot {
                    output: source.document() as *const SceneDocument as usize,
                    classification: source.source() as *const AuthoringDocumentSource as usize,
                    revision: source.revision(),
                    content: source.content_state(),
                },
                PreparedSourceKind::Prefab(source) => TokenSnapshot {
                    output: source.document() as *const PrefabDocument as usize,
                    classification: source.source() as *const AuthoringDocumentSource as usize,
                    revision: source.revision(),
                    content: source.content_state(),
                },
            })
        })
        .collect()
}

#[test]
fn worker_prepares_only_actual_changed_outputs_under_the_original_source_policies() {
    let basis = basis();
    for rewrite_consumers in [false, true] {
        let (workspace, _, documents) = source_install_fixture(&basis, rewrite_consumers);
        let before = snapshot(&workspace);
        let prepared = source_corpus(&workspace, &basis, documents, Default::default());
        let owners = corpus_owners(prepared.corpus());
        let spent = prepared.usage();
        let prepared = std::thread::spawn(move || prepared.prepare_sources())
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(corpus_owners(prepared.prepared().corpus()), owners);
        assert!(prepared.usage().work_bytes > spent.work_bytes);
        assert_eq!(prepared.sources.len(), workspace.len());
        for (index, binding) in prepared.prepared().bindings().iter().enumerate() {
            assert_eq!(
                prepared.sources[index].is_some(),
                prepared
                    .prepared()
                    .corpus()
                    .source_changed(binding.source)
                    .unwrap(),
            );
        }
        assert!(prepared.scene_source(documents[1]).is_none());
        assert_eq!(
            prepared.scene_source(documents[0]).is_some(),
            rewrite_consumers
        );
        let prefab = prepared.prefab_source(documents[2]).unwrap();
        let input = prepared
            .prepared()
            .input()
            .prefab_input(documents[2])
            .unwrap();
        assert_eq!(prefab.schema_basis(), basis);
        assert_eq!(prefab.file_limits(), input.file_limits());
        assert_eq!(prefab.history_limits(), input.history_limits());
        assert_eq!(prefab.base_revision(), input.revision());
        assert_eq!(prefab.base_content_state(), input.content_state());
        assert_eq!(prefab.source(), input.source());
        assert_eq!(
            prefab.source_key(),
            &corpus_key(P_PATH, SourceDocumentKind::Prefab)
        );
        let CapturedExtractionSource::Prefab { source } =
            &prepared.prepared().corpus().sources()[0]
        else {
            unreachable!()
        };
        assert!(std::ptr::eq(prefab.document(), source.document()));
        workspace
            .prefab(documents[2])
            .unwrap()
            .session()
            .validate_extracted_source(prefab)
            .unwrap();
        if let Some(scene) = prepared.scene_source(documents[0]) {
            let input = prepared
                .prepared()
                .input()
                .scene_input(documents[0])
                .unwrap();
            assert_eq!(scene.schema_basis(), basis);
            assert_eq!(scene.file_limits(), input.file_limits());
            assert_eq!(scene.history_limits(), input.history_limits());
            assert_eq!(scene.base_revision(), input.revision());
            assert_eq!(scene.base_content_state(), input.content_state());
            assert_eq!(scene.source(), input.source());
            workspace
                .scene(documents[0])
                .unwrap()
                .session()
                .validate_extracted_source(scene)
                .unwrap();
        }
        drop(prepared);
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn bound_source_drop_and_retry_preserve_all_incumbents_exact_tokens_and_future_incarnations() {
    let basis = basis();
    let (mut workspace, authority, documents) = source_install_fixture(&basis, true);
    let mut prepared = source_corpus(&workspace, &basis, documents, Default::default())
        .prepare_sources()
        .unwrap();
    let owners = corpus_owners(prepared.prepared().corpus());
    let tokens = token_snapshot(&prepared);
    let allocation = prepared.sources.as_ptr();
    assert!(workspace.activate_document(documents[1]).applied);
    select(&mut workspace, documents[2], "branch");
    let before = snapshot(&workspace);
    for _ in 0..2 {
        let spent = prepared.usage();
        let bound = authority
            .bind_prefab_extraction_sources(&mut workspace, prepared)
            .unwrap();
        assert!(bound.prepared().usage().work_bytes > spent.work_bytes);
        assert_eq!(bound.sources.len(), 3);
        assert!(matches!(&bound.sources[0].kind, BoundSourceKind::Scene(_)));
        assert!(matches!(
            &bound.sources[1].kind,
            BoundSourceKind::SceneGuard(_)
        ));
        assert!(matches!(&bound.sources[2].kind, BoundSourceKind::Prefab(_)));
        assert_eq!(
            bound.scene_source(documents[0]).unwrap().revision(),
            tokens[0].as_ref().unwrap().revision
        );
        assert!(bound.scene_source(documents[1]).is_none());
        assert_eq!(
            bound.prefab_source(documents[2]).unwrap().content_state(),
            tokens[2].as_ref().unwrap().content
        );
        let bound_usage = bound.prepared().usage();
        prepared = bound.into_prepared();
        assert_eq!(prepared.usage(), bound_usage);
        assert_eq!(prepared.sources.as_ptr(), allocation);
        assert_eq!(corpus_owners(prepared.prepared().corpus()), owners);
        assert_eq!(token_snapshot(&prepared), tokens);
        assert_eq!(snapshot(&workspace), before);
    }
    drop(
        authority
            .bind_prefab_extraction_sources(&mut workspace, prepared)
            .unwrap(),
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn selective_source_commit_installs_real_successors_and_preserves_dirty_guard_saved_and_reload_state()
 {
    let basis = basis();
    let (mut workspace, authority, documents) = source_install_fixture(&basis, true);
    for (index, document) in documents.iter().enumerate() {
        let slot = workspace.documents.get_mut(document).unwrap();
        slot.mark_external_changed();
        slot.set_saved_digest(EditorDocumentDigest::new(31, [index as u8; 32]));
    }
    let prepared = source_corpus(&workspace, &basis, documents, Default::default())
        .prepare_sources()
        .unwrap();
    let tokens = token_snapshot(&prepared);
    let scene_policy = prepared.scene_source(documents[0]).unwrap().file_limits();
    let prefab_policy = prepared.prefab_source(documents[2]).unwrap().file_limits();
    let history_policy = prepared
        .prefab_source(documents[2])
        .unwrap()
        .history_limits();
    assert_ne!(scene_policy, SceneFileLimits::default());
    assert_ne!(history_policy, SceneAuthoringHistoryLimits::default());
    assert!(workspace.activate_document(documents[1]).applied);
    select(&mut workspace, documents[2], "branch");
    let before = snapshot(&workspace);
    authority
        .bind_prefab_extraction_sources(&mut workspace, prepared)
        .unwrap()
        .commit();
    let after = snapshot(&workspace);
    assert_eq!(workspace.len(), 3); // Q is still unopened; no publication receipt is invented.
    assert_eq!(after.active, before.active);
    assert_eq!(after.sources[1], before.sources[1]);
    assert_eq!(after.tabs[1], before.tabs[1]);
    for index in [0, 2] {
        let token = tokens[index].as_ref().unwrap();
        assert_eq!(after.sources[index].address, token.output);
        assert_eq!(after.sources[index].content, token.content);
        assert_eq!(after.tabs[index].revision, token.revision);
        assert_ne!(
            after.tabs[index].revision.source_id(),
            before.tabs[index].revision.source_id()
        );
        assert_eq!(
            after.sources[index].usage,
            SceneAuthoringHistoryUsage::default()
        );
        assert_eq!(after.tabs[index].history, DocumentHistoryStatus::default());
        assert_eq!(
            after.tabs[index].saved_revision,
            before.tabs[index].saved_revision
        );
        assert_eq!(
            after.sources[index].saved_content,
            before.sources[index].saved_content
        );
        assert_eq!(after.sources[index].digest, before.sources[index].digest);
        assert_eq!(
            after.sources[index].selection,
            before.sources[index].selection
        );
        assert_eq!(
            after.tabs[index].external_reload,
            EditorExternalReloadState::Conflict
        );
        assert!(after.tabs[index].dirty);
    }
    let scene = workspace.scene(documents[0]).unwrap().session();
    let prefab = workspace.prefab(documents[2]).unwrap().session();
    assert_eq!(scene.authoring_schema_basis(), Some(basis.clone()));
    assert_eq!(prefab.authoring_schema_basis(), Some(basis.clone()));
    assert_eq!(scene.history_limits(), history_policy);
    assert_eq!(prefab.history_limits(), history_policy);
    let mut policy_probe = work();
    assert_eq!(
        scene
            .capture_prefab_extraction_input(&mut policy_probe)
            .unwrap()
            .file_limits(),
        scene_policy
    );
    assert_eq!(
        prefab
            .capture_prefab_extraction_input(&mut policy_probe)
            .unwrap()
            .file_limits(),
        prefab_policy
    );
    for anchor in ["path-instance", "stable-instance"] {
        let instance = scene
            .document()
            .entities
            .iter()
            .find(|entity| entity.id == id(anchor))
            .unwrap()
            .prefab
            .as_ref()
            .unwrap();
        let PrefabInstanceBinding::Root { root, .. } = &instance.binding else {
            unreachable!()
        };
        assert_eq!(root.through.len(), 1);
        assert_eq!(
            root.through[0].expected_base,
            corpus_key(Q_PATH, SourceDocumentKind::Prefab)
        );
    }
    // The unchanged dirty guard still owns its original Undo and Redo branches.
    replay(&mut workspace, documents[1], false);
    replay(&mut workspace, documents[1], true);
}

#[test]
fn late_source_changes_and_foreign_authority_refuse_with_the_same_exact_prepared_tokens() {
    let basis = basis();
    for change in [
        "undo",
        "undo-redo",
        "reopen",
        "scene-binding",
        "prefab-binding",
        "open",
        "close",
        "foreign-authority",
    ] {
        let (mut workspace, mut authority, documents) = source_install_fixture(&basis, true);
        let prepared = source_corpus(&workspace, &basis, documents, Default::default())
            .prepare_sources()
            .unwrap();
        let owners = corpus_owners(prepared.prepared().corpus());
        let tokens = token_snapshot(&prepared);
        let allocation = prepared.sources.as_ptr();
        let spent = prepared.usage();
        let (_, foreign) = EditorWorkspace::new_hosted();
        match change {
            "undo" => replay(&mut workspace, documents[1], true),
            "undo-redo" => {
                replay(&mut workspace, documents[1], true);
                replay(&mut workspace, documents[1], false);
            }
            "reopen" => {
                let slot = workspace.scene(documents[1]).unwrap();
                let candidate = SceneDocumentCandidate::decode_json_str(
                    &slot.session().document().to_json_string().unwrap(),
                )
                .unwrap();
                let session = slot
                    .session()
                    .try_reopen_authoring_candidate(candidate)
                    .unwrap();
                let revision = slot.revision();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        documents[1],
                        revision,
                        session,
                        EditorDocumentDigest::new(19, [0x31; 32]),
                    )
                    .unwrap();
            }
            "scene-binding" => {
                let EditorDocumentSlot::Scene(slot) =
                    workspace.documents.get_mut(&documents[1]).unwrap()
                else {
                    unreachable!()
                };
                slot.source_binding = Some(EditorDocumentSourceBinding::new(
                    "scenes/rebound.scene.json",
                ));
            }
            "prefab-binding" => {
                let EditorDocumentSlot::Prefab(slot) =
                    workspace.documents.get_mut(&documents[2]).unwrap()
                else {
                    unreachable!()
                };
                slot.source_binding =
                    EditorDocumentSourceBinding::new("prefabs/rebound.prefab.json");
            }
            "open" => {
                open(&mut workspace, EditorDocumentKind::Scene, &basis);
            }
            "close" => {
                assert!(workspace.__discard_document(documents[1]).applied);
            }
            "foreign-authority" => {}
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        let binder = if change == "foreign-authority" {
            &foreign
        } else {
            &authority
        };
        let failure = binder
            .bind_prefab_extraction_sources(&mut workspace, prepared)
            .unwrap_err();
        assert_refused(failure.report());
        let (prepared, report) = failure.into_parts();
        assert_refused(&report);
        assert_eq!(prepared.sources.as_ptr(), allocation, "{change}");
        assert_eq!(token_snapshot(&prepared), tokens, "{change}");
        assert_eq!(corpus_owners(prepared.prepared().corpus()), owners);
        assert!(prepared.usage().work_bytes >= spent.work_bytes);
        assert_eq!(snapshot(&workspace), before, "{change}");
        if change == "foreign-authority" {
            assert_eq!(prepared.usage(), spent);
            drop(
                authority
                    .bind_prefab_extraction_sources(&mut workspace, prepared)
                    .unwrap(),
            );
            assert_eq!(snapshot(&workspace), before);
        }
    }
}

#[test]
fn complete_source_binding_refuses_a_real_valid_token_from_another_corpus() {
    let basis = basis();
    for index in [0, 2] {
        let (mut workspace, authority, documents) = source_install_fixture(&basis, true);
        let mut first = source_corpus(&workspace, &basis, documents, Default::default())
            .prepare_sources()
            .unwrap();
        let mut other = source_corpus(&workspace, &basis, documents, Default::default())
            .prepare_sources()
            .unwrap();
        std::mem::swap(&mut first.sources[index], &mut other.sources[index]);
        // A source owner accepts this real token in isolation; the paired corpus must refuse.
        if index == 0 {
            workspace
                .scene(documents[0])
                .unwrap()
                .session()
                .validate_extracted_source(first.scene_source(documents[0]).unwrap())
                .unwrap();
        } else {
            workspace
                .prefab(documents[2])
                .unwrap()
                .session()
                .validate_extracted_source(first.prefab_source(documents[2]).unwrap())
                .unwrap();
        }
        let tokens = token_snapshot(&first);
        let owners = corpus_owners(first.prepared().corpus());
        let spent = first.usage();
        let before = snapshot(&workspace);
        let failure = authority
            .bind_prefab_extraction_sources(&mut workspace, first)
            .unwrap_err();
        assert_refused(failure.report());
        let (first, _) = failure.into_parts();
        assert_eq!(token_snapshot(&first), tokens);
        assert_eq!(corpus_owners(first.prepared().corpus()), owners);
        assert!(first.usage().work_bytes > spent.work_bytes);
        assert_eq!(snapshot(&workspace), before);
    }
}

#[test]
fn complete_source_binding_refuses_missing_extra_or_wrong_kind_tokens_without_touching_incumbents()
{
    let basis = basis();
    for change in [
        "missing",
        "extra",
        "wrong-kind",
        "missing-classification",
        "incomplete",
    ] {
        let (mut workspace, authority, documents) = source_install_fixture(&basis, true);
        let mut prepared = source_corpus(&workspace, &basis, documents, Default::default())
            .prepare_sources()
            .unwrap();
        let mut retained = None;
        match change {
            "missing" => {
                retained = prepared.sources[2].take();
            }
            "extra" => {
                let input = prepared.prepared.input.scene_input(documents[0]).unwrap();
                let source = prepared
                    .prepared
                    .corpus
                    .prepare_scene_source(2, input)
                    .unwrap();
                prepared.sources[1] = Some(PreparedSourceKind::Scene(source));
            }
            "wrong-kind" => prepared.sources.swap(0, 2),
            "missing-classification" => prepared.prepared.bindings[0].source = usize::MAX,
            "incomplete" => {
                retained = prepared.sources.pop().unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(&workspace);
        let tokens = token_snapshot(&prepared);
        let failure = authority
            .bind_prefab_extraction_sources(&mut workspace, prepared)
            .unwrap_err();
        assert_refused(failure.report());
        let (prepared, _) = failure.into_parts();
        assert_eq!(token_snapshot(&prepared), tokens);
        assert_eq!(snapshot(&workspace), before);
        drop(retained);
    }
}

#[test]
fn exact_and_one_short_source_classification_and_guard_storage_keep_the_original_spent_corpus() {
    let basis = basis();
    let (mut workspace, authority, documents) = source_install_fixture(&basis, true);
    let before = snapshot(&workspace);
    let prepared = source_corpus(&workspace, &basis, documents, Default::default())
        .prepare_sources()
        .unwrap();
    let classification = prepared.usage();
    let bound = authority
        .bind_prefab_extraction_sources(&mut workspace, prepared)
        .unwrap();
    let binding = bound.prepared().usage();
    let prepared = bound.into_prepared();
    assert_eq!(prepared.usage(), binding);
    drop(prepared);
    for (required, bind) in [(classification, false), (binding, true)] {
        let exact = SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(required.work_bytes).unwrap());
        let prepared = source_corpus(&workspace, &basis, documents, exact)
            .prepare_sources()
            .unwrap();
        if bind {
            let bound = authority
                .bind_prefab_extraction_sources(&mut workspace, prepared)
                .unwrap();
            assert_eq!(bound.prepared().usage(), required);
            drop(bound);
        } else {
            assert_eq!(prepared.usage(), required);
        }
        let short = exact.with_working_bytes(ByteLimit::new(required.work_bytes - 1).unwrap());
        let original = source_corpus(&workspace, &basis, documents, short);
        let owners = corpus_owners(original.corpus());
        let input_allocation = original.input.sources.as_ptr();
        let mapping_allocation = original.bindings.as_ptr();
        if bind {
            let prepared = original.prepare_sources().unwrap();
            let tokens = token_snapshot(&prepared);
            let allocation = prepared.sources.as_ptr();
            let spent = prepared.usage();
            let failure = authority
                .bind_prefab_extraction_sources(&mut workspace, prepared)
                .unwrap_err();
            assert_refused(failure.report());
            assert!(
                failure
                    .report()
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code().as_str() == "scene.prefab-extraction-capacity")
            );
            let (prepared, _) = failure.into_parts();
            assert_eq!(prepared.sources.as_ptr(), allocation);
            assert_eq!(token_snapshot(&prepared), tokens);
            assert_eq!(corpus_owners(prepared.prepared().corpus()), owners);
            assert!(prepared.usage().work_bytes > spent.work_bytes);
            let guard_storage = documents.len() * size_of::<BoundSource<'_>>()
                + size_of::<EditorWorkspacePrefabExtractionSourceCommit<'_>>()
                    .max(size_of::<EditorWorkspacePrefabExtractionSourceBindFailure>());
            assert_eq!(
                prepared.usage().work_bytes,
                required.work_bytes - guard_storage
            );
            let used = prepared.usage();
            let failure = authority
                .bind_prefab_extraction_sources(&mut workspace, prepared)
                .unwrap_err();
            let (prepared, _) = failure.into_parts();
            assert_eq!(token_snapshot(&prepared), tokens);
            assert!(prepared.usage().work_bytes >= used.work_bytes);
        } else {
            let spent = original.usage();
            let failure = original.prepare_sources().unwrap_err();
            assert_refused(failure.report());
            assert!(
                failure
                    .report()
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code().as_str() == "scene.prefab-extraction-capacity")
            );
            let (original, _) = failure.into_parts();
            assert_eq!(corpus_owners(original.corpus()), owners);
            assert_eq!(original.input.sources.as_ptr(), input_allocation);
            assert_eq!(original.bindings.as_ptr(), mapping_allocation);
            assert!(original.usage().work_bytes > spent.work_bytes);
            let used = original.usage();
            let failure = original.prepare_sources().unwrap_err();
            let (original, _) = failure.into_parts();
            assert_eq!(corpus_owners(original.corpus()), owners);
            assert!(original.usage().work_bytes >= used.work_bytes);
        }
        assert_eq!(snapshot(&workspace), before);
    }
}

// The existing extension seam wraps a real source/history owner; it is never an extraction input.
struct UnsupportedExtension(SceneAuthoringSession);

impl EditorExtensionDocument for UnsupportedExtension {
    type Command = Infallible;
    type View = ();
    const TYPE_ID: &'static str = "test.prefab-extraction.unsupported";

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
