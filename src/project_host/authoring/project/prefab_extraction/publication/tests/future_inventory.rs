//! Ordinary future admission driven by the actual live and fresh recovered producers.

use nara_scene::{__private::PrefabExtractionUsage, AuthoringSchemaBasis};

use super::super::super::publication::{
    ProjectPrefabExtractionFutureInventory, ProjectPrefabExtractionFutureInventoryError as Error,
};
use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionLimits, AuthoringProjectAdmissionUsage,
    recovery_storage::RecoveryStorageError,
    source_inventory::{SourceInventory, SourceInventoryError, SourceRootObservation},
};

const LATE_SOURCE: &str = "scenes/late.scene.json";
const LATE_METADATA: &str = "mobile/assets/late.bin.meta";
const LATE_ROOT: &str = "dormant/scenes";

fn publication(
    fixture: &mut Fixture,
    recovered: bool,
) -> (
    ProjectPrefabExtractionPublication,
    Vec<(ExtractionFileMember, String, Vec<u8>)>,
) {
    if recovered {
        let owner = fresh_publication(fixture);
        let files = fixed_files(owner.recovered_generation().unwrap().pending().recovery());
        (owner, files)
    } else {
        let storage = confirmed(fixture);
        let files = fixed_files(storage.recovery().unwrap());
        (storage.into_publication().unwrap(), files)
    }
}

pub(super) fn checkpoints_complete(
    mut owner: ProjectPrefabExtractionPublication,
) -> ProjectPrefabExtractionPublication {
    while owner.phase() != Phase::CheckpointsComplete {
        owner = owner.advance().unwrap();
    }
    assert_eq!(owner.position(), owner.file_count());
    assert_eq!(owner.checkpoint_position(), Some(owner.file_count()));
    owner
}

fn original_inventory(owner: &ProjectPrefabExtractionPublication) -> &SourceInventory {
    if let Some(outputs) = owner.live_outputs() {
        &outputs.review().context.inventory
    } else {
        owner
            .recovered_generation()
            .unwrap()
            .observation()
            .inventory()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Semantics {
    basis: AuthoringSchemaBasis,
    allocations: Vec<usize>,
    work: PrefabExtractionUsage,
}

fn semantics(owner: &ProjectPrefabExtractionPublication) -> Semantics {
    if let Some(outputs) = owner.live_outputs() {
        let review = outputs.review();
        let corpus = review.persistence.sources().prepared().corpus();
        Semantics {
            basis: corpus.schema_basis().clone(),
            allocations: vec![
                corpus.original_sources().as_ptr() as usize,
                corpus.sources().as_ptr() as usize,
                review.context.disk.as_ptr() as usize,
                review.saved.sources().as_ptr() as usize,
                review.saved.published().as_ptr() as usize,
                review.saved.extracted().document() as *const _ as usize,
                std::sync::Arc::as_ptr(&review.context.schema) as usize,
            ],
            work: review.persistence.usage(),
        }
    } else {
        let generation = owner.recovered_generation().unwrap();
        let (corpus, saved) = generation.verified().unwrap();
        Semantics {
            basis: generation.observation().basis().clone(),
            allocations: vec![
                corpus.original_sources().as_ptr() as usize,
                corpus.sources().as_ptr() as usize,
                saved.sources().as_ptr() as usize,
                saved.published().as_ptr() as usize,
                saved.extracted().document() as *const _ as usize,
                generation.observation().models().adapters.as_ptr() as usize,
                generation.pending().recovery().encoded_bytes().as_ptr() as usize,
            ],
            work: generation.source_usage(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FileResultState {
    member: ExtractionFileMember,
    observation: (FileIdentity, Option<FileIdentity>, ContentDigest),
    checkpoint: Option<bool>,
    allocation: usize,
}

fn file_results(owner: &ProjectPrefabExtractionPublication) -> Vec<FileResultState> {
    (0..owner.file_count())
        .map(|index| {
            let (member, result) = owner.result(index).unwrap();
            FileResultState {
                member,
                observation: result.observation(),
                checkpoint: match result {
                    ExtractionFileResult::Checkpoint(acceptance) => Some(acceptance.accepted()),
                    _ => None,
                },
                allocation: result as *const _ as usize,
            }
        })
        .collect()
}

fn disk_state(fixture: &Fixture) -> Vec<(PathBuf, FileIdentity, Vec<u8>)> {
    let root = fixture.project.project_root();
    fixture
        .disk_state()
        .into_iter()
        .map(|(path, bytes)| {
            let canonical = path
                .iter()
                .map(|component| component.to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/");
            let file = root
                .open_file_exact(&RelativePath::new(canonical).unwrap())
                .unwrap();
            (path, file.identity(), bytes)
        })
        .collect()
}

fn assert_released_temporary_storage(
    before: AuthoringProjectAdmissionUsage,
    after: AuthoringProjectAdmissionUsage,
) {
    assert_eq!(
        after.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        after.retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(after.decode_work() > before.decode_work());
    assert!(after.aggregate_encoded_bytes() >= before.aggregate_encoded_bytes());
}

fn assert_native_generation(
    fixture: &Fixture,
    future: &ProjectPrefabExtractionFutureInventory,
    files: &[(ExtractionFileMember, String, Vec<u8>)],
    stable: bool,
    recovered: bool,
) {
    let owner = future.publication();
    let original = original_inventory(owner);
    let inventory = future.inventory();
    let mut paths: Vec<_> = original
        .participants()
        .map(|source| source.file().path())
        .collect();
    paths.push(Q);
    paths.sort_unstable();
    assert_eq!(
        inventory
            .participants()
            .map(|source| source.file().path())
            .collect::<Vec<_>>(),
        paths,
    );
    assert_eq!(inventory.metadata().len(), original.metadata().len() + 1);
    assert!(inventory.file_observation(Q).is_none());
    assert!(inventory.file_observation(Q_SIDECAR).is_none());
    assert!(inventory.roots().eq(original.roots()));
    assert!(inventory.missing_sidecars().eq(original.missing_sidecars()));
    assert_eq!(inventory.scene_limits(), original.scene_limits());
    assert_eq!(inventory.bundle_digest(), original.bundle_digest());
    for before in original.metadata() {
        assert_eq!(
            inventory.metadata_for_source(before.record().path().as_str()),
            Some(before)
        );
    }
    for before in original.participants() {
        let after = inventory.participant(before.file().path()).unwrap();
        assert_eq!(after.source_key(), before.source_key());
        assert_eq!(after.encoding(), before.encoding());
        if let Some((position, (_, _, bytes))) = files
            .iter()
            .enumerate()
            .find(|(_, (_, path, _))| path == before.file().path())
        {
            assert_eq!(after.bytes(), bytes);
            assert_eq!(
                (
                    after.file().identity(),
                    after.file().parent_identity(),
                    after.file().digest()
                ),
                owner.result(position).unwrap().1.observation(),
            );
            assert_eq!(
                after.native_source(),
                &before
                    .native_source()
                    .with_source_identity(after.file().identity()),
            );
        } else {
            assert_eq!(after, before);
        }
    }
    let root = if let Some(generation) = owner.recovered_generation() {
        generation.pending().root().share()
    } else {
        fixture.project.project_root()
    };
    for (position, (_, path, bytes)) in files.iter().enumerate() {
        let (fact, captured_bytes) = if path == Q_SIDECAR {
            let metadata = inventory.metadata_for_source(Q).unwrap();
            (metadata.file(), metadata.bytes())
        } else {
            let participant = inventory.participant(path).unwrap();
            (participant.file(), participant.bytes())
        };
        let current = root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        assert_eq!(fact.path(), path);
        assert_eq!(fact.identity(), current.identity());
        assert_eq!(fact.digest(), ContentDigest::of_bytes(bytes));
        assert_eq!(captured_bytes, bytes);
        assert_eq!(
            (fact.identity(), fact.parent_identity(), fact.digest()),
            owner.result(position).unwrap().1.observation(),
        );
        assert!(
            inventory
                .shared_file(path)
                .unwrap()
                .same_object(&current)
                .unwrap()
        );
    }
    let q = inventory.participant(Q).unwrap();
    let metadata = inventory.metadata_for_source(Q).unwrap();
    let expected_key = if stable {
        SourceDocumentKey::stable_asset(fixture.target.stable_id(), SourceDocumentKind::Prefab)
    } else {
        SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(Q).unwrap(),
            SourceDocumentKind::Prefab,
        )
    };
    assert_eq!(q.source_key(), &expected_key);
    assert_eq!(q.native_source().source_identity(), q.file().identity());
    assert_eq!(metadata.file().path(), Q_SIDECAR);
    assert_eq!(metadata.record().path().as_str(), Q);
    assert_eq!(metadata.record().stable_id(), fixture.target.stable_id());
    assert_eq!(metadata.record().source_kind(), &AssetSourceKind::Prefab);
    assert_eq!(metadata.authored_path(), fixture.target.path());
    let ordinary = NativeDocumentSourcePolicy::new(fixture.project.settings())
        .admit(&root, Q, SourceDocumentKind::Prefab, q.file().identity())
        .unwrap();
    assert_eq!(q.source_key(), ordinary.key());
    if stable {
        let native_metadata = q.native_source().metadata_source().unwrap();
        assert_eq!(native_metadata.project_path().as_str(), Q_SIDECAR);
        assert_eq!(native_metadata.identity(), metadata.file().identity());
        assert_eq!(
            native_metadata.parent_identity(),
            metadata.file().parent_identity()
        );
        assert_eq!(native_metadata.digest(), metadata.file().digest());
        assert_eq!(native_metadata.encoded_bytes(), metadata.bytes().len());
        assert_eq!(
            q.native_source().metadata_source(),
            ordinary.metadata_source()
        );
    } else {
        assert!(q.native_source().metadata_source().is_none());
    }
    assert_eq!(
        inventory.configurations().len(),
        original.configurations().len()
    );
    for (index, (after, before)) in inventory
        .configurations()
        .iter()
        .zip(original.configurations())
        .enumerate()
    {
        assert_eq!(after.profile_name(), before.profile_name());
        assert!(after.native_scopes().eq(before.native_scopes()));
        assert_eq!(after.catalogue_bindings(), before.catalogue_bindings());
        assert!(
            after
                .participant_paths()
                .filter(|path| *path != Q)
                .eq(before.participant_paths())
        );
        let visible = matches!(after.profile_name(), None | Some("desktop_scripts"));
        assert_eq!(after.participant_paths().any(|path| path == Q), visible);
        assert_eq!(
            metadata
                .visibility()
                .iter()
                .any(|policy| policy.configuration_index() == index),
            visible
        );
        if after.profile_name() == Some("mobile") {
            assert!(after.participant_paths().any(|path| path == R));
            assert!(!after.participant_paths().any(|path| path == P));
        }
        if after.profile_name() == Some("desktop_scripts") {
            assert!(before.participant_paths().any(|path| path == P));
            assert!(!before.participant_paths().any(|path| path == R));
            assert!(after.participant_paths().any(|path| path == P));
            assert!(!after.participant_paths().any(|path| path == R));
        }
    }
    assert!(
        inventory
            .configurations()
            .iter()
            .any(|configuration| configuration.profile_name() == Some("mobile"))
    );
    assert_eq!(
        inventory.captured_open_sources().len(),
        if recovered { 0 } else { fixture.open.len() }
    );
    if !recovered {
        let guard = fixture.workspace.prefab(fixture.document(GUARD)).unwrap();
        assert_ne!(
            inventory.participant(GUARD).unwrap().bytes(),
            guard
                .session()
                .document()
                .to_ron_string()
                .unwrap()
                .as_bytes()
        );
        assert_eq!(
            owner
                .live_outputs()
                .unwrap()
                .review()
                .persistence
                .disposition(fixture.document(GUARD)),
            Some(EditorPrefabExtractionDisposition::Guard)
        );
    }
    for captured in inventory.captured_open_sources() {
        let original = original
            .captured_open_sources()
            .find(|source| source.relative_path() == captured.relative_path())
            .unwrap();
        let participant = inventory.participant(captured.relative_path()).unwrap();
        assert_eq!(captured.source_key(), participant.source_key());
        if let Some((position, _)) = files
            .iter()
            .enumerate()
            .find(|(_, (_, path, _))| path == captured.relative_path())
        {
            let (_, ExtractionFileResult::Checkpoint(acceptance)) = owner.result(position).unwrap()
            else {
                panic!("an open output must retain its original accepted checkpoint")
            };
            assert!(acceptance.accepted());
            let (identity, parent, digest) = acceptance.observation();
            assert!(captured.matches_persistence_observation(original, identity, parent, digest));
        } else {
            assert_eq!(captured, original);
        }
    }
}

#[test]
fn actual_live_and_recovered_generations_admit_q_as_an_ordinary_native_source_in_both_modes() {
    for (stable, mixed_scripts) in [(false, false), (true, false), (false, true)] {
        for recovered in [false, true] {
            let mut fixture = if mixed_scripts {
                Fixture::with_profiles(
                    "mobile/assets",
                    "[profiles.desktop_scripts.paths]\nprefabs = \"desktop/prefabs\"\nscripts = \"mobile/prefabs\"\n",
                )
            } else {
                Fixture::with_native_identity(if stable {
                    "stable_metadata"
                } else {
                    "project_location"
                })
            };
            if !stable {
                add_closed_rewrite(&fixture);
            }
            let (owner, files) = publication(&mut fixture, recovered);
            assert_eq!(owner.phase(), Phase::Files);
            let before = disk_state(&fixture);
            let failure = owner.into_future_inventory().unwrap_err();
            assert!(matches!(failure.error(), Error::Phase));
            assert_eq!(failure.owner().position(), 0);
            assert_eq!(disk_state(&fixture), before);
            let mut owner = failure.into_owner();
            while owner.phase() != Phase::FilesComplete {
                owner = owner.advance().unwrap();
            }
            let before = disk_state(&fixture);
            let results = file_results(&owner);
            let failure = owner.into_future_inventory().unwrap_err();
            assert!(matches!(failure.error(), Error::Phase));
            assert_eq!(failure.owner().phase(), Phase::FilesComplete);
            assert_eq!(file_results(failure.owner()), results);
            assert_eq!(disk_state(&fixture), before);
            let owner = checkpoints_complete(failure.into_owner());
            let semantic = semantics(&owner);
            let results = file_results(&owner);
            let original_custody = original_inventory(&owner).shared_custody().weak_owner();
            assert_eq!(
                semantic.basis == fixture.project.startup_scene.schema_basis(),
                !recovered
            );
            let physical = disk_state(&fixture);
            let live = live_state(&fixture);
            let mut future = owner.into_future_inventory().unwrap();
            assert_native_generation(&fixture, &future, &files, stable, recovered);
            assert_eq!(
                future.inventory().configurations().len(),
                if mixed_scripts { 3 } else { 2 }
            );
            assert_eq!(
                future
                    .inventory()
                    .configurations()
                    .iter()
                    .any(|configuration| configuration.profile_name() == Some("desktop_scripts")),
                mixed_scripts
            );
            let custody = future.inventory().shared_custody().weak_owner();
            assert!(!std::sync::Weak::ptr_eq(&custody, &original_custody));
            let inventory = future.inventory() as *const _;
            let q_bytes = future.inventory().participant(Q).unwrap().bytes().as_ptr();
            let metadata_bytes = future
                .inventory()
                .metadata_for_source(Q)
                .unwrap()
                .bytes()
                .as_ptr();
            for _ in 0..2 {
                let usage = future.host_usage().unwrap();
                future.validate().unwrap();
                assert_released_temporary_storage(usage, future.host_usage().unwrap());
                assert_eq!(future.inventory() as *const _, inventory);
                assert_eq!(
                    future.inventory().participant(Q).unwrap().bytes().as_ptr(),
                    q_bytes
                );
                assert_eq!(
                    future
                        .inventory()
                        .metadata_for_source(Q)
                        .unwrap()
                        .bytes()
                        .as_ptr(),
                    metadata_bytes
                );
                assert!(std::sync::Weak::ptr_eq(
                    &future.inventory().shared_custody().weak_owner(),
                    &custody
                ));
                assert!(std::sync::Weak::ptr_eq(
                    &original_inventory(future.publication())
                        .shared_custody()
                        .weak_owner(),
                    &original_custody
                ));
                assert_eq!(semantics(future.publication()), semantic);
                assert_eq!(file_results(future.publication()), results);
            }
            assert_eq!(disk_state(&fixture), physical);
            assert_eq!(live_state(&fixture), live);
            assert_eq!(future.publication().phase(), Phase::CheckpointsComplete);
            assert_pending(&fixture);
            drop(future);
            assert!(custody.upgrade().is_none());
            assert!(original_custody.upgrade().is_none());
        }
    }
}

#[test]
fn late_source_metadata_or_new_root_refuses_admission_and_source_removal_retries_the_same_owner() {
    for recovered in [false, true] {
        for change in [LATE_SOURCE, LATE_METADATA, LATE_ROOT] {
            let mut fixture = if change == LATE_ROOT {
                Fixture::with_profiles(
                    "mobile/assets",
                    "[profiles.dormant.paths]\nscenes = 'dormant/scenes'\n[profiles.dormant.startup]\nclear_default_scene = true\n",
                )
            } else {
                Fixture::new()
            };
            if change == LATE_METADATA {
                fs::write(
                    fixture.root.join("mobile/assets/late.bin"),
                    b"ordinary image source",
                )
                .unwrap();
            }
            let (owner, _) = publication(&mut fixture, recovered);
            let mut owner = checkpoints_complete(owner);
            let semantic = semantics(&owner);
            let results = file_results(&owner);
            let custody = original_inventory(&owner).shared_custody().weak_owner();
            let usage = owner.host_usage().unwrap();
            let physical = disk_state(&fixture);
            if change == LATE_SOURCE {
                fs::write(
                    fixture.root.join(LATE_SOURCE),
                    SceneDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
            } else if change == LATE_METADATA {
                fs::write(
                    fixture.root.join(LATE_METADATA),
                    AssetMeta::new(
                        AssetMetaIdentity::new(
                            StableAssetId::parse_str("89a93041-23ef-46a8-95b7-3d53c3e07a92")
                                .unwrap(),
                            AssetPath::new("late.bin").unwrap(),
                            AssetSourceKind::Image,
                        ),
                        0_u32,
                    )
                    .to_json_string()
                    .unwrap(),
                )
                .unwrap();
            } else {
                assert!(
                    original_inventory(&owner)
                        .roots()
                        .any(|root| root.path() == LATE_ROOT
                            && matches!(root.observation(), SourceRootObservation::Missing { .. }))
                );
                fs::create_dir_all(fixture.root.join(LATE_ROOT)).unwrap();
            }
            let changed = disk_state(&fixture);
            let failure = owner.into_future_inventory().unwrap_err();
            assert!(
                matches!(
                    failure.error(),
                    Error::Generation | Error::Inventory(SourceInventoryError::MembershipChanged)
                ),
                "{:?}",
                failure.error()
            );
            assert_eq!(semantics(failure.owner()), semantic);
            assert_eq!(file_results(failure.owner()), results);
            assert_eq!(failure.owner().phase(), Phase::CheckpointsComplete);
            let mut owner = failure.into_owner();
            let spent = owner.host_usage().unwrap();
            assert_released_temporary_storage(usage, spent);
            assert!(std::sync::Weak::ptr_eq(
                &original_inventory(&owner).shared_custody().weak_owner(),
                &custody
            ));
            let failure = owner.into_future_inventory().unwrap_err();
            assert_eq!(semantics(failure.owner()), semantic);
            assert_eq!(file_results(failure.owner()), results);
            assert_eq!(disk_state(&fixture), changed);
            if change == LATE_SOURCE {
                fs::remove_file(fixture.root.join(LATE_SOURCE)).unwrap();
                let mut future = failure.retry().unwrap();
                assert_eq!(semantics(future.publication()), semantic);
                assert_eq!(file_results(future.publication()), results);
                assert!(std::sync::Weak::ptr_eq(
                    &original_inventory(future.publication())
                        .shared_custody()
                        .weak_owner(),
                    &custody
                ));
                assert!(future.host_usage().unwrap().decode_work() > spent.decode_work());
                future.validate().unwrap();
                assert_eq!(disk_state(&fixture), physical);
                drop(future);
            } else {
                drop(failure);
            }
            assert_pending(&fixture);
            assert!(custody.upgrade().is_none());
        }
    }
}

#[test]
fn equal_byte_replacement_of_an_original_guard_or_metadata_is_not_a_future_generation() {
    for recovered in [false, true] {
        for path in [GUARD, "desktop/prefabs/actor.prefab.json.meta"] {
            let mut fixture = Fixture::new();
            let (owner, _) = publication(&mut fixture, recovered);
            let mut owner = checkpoints_complete(owner);
            let semantic = semantics(&owner);
            let results = file_results(&owner);
            let retained = original_inventory(&owner).shared_file(path).unwrap();
            let bytes = fs::read(fixture.root.join(path)).unwrap();
            let usage = owner.host_usage().unwrap();
            let root = fixture.project.project_root();
            replace(&root, path, &bytes);
            let replacement = root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            assert_ne!(retained.identity(), replacement.identity());
            assert!(!retained.same_object(&replacement).unwrap());
            assert_eq!(fs::read(fixture.root.join(path)).unwrap(), bytes);
            let changed = disk_state(&fixture);
            let failure = owner.into_future_inventory().unwrap_err();
            assert!(
                matches!(
                    failure.error(),
                    Error::Generation
                        | Error::Inventory(
                            SourceInventoryError::SourceChanged
                                | SourceInventoryError::OpenSourceChanged
                        )
                ),
                "{:?}",
                failure.error()
            );
            assert_eq!(semantics(failure.owner()), semantic);
            assert_eq!(file_results(failure.owner()), results);
            let mut owner = failure.into_owner();
            assert_released_temporary_storage(usage, owner.host_usage().unwrap());
            let failure = owner.into_future_inventory().unwrap_err();
            assert_eq!(semantics(failure.owner()), semantic);
            assert_eq!(file_results(failure.owner()), results);
            assert_eq!(disk_state(&fixture), changed);
            assert_pending(&fixture);
        }
    }
}

#[test]
fn validation_refuses_equal_byte_output_replacement_and_keeps_the_successful_future_owner() {
    for recovered in [false, true] {
        for path in [Q, Q_SIDECAR] {
            let mut fixture = Fixture::new();
            let (owner, _) = publication(&mut fixture, recovered);
            let mut future = checkpoints_complete(owner).into_future_inventory().unwrap();
            let semantic = semantics(future.publication());
            let results = file_results(future.publication());
            let custody = future.inventory().shared_custody().weak_owner();
            let inventory = future.inventory() as *const _;
            let retained = future.inventory().shared_file(path).unwrap();
            let bytes = fs::read(fixture.root.join(path)).unwrap();
            let root = fixture.project.project_root();
            replace(&root, path, &bytes);
            let replacement = root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            assert_ne!(retained.identity(), replacement.identity());
            assert!(!retained.same_object(&replacement).unwrap());
            let changed = disk_state(&fixture);
            for _ in 0..2 {
                let usage = future.host_usage().unwrap();
                let error = future.validate().unwrap_err();
                assert!(
                    matches!(
                        error,
                        Error::Inventory(
                            SourceInventoryError::SourceChanged
                                | SourceInventoryError::MembershipChanged
                        )
                    ),
                    "{error:?}"
                );
                assert_released_temporary_storage(usage, future.host_usage().unwrap());
                assert_eq!(future.inventory() as *const _, inventory);
                assert_eq!(
                    future.inventory().shared_file(path).unwrap().identity(),
                    retained.identity()
                );
                assert!(std::sync::Weak::ptr_eq(
                    &future.inventory().shared_custody().weak_owner(),
                    &custody
                ));
                assert_eq!(semantics(future.publication()), semantic);
                assert_eq!(file_results(future.publication()), results);
                assert_eq!(disk_state(&fixture), changed);
            }
            assert_pending(&fixture);
            drop(future);
            assert!(custody.upgrade().is_none());
        }
    }
}

pub(super) fn recovered_with_budget(
    fixture: &mut Fixture,
    mut host: AuthoringProjectAdmissionBudget,
) -> ProjectPrefabExtractionPublication {
    drop(confirmed(fixture));
    fixture.open.clear();
    fixture.workspace = EditorWorkspace::new();
    let pending = load(fixture, &mut host);
    checkpoints_complete(generation(pending, host).into_publication().unwrap())
}

#[test]
fn future_admission_budget_refusal_releases_only_new_storage_and_retains_spent_allowance() {
    let mut probe = Fixture::new();
    let owner = recovered_with_budget(&mut probe, budget());
    let mut future = owner.into_future_inventory().unwrap();
    let exact = future.host_usage().unwrap().aggregate_encoded_bytes();
    drop(future);

    let mut fixture = Fixture::new();
    // Select the entire recovered operation's cap before its first Pending read or capture.
    // The actual producer above has its own original live allowance; neither is replenished.
    let host = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::new(
        ByteLimit::new(exact - 1).unwrap(),
    ))
    .for_source_operation(
        ItemLimit::new(200_000_000).unwrap(),
        ItemLimit::new(128).unwrap(),
    );
    let mut owner = recovered_with_budget(&mut fixture, host);
    let semantic = semantics(&owner);
    let results = file_results(&owner);
    let custody = original_inventory(&owner).shared_custody().weak_owner();
    let physical = disk_state(&fixture);
    let before = owner.host_usage().unwrap();
    let failure = owner.into_future_inventory().unwrap_err();
    assert!(
        matches!(
            failure.error(),
            Error::Inventory(SourceInventoryError::Admission(_))
                | Error::Publication(ProjectPrefabExtractionPublicationError::Admission(_))
                | Error::Publication(ProjectPrefabExtractionPublicationError::Pending(
                    RecoveryStorageError::Admission(_)
                ))
        ),
        "{:?}",
        failure.error()
    );
    assert_eq!(semantics(failure.owner()), semantic);
    assert_eq!(file_results(failure.owner()), results);
    let mut owner = failure.into_owner();
    let spent = owner.host_usage().unwrap();
    assert_released_temporary_storage(before, spent);
    let failure = owner.into_future_inventory().unwrap_err();
    assert_eq!(semantics(failure.owner()), semantic);
    assert_eq!(file_results(failure.owner()), results);
    let mut owner = failure.into_owner();
    let retried = owner.host_usage().unwrap();
    assert_eq!(
        retried.retained_candidate_bytes(),
        spent.retained_candidate_bytes()
    );
    assert_eq!(
        retried.retained_open_handles(),
        spent.retained_open_handles()
    );
    assert!(retried.aggregate_encoded_bytes() >= spent.aggregate_encoded_bytes());
    assert!(retried.decode_work() >= spent.decode_work());
    assert!(std::sync::Weak::ptr_eq(
        &original_inventory(&owner).shared_custody().weak_owner(),
        &custody
    ));
    assert_eq!(disk_state(&fixture), physical);
    assert_pending(&fixture);
    drop(owner);
    assert!(custody.upgrade().is_none());
}

#[test]
fn live_future_native_peak_is_exact_and_one_short_preserves_the_original_publication() {
    let mut probe = Fixture::new();
    let (owner, _) = publication(&mut probe, false);
    let mut future = checkpoints_complete(owner).into_future_inventory().unwrap();
    future.validate().unwrap();
    let required = future.host_usage().unwrap().peak_open_handles();
    assert!(required < 128);
    drop(future);

    for limit in [required, required - 1] {
        let fixture = Fixture::new();
        // Select the whole operation's allowance before retaining its first open proof. The
        // old corpus and new ordinary inventory deliberately coexist until final binding.
        let request = fixture.request_with_host_handles(
            SceneCompilationLimits::default(),
            ItemLimit::new(limit).unwrap(),
        );
        let storage = fixture::confirmed_request(&fixture, request);
        let mut owner = checkpoints_complete(storage.into_publication().unwrap());
        let before = owner.host_usage().unwrap();
        let semantic = semantics(&owner);
        let results = file_results(&owner);
        let physical = disk_state(&fixture);
        match owner.into_future_inventory() {
            Ok(mut future) => {
                let before = future.host_usage().unwrap();
                match future.validate() {
                    Ok(()) => {
                        assert_eq!(limit, required);
                        assert_eq!(future.host_usage().unwrap().peak_open_handles(), required);
                    }
                    Err(error) => {
                        assert_eq!(limit, required - 1);
                        assert!(matches!(
                            error,
                            Error::Inventory(SourceInventoryError::Admission(
                                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
                            ))
                        ));
                        assert_released_temporary_storage(before, future.host_usage().unwrap());
                    }
                }
                assert_eq!(semantics(future.publication()), semantic);
                assert_eq!(file_results(future.publication()), results);
            }
            Err(failure) => {
                assert_eq!(limit, required - 1);
                assert!(matches!(
                    failure.error(),
                    Error::Inventory(SourceInventoryError::Admission(
                        AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
                    ))
                ));
                let mut owner = failure.into_owner();
                assert_released_temporary_storage(before, owner.host_usage().unwrap());
                assert_eq!(semantics(&owner), semantic);
                assert_eq!(file_results(&owner), results);
            }
        }
        assert_eq!(disk_state(&fixture), physical);
        assert_pending(&fixture);
    }
}
