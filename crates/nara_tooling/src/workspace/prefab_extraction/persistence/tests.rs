use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(not(windows))]
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::{ByteLimit, DocumentHistoryStatus};
use nara_fs::{
    CapabilityRights, DirectoryCapability, ExpectedTarget, HostCapabilityOptions,
    RelativeComponent, RelativePath, TrustMode,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{
        AuthoringAdapterReceipt, CapturedExtractionSource, PrefabExtractionBinding,
        PrefabExtractionDestination, PrefabExtractionEnvironment, PrefabExtractionRequest,
    },
    AuthoringFieldEdit, PrefabDocumentCandidate, PrefabEntityTarget, PrefabInstance, PrefabOverlay,
    ResolvedPrefabSource, SceneAuthoringHistoryLimits, SceneCompilationLimits,
    SceneComponentRecord, SceneDocumentCandidate, SceneEntityPlacement, SceneEntityRecord,
    ScenePlacement, ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

use super::*;
use crate::workspace::{EditorDocumentTabModel, EditorWorkspaceCommand};

const COMPONENT: &str = "test.extraction-saved.Counter";
const P_PATH: &str = "prefabs/source.prefab.json";
const Q_PATH: &str = "prefabs/extracted.prefab.json";
const P_ASSET: &str = "63d8475a-1b07-4a22-aea7-3dc24991861f";
const Q_ASSET: &str = "626c4cba-1c8c-4880-bb54-b0575f1860c6";

mod encoding;
mod saved_generation;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.extraction-saved").unwrap()
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

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn metadata(path: &str, asset: &str) -> AssetRecord {
    AssetRecord::new(
        asset.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Prefab,
    )
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
    assert!(workspace.commit_authoring_edit(prepared).applied);
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
    assert!(workspace.apply_authoring_command(command).applied);
}

fn fixture(
    basis: &AuthoringSchemaBasis,
) -> (
    EditorWorkspace,
    EditorPersistenceAuthority,
    [EditorDocumentId; 4],
) {
    let (mut workspace, authority) = EditorWorkspace::new_hosted();
    let mut prefab = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("a-first")),
        SceneEntityRecord::new(id("branch")).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("value", ComponentValue::I64(7))]),
            )
            .with_owner(owner()),
        ),
    ]);
    prefab.entities.push(SceneEntityRecord::new(id("inside")));
    prefab.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::source(id("inside")),
        ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
            id("branch"),
        ))),
    ));
    let file_limits =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(1024 * 1024).unwrap());
    let history_limits =
        SceneAuthoringHistoryLimits::new(32, 4096, 4096, 1024 * 1024, 1024, 1024, 64 * 1024)
            .unwrap();
    let scenes = std::array::from_fn::<_, 3, _>(|index| {
        let mut records = prefab.entities.clone();
        if index == 0 {
            let mut instance = PrefabInstance::root(
                AssetRef::path(P_PATH).unwrap(),
                key(P_PATH, SourceDocumentKind::Prefab),
                PrefabEntityTarget::source(id("inside")),
            );
            let mut overlay = PrefabOverlay::new(key(P_PATH, SourceDocumentKind::Prefab));
            overlay
                .placements
                .push(SceneEntityPlacement::root(id("inside")));
            overlay.suppressions.extend([
                PrefabEntityTarget::source(id("a-first")),
                PrefabEntityTarget::source(id("branch")),
            ]);
            instance.overrides = Some(overlay);
            let mut mount = SceneEntityRecord::new(id("mount"));
            mount.prefab = Some(instance);
            records.push(mount);
        }
        let path = format!("scenes/source-{index}.scene.json");
        let source = SceneDocument::from_ordered_roots(records);
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
                &path,
                Some(EditorDocumentSourceBinding::new(path.as_str())),
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
            P_PATH,
            EditorDocumentSourceBinding::new(P_PATH),
            PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
                published,
                history_limits,
            ),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let documents = [scenes[0], scenes[1], scenes[2], selected];
    for document in documents {
        set_value(&mut workspace, document, 8);
        set_value(&mut workspace, document, 9);
        replay(&mut workspace, document, true);
    }
    (workspace, authority, documents)
}

fn sources(
    workspace: &EditorWorkspace,
    basis: &AuthoringSchemaBasis,
    documents: [EditorDocumentId; 4],
    limits: SceneCompilationLimits,
) -> PreparedEditorWorkspaceExtractionSources {
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let input = workspace
        .capture_prefab_extraction_input(documents[3], &id("branch"), basis, &mut work)
        .unwrap();
    // Admit the actual environment's membership and before/after bindings before allocation,
    // alongside the existing fixed request/key/destination storage.
    let environment_bytes = size_of::<PrefabExtractionEnvironment>()
        + "default".len()
        + input.documents().len() * size_of::<usize>()
        + 3 * size_of::<PrefabExtractionBinding>()
        + 2 * (P_PATH.len() + 2 * size_of::<usize>())
        + Q_PATH.len()
        + 2 * size_of::<usize>()
        + 2 * P_PATH.len();
    work.admit_capture_storage(Some(16 * 1024 + environment_bytes))
        .unwrap();
    let members = input
        .documents()
        .map(|document| {
            let path = input.document_source_binding(document).unwrap().locator();
            if let Some(source) = input.scene_input(document) {
                CapturedExtractionSource::Scene {
                    key: key(path, SourceDocumentKind::Scene),
                    document: source.shared_document(),
                    basis: source.schema_basis(),
                }
            } else {
                let source = input.prefab_input(document).unwrap();
                CapturedExtractionSource::Prefab {
                    source: ResolvedPrefabSource::bound_authoring_source(
                        source.shared_document(),
                        source.schema_basis(),
                        key(path, SourceDocumentKind::Prefab),
                    )
                    .with_source_asset(&metadata(P_PATH, P_ASSET))
                    .unwrap(),
                }
            }
        })
        .collect();
    let environment = PrefabExtractionEnvironment {
        name: "default".into(),
        sources: (0..input.documents().len()).collect(),
        before: vec![PrefabExtractionBinding {
            reference: AssetRef::path(P_PATH).unwrap(),
            source: key(P_PATH, SourceDocumentKind::Prefab),
        }],
        after: vec![
            PrefabExtractionBinding {
                reference: AssetRef::path(P_PATH).unwrap(),
                source: key(P_PATH, SourceDocumentKind::Prefab),
            },
            PrefabExtractionBinding {
                reference: AssetRef::StableId(Q_ASSET.parse().unwrap()),
                source: key(Q_PATH, SourceDocumentKind::Prefab),
            },
        ],
        assets_before: None,
        assets_after: None,
    };
    let request = PrefabExtractionRequest::new(
        members,
        vec![environment],
        0,
        basis.clone(),
        key(P_PATH, SourceDocumentKind::Prefab),
        id("branch"),
        PrefabExtractionDestination {
            key: key(Q_PATH, SourceDocumentKind::Prefab),
            reference: AssetRef::StableId(Q_ASSET.parse().unwrap()),
            metadata: metadata(Q_PATH, Q_ASSET),
        },
    );
    let mut corpus = request.prepare(work).unwrap();
    corpus.admit_binding_storage(Some(64 * 1024)).unwrap();
    let bindings = documents
        .into_iter()
        .enumerate()
        .map(|(source, document)| EditorPrefabExtractionCorpusBinding { document, source })
        .collect();
    input
        .bind_corpus(corpus, bindings)
        .unwrap()
        .prepare_sources()
        .unwrap()
}

fn stage(
    workspace: &EditorWorkspace,
    basis: &AuthoringSchemaBasis,
    documents: [EditorDocumentId; 4],
) -> PreparedEditorWorkspaceExtractionPersistence {
    sources(workspace, basis, documents, Default::default())
        .prepare_persistence(vec![documents[1]])
        .unwrap()
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara_extraction_saved_{}_{}_{}",
            std::process::id(),
            stamp,
            sequence,
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn directory(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let handle = {
            use std::os::windows::fs::OpenOptionsExt;
            // FILE_FLAG_BACKUP_SEMANTICS and FILE_FLAG_OPEN_REPARSE_POINT, without adding a
            // platform dependency just to import an actual directory handle in this test.
            OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.0)
                .unwrap()
        };
        #[cfg(not(windows))]
        let handle = File::open(&self.0).unwrap();
        DirectoryCapability::from_host_handle(
            handle,
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn published(encoded: EncodedEditorPersistenceCheckpoint) -> EditorPersistenceCommit {
    let root = TestRoot::new();
    fs::write(root.0.join("source.scene"), b"old source").unwrap();
    let directory = root.directory();
    let old = directory
        .open_file(&RelativePath::new("source.scene").unwrap())
        .unwrap();
    let mut candidate = directory
        .create_temp(&RelativeComponent::new("candidate.tmp").unwrap())
        .unwrap();
    candidate.write_all(encoded.bytes()).unwrap();
    candidate.sync().unwrap();
    let receipt = directory
        .replace_temp(
            candidate,
            &RelativeComponent::new("source.scene").unwrap(),
            ExpectedTarget::Identity(old.identity()),
        )
        .unwrap();
    assert_eq!(receipt.verified_content_digest(), Some(encoded.digest()));
    let commit =
        EditorPersistenceCommit::from_publication(encoded, old.identity(), receipt).unwrap();
    drop(old);
    drop(directory);
    commit
}

fn publish_all(stage: &mut PreparedEditorWorkspaceExtractionPersistence) {
    let documents: Vec<_> = stage
        .documents()
        .filter(|(_, disposition)| *disposition != EditorPrefabExtractionDisposition::Guard)
        .map(|(document, _)| document)
        .collect();
    for document in documents {
        let encoded = stage
            .encode_checkpoint(document, SceneFileEncoding::Json)
            .unwrap();
        let readmission = stage.authoring_readmission(document).unwrap();
        let limits = stage.file_limits(document).unwrap();
        let preserved = match encoded.checkpoint().kind() {
            EditorDocumentKind::Scene => readmission
                .preserves_scene_candidate(
                    SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.bytes(), limits)
                        .unwrap(),
                )
                .unwrap(),
            EditorDocumentKind::Prefab => readmission
                .preserves_prefab_candidate(
                    PrefabDocumentCandidate::decode_json_bytes_with_limits(encoded.bytes(), limits)
                        .unwrap(),
                )
                .unwrap(),
            EditorDocumentKind::Extension(_) => unreachable!(),
        };
        assert!(preserved);
        stage.accept_commit(published(encoded)).unwrap();
    }
}

#[derive(Debug, PartialEq)]
struct SlotSnapshot {
    tab: EditorDocumentTabModel,
    source: usize,
    classification: usize,
    content: SceneAuthoringContentState,
    saved: SceneAuthoringContentState,
    digest: Option<EditorDocumentDigest>,
    upgrade: bool,
}

fn snapshot(workspace: &EditorWorkspace) -> Vec<SlotSnapshot> {
    workspace
        .authoring_model()
        .documents
        .into_iter()
        .map(|tab| {
            let (source, classification, content, saved, digest, upgrade) =
                match &workspace.documents[&tab.document] {
                    EditorDocumentSlot::Scene(slot) => (
                        slot.session.document() as *const SceneDocument as usize,
                        slot.session.authoring_source().unwrap() as *const AuthoringDocumentSource
                            as usize,
                        slot.session.content_state(),
                        slot.saved_content_state,
                        slot.saved_digest,
                        slot.session.source_upgrade_required(),
                    ),
                    EditorDocumentSlot::Prefab(slot) => (
                        slot.session.document() as *const PrefabDocument as usize,
                        slot.session.authoring_source().unwrap() as *const AuthoringDocumentSource
                            as usize,
                        slot.session.content_state(),
                        slot.saved_content_state,
                        slot.saved_digest,
                        slot.session.source_upgrade_required(),
                    ),
                    EditorDocumentSlot::Extension(_) => unreachable!(),
                };
            SlotSnapshot {
                tab,
                source,
                classification,
                content,
                saved,
                digest,
                upgrade,
            }
        })
        .collect()
}

#[test]
fn borrowed_saved_outputs_match_exact_successors_and_current_input_without_mutation() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let stage = stage(&workspace, &basis, documents);
    let scene = stage
        .sources()
        .scene_source(documents[0])
        .unwrap()
        .shared_document();
    let current = stage
        .sources()
        .prepared()
        .input()
        .scene_input(documents[1])
        .unwrap()
        .shared_document();
    let prefab = stage
        .sources()
        .prefab_source(documents[3])
        .unwrap()
        .shared_document();
    let usage = stage.usage();
    let counts = [
        Arc::strong_count(&scene),
        Arc::strong_count(&current),
        Arc::strong_count(&prefab),
    ];
    assert!(Arc::ptr_eq(
        stage.saved_scene(documents[0]).unwrap(),
        &scene
    ));
    assert!(Arc::ptr_eq(
        stage.saved_scene(documents[1]).unwrap(),
        &current
    ));
    assert!(Arc::ptr_eq(
        stage.saved_prefab(documents[3]).unwrap(),
        &prefab
    ));
    assert_ne!(Arc::as_ptr(&scene) as usize, before[0].source);
    assert_eq!(Arc::as_ptr(&current) as usize, before[1].source);
    assert_ne!(Arc::as_ptr(&prefab) as usize, before[3].source);
    for document in documents[..3].iter().copied() {
        assert!(stage.saved_prefab(document).is_none());
    }
    for document in [documents[2], documents[3], EditorDocumentId::from_raw(9001)] {
        assert!(stage.saved_scene(document).is_none());
    }
    assert!(
        stage
            .saved_prefab(EditorDocumentId::from_raw(9001))
            .is_none()
    );
    assert_eq!(stage.usage(), usage);
    assert_eq!(
        [
            Arc::strong_count(&scene),
            Arc::strong_count(&current),
            Arc::strong_count(&prefab),
        ],
        counts
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn borrowed_saved_outputs_remain_available_across_checkpoint_evidence_states() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut stage = stage(&workspace, &basis, documents);
    for document in [documents[0], documents[1], documents[3]] {
        let scene = stage.saved_scene(document).map(Arc::as_ptr);
        let prefab = stage.saved_prefab(document).map(Arc::as_ptr);
        let assert_fixed = |stage: &PreparedEditorWorkspaceExtractionPersistence| {
            let usage = stage.usage();
            assert_eq!(stage.saved_scene(document).map(Arc::as_ptr), scene);
            assert_eq!(stage.saved_prefab(document).map(Arc::as_ptr), prefab);
            assert!(stage.saved_scene(documents[2]).is_none());
            assert!(stage.saved_prefab(documents[2]).is_none());
            assert_eq!(stage.usage(), usage);
            assert_eq!(snapshot(&workspace), before);
        };
        let (checkpoint, _) = stage.take_checkpoint(document).unwrap();
        assert_eq!(checkpoint.scene().map(std::ptr::from_ref), scene);
        assert_eq!(checkpoint.prefab().map(std::ptr::from_ref), prefab);
        assert_fixed(&stage);
        stage.return_checkpoint(checkpoint).unwrap();
        assert_fixed(&stage);
        let encoded = stage
            .encode_checkpoint(document, SceneFileEncoding::Json)
            .unwrap();
        assert_fixed(&stage);
        let commit = published(encoded);
        assert_fixed(&stage);
        stage.accept_commit(commit).unwrap();
        assert_fixed(&stage);
    }
    drop(stage);
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn three_dispositions_install_exact_future_saves_and_preserve_current_history_and_conflict() {
    let basis = basis();
    let (mut workspace, authority, documents) = fixture(&basis);
    for (index, document) in documents.into_iter().enumerate() {
        match workspace.documents.get_mut(&document).unwrap() {
            EditorDocumentSlot::Scene(slot) => {
                slot.external_reload = if index == 0 {
                    EditorExternalReloadState::Pending
                } else {
                    EditorExternalReloadState::Conflict
                }
            }
            EditorDocumentSlot::Prefab(slot) => {
                slot.external_reload = EditorExternalReloadState::Conflict
            }
            EditorDocumentSlot::Extension(_) => unreachable!(),
        }
    }
    let before = snapshot(&workspace);
    let mut stage = stage(&workspace, &basis, documents);
    assert_eq!(
        stage.documents().collect::<Vec<_>>(),
        [
            (
                documents[0],
                EditorPrefabExtractionDisposition::ReplaceSource
            ),
            (documents[1], EditorPrefabExtractionDisposition::SaveCurrent),
            (documents[2], EditorPrefabExtractionDisposition::Guard),
            (
                documents[3],
                EditorPrefabExtractionDisposition::ReplaceSource
            ),
        ]
    );
    assert!(stage.authoring_readmission(documents[2]).is_none());
    assert!(stage.take_checkpoint(documents[2]).is_err());
    publish_all(&mut stage);
    assert_eq!(snapshot(&workspace), before);
    let expected: Vec<_> = stage
        .entries
        .iter()
        .map(|entry| {
            entry.save.as_ref().map(|save| {
                let SavedEvidence::Published(commit) = &save.evidence else {
                    unreachable!()
                };
                (save.revision, save.content_state, commit.digest())
            })
        })
        .collect();
    authority
        .bind_prefab_extraction_persistence(&mut workspace, stage)
        .unwrap()
        .commit();
    let after = snapshot(&workspace);
    assert_eq!(workspace.len(), 4); // Q remains closed; this is not Project publication.
    assert_eq!(after[2], before[2]);
    assert_eq!(after[1].source, before[1].source);
    assert_eq!(after[1].classification, before[1].classification);
    assert_eq!(after[1].tab.history, before[1].tab.history);
    assert_eq!(after[1].tab.revision, before[1].tab.revision);
    assert_eq!(after[1].content, before[1].content);
    assert_eq!(
        after[1].tab.external_reload,
        EditorExternalReloadState::Conflict
    );
    for index in [0, 1, 3] {
        let (revision, content, digest) = expected[index].unwrap();
        assert_eq!(after[index].tab.saved_revision, revision);
        assert_eq!(after[index].tab.revision, revision);
        assert_eq!(after[index].saved, content);
        assert_eq!(after[index].digest, Some(digest));
        assert!(!after[index].tab.dirty);
        assert!(!after[index].upgrade);
    }
    for index in [0, 3] {
        assert_ne!(after[index].source, before[index].source);
        assert_ne!(after[index].classification, before[index].classification);
        assert_ne!(
            after[index].tab.revision.source_id(),
            before[index].tab.revision.source_id()
        );
        assert_eq!(after[index].tab.history, DocumentHistoryStatus::default());
    }
    assert_eq!(
        after[0].tab.external_reload,
        EditorExternalReloadState::Clean
    );
    assert_eq!(
        after[3].tab.external_reload,
        EditorExternalReloadState::Conflict
    );
    for index in [1, 2] {
        replay(&mut workspace, documents[index], false);
        replay(&mut workspace, documents[index], true);
    }
    assert_eq!(
        workspace
            .scene(documents[1])
            .unwrap()
            .session()
            .content_state(),
        before[1].content
    );
}

#[test]
fn drop_and_release_retry_keep_exact_tables_captures_commits_and_spent_work() {
    let basis = basis();
    let (mut workspace, authority, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut stage = stage(&workspace, &basis, documents);
    publish_all(&mut stage);
    let table = stage.entries.as_ptr();
    let source_table = stage.sources.sources.as_ptr();
    let captures: Vec<_> = stage
        .entries
        .iter()
        .map(|entry| entry.save.as_ref().map(|save| Arc::as_ptr(&save.capture)))
        .collect();
    for _ in 0..2 {
        let spent = stage.usage();
        let guard = authority
            .bind_prefab_extraction_persistence(&mut workspace, stage)
            .unwrap();
        let bound = guard.prepared().usage();
        assert!(bound.work_bytes > spent.work_bytes);
        stage = guard.into_prepared();
        assert_eq!(stage.usage(), bound);
        assert_eq!(stage.entries.as_ptr(), table);
        assert_eq!(stage.sources.sources.as_ptr(), source_table);
        for (entry, capture) in stage.entries.iter().zip(&captures) {
            assert_eq!(
                entry.save.as_ref().map(|save| Arc::as_ptr(&save.capture)),
                *capture
            );
            if let Some(save) = &entry.save {
                assert!(matches!(&save.evidence, SavedEvidence::Published(_)));
            }
        }
        assert_eq!(snapshot(&workspace), before);
    }
    drop(
        authority
            .bind_prefab_extraction_persistence(&mut workspace, stage)
            .unwrap(),
    );
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn encoding_failure_returns_same_checkpoint_and_stage_encoding_refusal_keeps_it_available() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let mut stage = stage(&workspace, &basis, documents);
    let (checkpoint, _) = stage.take_checkpoint(documents[1]).unwrap();
    let capture = Arc::clone(&checkpoint.capture);
    let failure = checkpoint
        .encode(
            SceneFileEncoding::Json,
            SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(1).unwrap()),
        )
        .unwrap_err();
    let checkpoint = failure.into_checkpoint();
    assert!(Arc::ptr_eq(&capture, &checkpoint.capture));
    stage.return_checkpoint(checkpoint).unwrap();
    let encoded = stage
        .encode_checkpoint(documents[1], SceneFileEncoding::Json)
        .unwrap();
    assert!(Arc::ptr_eq(&capture, &encoded.checkpoint().capture));
    assert!(
        stage
            .encode_checkpoint(documents[1], SceneFileEncoding::Json)
            .is_err()
    );
    stage.return_checkpoint(encoded.into_checkpoint()).unwrap();
    let encoded = stage
        .encode_checkpoint(documents[1], SceneFileEncoding::Ron)
        .unwrap();
    assert!(Arc::ptr_eq(&capture, &encoded.checkpoint().capture));
    stage.accept_commit(published(encoded)).unwrap();
}

#[test]
fn ordinary_equal_capture_foreign_workspace_and_guard_publications_cannot_substitute() {
    let basis = basis();
    let (workspace, mut authority, documents) = fixture(&basis);
    let (foreign, mut foreign_authority, foreign_documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut stage = stage(&workspace, &basis, documents);
    let encoded = stage
        .encode_checkpoint(documents[1], SceneFileEncoding::Json)
        .unwrap();
    for checkpoint in [
        authority.capture(&workspace, documents[1]).unwrap(),
        foreign_authority
            .capture(&foreign, foreign_documents[1])
            .unwrap(),
        authority.capture(&workspace, documents[2]).unwrap(),
        authority.capture(&workspace, documents[3]).unwrap(),
    ] {
        let capture = Arc::clone(&checkpoint.capture);
        let commit = published(
            checkpoint
                .encode(SceneFileEncoding::Json, Default::default())
                .unwrap(),
        );
        let failure = stage.accept_commit(commit).unwrap_err();
        assert!(!failure.report().applied);
        let (commit, _) = failure.into_parts();
        assert!(Arc::ptr_eq(&capture, &commit.checkpoint.capture));
        assert_eq!(snapshot(&workspace), before);
    }
    stage.accept_commit(published(encoded)).unwrap();
}

// Adversarial owning fixture: copy a private checkpoint, never manufacture a raw digest proof.
// Every resulting commit still requires the actual codec output and filesystem receipt.
fn copy_checkpoint(
    checkpoint: &EditorPersistenceCheckpoint,
    equal_allocation: bool,
) -> EditorPersistenceCheckpoint {
    EditorPersistenceCheckpoint {
        identity: Arc::clone(&checkpoint.identity),
        capture: Arc::clone(&checkpoint.capture),
        document: checkpoint.document,
        kind: checkpoint.kind,
        source_binding: checkpoint.source_binding.clone(),
        revision: checkpoint.revision,
        content_state: checkpoint.content_state,
        content: match &checkpoint.content {
            EditorPersistenceContent::Scene(source) => {
                EditorPersistenceContent::Scene(if equal_allocation {
                    Arc::new(source.as_ref().clone())
                } else {
                    Arc::clone(source)
                })
            }
            EditorPersistenceContent::Prefab(source) => {
                EditorPersistenceContent::Prefab(if equal_allocation {
                    Arc::new(source.as_ref().clone())
                } else {
                    Arc::clone(source)
                })
            }
            EditorPersistenceContent::Extension(_) => unreachable!(),
        },
    }
}

#[test]
fn equal_output_allocation_wrong_binding_and_duplicate_commit_are_refused_even_with_same_capture() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let mut stage = stage(&workspace, &basis, documents);
    let (checkpoint, _) = stage.take_checkpoint(documents[1]).unwrap();
    let duplicate = copy_checkpoint(&checkpoint, false);
    let equal = copy_checkpoint(&checkpoint, true);
    assert_eq!(equal.scene(), checkpoint.scene());
    assert!(
        stage
            .accept_commit(published(
                equal
                    .encode(SceneFileEncoding::Json, Default::default())
                    .unwrap()
            ))
            .is_err()
    );
    let mut wrong_binding = copy_checkpoint(&checkpoint, false);
    wrong_binding.source_binding =
        Some(EditorDocumentSourceBinding::new("scenes/other.scene.json"));
    assert!(
        stage
            .accept_commit(published(
                wrong_binding
                    .encode(SceneFileEncoding::Json, Default::default())
                    .unwrap()
            ))
            .is_err()
    );
    let mut wrong_revision = copy_checkpoint(&checkpoint, false);
    wrong_revision.revision = stage.sources.scene_source(documents[0]).unwrap().revision();
    assert!(
        stage
            .accept_commit(published(
                wrong_revision
                    .encode(SceneFileEncoding::Json, Default::default())
                    .unwrap()
            ))
            .is_err()
    );
    let mut wrong_content = copy_checkpoint(&checkpoint, false);
    wrong_content.content_state = stage
        .sources
        .scene_source(documents[0])
        .unwrap()
        .content_state();
    assert!(
        stage
            .accept_commit(published(
                wrong_content
                    .encode(SceneFileEncoding::Json, Default::default())
                    .unwrap()
            ))
            .is_err()
    );
    stage
        .accept_commit(published(
            checkpoint
                .encode(SceneFileEncoding::Json, Default::default())
                .unwrap(),
        ))
        .unwrap();
    assert!(
        stage
            .accept_commit(published(
                duplicate
                    .encode(SceneFileEncoding::Json, Default::default())
                    .unwrap()
            ))
            .is_err()
    );
}

#[test]
fn missing_commits_and_stale_live_sources_saved_fields_reload_or_foreign_authority_preserve_stage()
{
    let basis = basis();
    for change in ["missing", "source", "saved", "reload", "foreign"] {
        let (mut workspace, mut authority, documents) = fixture(&basis);
        let mut stage = stage(&workspace, &basis, documents);
        if change != "missing" {
            publish_all(&mut stage);
        }
        match change {
            "source" => replay(&mut workspace, documents[2], true),
            "saved" => {
                let checkpoint = authority.capture(&workspace, documents[2]).unwrap();
                let commit = published(
                    checkpoint
                        .encode(SceneFileEncoding::Json, Default::default())
                        .unwrap(),
                );
                assert!(authority.commit(&mut workspace, commit).applied);
            }
            "reload" => workspace
                .documents
                .get_mut(&documents[2])
                .unwrap()
                .mark_external_changed(),
            _ => {}
        }
        let before = snapshot(&workspace);
        let table = stage.entries.as_ptr();
        let source_table = stage.sources.sources.as_ptr();
        let spent = stage.usage();
        let (_, other_authority) = EditorWorkspace::new_hosted();
        let binder = if change == "foreign" {
            &other_authority
        } else {
            &authority
        };
        let failure = binder
            .bind_prefab_extraction_persistence(&mut workspace, stage)
            .unwrap_err();
        assert!(!failure.report().applied);
        let (stage, _) = failure.into_parts();
        assert_eq!(stage.entries.as_ptr(), table);
        assert_eq!(stage.sources.sources.as_ptr(), source_table);
        assert!(stage.usage().work_bytes >= spent.work_bytes);
        assert_eq!(snapshot(&workspace), before);
        if change == "foreign" {
            authority
                .bind_prefab_extraction_persistence(&mut workspace, stage)
                .unwrap()
                .commit();
        }
    }
}

#[test]
fn failed_review_selection_returns_exact_sources_and_selection_without_new_checkpoint() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    for selection in [
        vec![documents[1], documents[1]],
        vec![documents[3]],
        vec![EditorDocumentId::from_raw(9001)],
    ] {
        let sources = sources(&workspace, &basis, documents, Default::default());
        let source_table = sources.sources.as_ptr();
        let selection_table = selection.as_ptr();
        let expected = selection.clone();
        let spent = sources.usage();
        let failure = sources.prepare_persistence(selection).unwrap_err();
        let (sources, selection, _) = failure.into_parts();
        assert_eq!(sources.sources.as_ptr(), source_table);
        assert_eq!(selection.as_ptr(), selection_table);
        assert_eq!(selection, expected);
        assert!(sources.usage().work_bytes > spent.work_bytes);
    }
}

#[test]
fn another_future_history_for_the_same_output_cannot_replace_the_fixed_saved_source() {
    let basis = basis();
    let (mut workspace, authority, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut stage = stage(&workspace, &basis, documents);
    publish_all(&mut stage);
    let prepared = &mut stage.sources.prepared;
    let replacement = prepared
        .corpus
        .prepare_prefab_source(
            prepared.bindings[3].source,
            prepared.input.prefab_input(documents[3]).unwrap(),
        )
        .unwrap();
    let original = stage.sources.sources[3].replace(PreparedSourceKind::Prefab(replacement));
    let failure = authority
        .bind_prefab_extraction_persistence(&mut workspace, stage)
        .unwrap_err();
    let (mut stage, _) = failure.into_parts();
    assert_eq!(snapshot(&workspace), before);
    stage.sources.sources[3] = original;
    authority
        .bind_prefab_extraction_persistence(&mut workspace, stage)
        .unwrap()
        .commit();
}

#[test]
fn exact_and_one_short_encoding_allowance_restore_the_same_capture_without_refund() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let mut measured = stage(&workspace, &basis, documents);
    let encoded = measured
        .encode_checkpoint(documents[1], SceneFileEncoding::Json)
        .unwrap();
    let required = measured.usage().work_bytes;
    drop(encoded);
    for deficit in [0, 1] {
        let limits = SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(required - deficit).unwrap());
        let mut stage = sources(&workspace, &basis, documents, limits)
            .prepare_persistence(vec![documents[1]])
            .unwrap();
        let capture = Arc::clone(&stage.entries[1].save.as_ref().unwrap().capture);
        let spent = stage.usage();
        if deficit == 0 {
            let encoded = stage
                .encode_checkpoint(documents[1], SceneFileEncoding::Json)
                .unwrap();
            assert_eq!(stage.usage().work_bytes, required);
            assert!(Arc::ptr_eq(&capture, &encoded.checkpoint().capture));
        } else {
            let failure = stage
                .encode_checkpoint(documents[1], SceneFileEncoding::Json)
                .unwrap_err();
            assert!(!failure.report().applied);
            let save = stage.entries[1].save.as_ref().unwrap();
            let SavedEvidence::Available(checkpoint) = &save.evidence else {
                panic!("failed encoding lost its checkpoint")
            };
            assert!(Arc::ptr_eq(&capture, &checkpoint.capture));
            assert!(stage.usage().work_bytes > spent.work_bytes);
            let failed = stage.usage();
            assert!(
                stage
                    .encode_checkpoint(documents[1], SceneFileEncoding::Json)
                    .is_err()
            );
            assert!(stage.usage().work_bytes >= failed.work_bytes);
        }
    }
}

#[test]
fn saved_stage_and_complete_guard_storage_are_admitted_before_allocation_or_installation() {
    let basis = basis();
    let (mut workspace, authority, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut measured = stage(&workspace, &basis, documents);
    let preparation = measured.usage().work_bytes;
    publish_all(&mut measured);
    let guard = authority
        .bind_prefab_extraction_persistence(&mut workspace, measured)
        .unwrap();
    let binding = guard.prepared().usage().work_bytes;
    drop(guard);
    for (required, bind) in [(preparation, false), (binding, true)] {
        for deficit in [0, 1] {
            let limits = SceneCompilationLimits::default()
                .with_working_bytes(ByteLimit::new(required - deficit).unwrap());
            let sources = sources(&workspace, &basis, documents, limits);
            let source_table = sources.sources.as_ptr();
            let result = sources.prepare_persistence(vec![documents[1]]);
            if !bind && deficit == 1 {
                let (sources, selection, report) = result.unwrap_err().into_parts();
                assert_eq!(sources.sources.as_ptr(), source_table);
                assert_eq!(selection, [documents[1]]);
                assert!(!report.applied);
                assert_eq!(snapshot(&workspace), before);
                continue;
            }
            let mut stage = result.unwrap();
            if !bind {
                assert_eq!(stage.usage().work_bytes, required);
                continue;
            }
            publish_all(&mut stage);
            let table = stage.entries.as_ptr();
            let spent = stage.usage();
            let result = authority.bind_prefab_extraction_persistence(&mut workspace, stage);
            if deficit == 0 {
                let guard = result.unwrap();
                assert_eq!(guard.prepared().usage().work_bytes, required);
                drop(guard);
            } else {
                let (stage, report) = result.unwrap_err().into_parts();
                assert_eq!(stage.entries.as_ptr(), table);
                assert_eq!(stage.sources.sources.as_ptr(), source_table);
                assert!(!report.applied);
                assert!(stage.usage().work_bytes >= spent.work_bytes);
                assert!(stage.entries.iter().all(|entry| {
                    entry
                        .save
                        .as_ref()
                        .is_none_or(|save| matches!(&save.evidence, SavedEvidence::Published(_)))
                }));
            }
            assert_eq!(snapshot(&workspace), before);
        }
    }
}
