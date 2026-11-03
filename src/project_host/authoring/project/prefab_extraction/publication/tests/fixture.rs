//! Real producer, fresh Pending admission and narrow native-file controls.

use super::*;

pub(super) const RECOVERY: &str = "recovery/publication.json";
pub(super) const CLOSED_REWRITE: &str = "desktop/prefabs/closed.prefab.ron";

pub(super) fn add_closed_rewrite(fixture: &Fixture) {
    let key = SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(P).unwrap(),
        SourceDocumentKind::Prefab,
    );
    let mut overlay = nara_scene::PrefabOverlay::new(key);
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(id("desktop-leaf")));
    let mut instance = PrefabInstance::forest(AssetRef::path(ALIAS).unwrap());
    instance.overrides = Some(overlay);
    let mut actor = SceneEntityRecord::new(id("actor"));
    actor.prefab = Some(instance);
    fs::write(
        fixture.root.join(CLOSED_REWRITE),
        PrefabDocument::from_ordered_roots([actor])
            .to_ron_string()
            .unwrap(),
    )
    .unwrap();
}

pub(super) fn confirmed(fixture: &Fixture) -> ProjectPrefabExtractionStorage {
    confirmed_request(fixture, fixture.request())
}

pub(super) fn confirmed_request(
    fixture: &Fixture,
    request: PrefabExtractionCaptureRequest,
) -> ProjectPrefabExtractionStorage {
    fs::create_dir_all(fixture.root.join("recovery")).unwrap();
    let prepared = request
        .capture()
        .unwrap()
        .prepare()
        .unwrap()
        .prepare_review(vec![fixture.document(INACTIVE)])
        .unwrap()
        .prepare_outputs()
        .unwrap()
        .prepare_recovery(Default::default())
        .unwrap();
    let mut storage = prepared
        .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
        .unwrap();
    for _ in 0..8 {
        if storage.phase() == ProjectPrefabExtractionStoragePhase::Confirmed {
            assert!(storage.pending().is_some());
            return storage;
        }
        storage = storage.advance().unwrap();
    }
    panic!("the actual producer did not confirm Pending");
}

pub(super) fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(Default::default()).for_source_operation(
        ItemLimit::new(200_000_000).unwrap(),
        ItemLimit::new(128).unwrap(),
    )
}

pub(super) fn load(
    fixture: &Fixture,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> ObservedPendingRecoveryStorage<PrefabExtractionRecovery> {
    load_pending(
        crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
        Default::default(),
        budget,
    )
    .unwrap()
}

pub(super) fn generation(
    pending: ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    mut budget: AuthoringProjectAdmissionBudget,
) -> RecoveredPrefabExtractionGeneration {
    let observation = RecoveredPrefabExtractionInventory::capture(
        &pending,
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let mut owner = RecoveredPrefabExtractionGeneration::new(
        pending,
        observation,
        budget,
        PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap(),
    );
    for phase in [
        RecoveredPrefabExtractionGenerationPhase::Decode,
        RecoveredPrefabExtractionGenerationPhase::Verify,
        RecoveredPrefabExtractionGenerationPhase::Saved,
    ] {
        assert_eq!(owner.phase(), phase);
        owner = owner.advance().unwrap();
    }
    assert_eq!(
        owner.phase(),
        RecoveredPrefabExtractionGenerationPhase::Complete
    );
    assert!(owner.verified().is_some());
    owner
}

pub(super) fn fresh_publication(fixture: &mut Fixture) -> ProjectPrefabExtractionPublication {
    drop(confirmed(fixture));
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let mut host = budget();
    let pending = load(fixture, &mut host);
    generation(pending, host).into_publication().unwrap()
}

pub(super) fn replace(root: &DirectoryCapability, path: &str, bytes: &[u8]) {
    let file = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let maximum = 64 * 1024 * 1024;
    let digest = file.digest(maximum).unwrap();
    let mut publisher =
        FilePublicationHost::bind_existing(root, path, &file, digest, maximum).unwrap();
    assert!(matches!(
        publisher.publish(bytes),
        FilePublicationOutcome::Published(_)
    ));
}

pub(super) fn fixed_files(
    recovery: &PrefabExtractionRecovery,
) -> Vec<(ExtractionFileMember, String, Vec<u8>)> {
    let mut files = vec![
        (
            ExtractionFileMember::Prefab,
            recovery.q().path().to_owned(),
            recovery.q().output().bytes().to_vec(),
        ),
        (
            ExtractionFileMember::Metadata,
            recovery.q_sidecar().path().to_owned(),
            recovery.q_sidecar().output().bytes().to_vec(),
        ),
    ];
    files.extend(
        recovery
            .complete_original_sources()
            .iter()
            .enumerate()
            .filter_map(|(index, source)| {
                Some((
                    ExtractionFileMember::Original(index),
                    source.path().to_owned(),
                    source.output()?.bytes().to_vec(),
                ))
            }),
    );
    files
}

pub(super) fn assert_published(
    root: &DirectoryCapability,
    path: &str,
    bytes: &[u8],
    accepted: &AcceptedFilePublication,
    previous: Option<FileIdentity>,
) {
    let file = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let digest = ContentDigest::of_bytes(bytes);
    let receipt = accepted.evidence().receipt();
    assert_eq!(accepted.target().to_str(), path.rsplit('/').next());
    assert_eq!(accepted.source().identity(), file.identity());
    assert_eq!(accepted.source().parent_identity(), file.parent_identity());
    assert_eq!(accepted.source().digest(), digest);
    assert_eq!(receipt.previous_identity(), previous);
    assert_eq!(receipt.candidate_identity(), file.identity());
    assert_eq!(receipt.published_identity(), Some(file.identity()));
    assert_eq!(receipt.written_content_digest(), digest);
    assert_eq!(receipt.verified_content_digest(), Some(digest));
    assert_eq!(file.read_to_end_bounded(bytes.len() as u64).unwrap(), bytes);
}

pub(super) fn allocations(outputs: &PreparedProjectPrefabExtractionOutputs) -> Vec<usize> {
    let review = outputs.review();
    let corpus = review.persistence.sources().prepared().corpus();
    let mut pointers = vec![
        corpus.original_sources().as_ptr() as usize,
        corpus.sources().as_ptr() as usize,
        review.context.disk.as_ptr() as usize,
        review.saved.sources().as_ptr() as usize,
        review.saved.published().as_ptr() as usize,
        review.saved.extracted().document() as *const PrefabDocument as usize,
    ];
    pointers.extend(outputs.outputs().flat_map(|output| {
        [
            output.bytes().unwrap().as_ptr() as usize,
            output
                .encoded_checkpoint()
                .map_or(0, |encoded| encoded as *const _ as usize),
            output
                .checkpoint()
                .map_or(0, |checkpoint| checkpoint as *const _ as usize),
        ]
    }));
    pointers
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct LiveState {
    document: EditorDocumentId,
    source: usize,
    revision: SceneAuthoringRevision,
    content: SceneAuthoringContentState,
    saved_revision: SceneAuthoringRevision,
    saved_content: SceneAuthoringContentState,
    saved_digest: Option<EditorDocumentDigest>,
    history: SceneAuthoringHistoryStatus,
    usage: SceneAuthoringHistoryUsage,
    dirty: bool,
}

pub(super) fn live_state(fixture: &Fixture) -> Vec<LiveState> {
    fixture
        .open
        .iter()
        .map(|(document, _, _)| {
            let (
                source,
                revision,
                content,
                saved_revision,
                saved_content,
                saved_digest,
                history,
                usage,
                dirty,
            ) = if let Some(slot) = fixture.workspace.scene(*document) {
                let session = slot.session();
                (
                    session.document() as *const SceneDocument as usize,
                    session.revision(),
                    session.content_state(),
                    slot.saved_revision(),
                    slot.saved_content_state(),
                    slot.saved_digest(),
                    session.history_status(),
                    session.history_usage(),
                    slot.is_dirty(),
                )
            } else {
                let slot = fixture.workspace.prefab(*document).unwrap();
                let session = slot.session();
                (
                    session.document() as *const PrefabDocument as usize,
                    session.revision(),
                    session.content_state(),
                    slot.saved_revision(),
                    slot.saved_content_state(),
                    slot.saved_digest(),
                    session.history_status(),
                    session.history_usage(),
                    slot.is_dirty(),
                )
            };
            LiveState {
                document: *document,
                source,
                revision,
                content,
                saved_revision,
                saved_content,
                saved_digest,
                history,
                usage,
                dirty,
            }
        })
        .collect()
}

pub(super) fn assert_pending(fixture: &Fixture) {
    let parsed =
        ProjectManifest::parse_toml_bytes(&fs::read(fixture.root.join("nara.toml")).unwrap());
    assert!(matches!(
        parsed.manifest.unwrap().authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(
            crate::test_host_fs::writable_directory(&fixture.root).unwrap(),
            None,
        ),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}
