use super::super::super::source_upgrade_adoption::{
    PreparedSourceUpgradeProject, SourceUpgradeProjectError, SourceUpgradeProjectOutcome,
};
use super::*;
use crate::project_host::{
    authoring::{
        source_upgrade_inventory::VerifiedSourceUpgradeInventory,
        source_upgrade_ready::{PreparedSourceUpgradeReady, SourceUpgradePendingManifest},
        source_upgrade_storage::{
            PendingRecoveryStorage, RecoveryPendingPublication, RecoveryStorageHost,
            RecoveryStoragePublication,
        },
    },
    file_publication::{FilePublicationHost, FilePublicationOutcome},
};

fn enter_pending(
    fixture: &Fixture,
    recovery: SourceUpgradeRecovery,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> PendingRecoveryStorage {
    fs::create_dir_all(fixture.root.join(".nara/upgrades")).unwrap();
    let host = RecoveryStorageHost::capture(fixture.schema.project_root(), budget).unwrap();
    let prepared = host
        .prepare(
            recovery.into_shared(budget).unwrap(),
            ProjectPath::new(".nara/upgrades/recovery.json").unwrap(),
            budget,
        )
        .unwrap();
    let stored = match prepared.publish(budget) {
        RecoveryStoragePublication::Published(stored) => stored,
        _ => panic!("recovery package publication did not succeed"),
    };
    match stored.bind_pending(budget).unwrap().publish(budget) {
        RecoveryPendingPublication::Published(pending) => pending,
        _ => panic!("Pending publication did not succeed"),
    }
}

fn publish_source(root: &DirectoryCapability, path: &str, old: &[u8], next: &[u8]) {
    if old == next {
        return;
    }
    let file = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let mut publisher = FilePublicationHost::bind_existing(
        root,
        path,
        &file,
        ContentDigest::of_bytes(old),
        SceneFileLimits::default().encoded_bytes().get() as u64,
    )
    .unwrap();
    assert!(matches!(
        publisher.publish(next),
        FilePublicationOutcome::Published(_)
    ));
}

fn published_sources(fixture: &Fixture, pending: &PendingRecoveryStorage) {
    let root = fixture.schema.project_root();
    for member in pending.observed().recovery().members() {
        publish_source(
            &root,
            &member.path,
            member.old_disk.bytes(),
            member.successor.bytes(),
        );
    }
    let bundle = pending.observed().recovery().bundle();
    publish_source(
        &root,
        &bundle.path,
        bundle.old_disk.bytes(),
        bundle.successor.bytes(),
    );
}

fn verified_project_sources(
    fixture: &Fixture,
    pending: &PendingRecoveryStorage,
    ready: &PreparedSourceUpgradeReady,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> VerifiedSourceUpgradeInventory {
    let mut facts = Vec::new();
    for (_, source) in &fixture.open {
        let file = fixture
            .schema
            .project_root()
            .open_file_exact(&RelativePath::new(source.relative_path()).unwrap())
            .unwrap();
        let digest = file
            .digest(SceneFileLimits::default().encoded_bytes().get() as u64)
            .unwrap();
        facts.push(
            source
                .with_persistence_observation(file.identity(), file.parent_identity(), digest)
                .unwrap(),
        );
    }
    let open: Vec<_> = facts.iter().map(SourceUpgradeOpenSource::Bound).collect();
    VerifiedSourceUpgradeInventory::capture(
        ready.observed_pending_schema(),
        pending.observed().recovery(),
        &open,
        SceneFileLimits::default(),
        budget,
    )
    .unwrap()
}

fn prepare_project(
    fixture: &Fixture,
    pending: &PendingRecoveryStorage,
    ready: &PreparedSourceUpgradeReady,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> PreparedSourceUpgradeProject {
    let verified = verified_project_sources(fixture, pending, ready, budget);
    PreparedSourceUpgradeProject::prepare(
        ready,
        verified,
        pending.observed().shared_recovery(),
        SceneFileLimits::default(),
        budget,
    )
    .unwrap()
}

#[test]
fn refused_project_preparation_returns_existing_custody_and_releases_only_attempt_storage() {
    let fixture = Fixture::new();
    let recovery = fixture.recovery(false, false);
    let mut budget = budget();
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    let pending = enter_pending(&fixture, recovery, &mut budget);
    published_sources(&fixture, &pending);
    let ready = PreparedSourceUpgradeReady::prepare(
        selected,
        pending.observed().recovery(),
        SourceUpgradePendingManifest {
            capability: pending.capability(),
            digest: pending.digest(),
        },
        &mut budget,
    )
    .unwrap();
    let mut verified = verified_project_sources(&fixture, &pending, &ready, &mut budget);
    let recovery = pending.observed().shared_recovery();
    let original_leaf = verified.inventory().shared_file(PATHS[0]).unwrap();
    let original_encoded = recovery.encoded_bytes().as_ptr();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    let retained_limit = budget.limits.retained_candidate_bytes;

    for refuse_at_entry in [true, false] {
        let work_before = budget.usage().decode_work();
        let scene_limits = if refuse_at_entry {
            budget.limits.retained_candidate_bytes = nara_core::ByteLimit::new(retained).unwrap();
            SceneFileLimits::default()
        } else {
            budget.limits.retained_candidate_bytes = retained_limit;
            SceneFileLimits::default().with_encoded_bytes(nara_core::ByteLimit::new(1).unwrap())
        };
        let failure = match PreparedSourceUpgradeProject::prepare(
            &ready,
            verified,
            Arc::clone(&recovery),
            scene_limits,
            &mut budget,
        ) {
            Ok(_) => panic!("the constrained project preparation must refuse"),
            Err(failure) => failure,
        };
        if refuse_at_entry {
            assert!(matches!(failure.error, SourceUpgradeProjectError::Ready(_)));
        } else {
            assert!(matches!(
                failure.error,
                SourceUpgradeProjectError::Project(_)
            ));
            assert!(budget.usage().decode_work() > work_before);
        }
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(Arc::ptr_eq(&failure.recovery, &recovery));
        assert_eq!(failure.recovery.encoded_bytes().as_ptr(), original_encoded);
        let returned_leaf = failure.verified.inventory().shared_file(PATHS[0]).unwrap();
        assert!(Arc::ptr_eq(&returned_leaf, &original_leaf));
        let original_digest = failure
            .verified
            .participants()
            .find(|source| source.file().path() == PATHS[0])
            .unwrap()
            .file()
            .digest();
        original_leaf
            .verify_digest(
                original_digest,
                SceneFileLimits::default().encoded_bytes().get() as u64,
            )
            .unwrap();
        verified = failure.verified;
    }

    budget.limits.retained_candidate_bytes = retained_limit;
    let prepared = PreparedSourceUpgradeProject::prepare(
        &ready,
        verified,
        recovery,
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    prepared.validate(&mut budget).unwrap();
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.schema.project_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn final_ready_installs_the_prepared_project_without_readmission_and_rebinds_all_manifest_facts() {
    let fixture = Fixture::new();
    let recovery = fixture.recovery(false, false);
    let mut budget = budget();
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    let basis = selected.basis().clone();
    let pending = enter_pending(&fixture, recovery, &mut budget);
    published_sources(&fixture, &pending);
    let ready = PreparedSourceUpgradeReady::prepare(
        selected,
        pending.observed().recovery(),
        SourceUpgradePendingManifest {
            capability: pending.capability(),
            digest: pending.digest(),
        },
        &mut budget,
    )
    .unwrap();
    let verified = verified_project_sources(&fixture, &pending, &ready, &mut budget);
    let leaf = verified.inventory().shared_file(PATHS[0]).unwrap();
    let retained_leaf = Arc::downgrade(&leaf);
    let leaf_identity = leaf.identity();
    drop(leaf);
    let closed_leaf = verified.inventory().shared_file(PATHS[3]).unwrap();
    let retained_closed_leaf = Arc::downgrade(&closed_leaf);
    drop(closed_leaf);
    let project = PreparedSourceUpgradeProject::prepare(
        &ready,
        verified,
        pending.observed().shared_recovery(),
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let mut previous_work = budget.usage().decode_work();
    for _ in 0..3 {
        project.validate(&mut budget).unwrap();
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert!(budget.usage().decode_work() > previous_work);
        previous_work = budget.usage().decode_work();
    }
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.schema.project_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
    let bound = match project.bind(ready.bind(&mut budget).unwrap(), &mut budget) {
        Ok(bound) => bound,
        Err(failure) => panic!("project binding failed: {:?}", failure.error),
    };
    let retained_before_publication = budget.usage().retained_candidate_bytes();
    let handles_before_publication = budget.usage().retained_open_handles();
    let published = match bound.publish(&mut budget) {
        Ok(SourceUpgradeProjectOutcome::Published(published)) => published,
        Err(refusal) => panic!("project publication refused: {:?}", refusal.error),
        _ => panic!("final Ready did not publish"),
    };
    assert!(budget.usage().retained_candidate_bytes() < retained_before_publication);
    // The old bound publisher, private recovery read and last Pending Schema observation
    // have closed. The transferred inventory's native pins continue to be charged.
    assert_eq!(
        budget.usage().retained_open_handles(),
        handles_before_publication - 4
    );
    // The operation still owns its old inventory/recovery. The installed generation's live
    // counters exclude those owners while keeping the performed work and peak evidence.
    assert!(
        published.session.usage.retained_candidate_bytes()
            < budget.usage().retained_candidate_bytes()
    );
    assert!(
        published.session.usage.retained_open_handles() < budget.usage().retained_open_handles()
    );
    assert_eq!(
        published.session.usage.decode_work(),
        budget.usage().decode_work()
    );
    assert_eq!(
        published.session.usage.peak_open_handles(),
        budget.usage().peak_open_handles()
    );
    assert_eq!(published.session.schema.limits(), fixture.schema.limits());
    assert_eq!(published.session.schema.usage().decode_work(), 0);
    let ordinary = published.session.admit_source_capture_inputs(0, 0).unwrap();
    assert_eq!(ordinary.budget.limits, fixture.schema.limits());
    assert_eq!(ordinary.budget.usage().decode_work(), 0);
    assert!(
        ordinary.budget.usage().retained_candidate_bytes()
            > published.session.schema.usage().retained_candidate_bytes()
    );
    assert!(
        ordinary.budget.usage().retained_open_handles()
            > published.session.schema.usage().retained_open_handles()
    );
    let next = published
        .session
        .admit_authored_upgrade_inputs(
            0,
            0,
            ItemLimit::new(1).unwrap(),
            ItemLimit::new(64).unwrap(),
        )
        .unwrap();
    assert_eq!(next.budget.usage().decode_work(), 0);
    assert_eq!(
        next.budget.usage().retained_candidate_bytes(),
        ordinary.budget.usage().retained_candidate_bytes()
    );
    assert_eq!(
        next.budget.usage().retained_open_handles(),
        ordinary.budget.usage().retained_open_handles()
    );
    assert_eq!(retained_leaf.upgrade().unwrap().identity(), leaf_identity);
    assert!(retained_closed_leaf.upgrade().is_some());
    assert_eq!(published.session.startup_scene.schema_basis(), basis);
    assert!(matches!(
        published.session.settings().authoring_state,
        nara_project::ProjectAuthoringState::Ready
    ));
    published
        .session
        .startup_source_validation()
        .validate()
        .unwrap();
    assert!(published.session.source_facts.iter().all(|source| {
        source.kind() != AuthoringSourceKind::ProjectManifest
            && source.kind() != AuthoringSourceKind::SchemaBundle
    }));
    let manifest = published
        .session
        .project_root()
        .open_file_exact(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    assert_eq!(
        manifest.identity(),
        published.session.schema.manifest_identity()
    );
    assert_eq!(
        manifest
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap(),
        published.session.schema.manifest_digest()
    );
    assert_eq!(
        published.publication.receipt().previous_identity(),
        Some(pending.capability().identity())
    );
    let actual = published.session.admit_startup_document().unwrap();
    match actual {
        super::super::super::super::document::AdmittedAuthoringDocument::Scene {
            document, ..
        } => {
            assert_eq!(document.schema_basis(), basis);
            assert_eq!(
                document.document().entities[0].components[&ComponentTypeId::new(COMPONENT)]
                    .value
                    .field("wire_health")
                    .unwrap()
                    .field_i64("milli")
                    .unwrap(),
                109000
            );
        }
        _ => panic!("startup kind changed"),
    }
    // Ordinary validation observations may outlive the Session. Their actual native lifetime
    // must survive that move, while the full verified wire corpus has already been discarded.
    let validation = published.session.startup_source_validation();
    drop(published);
    assert!(retained_leaf.upgrade().is_some());
    assert!(retained_closed_leaf.upgrade().is_some());
    drop(validation);
    assert!(retained_leaf.upgrade().is_none());
    assert!(retained_closed_leaf.upgrade().is_none());
}

#[test]
fn changed_source_after_project_preparation_refuses_before_ready() {
    let fixture = Fixture::new();
    let recovery = fixture.recovery(false, false);
    let mut budget = budget();
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    let pending = enter_pending(&fixture, recovery, &mut budget);
    published_sources(&fixture, &pending);
    let ready = PreparedSourceUpgradeReady::prepare(
        selected,
        pending.observed().recovery(),
        SourceUpgradePendingManifest {
            capability: pending.capability(),
            digest: pending.digest(),
        },
        &mut budget,
    )
    .unwrap();
    let project = prepare_project(&fixture, &pending, &ready, &mut budget);
    fs::write(
        fixture.root.join(PATHS[0]),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let ready = ready.bind(&mut budget).unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let failure = match project.bind(ready, &mut budget) {
        Err(failure) => failure,
        Ok(_) => panic!("an external source edit escaped final project binding"),
    };
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert!(matches!(
        failure.error,
        super::super::super::source_upgrade_adoption::SourceUpgradeProjectError::Inventory(_)
    ));
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.schema.project_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn project_adapter_and_index_copies_include_owned_owner_coverage_and_path_storage() {
    let fixture = Fixture::new();
    let adapters =
        super::super::super::super::project_authoring_adapters(fixture.schema.bundle()).unwrap();
    let actual = adapter_receipts_retained_bytes(&adapters).unwrap();
    let inline_and_components: usize = adapters
        .iter()
        .map(|adapter| {
            size_of::<AuthoringAdapterReceipt>()
                + adapter
                    .components()
                    .map(|component| size_of::<ComponentTypeId>() + component.as_str().len())
                    .sum::<usize>()
        })
        .sum();
    assert!(actual >= inline_and_components + "trial.enemy".len() + 6 * size_of::<usize>());
    let settings = fixture.schema.settings();
    let paths: usize = [
        fixture.schema.bundle_path().as_str(),
        settings.paths.assets.as_str(),
        settings.paths.scenes.as_str(),
        settings.paths.prefabs.as_str(),
        settings.paths.scripts.as_str(),
    ]
    .into_iter()
    .map(str::len)
    .sum();
    assert!(
        ProjectSourceIndexState::checked_retained_bytes_for_schema(&fixture.schema).unwrap()
            >= size_of::<ProjectSourceIndexState>()
                + paths
                + 4 * size_of::<(
                    Arc<str>,
                    super::super::super::super::source_index::ProjectSourceRootRole
                )>()
    );
}
