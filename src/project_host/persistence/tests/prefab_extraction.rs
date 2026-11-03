//! The real filesystem consumer of selective Workspace source/saved guards.
//! Complete Project Pending/Ready, Q/sidecar publication and crash recovery are separate owners.

use super::*;

use nara_asset::{AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath};
use nara_core::ByteLimit;
use nara_scene::{
    __private::{
        CapturedExtractionSource, PrefabExtractionBinding, PrefabExtractionDestination,
        PrefabExtractionEnvironment, PrefabExtractionRequest, PrefabExtractionWork,
    },
    PrefabEntityTarget, ResolvedPrefabSource, SceneCompilationLimits, SceneEntityPlacement,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementParent,
    SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::{
    EditorPrefabExtractionCorpusBinding, EditorPrefabExtractionDisposition,
    PreparedEditorWorkspaceExtractionPersistence,
};

const P_PATH: &str = "reference-ui.prefab.json";
const Q_PATH: &str = "extracted.prefab.json";

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

struct SavedSceneFixture {
    workspace: EditorWorkspace,
    basis: AuthoringSchemaBasis,
    host: ScenePersistenceHost,
    stage: PreparedEditorWorkspaceExtractionPersistence,
    current: EditorDocumentId,
    selected: EditorDocumentId,
}

fn saved_scene_fixture(root: &TestRoot, limits: SceneCompilationLimits) -> SavedSceneFixture {
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("original"))]);
    let prefab = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")),
        SceneEntityRecord::new(id("branch")),
    ]);
    fs::write(root.scene_path(), scene.to_json_string().unwrap()).unwrap();
    fs::write(root.path.join(P_PATH), prefab.to_json_string().unwrap()).unwrap();
    let directory = root.capability();
    let file = directory
        .open_file(&RelativePath::new("startup.scene.json").unwrap())
        .unwrap();
    let bytes = file.read_to_end_bounded(65_536).unwrap();
    let published = SceneDocumentCandidate::decode_json_bytes(&bytes)
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("startup.scene.json").unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let (host, _) = ScenePersistenceHost::bind_authoring_source(
        &directory,
        "startup.scene.json",
        file,
        ContentDigest::of_bytes(&bytes),
    )
    .unwrap();
    let (mut workspace, _) = EditorWorkspace::new_hosted();
    let current = workspace
        .open_authoring_scene_document(
            "Current",
            EditorDocumentSourceBinding::new("startup.scene.json"),
            published,
        )
        .unwrap()
        .opened_document
        .unwrap();
    add(&mut workspace, current, "edited");
    let published = PrefabDocumentCandidate::decode_json_str(&prefab.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(P_PATH).unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let selected = workspace
        .open_authoring_prefab_document("P", EditorDocumentSourceBinding::new(P_PATH), published)
        .unwrap()
        .opened_document
        .unwrap();
    let stage = persistence_stage(&workspace, &basis, &[current, selected], limits);
    SavedSceneFixture {
        workspace,
        basis,
        host,
        stage,
        current,
        selected,
    }
}

#[test]
fn real_mismatched_receipt_returns_the_exact_encoded_checkpoint_receipt_and_file() {
    let root = TestRoot::new();
    let mut fixture = saved_scene_fixture(&root, SceneCompilationLimits::default());
    let before = fixture.host.accepted_source_observation().unwrap();
    let revision = fixture
        .workspace
        .document_revision(fixture.current)
        .unwrap();
    let mut encoded = fixture
        .stage
        .encode_checkpoint(fixture.current, SceneFileEncoding::Json)
        .unwrap();
    let allocation = encoded.bytes().as_ptr();
    let source = std::ptr::from_ref(encoded.checkpoint().scene().unwrap());
    let expected = encoded.bytes().to_vec();
    let unrelated = b"unrelated actual extraction output";
    let FilePublicationOutcome::Published(publication) =
        fixture.host.file.publication.publish(unrelated)
    else {
        panic!("the mismatched filesystem control must publish successfully")
    };
    let (evidence, published) = publication.into_parts();
    let (mut receipt, _) = evidence.into_parts();
    let (file, digest) = published.into_parts();
    let identity = file.identity();
    let spent = fixture.stage.usage();
    for _ in 0..2 {
        let failure =
            EditorPersistenceCommit::from_publication(encoded, before.0, receipt).unwrap_err();
        (encoded, receipt) = failure.into_parts();
        assert_eq!(encoded.bytes().as_ptr(), allocation);
        assert_eq!(
            std::ptr::from_ref(encoded.checkpoint().scene().unwrap()),
            source
        );
        assert_eq!(encoded.checkpoint().revision(), revision);
        assert_eq!(encoded.bytes(), expected);
        assert_eq!(receipt.previous_identity(), Some(before.0));
        assert_eq!(receipt.candidate_identity(), identity);
        assert_eq!(receipt.published_identity(), Some(identity));
        assert_eq!(receipt.written_content_digest(), digest);
        assert_eq!(receipt.verified_content_digest(), Some(digest));
        assert_ne!(digest, encoded.digest());
        assert_eq!(file.read_to_end_bounded(65_536).unwrap(), unrelated);
        assert_eq!(fs::read(root.scene_path()).unwrap(), unrelated);
        assert_eq!(fixture.stage.usage(), spent);
        assert_eq!(fixture.host.accepted_source_observation(), Some(before));
        assert_eq!(
            fixture.workspace.document_revision(fixture.current),
            Some(revision)
        );
        assert!(fixture.workspace.scene(fixture.current).unwrap().is_dirty());
    }
    fixture
        .stage
        .return_checkpoint(encoded.into_checkpoint())
        .unwrap();
}

#[test]
fn extraction_acceptance_retains_a_real_mismatched_publication_on_every_refusal() {
    let root = TestRoot::new();
    let mut fixture = saved_scene_fixture(&root, SceneCompilationLimits::default());
    let before = fixture.host.accepted_source_observation().unwrap();
    let revision = fixture
        .workspace
        .document_revision(fixture.current)
        .unwrap();
    let encoded = fixture
        .stage
        .encode_checkpoint(fixture.current, SceneFileEncoding::Json)
        .unwrap();
    let unrelated = b"unrelated actual extraction output";
    let FilePublicationOutcome::Published(publication) =
        fixture.host.file.publication.publish(unrelated)
    else {
        panic!("the mismatched filesystem control must publish successfully")
    };
    let evidence = editor_persistence_receipt(encoded.checkpoint(), publication.evidence());
    let observation = (
        publication.source().identity(),
        publication.source().parent_identity(),
        publication.source().digest(),
    );
    let mut acceptance = ExtractionCheckpointAcceptance::published(encoded, before.0, publication);
    let spent = fixture.stage.usage();
    for _ in 0..2 {
        assert!(matches!(
            acceptance.advance(&mut fixture.stage),
            Err(ExtractionPersistenceError::Rejected(
                EditorPersistenceRejection::SourceBindingMismatch
            ))
        ));
        assert!(!acceptance.accepted());
        let failure = acceptance
            .bind_scene_leaf(&mut fixture.host, before)
            .unwrap_err();
        let (returned, reason) = failure.into_parts();
        acceptance = returned;
        assert_eq!(reason, EditorPersistenceRejection::ResultMismatch);
        assert_eq!(acceptance.observation(), observation);
        assert_eq!(acceptance.evidence(), Some(evidence));
        assert_eq!(fixture.stage.usage(), spent);
        let current = fixture.host.read_source().open().unwrap();
        assert_eq!(current.identity(), observation.0);
        assert_eq!(current.parent_identity(), observation.1);
        assert_eq!(current.digest(65_536).unwrap(), observation.2);
        assert_eq!(fs::read(root.scene_path()).unwrap(), unrelated);
        assert_eq!(fixture.host.accepted_source_observation(), Some(before));
        assert_eq!(
            fixture.workspace.document_revision(fixture.current),
            Some(revision)
        );
        assert!(fixture.workspace.scene(fixture.current).unwrap().is_dirty());
    }
}

#[test]
fn extraction_acceptance_retries_the_original_stage_without_another_file_write() {
    for observed in [false, true] {
        let root = TestRoot::new();
        let mut fixture = saved_scene_fixture(&root, SceneCompilationLimits::default());
        let mut other_stage = persistence_stage(
            &fixture.workspace,
            &fixture.basis,
            &[fixture.current, fixture.selected],
            SceneCompilationLimits::default(),
        );
        let before = fixture.host.accepted_source_observation().unwrap();
        let revision = fixture
            .workspace
            .document_revision(fixture.current)
            .unwrap();
        let saved_revision = fixture
            .workspace
            .scene(fixture.current)
            .unwrap()
            .saved_revision();
        let encoded = fixture
            .stage
            .encode_checkpoint(fixture.current, SceneFileEncoding::Json)
            .unwrap();
        let other = other_stage
            .encode_checkpoint(fixture.current, SceneFileEncoding::Json)
            .unwrap();
        // Equal bytes and the same source allocation still belong to distinct nominal captures.
        assert_eq!(encoded.bytes(), other.bytes());
        assert!(std::ptr::eq(
            encoded.checkpoint().scene().unwrap(),
            other.checkpoint().scene().unwrap()
        ));
        let source = std::ptr::from_ref(encoded.checkpoint().scene().unwrap());
        let bytes = encoded.bytes().to_vec();
        let FilePublicationOutcome::Published(publication) =
            fixture.host.file.publication.publish(encoded.bytes())
        else {
            panic!("the exact captured output must publish successfully")
        };
        let observation = (
            publication.source().identity(),
            publication.source().parent_identity(),
            publication.source().digest(),
        );
        let evidence = (!observed)
            .then(|| editor_persistence_receipt(encoded.checkpoint(), publication.evidence()));
        let mut acceptance = if observed {
            let (_, published) = publication.into_parts();
            let (file, digest) = published.into_parts();
            ExtractionCheckpointAcceptance::observed(encoded, file, digest)
        } else {
            ExtractionCheckpointAcceptance::published(encoded, before.0, publication)
        };
        assert!(!acceptance.accepted());
        assert_eq!(acceptance.observation(), observation);
        assert_eq!(acceptance.evidence(), evidence);
        assert_eq!(acceptance.evidence().is_none(), observed);
        let failure = acceptance
            .bind_scene_leaf(&mut fixture.host, before)
            .unwrap_err();
        let (returned, reason) = failure.into_parts();
        acceptance = returned;
        assert_eq!(reason, EditorPersistenceRejection::ResultMismatch);
        let original_usage = fixture.stage.usage();
        let other_usage = other_stage.usage();
        let ExtractionPersistenceError::Admission(report) =
            acceptance.advance(&mut other_stage).unwrap_err()
        else {
            panic!("an equal output from another stage cannot replace the nominal checkpoint")
        };
        assert!(!report.applied);
        assert!(other_stage.usage().work_bytes > other_usage.work_bytes);
        assert_eq!(fixture.stage.usage(), original_usage);
        assert!(!acceptance.accepted());
        let failure = acceptance
            .bind_scene_leaf(&mut fixture.host, before)
            .unwrap_err();
        let (returned, reason) = failure.into_parts();
        acceptance = returned;
        assert_eq!(reason, EditorPersistenceRejection::ResultMismatch);
        assert_eq!(acceptance.observation(), observation);
        assert_eq!(acceptance.evidence(), evidence);

        acceptance.advance(&mut fixture.stage).unwrap();
        assert!(acceptance.accepted());
        assert!(fixture.stage.usage().work_bytes > original_usage.work_bytes);
        let accepted_usage = fixture.stage.usage();
        acceptance.advance(&mut fixture.stage).unwrap();
        assert_eq!(fixture.stage.usage(), accepted_usage);
        assert_eq!(acceptance.evidence(), evidence);
        assert_eq!(acceptance.observation(), observation);
        assert_eq!(fs::read(root.scene_path()).unwrap(), bytes);
        assert_eq!(
            fixture.host.read_source().open().unwrap().identity(),
            observation.0
        );
        assert_eq!(
            std::ptr::from_ref(fixture.stage.saved_scene(fixture.current).unwrap().as_ref()),
            source
        );
        other_stage
            .return_checkpoint(other.into_checkpoint())
            .unwrap();
        assert_eq!(fixture.host.accepted_source_observation(), Some(before));
        assert_eq!(
            fixture.workspace.document_revision(fixture.current),
            Some(revision)
        );
        let scene = fixture.workspace.scene(fixture.current).unwrap();
        assert_eq!(scene.saved_revision(), saved_revision);
        assert!(scene.is_dirty());

        admit_leaf(&mut fixture.stage, "startup.scene.json", observation.2);
        let admitted = fixture.stage.usage();
        let bound = acceptance
            .bind_scene_leaf(&mut fixture.host, before)
            .unwrap();
        let acceptance = bound.into_acceptance();
        assert!(acceptance.accepted());
        assert_eq!(acceptance.observation(), observation);
        assert_eq!(acceptance.evidence(), evidence);
        assert_eq!(acceptance.evidence().is_none(), observed);
        assert_eq!(fixture.stage.usage(), admitted);
        assert_eq!(fixture.host.accepted_source_observation(), Some(before));
        assert_eq!(fs::read(root.scene_path()).unwrap(), bytes);
        admit_leaf(&mut fixture.stage, "startup.scene.json", observation.2);
        let bound = acceptance
            .bind_scene_leaf(&mut fixture.host, before)
            .unwrap();
        assert_eq!(bound.commit(), evidence);
        assert_eq!(
            fixture.host.accepted_source_observation(),
            Some(observation)
        );
        let scene = fixture.workspace.scene(fixture.current).unwrap();
        assert_eq!(scene.saved_revision(), saved_revision);
        assert!(scene.is_dirty());
    }
}

#[test]
fn extraction_acceptance_uses_the_original_cumulative_stage_allowance_after_publication() {
    let probe_root = TestRoot::new();
    let mut probe = saved_scene_fixture(&probe_root, SceneCompilationLimits::default());
    let encoded = probe
        .stage
        .encode_checkpoint(probe.current, SceneFileEncoding::Json)
        .unwrap();
    let previous = probe.host.accepted_source_observation().unwrap();
    let FilePublicationOutcome::Published(publication) =
        probe.host.file.publication.publish(encoded.bytes())
    else {
        panic!("the allowance probe must publish the actual captured output")
    };
    let mut acceptance =
        ExtractionCheckpointAcceptance::published(encoded, previous.0, publication);
    let before = probe.stage.usage();
    acceptance.advance(&mut probe.stage).unwrap();
    let required = probe.stage.usage().work_bytes;
    assert!(required > before.work_bytes);
    for deficit in [0, 1] {
        let root = TestRoot::new();
        let limits = SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(required - deficit).unwrap());
        let mut fixture = saved_scene_fixture(&root, limits);
        let previous = fixture.host.accepted_source_observation().unwrap();
        let revision = fixture
            .workspace
            .document_revision(fixture.current)
            .unwrap();
        let encoded = fixture
            .stage
            .encode_checkpoint(fixture.current, SceneFileEncoding::Json)
            .unwrap();
        let bytes = encoded.bytes().to_vec();
        let FilePublicationOutcome::Published(publication) =
            fixture.host.file.publication.publish(encoded.bytes())
        else {
            panic!("acceptance allowance must be tested after actual publication")
        };
        let evidence = editor_persistence_receipt(encoded.checkpoint(), publication.evidence());
        let mut acceptance =
            ExtractionCheckpointAcceptance::published(encoded, previous.0, publication);
        let observation = acceptance.observation();
        let spent = fixture.stage.usage();
        let result = acceptance.advance(&mut fixture.stage);
        if deficit == 0 {
            result.unwrap();
            assert!(acceptance.accepted());
            assert_eq!(fixture.stage.usage().work_bytes, required);
            assert_eq!(acceptance.observation(), observation);
            assert_eq!(acceptance.evidence(), Some(evidence));
        } else {
            let ExtractionPersistenceError::Admission(report) = result.unwrap_err() else {
                panic!("insufficient original allowance must refuse checkpoint acceptance")
            };
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code().as_str() == "scene.prefab-extraction-capacity")
            );
            assert!(!acceptance.accepted());
            let failure = acceptance
                .bind_scene_leaf(&mut fixture.host, previous)
                .unwrap_err();
            let (returned, reason) = failure.into_parts();
            acceptance = returned;
            assert_eq!(reason, EditorPersistenceRejection::ResultMismatch);
            assert_eq!(acceptance.observation(), observation);
            assert_eq!(acceptance.evidence(), Some(evidence));
            assert!(fixture.stage.usage().work_bytes >= spent.work_bytes);
            let failed_usage = fixture.stage.usage();
            assert!(matches!(
                acceptance.advance(&mut fixture.stage),
                Err(ExtractionPersistenceError::Admission(_))
            ));
            assert!(!acceptance.accepted());
            assert_eq!(acceptance.observation(), observation);
            assert_eq!(acceptance.evidence(), Some(evidence));
            assert!(fixture.stage.usage().work_bytes >= failed_usage.work_bytes);
        }
        assert_eq!(fixture.host.accepted_source_observation(), Some(previous));
        assert_eq!(
            fixture.workspace.document_revision(fixture.current),
            Some(revision)
        );
        assert!(fixture.workspace.scene(fixture.current).unwrap().is_dirty());
        let current = fixture.host.read_source().open().unwrap();
        assert_eq!(current.identity(), observation.0);
        assert_eq!(current.digest(65_536).unwrap(), observation.2);
        assert_eq!(fs::read(root.scene_path()).unwrap(), bytes);
    }
}

fn key(path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::project_location(
        "9c69fc1f-0cdd-4fb5-9fc7-a64671a39a04".parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    )
}

fn metadata(path: &str, stable: &str) -> AssetRecord {
    AssetRecord::new(
        stable.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn add(workspace: &mut EditorWorkspace, document: EditorDocumentId, entity: &str) {
    let expected = workspace
        .scene(document)
        .unwrap()
        .session()
        .document()
        .placements
        .clone();
    let mut value = expected.clone();
    value.push(SceneEntityPlacement::root(id(entity)));
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id(entity)),
        },
        ScenePatchOperation::SetPlacements { expected, value },
    ]);
    let query = workspace.document_query_basis(document).unwrap();
    let prepared = workspace.prepare_authoring_edit(&query, &patch).unwrap();
    assert!(workspace.commit_authoring_edit(prepared).applied);
}

fn history(workspace: &mut EditorWorkspace, document: EditorDocumentId, undo: bool) {
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

fn admit_leaf(
    stage: &mut PreparedEditorWorkspaceExtractionPersistence,
    path: &str,
    digest: ContentDigest,
) {
    let cost = PersistenceLeafValidationCost::checked(digest, path.len()).unwrap();
    stage
        .admit_persistence_storage(
            cost.work_bytes
                .checked_add(cost.encoded_bytes)
                .and_then(|bytes| bytes.checked_add(cost.scratch_bytes))
                .and_then(|bytes| {
                    bytes
                        .checked_add(cost.transient_handles * std::mem::size_of::<FileCapability>())
                }),
        )
        .unwrap();
}

fn persistence_stage(
    workspace: &EditorWorkspace,
    basis: &AuthoringSchemaBasis,
    documents: &[EditorDocumentId],
    limits: SceneCompilationLimits,
) -> PreparedEditorWorkspaceExtractionPersistence {
    let selected = *documents.last().unwrap();
    let mut work = PrefabExtractionWork::new(limits).unwrap();
    let input = workspace
        .capture_prefab_extraction_input(selected, &id("branch"), basis, &mut work)
        .unwrap();
    // Admit the actual environment's membership and before/after bindings before allocation,
    // alongside the existing fixed request/key/destination storage.
    let environment_bytes = std::mem::size_of::<PrefabExtractionEnvironment>()
        + "default".len()
        + input.documents().len() * std::mem::size_of::<usize>()
        + 3 * std::mem::size_of::<PrefabExtractionBinding>()
        + 2 * (P_PATH.len() + 2 * std::mem::size_of::<usize>())
        + Q_PATH.len()
        + 2 * std::mem::size_of::<usize>()
        + 2 * P_PATH.len()
        + Q_PATH.len();
    work.admit_capture_storage(Some(8192 + environment_bytes))
        .unwrap();
    let sources = input
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
                    .with_source_asset(&metadata(P_PATH, "0604c637-c183-4a36-83e5-66c647d7c515"))
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
                reference: AssetRef::path(Q_PATH).unwrap(),
                source: key(Q_PATH, SourceDocumentKind::Prefab),
            },
        ],
        assets_before: None,
        assets_after: None,
    };
    let request = PrefabExtractionRequest::new(
        sources,
        vec![environment],
        0,
        basis.clone(),
        key(P_PATH, SourceDocumentKind::Prefab),
        id("branch"),
        PrefabExtractionDestination {
            key: key(Q_PATH, SourceDocumentKind::Prefab),
            reference: AssetRef::path(Q_PATH).unwrap(),
            metadata: metadata(Q_PATH, "945bac7b-ff3a-4648-8ec3-612d0bff11a4"),
        },
    );
    let mut corpus = request.prepare(work).unwrap();
    corpus.admit_binding_storage(Some(4096)).unwrap();
    let bindings = documents
        .iter()
        .enumerate()
        .map(|(source, &document)| EditorPrefabExtractionCorpusBinding { document, source })
        .collect();
    input
        .bind_corpus(corpus, bindings)
        .unwrap()
        .prepare_sources()
        .unwrap()
        .prepare_persistence(vec![documents[0]])
        .unwrap()
}

#[test]
fn real_extraction_publications_bind_all_source_saved_and_host_destinations_before_installing() {
    let root = TestRoot::new();
    let basis = AuthoringSchemaBasis::admit(&AuthoringSchemaBundle::new([]).unwrap(), &[]).unwrap();
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("original"))]);
    let mut prefab = PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root")),
        SceneEntityRecord::new(id("branch")),
        SceneEntityRecord::new(id("child")),
    ]);
    prefab.placements[1].placement = ScenePlacement::last(ScenePlacementParent::Entity(
        PrefabEntityTarget::source(id("root")),
    ));
    prefab.placements[2].placement = ScenePlacement::last(ScenePlacementParent::Entity(
        PrefabEntityTarget::source(id("branch")),
    ));
    for path in ["startup.scene.json", "guard.scene.ron"] {
        let text = if path.ends_with("ron") {
            scene.to_ron_string().unwrap()
        } else {
            scene.to_json_string().unwrap()
        };
        fs::write(root.path.join(path), text).unwrap();
    }
    fs::write(root.path.join(P_PATH), prefab.to_json_string().unwrap()).unwrap();
    let guard_disk = fs::read(root.path.join("guard.scene.ron")).unwrap();
    let directory = root.capability();
    let (mut workspace, authority) = EditorWorkspace::new_hosted();
    let mut scene_hosts = Vec::new();
    let mut documents = Vec::new();
    for path in ["startup.scene.json", "guard.scene.ron"] {
        let file = directory
            .open_file(&RelativePath::new(path).unwrap())
            .unwrap();
        let bytes = file.read_to_end_bounded(65_536).unwrap();
        let candidate = decode_scene_candidate(
            document_encoding(path).unwrap(),
            &bytes,
            SceneFileLimits::default(),
        )
        .unwrap();
        let published = candidate
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(path).unwrap()),
                &basis,
            )
            .unwrap()
            .publish();
        let (host, digest) = ScenePersistenceHost::bind_authoring_source(
            &directory,
            path,
            file,
            ContentDigest::of_bytes(&bytes),
        )
        .unwrap();
        let document = workspace
            .open_authoring_scene_document(path, EditorDocumentSourceBinding::new(path), published)
            .unwrap()
            .opened_document
            .unwrap();
        assert_eq!(digest, editor_digest(ContentDigest::of_bytes(&bytes)));
        scene_hosts.push(host);
        documents.push(document);
        add(&mut workspace, document, "first-edit");
        add(&mut workspace, document, "second-edit");
        history(&mut workspace, document, true);
    }
    let file = directory
        .open_file(&RelativePath::new(P_PATH).unwrap())
        .unwrap();
    let bytes = file.read_to_end_bounded(65_536).unwrap();
    let published = PrefabDocumentCandidate::decode_json_bytes(&bytes)
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path(P_PATH).unwrap()),
            &basis,
        )
        .unwrap()
        .publish();
    let (mut prefab_host, _) = PrefabPersistenceHost::bind_authoring_source(
        &directory,
        EditorDocumentSourceBinding::new(P_PATH),
        file,
        ContentDigest::of_bytes(&bytes),
    )
    .unwrap();
    let selected = workspace
        .open_authoring_prefab_document(P_PATH, EditorDocumentSourceBinding::new(P_PATH), published)
        .unwrap()
        .opened_document
        .unwrap();
    documents.push(selected);
    let selected_revision = workspace.document_revision(selected).unwrap();
    let current = documents[0];
    let guard_only = documents[1];
    let current_revision = workspace.document_revision(current).unwrap();
    let current_content = workspace.scene(current).unwrap().session().content_state();
    let current_history = workspace.scene(current).unwrap().session().history_status();
    let guard_source = workspace
        .scene(guard_only)
        .unwrap()
        .session()
        .shared_document();
    let guard_revision = workspace.document_revision(guard_only).unwrap();
    let guard_saved = workspace.scene(guard_only).unwrap().saved_revision();
    let guard_history = workspace
        .scene(guard_only)
        .unwrap()
        .session()
        .history_status();
    let previous_p = prefab_host.accepted_source_observation().unwrap();
    let previous_s = scene_hosts[0].accepted_source_observation().unwrap();

    let mut stage = persistence_stage(
        &workspace,
        &basis,
        &documents,
        SceneCompilationLimits::default(),
    );
    assert_eq!(
        stage.disposition(selected),
        Some(EditorPrefabExtractionDisposition::ReplaceSource)
    );
    assert_eq!(
        stage.disposition(current),
        Some(EditorPrefabExtractionDisposition::SaveCurrent)
    );
    assert_eq!(
        stage.disposition(guard_only),
        Some(EditorPrefabExtractionDisposition::Guard)
    );
    let p_source = std::sync::Arc::clone(stage.saved_prefab(selected).unwrap());
    let s_source = std::sync::Arc::clone(stage.saved_scene(current).unwrap());
    assert!(std::sync::Arc::ptr_eq(
        &s_source,
        &workspace
            .scene(current)
            .unwrap()
            .session()
            .shared_document()
    ));
    let encoded = stage
        .encode_checkpoint(selected, SceneFileEncoding::Json)
        .unwrap();
    assert!(std::ptr::eq(
        encoded.checkpoint().prefab().unwrap(),
        p_source.as_ref()
    ));
    let future_revision = encoded.checkpoint().revision();
    let p_bytes = encoded.bytes().to_vec();
    let FilePublicationOutcome::Published(publication) =
        prefab_host.file.publication.publish(encoded.bytes())
    else {
        panic!("the actual fixed Prefab successor must publish successfully")
    };
    let published_p = ExtractionCheckpointAcceptance::published(encoded, previous_p.0, publication);
    assert!(!published_p.accepted());
    assert_eq!(published_p.evidence().unwrap().document(), selected);
    assert_eq!(prefab_host.accepted_source_observation(), Some(previous_p));
    let p_result = published_p.observation();
    let p_evidence = published_p.evidence();
    let failure = published_p
        .bind_prefab_leaf(
            &mut prefab_host,
            &EditorDocumentSourceBinding::new(P_PATH),
            previous_p,
        )
        .unwrap_err();
    let (mut published_p, reason) = failure.into_parts();
    assert_eq!(reason, EditorPersistenceRejection::ResultMismatch);
    assert_eq!(published_p.observation(), p_result);
    assert_eq!(published_p.evidence(), p_evidence);
    assert!(!published_p.accepted());
    published_p.advance(&mut stage).unwrap();
    assert!(published_p.accepted());
    let p_observation = published_p.observation();
    let p_digest = p_observation.2;
    let p_evidence = published_p.evidence();
    assert_eq!(p_evidence.unwrap().revision(), future_revision);
    let encoded = stage
        .encode_checkpoint(current, SceneFileEncoding::Json)
        .unwrap();
    assert!(std::ptr::eq(
        encoded.checkpoint().scene().unwrap(),
        s_source.as_ref()
    ));
    let s_bytes = encoded.bytes().to_vec();
    let FilePublicationOutcome::Published(publication) =
        scene_hosts[0].file.publication.publish(encoded.bytes())
    else {
        panic!("the actual captured SaveCurrent source must publish successfully")
    };
    let mut published_s =
        ExtractionCheckpointAcceptance::published(encoded, previous_s.0, publication);
    assert!(!published_s.accepted());
    published_s.advance(&mut stage).unwrap();
    assert!(published_s.accepted());
    let s_observation = published_s.observation();
    let s_digest = s_observation.2;
    let s_evidence = published_s.evidence();
    assert_eq!(s_evidence.unwrap().document(), current);
    assert!(std::sync::Arc::ptr_eq(
        stage.saved_prefab(selected).unwrap(),
        &p_source
    ));
    assert!(std::sync::Arc::ptr_eq(
        stage.saved_scene(current).unwrap(),
        &s_source
    ));
    assert_eq!(
        workspace.document_revision(selected),
        Some(selected_revision)
    );
    assert_eq!(prefab_host.accepted_source_observation(), Some(previous_p));
    assert_eq!(
        scene_hosts[0].accepted_source_observation(),
        Some(previous_s)
    );

    admit_leaf(&mut stage, P_PATH, p_digest);
    admit_leaf(&mut stage, "startup.scene.json", s_digest);
    let bound = authority
        .bind_prefab_extraction_persistence(&mut workspace, stage)
        .unwrap();
    let failure = published_p
        .bind_prefab_leaf(
            &mut prefab_host,
            &EditorDocumentSourceBinding::new("unrelated.prefab.json"),
            previous_p,
        )
        .unwrap_err();
    let (published_p, reason) = failure.into_parts();
    assert_eq!(reason, EditorPersistenceRejection::SourceBindingMismatch);
    assert!(published_p.accepted());
    assert_eq!(published_p.observation(), p_observation);
    assert_eq!(published_p.evidence(), p_evidence);
    let p_bound = published_p
        .bind_prefab_leaf(
            &mut prefab_host,
            &EditorDocumentSourceBinding::new(P_PATH),
            previous_p,
        )
        .unwrap();
    let failure = published_s
        .bind_scene_leaf(
            &mut scene_hosts[0],
            (
                previous_s.0,
                previous_s.1,
                ContentDigest::of_bytes(b"wrong predecessor"),
            ),
        )
        .unwrap_err();
    let (published_s, reason) = failure.into_parts();
    assert_eq!(reason, EditorPersistenceRejection::TargetChanged);
    assert!(published_s.accepted());
    assert_eq!(published_s.observation(), s_observation);
    assert_eq!(published_s.evidence(), s_evidence);
    let s_bound = published_s
        .bind_scene_leaf(&mut scene_hosts[0], previous_s)
        .unwrap();
    // Releasing all guards installs nothing and returns the exact fixed owners for rebinding.
    let mut stage = bound.into_prepared();
    let published_p = p_bound.into_acceptance();
    let published_s = s_bound.into_acceptance();
    assert!(published_p.accepted());
    assert_eq!(published_p.observation(), p_observation);
    assert_eq!(published_p.evidence(), p_evidence);
    assert!(published_s.accepted());
    assert_eq!(published_s.observation(), s_observation);
    assert_eq!(published_s.evidence(), s_evidence);
    assert_eq!(workspace.document_revision(current), Some(current_revision));
    assert!(workspace.scene(current).unwrap().is_dirty());
    assert_eq!(prefab_host.accepted_source_observation(), Some(previous_p));
    assert_eq!(
        scene_hosts[0].accepted_source_observation(),
        Some(previous_s)
    );
    admit_leaf(&mut stage, P_PATH, p_digest);
    admit_leaf(&mut stage, "startup.scene.json", s_digest);
    let bound = authority
        .bind_prefab_extraction_persistence(&mut workspace, stage)
        .unwrap();
    let p_bound = published_p
        .bind_prefab_leaf(
            &mut prefab_host,
            &EditorDocumentSourceBinding::new(P_PATH),
            previous_p,
        )
        .unwrap();
    let s_bound = published_s
        .bind_scene_leaf(&mut scene_hosts[0], previous_s)
        .unwrap();
    bound.commit();
    assert_eq!(p_bound.commit(), p_evidence);
    assert_eq!(s_bound.commit(), s_evidence);

    let saved = workspace.scene(current).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &s_source,
        &saved.session().shared_document()
    ));
    assert_eq!(saved.revision(), current_revision);
    assert_eq!(saved.session().content_state(), current_content);
    assert_eq!(saved.session().history_status(), current_history);
    assert!(!saved.is_dirty());
    let guarded = workspace.scene(guard_only).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &guard_source,
        &guarded.session().shared_document()
    ));
    assert_eq!(guarded.revision(), guard_revision);
    assert_eq!(guarded.saved_revision(), guard_saved);
    assert_eq!(guarded.session().history_status(), guard_history);
    assert!(guarded.is_dirty());
    assert!(!workspace.prefab(selected).unwrap().is_dirty());
    assert!(std::sync::Arc::ptr_eq(
        &p_source,
        &workspace
            .prefab(selected)
            .unwrap()
            .session()
            .shared_document()
    ));
    assert_eq!(workspace.document_revision(selected), Some(future_revision));
    assert_ne!(future_revision.source_id(), selected_revision.source_id());
    assert_eq!(
        workspace
            .prefab(selected)
            .unwrap()
            .session()
            .history_status()
            .undo_depth,
        0
    );
    assert_eq!(fs::read(root.path.join(P_PATH)).unwrap(), p_bytes);
    assert_eq!(fs::read(root.scene_path()).unwrap(), s_bytes);
    assert_eq!(
        prefab_host.accepted_source_observation(),
        Some(p_observation)
    );
    assert_eq!(
        scene_hosts[0].accepted_source_observation(),
        Some(s_observation)
    );
    assert_eq!(
        fs::read(root.path.join("guard.scene.ron")).unwrap(),
        guard_disk
    );
    assert_eq!(workspace.len(), 3);
    history(&mut workspace, current, true);
    assert!(workspace.scene(current).unwrap().is_dirty());
    history(&mut workspace, current, false);
    assert!(!workspace.scene(current).unwrap().is_dirty());
    history(&mut workspace, current, false);
    assert!(workspace.scene(current).unwrap().is_dirty());
}
