use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetSourcePath;
use nara_core::ItemLimit;
use nara_fs::{ContentDigest, DirectoryCapability, RelativePath};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, ProjectAuthoringRecovery, ProjectAuthoringRecoveryDigest,
    ProjectAuthoringState, ProjectManifest, ProjectPath,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{SceneDocument, SourceDocumentKey, SourceDocumentKind};

use super::super::recovery_storage::*;
use super::*;
use crate::project_host::file_publication::{
    FilePublicationFailureStage, FilePublicationHost, FilePublicationOutcome,
    FilePublicationRejection,
};
use crate::{
    project_host::authoring::{
        AuthoringProjectAdmissionError, AuthoringProjectAdmissionHost,
        AuthoringProjectAdmissionLimits, PROJECT_MANIFEST_PATH,
        source_upgrade_recovery::{
            RecoverySourceEncoding, SourceUpgradeRecoveryInput, SourceUpgradeRecoveryMemberInput,
            SourceUpgradeRecoveryRootInput,
        },
    },
    test_host_fs as host_fs,
};

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);
const RECOVERY_PATH: &str = ".nara/recovery.json";
const BUNDLE_PATH: &str = "schema/bundle.json";
const SOURCE_PATH: &str = "scenes/level.scene.json";
const UNIVERSE: &str = "421112bd-693a-4e6b-85bc-318d567b2f21";
const COMPONENT: &str = "trial.enemy.EnemyStats";

struct Fixture {
    directory: PathBuf,
    root: DirectoryCapability,
    manifest: Vec<u8>,
    future_manifest: Vec<u8>,
    old_bundle: Vec<u8>,
    next_bundle: Vec<u8>,
    scene: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nara-upgrade-storage-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        for parent in [".nara", "schema", "scenes"] {
            fs::create_dir(directory.join(parent)).unwrap();
        }
        let manifest = format!(
            "# Preserve the original Ready observation.\nschema_version = 1\n\
             [project]\nname = 'Recovery storage fixture'\nsource_universe = '{UNIVERSE}'\n\
             native_document_identity = 'project_location'\n\
             [authoring]\nstate = 'ready'\nschema_bundle = '{BUNDLE_PATH}'\n"
        )
        .into_bytes();
        let future_manifest = ProjectManifest::parse_toml_bytes(&manifest)
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        let old = ComponentSchemaCatalog {
            components: vec![schema(false)],
            ..Default::default()
        };
        let mut next = ComponentSchemaCatalog::successor_of(&old).unwrap();
        next.components.push(schema(true));
        let old_bundle = bundle(old.clone(), None);
        let next_bundle = bundle(next, Some(old));
        let scene = SceneDocument::default()
            .to_json_string()
            .unwrap()
            .into_bytes();
        fs::write(directory.join(PROJECT_MANIFEST_PATH), &manifest).unwrap();
        fs::write(directory.join(BUNDLE_PATH), &old_bundle).unwrap();
        fs::write(directory.join(SOURCE_PATH), &scene).unwrap();
        let root = host_fs::writable_directory(&directory).unwrap();
        Self {
            directory,
            root,
            manifest,
            future_manifest,
            old_bundle,
            next_bundle,
            scene,
        }
    }

    fn package(&self, budget: &mut AuthoringProjectAdmissionBudget) -> SourceUpgradeRecovery {
        let key = SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(SOURCE_PATH).unwrap(),
            SourceDocumentKind::Scene,
        );
        SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                selected_profile: None,
                old_ready_manifest: &self.manifest,
                future_ready_manifest: &self.future_manifest,
                bundle_path: BUNDLE_PATH,
                old_bundle: &self.old_bundle,
                successor_bundle: &self.next_bundle,
                component: &ComponentTypeId::new(COMPONENT),
                field: &ComponentFieldId::new("health"),
                successor_property: &ComponentPropertyPath::new(ComponentFieldId::new("health"))
                    .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                        "milli",
                    ))]),
                members: &[SourceUpgradeRecoveryMemberInput {
                    path: SOURCE_PATH,
                    key: &key,
                    encoding: RecoverySourceEncoding::Json,
                    explicit_native: false,
                    old_disk: &self.scene,
                    reviewed_working: &self.scene,
                    successor: &self.scene,
                }],
                metadata: &[],
                roots: &[
                    SourceUpgradeRecoveryRootInput {
                        path: "assets",
                        missing_ancestor: Some(""),
                    },
                    SourceUpgradeRecoveryRootInput {
                        path: "prefabs",
                        missing_ancestor: Some(""),
                    },
                    SourceUpgradeRecoveryRootInput {
                        path: "scenes",
                        missing_ancestor: None,
                    },
                ],
                missing_sidecars: &["scenes/level.scene.json.meta"],
            },
            SourceUpgradeRecoveryLimits::default(),
            budget,
        )
        .unwrap()
    }

    fn prepare(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> PreparedRecoveryStorage<SourceUpgradeRecovery> {
        let recovery = self.package(budget);
        RecoveryStorageHost::capture(self.root.share(), budget)
            .unwrap()
            .prepare(
                recovery.into_shared(budget).unwrap(),
                ProjectPath::new(RECOVERY_PATH).unwrap(),
                budget,
            )
            .unwrap()
    }

    fn stored(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> StoredRecovery<SourceUpgradeRecovery> {
        stored(self.prepare(budget).publish(budget))
    }

    fn pending(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> PendingRecoveryStorage<SourceUpgradeRecovery> {
        pending(
            self.stored(budget)
                .bind_pending(budget)
                .unwrap()
                .publish(budget),
        )
    }

    fn manifest(&self) -> ProjectManifest {
        ProjectManifest::parse_toml_bytes(
            &fs::read(self.directory.join(PROJECT_MANIFEST_PATH)).unwrap(),
        )
        .manifest
        .unwrap()
    }

    fn assert_original_ready(&self) {
        assert_eq!(
            fs::read(self.directory.join(PROJECT_MANIFEST_PATH)).unwrap(),
            self.manifest
        );
        self.assert_original_sources();
    }

    fn assert_original_sources(&self) {
        assert_eq!(
            fs::read(self.directory.join(BUNDLE_PATH)).unwrap(),
            self.old_bundle
        );
        assert_eq!(
            fs::read(self.directory.join(SOURCE_PATH)).unwrap(),
            self.scene
        );
    }

    fn write_manifest(&self, manifest: &ProjectManifest) {
        fs::write(
            self.directory.join(PROJECT_MANIFEST_PATH),
            manifest.to_canonical_toml_bytes().unwrap(),
        )
        .unwrap();
    }

    fn replace_manifest(&self, bytes: &[u8]) {
        let file = self
            .root
            .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let digest = file.digest(DEFAULT_MANIFEST_BYTE_LIMIT).unwrap();
        let mut publisher = FilePublicationHost::bind_existing(
            &self.root,
            PROJECT_MANIFEST_PATH,
            &file,
            digest,
            DEFAULT_MANIFEST_BYTE_LIMIT,
        )
        .unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (_, source) = accepted.into_parts();
                publisher.commit_published(source);
            }
            FilePublicationOutcome::Rejected(reason) => {
                panic!("fixture manifest replacement rejected: {reason:?}")
            }
            FilePublicationOutcome::Failed(stage) => {
                panic!("fixture manifest replacement failed: {stage:?}")
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("fixture manifest replacement uncertain: {evidence:?}")
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only fixed leaves of this exclusively created test directory are removed. Never
        // recursively remove a computed parent or share a directory with another test.
        for leaf in [
            RECOVERY_PATH,
            ".nara/other.json",
            BUNDLE_PATH,
            SOURCE_PATH,
            PROJECT_MANIFEST_PATH,
        ] {
            let _ = fs::remove_file(self.directory.join(leaf));
        }
        for parent in [".nara-original", ".nara", "schema", "scenes"] {
            let _ = fs::remove_dir(self.directory.join(parent));
        }
        if let Err(error) = fs::remove_dir(&self.directory) {
            if std::thread::panicking() {
                eprintln!("storage fixture cleanup refused: {error}");
            } else {
                panic!("storage fixture cleanup refused: {error}");
            }
        }
    }
}

fn schema(next: bool) -> ComponentSchema {
    let health = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("health"),
        "Health",
        ComponentFieldPath::from_fields(["wire_health"]),
        health,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn bundle(current: ComponentSchemaCatalog, predecessor: Option<ComponentSchemaCatalog>) -> Vec<u8> {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap(),
        current,
        predecessor,
    )])
    .unwrap()
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

fn stored(
    outcome: RecoveryStoragePublication<SourceUpgradeRecovery>,
) -> StoredRecovery<SourceUpgradeRecovery> {
    match outcome {
        RecoveryStoragePublication::Published(stored) => stored,
        RecoveryStoragePublication::Refused { error, .. } => panic!("package refused: {error:?}"),
        RecoveryStoragePublication::Rejected { reason, .. } => {
            panic!("package rejected: {reason:?}")
        }
        RecoveryStoragePublication::Failed { stage, .. } => panic!("package failed: {stage:?}"),
        RecoveryStoragePublication::Uncertain { publication, .. } => {
            panic!("package uncertain: {publication:?}")
        }
    }
}

fn pending(
    outcome: RecoveryPendingPublication<SourceUpgradeRecovery>,
) -> PendingRecoveryStorage<SourceUpgradeRecovery> {
    match outcome {
        RecoveryPendingPublication::Published(pending) => pending,
        RecoveryPendingPublication::Refused { error, .. } => panic!("Pending refused: {error:?}"),
        RecoveryPendingPublication::Rejected { reason, .. } => {
            panic!("Pending rejected: {reason:?}")
        }
        RecoveryPendingPublication::Failed { stage, .. } => panic!("Pending failed: {stage:?}"),
        RecoveryPendingPublication::Uncertain { publication, .. } => {
            panic!("Pending uncertain: {publication:?}")
        }
    }
}

struct WrongCommitment(Arc<SourceUpgradeRecovery>);

impl RecoveryPackageData for WrongCommitment {
    fn encoded_bytes(&self) -> &[u8] {
        self.0.encoded_bytes()
    }
    fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        let digest = self.0.digest();
        let mut bytes = *digest.as_bytes();
        bytes[0] ^= 1;
        ProjectAuthoringRecoveryDigest::from_parts(digest.length(), bytes)
    }
    fn source_universe(&self) -> SourceUniverseId {
        self.0.source_universe()
    }
    fn old_ready_manifest(&self) -> &[u8] {
        self.0.manifest().old_disk.bytes()
    }
    fn final_ready_manifest(&self) -> &[u8] {
        self.0.manifest().successor.bytes()
    }
    fn protected_paths(&self) -> impl Iterator<Item = &str> {
        self.0.protected_paths()
    }
}

#[test]
fn transport_rechecks_projected_commitments_before_writes_or_raw_capture_transfer() {
    for fresh in [false, true] {
        let fixture = Fixture::new();
        let mut work = budget();
        if fresh {
            drop(fixture.pending(&mut work));
            let loaded = RecoveryStorageHost::load_pending(
                fixture.root.share(),
                SourceUpgradeRecoveryLimits::default().encoded_bytes,
                &mut work,
            )
            .unwrap();
            let package = SourceUpgradeRecovery::decode(
                loaded.encoded_bytes(),
                Default::default(),
                &mut work,
            )
            .unwrap()
            .into_shared(&mut work)
            .unwrap();
            work.charge_source_bytes(0, 0, 0, 4 * std::mem::size_of::<usize>())
                .unwrap();
            let wrong = Arc::new(WrongCommitment(package));
            let original = Arc::as_ptr(&wrong);
            let encoded = loaded.encoded_bytes().as_ptr();
            let before = work.usage();
            let Err(failure) = loaded.bind(wrong, &mut work) else {
                panic!("a projected digest cannot replace the actual Pending commitment")
            };
            assert!(matches!(
                failure.error,
                RecoveryStorageError::PackageDigestMismatch
            ));
            assert_eq!(Arc::as_ptr(&failure.recovery), original);
            assert_eq!(failure.loaded.encoded_bytes().as_ptr(), encoded);
            assert_eq!(
                work.usage().retained_candidate_bytes(),
                before.retained_candidate_bytes()
            );
            assert_eq!(
                work.usage().retained_open_handles(),
                before.retained_open_handles()
            );
            assert!(work.usage().decode_work() > before.decode_work());
        } else {
            let package = fixture.package(&mut work).into_shared(&mut work).unwrap();
            work.charge_source_bytes(0, 0, 0, 4 * std::mem::size_of::<usize>())
                .unwrap();
            let wrong = Arc::new(WrongCommitment(package));
            let original = Arc::as_ptr(&wrong);
            let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
            let before = work.usage();
            let Err(failure) =
                host.prepare(wrong, ProjectPath::new(RECOVERY_PATH).unwrap(), &mut work)
            else {
                panic!("fixed bytes must match their commitment before publishing recovery data")
            };
            assert!(matches!(
                failure.error,
                RecoveryStorageError::PackageDigestMismatch
            ));
            assert_eq!(Arc::as_ptr(&failure.recovery), original);
            assert_eq!(
                work.usage().retained_candidate_bytes(),
                before.retained_candidate_bytes()
            );
            assert_eq!(
                work.usage().retained_open_handles(),
                before.retained_open_handles()
            );
            assert!(work.usage().decode_work() > before.decode_work());
            fixture.assert_original_ready();
            assert!(!fixture.directory.join(RECOVERY_PATH).exists());
        }
        fixture.assert_original_sources();
    }
}

#[test]
fn actual_package_precedes_pending_and_fresh_capabilities_read_the_same_operation() {
    let fixture = Fixture::new();
    let mut work = budget();
    let prepared = fixture.prepare(&mut work);
    fixture.assert_original_ready();
    assert!(!fixture.directory.join(RECOVERY_PATH).exists());
    let stored = stored(prepared.publish(&mut work));
    fixture.assert_original_ready();
    assert_eq!(stored.root().identity(), fixture.root.identity());
    assert_eq!(
        stored.digest(),
        stored
            .capability()
            .digest(stored.digest().length())
            .unwrap()
    );
    assert_eq!(stored.publication().receipt().previous_identity(), None);
    assert_eq!(
        stored.publication().receipt().candidate_identity(),
        stored.capability().identity()
    );
    assert_eq!(
        fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap(),
        stored.recovery().encoded_bytes()
    );
    let original_manifest_identity = fixture
        .root
        .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
        .unwrap()
        .identity();
    let confirmed = pending(stored.bind_pending(&mut work).unwrap().publish(&mut work));
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    fixture.assert_original_sources();
    let observed = confirmed.observed();
    observed.validate(&mut work).unwrap();
    assert_eq!(
        confirmed
            .pending_publication()
            .receipt()
            .previous_identity(),
        Some(original_manifest_identity)
    );
    assert_eq!(
        confirmed
            .pending_publication()
            .receipt()
            .candidate_identity(),
        confirmed.capability().identity()
    );
    assert_eq!(
        confirmed
            .recovery_publication()
            .receipt()
            .candidate_identity(),
        observed.recovery_file().identity()
    );
    assert_eq!(
        confirmed.digest(),
        ContentDigest::of_bytes(observed.manifest_bytes())
    );
    assert_eq!(
        observed.recovery_digest(),
        observed
            .recovery_file()
            .digest(observed.recovery_digest().length())
            .unwrap()
    );
    let operation = observed.recovery().digest();
    let original_session = observed.root().identity();
    drop(confirmed);

    // Open a completely new Host session. Serialized recovery has no FileIdentity to compare
    // with this session. It yields read observations, never old publication receipts.
    let fresh_root = host_fs::writable_directory(&fixture.directory).unwrap();
    assert_ne!(fresh_root.identity(), original_session);
    let reopened = load_pending(
        fresh_root,
        SourceUpgradeRecoveryLimits::default(),
        &mut budget(),
    )
    .unwrap();
    assert_eq!(reopened.recovery().digest(), operation);
    assert_eq!(reopened.reference().recovery_path().as_str(), RECOVERY_PATH);
    assert_eq!(
        reopened.capability().parent_identity(),
        Some(reopened.root().identity())
    );
    // Recovery storage does not bypass ordinary source readiness or activate old bundle data.
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(reopened.root().share(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn dropping_before_pending_preserves_ready_and_keeps_the_published_package() {
    let fixture = Fixture::new();
    let stored = fixture.stored(&mut budget());
    let digest = stored.digest();
    drop(stored);
    fixture.assert_original_ready();
    assert_eq!(
        ContentDigest::of_bytes(&fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap()),
        digest
    );
    assert!(matches!(
        load_pending(
            fixture.root.share(),
            SourceUpgradeRecoveryLimits::default(),
            &mut budget()
        ),
        Err(SourceUpgradeStorageError::Storage(
            RecoveryStorageError::PendingRequired
        ))
    ));
}

#[test]
fn existing_or_competing_recovery_leaf_is_never_replaced() {
    let fixture = Fixture::new();
    fs::write(
        fixture.directory.join(RECOVERY_PATH),
        b"unrelated existing data",
    )
    .unwrap();
    let mut work = budget();
    let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
    let recovery = fixture.package(&mut work);
    assert!(matches!(
        host.prepare(
            recovery.into_shared(&mut work).unwrap(),
            ProjectPath::new(RECOVERY_PATH).unwrap(),
            &mut work
        ),
        Err(RecoveryStoragePrepareFailure {
            error: RecoveryStorageError::RecoveryAlreadyExists,
            ..
        })
    ));
    fs::remove_file(fixture.directory.join(RECOVERY_PATH)).unwrap();
    let prepared = fixture.prepare(&mut work);
    fs::write(fixture.directory.join(RECOVERY_PATH), b"late external data").unwrap();
    let RecoveryStoragePublication::Rejected {
        prepared,
        reason: FilePublicationRejection::TargetChanged,
    } = prepared.publish(&mut work)
    else {
        panic!("the Missing publisher must reject a newly occupied name");
    };
    assert!(!prepared.uncertain());
    assert!(!prepared.cleanup_pending());
    assert_eq!(
        fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap(),
        b"late external data"
    );
    fixture.assert_original_ready();
}

#[test]
fn matching_recovery_bytes_at_an_existing_destination_are_not_reused() {
    let fixture = Fixture::new();
    let mut work = budget();
    let recovery = fixture.package(&mut work).into_shared(&mut work).unwrap();
    fs::write(
        fixture.directory.join(RECOVERY_PATH),
        recovery.encoded_bytes(),
    )
    .unwrap();
    let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
    let Err(refusal) = host.prepare(
        Arc::clone(&recovery),
        ProjectPath::new(RECOVERY_PATH).unwrap(),
        &mut work,
    ) else {
        panic!("matching bytes do not grant publication ownership of an existing destination");
    };
    assert!(matches!(
        &refusal.error,
        RecoveryStorageError::RecoveryAlreadyExists
    ));
    assert_eq!(refusal.path.as_str(), RECOVERY_PATH);
    assert!(Arc::ptr_eq(&refusal.recovery, &recovery));
    assert_eq!(
        fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap(),
        recovery.encoded_bytes()
    );
    fixture.assert_original_ready();
}

#[test]
fn capture_refusal_drops_only_new_storage_and_handles_while_work_accumulates() {
    let fixture = Fixture::new();
    fs::write(
        fixture.directory.join(PROJECT_MANIFEST_PATH),
        b"invalid = [",
    )
    .unwrap();
    let _incumbents = [BUNDLE_PATH, SOURCE_PATH].map(|path| {
        fixture
            .root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap()
    });
    let mut work = budget();
    work.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    work.charge_source_bytes(0, 0, 37, 321).unwrap();
    work.retain_open_handles(_incumbents.len()).unwrap();
    let before = work.usage();
    for _ in 0..2 {
        let performed = work.usage().decode_work();
        assert!(matches!(
            RecoveryStorageHost::capture(fixture.root.share(), &mut work),
            Err(RecoveryStorageError::ManifestInvalid)
        ));
        assert_eq!(
            work.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            work.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(work.usage().decode_work() > performed);
    }
}

#[test]
fn capture_handle_admission_accepts_the_exact_peak_and_refunds_a_one_short_refusal() {
    let fixture = Fixture::new();
    let mut measured = budget();
    let captured = RecoveryStorageHost::capture(fixture.root.share(), &mut measured).unwrap();
    let peak = measured.usage().peak_open_handles();
    assert_eq!(measured.usage().retained_open_handles(), 1);
    drop(captured);
    for (maximum, accepted) in [(peak, true), (peak - 1, false)] {
        let mut work = budget();
        work.limits.peak_open_handles = ItemLimit::new(maximum).unwrap();
        let result = RecoveryStorageHost::capture(fixture.root.share(), &mut work);
        if accepted {
            assert!(result.is_ok());
            assert_eq!(work.usage().retained_open_handles(), 1);
            assert_eq!(work.usage().peak_open_handles(), peak);
        } else {
            assert!(matches!(result, Err(RecoveryStorageError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: actual }
            )) if actual == maximum));
            assert_eq!(work.usage().retained_open_handles(), 0);
            assert_eq!(work.usage().retained_candidate_bytes(), 0);
            assert!(work.usage().decode_work() > 0);
        }
    }
}

#[test]
fn preparation_returns_incumbents_after_late_refusal_and_retries_the_same_shared_body() {
    for shared_elsewhere in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.directory.join(RECOVERY_PATH), b"external occupant").unwrap();
        let mut work = budget();
        work.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
        let mut host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
        let mut recovery = fixture.package(&mut work).into_shared(&mut work).unwrap();
        let _other_owner = shared_elsewhere.then(|| Arc::clone(&recovery));
        let original = Arc::as_ptr(&recovery);
        let digest = recovery.digest();
        let ready_identity = host.test_ready_observation().0.identity();
        let mut path = ProjectPath::new(RECOVERY_PATH).unwrap();
        let original_path = path.as_str().as_ptr();
        let before = work.usage();
        for _ in 0..2 {
            let performed = work.usage().decode_work();
            let Err(refusal) = host.prepare(recovery, path, &mut work) else {
                panic!("an occupied recovery name must refuse after preparing Pending bytes");
            };
            assert!(matches!(
                &refusal.error,
                RecoveryStorageError::RecoveryAlreadyExists
            ));
            assert_eq!(Arc::as_ptr(&refusal.recovery), original);
            assert_eq!(
                Arc::strong_count(&refusal.recovery),
                1 + usize::from(shared_elsewhere)
            );
            assert_eq!(refusal.recovery.digest(), digest);
            assert_eq!(
                refusal.host.test_ready_observation().0.identity(),
                ready_identity
            );
            assert_eq!(refusal.host.test_ready_observation().1, fixture.manifest);
            assert_eq!(refusal.path.as_str().as_ptr(), original_path);
            assert_eq!(
                work.usage().retained_candidate_bytes(),
                before.retained_candidate_bytes()
            );
            assert_eq!(
                work.usage().retained_open_handles(),
                before.retained_open_handles()
            );
            assert!(work.usage().decode_work() > performed);
            host = refusal.host;
            recovery = refusal.recovery;
            path = refusal.path;
        }
        fs::remove_file(fixture.directory.join(RECOVERY_PATH)).unwrap();
        let prepared = host.prepare(recovery, path, &mut work).unwrap();
        assert_eq!(std::ptr::from_ref(prepared.recovery()), original);
        assert_eq!(prepared.recovery().digest(), digest);
        assert_eq!(prepared.test_ready_identity(), ready_identity);
        assert_eq!(
            work.usage().retained_open_handles(),
            before.retained_open_handles() + 2
        );
        fixture.assert_original_ready();
    }
}

#[test]
fn preparation_capacity_refusals_return_live_inputs_without_refunding_their_storage() {
    for handles in [false, true] {
        let fixture = Fixture::new();
        let mut work = budget();
        work.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
        let mut host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
        let mut recovery = fixture.package(&mut work).into_shared(&mut work).unwrap();
        let mut path = ProjectPath::new(RECOVERY_PATH).unwrap();
        let before = work.usage();
        let original = Arc::as_ptr(&recovery);
        let byte_limit = work.limits.retained_candidate_bytes;
        let handle_limit = work.limits.peak_open_handles;
        if handles {
            work.limits.peak_open_handles =
                ItemLimit::new(before.retained_open_handles() + HANDLE_PEAK + 2 - 1).unwrap();
        } else {
            work.limits.retained_candidate_bytes = nara_core::ByteLimit::new(
                before.retained_candidate_bytes() + RECOVERY_PATH.len() - 1,
            )
            .unwrap();
        }
        for _ in 0..2 {
            let performed = work.usage().decode_work();
            let Err(refusal) = host.prepare(recovery, path, &mut work) else {
                panic!("the selected preparation capacity must refuse");
            };
            if handles {
                assert!(matches!(
                    &refusal.error,
                    RecoveryStorageError::Admission(
                        AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
                    )
                ));
            } else {
                assert!(matches!(
                    &refusal.error,
                    RecoveryStorageError::Admission(
                        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
                    )
                ));
            }
            assert_eq!(Arc::as_ptr(&refusal.recovery), original);
            assert_eq!(
                work.usage().retained_candidate_bytes(),
                before.retained_candidate_bytes()
            );
            assert_eq!(
                work.usage().retained_open_handles(),
                before.retained_open_handles()
            );
            assert!(work.usage().decode_work() > performed);
            host = refusal.host;
            recovery = refusal.recovery;
            path = refusal.path;
        }
        work.limits.retained_candidate_bytes = byte_limit;
        work.limits.peak_open_handles = handle_limit;
        let prepared = host.prepare(recovery, path, &mut work).unwrap();
        assert_eq!(std::ptr::from_ref(prepared.recovery()), original);
        fixture.assert_original_ready();
    }
}

#[test]
fn pending_binding_refusal_preserves_stored_custody_and_releases_only_its_new_publisher() {
    let fixture = Fixture::new();
    let mut work = budget();
    work.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let stored = fixture.stored(&mut work);
    let before = work.usage();
    let original = stored.capability().identity();
    let handle_limit = work.limits.peak_open_handles;
    work.limits.peak_open_handles =
        ItemLimit::new(before.retained_open_handles() + HANDLE_PEAK + 2 - 1).unwrap();
    let Err(refusal) = stored.bind_pending(&mut work) else {
        panic!("the new Pending publisher must exceed the selected handle capacity");
    };
    assert!(matches!(
        &refusal.error,
        RecoveryStorageError::Admission(
            AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
        )
    ));
    assert_eq!(refusal.stored.capability().identity(), original);
    assert_eq!(
        work.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(work.usage().decode_work() > before.decode_work());
    work.limits.peak_open_handles = handle_limit;
    let bound = refusal.stored.bind_pending(&mut work).unwrap();
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles() + 2
    );
    let pending = pending(bound.publish(&mut work));
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles()
    );
    pending.observed().validate(&mut work).unwrap();
    fixture.assert_original_sources();
}

#[test]
fn explicit_recovery_parent_must_exist_with_its_exact_spelling() {
    let fixture = Fixture::new();
    for path in ["missing/recovery.json", ".NARA/recovery.json"] {
        let mut work = budget();
        let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
        let recovery = fixture.package(&mut work);
        assert!(matches!(
            host.prepare(
                recovery.into_shared(&mut work).unwrap(),
                ProjectPath::new(path).unwrap(),
                &mut work
            ),
            Err(RecoveryStoragePrepareFailure {
                error: RecoveryStorageError::Filesystem {
                    kind: RecoveryStorageSource::Recovery,
                    ..
                },
                ..
            })
        ));
        fixture.assert_original_ready();
    }
    assert!(!fixture.directory.join("missing").exists());
    assert!(!fixture.directory.join(RECOVERY_PATH).exists());
}

#[test]
fn recovery_location_cannot_overlap_owned_files_roots_or_negative_sidecars() {
    let fixture = Fixture::new();
    for path in [
        "NARA.toml",
        "schema/BUNDLE.json",
        "schema",
        "SCENES/level.scene.json.meta",
        "scenes/recovery.json",
        "assets/recovery.json",
    ] {
        let mut work = budget();
        let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
        let recovery = fixture.package(&mut work);
        assert!(
            matches!(
                host.prepare(
                    recovery.into_shared(&mut work).unwrap(),
                    ProjectPath::new(path).unwrap(),
                    &mut work
                ),
                Err(RecoveryStoragePrepareFailure {
                    error: RecoveryStorageError::RecoveryPathConflict,
                    ..
                })
            ),
            "{path}"
        );
    }
    fixture.assert_original_ready();
    assert!(!fixture.directory.join(RECOVERY_PATH).exists());
}

#[test]
fn externally_edited_ready_is_refused_before_package_or_pending_writes() {
    {
        let fixture = Fixture::new();
        let mut work = budget();
        let prepared = fixture.prepare(&mut work);
        let mut external = fixture.manifest.clone();
        external.extend_from_slice(b"\n# Independent Editor save\n");
        fs::write(fixture.directory.join(PROJECT_MANIFEST_PATH), &external).unwrap();
        assert!(matches!(
            prepared.publish(&mut work),
            RecoveryStoragePublication::Refused {
                error: RecoveryStorageError::SourceChanged(RecoveryStorageSource::Manifest),
                ..
            }
        ));
        assert!(!fixture.directory.join(RECOVERY_PATH).exists());
        let host = RecoveryStorageHost::capture(fixture.root.share(), &mut work).unwrap();
        assert!(matches!(
            host.prepare(
                fixture.package(&mut work).into_shared(&mut work).unwrap(),
                ProjectPath::new(RECOVERY_PATH).unwrap(),
                &mut work
            ),
            Err(RecoveryStoragePrepareFailure {
                error: RecoveryStorageError::OriginalManifestMismatch,
                ..
            })
        ));
        assert_eq!(
            fs::read(fixture.directory.join(PROJECT_MANIFEST_PATH)).unwrap(),
            external
        );
        fixture.assert_original_sources();
    }
    {
        let fixture = Fixture::new();
        let mut work = budget();
        let stored = fixture.stored(&mut work);
        let mut external = fixture.manifest.clone();
        external.extend_from_slice(b"\n# Independent Editor save\n");
        fs::write(fixture.directory.join(PROJECT_MANIFEST_PATH), &external).unwrap();
        let Err(refusal) = stored.bind_pending(&mut work) else {
            panic!("external Ready edit must refuse Pending binding");
        };
        assert!(matches!(
            refusal.error,
            RecoveryStorageError::SourceChanged(RecoveryStorageSource::Manifest)
        ));
        assert_eq!(
            refusal.stored.recovery().encoded_bytes(),
            fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap()
        );
        assert_eq!(
            fs::read(fixture.directory.join(PROJECT_MANIFEST_PATH)).unwrap(),
            external
        );
        fixture.assert_original_sources();
    }
}

#[test]
fn late_ready_edit_or_same_byte_replacement_refuses_bound_pending_publication() {
    for change in [0, 1, 2] {
        let fixture = Fixture::new();
        let mut work = budget();
        let bound = fixture.stored(&mut work).bind_pending(&mut work).unwrap();
        let mut external = fixture.manifest.clone();
        if change == 1 {
            fixture.replace_manifest(&external);
        } else {
            if change == 2 {
                // Exercise an actually grown leaf without manufacturing a race between owner
                // checks. Verification's read bound belongs to FilePublicationHost's tests.
                external.resize(
                    usize::try_from(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT).unwrap(),
                    b' ',
                );
            } else {
                external.extend_from_slice(b"\n# External Ready bytes\n");
            }
            fs::write(fixture.directory.join(PROJECT_MANIFEST_PATH), &external).unwrap();
        }
        let RecoveryPendingPublication::Refused {
            pending,
            error: RecoveryStorageError::SourceChanged(RecoveryStorageSource::Manifest),
        } = bound.publish(&mut work)
        else {
            panic!(
                "a bound Pending publisher cannot overwrite another manifest incarnation or content"
            );
        };
        assert!(!pending.uncertain());
        assert_eq!(
            fs::read(fixture.directory.join(PROJECT_MANIFEST_PATH)).unwrap(),
            external
        );
        fixture.assert_original_sources();
    }
}

#[test]
fn changed_stored_package_cannot_authorize_pending() {
    let fixture = Fixture::new();
    let mut work = budget();
    let stored = fixture.stored(&mut work);
    let mut external = fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap();
    external[0] ^= 1;
    fs::write(fixture.directory.join(RECOVERY_PATH), &external).unwrap();
    let Err(refusal) = stored.bind_pending(&mut work) else {
        panic!("a changed package cannot authorize Pending");
    };
    assert!(matches!(
        refusal.error,
        RecoveryStorageError::SourceChanged(RecoveryStorageSource::Recovery)
    ));
    assert_eq!(
        fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap(),
        external
    );
    fixture.assert_original_ready();
}

#[test]
fn package_write_failure_preserves_ready_and_keeps_cleanup_custody() {
    let fixture = Fixture::new();
    let mut work = budget();
    let mut prepared = fixture.prepare(&mut work);
    let cleanup = prepared.test_fail_after_write();
    let RecoveryStoragePublication::Failed {
        mut prepared,
        stage: FilePublicationFailureStage::WriteTemporary,
    } = prepared.publish(&mut work)
    else {
        panic!("the injected package write must fail");
    };
    assert!(prepared.cleanup_pending());
    assert!(!prepared.retry_cleanup());
    fixture.assert_original_ready();
    assert!(!fixture.directory.join(RECOVERY_PATH).exists());
    cleanup.release();
    assert!(prepared.retry_cleanup());
    assert!(!prepared.cleanup_pending());
}

#[test]
fn pending_write_failure_preserves_ready_and_the_confirmed_recovery() {
    let fixture = Fixture::new();
    let mut work = budget();
    let mut bound = fixture.stored(&mut work).bind_pending(&mut work).unwrap();
    let cleanup = bound.test_fail_after_write();
    let RecoveryPendingPublication::Failed {
        mut pending,
        stage: FilePublicationFailureStage::WriteTemporary,
    } = bound.publish(&mut work)
    else {
        panic!("the injected Pending write must fail");
    };
    assert!(pending.cleanup_pending());
    assert!(!pending.retry_cleanup());
    fixture.assert_original_ready();
    assert_eq!(
        pending.recovery_digest(),
        ContentDigest::of_bytes(&fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap())
    );
    cleanup.release();
    assert!(pending.retry_cleanup());
}

#[test]
fn fresh_pending_read_refuses_missing_or_digest_mismatched_recovery_specifically() {
    for corrupt in [false, true] {
        let fixture = Fixture::new();
        drop(fixture.pending(&mut budget()));
        if corrupt {
            let mut bytes = fs::read(fixture.directory.join(RECOVERY_PATH)).unwrap();
            bytes[0] ^= 1;
            fs::write(fixture.directory.join(RECOVERY_PATH), bytes).unwrap();
        } else {
            fs::remove_file(fixture.directory.join(RECOVERY_PATH)).unwrap();
        }
        let mut work = budget();
        // Keep an enclosing owner's reservation alive across every failed read.
        work.charge_source_bytes(0, 0, 0, 128).unwrap();
        let retained_before = work.usage().retained_candidate_bytes();
        for _ in 0..3 {
            let before = work.usage();
            let actual = load_pending(
                host_fs::writable_directory(&fixture.directory).unwrap(),
                SourceUpgradeRecoveryLimits::default(),
                &mut work,
            );
            if corrupt {
                assert!(matches!(
                    actual,
                    Err(SourceUpgradeStorageError::Storage(
                        RecoveryStorageError::PackageDigestMismatch
                    ))
                ));
            } else {
                assert!(matches!(
                    actual,
                    Err(SourceUpgradeStorageError::Storage(
                        RecoveryStorageError::PackageMissing
                    ))
                ));
            }
            assert_eq!(work.usage().retained_candidate_bytes(), retained_before);
            assert!(work.usage().decode_work() > before.decode_work());
            assert!(work.usage().aggregate_encoded_bytes() > before.aggregate_encoded_bytes());
        }
        assert!(matches!(
            fixture.manifest().authoring.state,
            ProjectAuthoringState::Pending(_)
        ));
        fixture.assert_original_sources();
    }
}

#[test]
fn fresh_read_uses_the_actual_reference_and_refuses_changed_configuration() {
    for change in [0, 1, 2, 3] {
        let fixture = Fixture::new();
        drop(fixture.pending(&mut budget()));
        let mut manifest = fixture.manifest();
        match change {
            0 => {
                let ProjectAuthoringState::Pending(reference) = manifest.authoring.state else {
                    unreachable!()
                };
                manifest.authoring.state = ProjectAuthoringState::Pending(
                    ProjectAuthoringRecovery::new(
                        ProjectPath::new(".nara/other.json").unwrap(),
                        reference.recovery_digest(),
                    )
                    .unwrap(),
                );
                // The original valid package is not a fallback for the actual missing reference.
            }
            1 => {
                let ProjectAuthoringState::Pending(reference) = manifest.authoring.state else {
                    unreachable!()
                };
                manifest.authoring.state = ProjectAuthoringState::Pending(
                    ProjectAuthoringRecovery::new(
                        reference.recovery_path().clone(),
                        ProjectAuthoringRecoveryDigest::from_parts(
                            reference.recovery_digest().length(),
                            [7; 32],
                        ),
                    )
                    .unwrap(),
                );
            }
            2 => manifest.project.name = "Independent configuration edit".to_owned(),
            _ => {
                manifest.project.source_universe =
                    "ad322a15-01a2-4c04-847f-000000000001".parse().unwrap()
            }
        }
        fixture.write_manifest(&manifest);
        let mut work = budget();
        work.charge_source_bytes(0, 0, 0, 128).unwrap();
        let before = work.usage();
        let actual = load_pending(
            host_fs::writable_directory(&fixture.directory).unwrap(),
            SourceUpgradeRecoveryLimits::default(),
            &mut work,
        );
        match change {
            0 => assert!(matches!(
                actual,
                Err(SourceUpgradeStorageError::Storage(
                    RecoveryStorageError::PackageMissing
                ))
            )),
            1 => assert!(matches!(
                actual,
                Err(SourceUpgradeStorageError::Storage(
                    RecoveryStorageError::PackageDigestMismatch
                ))
            )),
            _ => assert!(matches!(
                actual,
                Err(SourceUpgradeStorageError::Storage(
                    RecoveryStorageError::ConfigurationMismatch
                ))
            )),
        }
        assert_eq!(
            work.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert!(work.usage().decode_work() > before.decode_work());
        assert_eq!(fixture.manifest(), manifest);
    }
}

#[test]
fn pending_read_admission_refusal_releases_retention_without_refunding_work() {
    let fixture = Fixture::new();
    drop(fixture.pending(&mut budget()));
    let mut work = budget();
    work.charge_source_bytes(0, 0, 0, 128).unwrap();
    work.limits.decode_work = ItemLimit::new(1).unwrap();
    let before = work.usage();
    assert!(matches!(
        load_pending(
            fixture.root.share(),
            SourceUpgradeRecoveryLimits::default(),
            &mut work,
        ),
        Err(SourceUpgradeStorageError::Storage(
            RecoveryStorageError::Admission(
                AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
            )
        ))
    ));
    assert_eq!(
        work.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles()
    );
    assert!(work.usage().decode_work() > before.decode_work());
    assert!(matches!(
        fixture.manifest().authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    fixture.assert_original_sources();
}

#[test]
fn successful_pending_read_keeps_its_observations_reserved() {
    let fixture = Fixture::new();
    drop(fixture.pending(&mut budget()));
    let mut work = budget();
    work.charge_source_bytes(0, 0, 0, 128).unwrap();
    let before = work.usage();
    let observed = load_pending(
        fixture.root.share(),
        SourceUpgradeRecoveryLimits::default(),
        &mut work,
    )
    .unwrap();
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles() + 3
    );
    assert!(
        work.usage().retained_candidate_bytes()
            >= before.retained_candidate_bytes()
                + observed.manifest_bytes().len()
                + observed.recovery().encoded_bytes().len()
                + observed.reference().recovery_path().as_str().len()
    );
    let retained = work.usage().retained_candidate_bytes();
    let decode_work = work.usage().decode_work();
    observed.validate(&mut work).unwrap();
    assert_eq!(work.usage().retained_candidate_bytes(), retained);
    assert_eq!(
        work.usage().retained_open_handles(),
        before.retained_open_handles() + 3
    );
    assert!(work.usage().decode_work() > decode_work);
    fixture.assert_original_sources();
}

#[test]
fn fresh_read_custody_refuses_a_later_same_byte_manifest_replacement() {
    let fixture = Fixture::new();
    drop(fixture.pending(&mut budget()));
    let observed = load_pending(
        fixture.root.share(),
        SourceUpgradeRecoveryLimits::default(),
        &mut budget(),
    )
    .unwrap();
    fixture.replace_manifest(observed.manifest_bytes());
    assert!(matches!(
        observed.validate(&mut budget()),
        Err(RecoveryStorageError::SourceChanged(
            RecoveryStorageSource::Manifest
        ))
    ));
    assert_eq!(
        observed
            .capability()
            .read_to_end_bounded(DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap(),
        observed.manifest_bytes()
    );
    fixture.assert_original_sources();
}

#[test]
fn exhausted_work_refuses_before_either_disk_publication() {
    let fixture = Fixture::new();
    let mut work = budget();
    let prepared = fixture.prepare(&mut work);
    work.limits.decode_work = ItemLimit::new(work.usage().decode_work()).unwrap();
    assert!(matches!(
        prepared.publish(&mut work),
        RecoveryStoragePublication::Refused {
            error: RecoveryStorageError::Admission(
                AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
            ),
            ..
        }
    ));
    fixture.assert_original_ready();
    assert!(!fixture.directory.join(RECOVERY_PATH).exists());
    let mut work = budget();
    let bound = fixture.stored(&mut work).bind_pending(&mut work).unwrap();
    work.limits.decode_work = ItemLimit::new(work.usage().decode_work()).unwrap();
    assert!(matches!(
        bound.publish(&mut work),
        RecoveryPendingPublication::Refused {
            error: RecoveryStorageError::Admission(
                AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
            ),
            ..
        }
    ));
    fixture.assert_original_ready();
    assert!(fixture.directory.join(RECOVERY_PATH).exists());
}
