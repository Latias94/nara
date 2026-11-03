//! The finite wrapper drives real temporary Projects and their exact original Workspace.

use super::*;
use crate::project_host::{
    authoring::project::prefab_extraction::session_publication::PrefabExtractionSessionPublicationPhase as Phase,
    persistence::{PrefabPersistenceHost, ScenePersistenceHost},
};

fn publication(fixture: &Fixture) -> PrefabExtractionSessionPublication {
    let mut review =
        PrefabExtractionReviewRequest::new(fixture.request(), vec![fixture.document(INACTIVE)])
            .unwrap()
            .prepare()
            .unwrap();
    review
        .validate_live(&fixture.project, &fixture.workspace)
        .unwrap();
    review.into_publication()
}

fn advance(mut owner: PrefabExtractionSessionPublication) -> PrefabExtractionSessionPublication {
    // Model the actual worker's slot/capture storage through every transition and refusal.
    // In particular native cancellation must preserve this reservation until physical return.
    owner.admit_task_storage(256).unwrap();
    let mut returned = owner.advance();
    returned.release_task_storage(256);
    returned
}

fn until(
    mut owner: PrefabExtractionSessionPublication,
    phase: Phase,
) -> PrefabExtractionSessionPublication {
    let inventory = owner.test_original_inventory();
    for _ in 0..96 {
        assert!(owner.failure().is_none(), "{:?}", owner.failure());
        assert_eq!(owner.test_original_inventory(), inventory);
        if owner.phase() == phase {
            return owner;
        }
        owner = advance(owner);
    }
    panic!("the finite owner did not reach {phase:?}")
}

fn ready_on_disk(fixture: &Fixture) -> bool {
    let manifest = fs::read_to_string(fixture.root.join("nara.toml")).unwrap();
    matches!(
        nara_project::ProjectManifest::parse_toml_str(&manifest)
            .manifest
            .unwrap()
            .authoring
            .state,
        nara_project::ProjectAuthoringState::Ready
    )
}

#[test]
fn a_memory_only_cancel_settles_even_after_the_original_host_work_is_exhausted() {
    let fixture = Fixture::new();
    let original_disk = fixture.disk_state();
    let mut owner = publication(&fixture);
    owner.test_exhaust_work();
    let error = owner.admit_installation_storage(1).unwrap_err();
    owner.record_failure(error);
    assert!(owner.host_work_exhausted());
    assert!(!owner.can_retry());
    owner.request_cancellation();
    assert!(owner.settle_cancellation());
    assert!(!owner.retry_requested());
    assert_eq!(fixture.disk_state(), original_disk);
}

#[test]
fn cancellation_keeps_the_failed_native_package_until_actual_cleanup_returns() {
    let fixture = Fixture::new();
    let mut owner = publication(&fixture);
    let original = owner.test_original_inventory();
    let control = loop {
        if let Some(control) = owner.test_fail_recovery_write() {
            break control;
        }
        owner = advance(owner);
        assert!(owner.failure().is_none(), "{:?}", owner.failure());
    };
    owner = advance(owner);
    assert!(owner.failure().is_some());
    assert!(owner.cancellation_allowed());
    owner.request_cancellation();
    assert!(!owner.settle_cancellation());
    owner = advance(owner);
    assert!(!owner.settle_cancellation());
    assert!(owner.failure().is_some());
    assert!(owner.can_retry());
    let work = owner.test_usage().decode_work();
    control.release();
    assert!(owner.request_retry());
    owner = advance(owner);
    assert!(owner.settle_cancellation(), "{:?}", owner.failure());
    assert_eq!(owner.test_original_inventory(), original);
    assert!(owner.test_usage().decode_work() > work);
    assert!(ready_on_disk(&fixture));
    assert!(!fixture.root.join(Q).exists());
}

#[test]
fn an_unstarted_pending_task_and_a_real_pending_write_refusal_retry_the_exact_owner() {
    let fixture = Fixture::new();
    let mut owner = until(publication(&fixture), Phase::Pending);
    let original = owner.test_original_inventory();
    let work = owner.test_usage().decode_work();
    assert!(owner.pending_attempt_is_next());
    assert!(owner.cancellation_allowed());
    owner.defer_retry();
    assert!(owner.failure().is_some());
    assert!(owner.can_retry());
    assert!(owner.request_retry());
    // A Task rejection has performed no Pending attempt. Inject the actual publisher failure
    // into that same owner; the later return must keep its pre-dispatch cancellation fence.
    let control = owner.test_fail_pending_write();
    owner = advance(owner);
    assert!(owner.failure().is_some());
    assert!(!owner.cancellation_allowed());
    assert!(ready_on_disk(&fixture));
    assert_eq!(owner.test_original_inventory(), original);
    assert!(owner.test_usage().decode_work() > work);
    control.release();
    assert!(owner.request_retry());
    owner = advance(owner);
    assert!(owner.failure().is_none(), "{:?}", owner.failure());
    assert!(!owner.cancellation_allowed());
    assert!(!ready_on_disk(&fixture));
    let owner = until(owner, Phase::Ready);
    assert_eq!(owner.test_original_inventory(), original);
    assert!(fixture.root.join(Q).is_file());
}

enum Host {
    Scene(EditorDocumentId, String, ScenePersistenceHost),
    Prefab(EditorDocumentId, String, PrefabPersistenceHost),
}

impl Host {
    fn document(&self) -> EditorDocumentId {
        match self {
            Self::Scene(document, ..) | Self::Prefab(document, ..) => *document,
        }
    }

    fn accepted(&self) -> (FileIdentity, Option<FileIdentity>, ContentDigest) {
        match self {
            Self::Scene(_, _, host) => host.accepted_source_observation().unwrap(),
            Self::Prefab(_, _, host) => host.accepted_source_observation().unwrap(),
        }
    }

    fn destination(&mut self) -> PrefabExtractionPersistenceDestination<'_> {
        match self {
            Self::Scene(document, project_path, host) => {
                PrefabExtractionPersistenceDestination::Scene {
                    document: *document,
                    project_path,
                    host,
                }
            }
            Self::Prefab(document, project_path, host) => {
                PrefabExtractionPersistenceDestination::Prefab {
                    document: *document,
                    project_path,
                    host,
                }
            }
        }
    }
}

fn original_hosts(fixture: &Fixture) -> Vec<Host> {
    let root = fixture.project.project_root();
    let mut hosts: Vec<_> = fixture
        .open
        .iter()
        .map(|(document, source, _)| {
            let file = root
                .open_file_exact(&RelativePath::new(source.relative_path()).unwrap())
                .unwrap();
            let binding = fixture
                .workspace
                .document_source_binding(*document)
                .unwrap();
            match source.source_key().kind() {
                SourceDocumentKind::Scene => Host::Scene(
                    *document,
                    source.relative_path().to_owned(),
                    ScenePersistenceHost::bind_authoring_source(
                        &root,
                        binding.locator(),
                        file,
                        source.digest(),
                    )
                    .unwrap()
                    .0,
                ),
                SourceDocumentKind::Prefab => Host::Prefab(
                    *document,
                    source.relative_path().to_owned(),
                    PrefabPersistenceHost::bind_authoring_source(
                        &root,
                        binding.clone(),
                        file,
                        source.digest(),
                    )
                    .unwrap()
                    .0,
                ),
            }
        })
        .collect();
    hosts.sort_by_key(Host::document);
    hosts
}

#[test]
fn the_all_bound_install_restores_on_refusal_then_moves_prepared_facts_with_the_same_basis() {
    let mut fixture = Fixture::new();
    let mut hosts = original_hosts(&fixture);
    let basis = fixture.project.startup_scene.schema_basis();
    let open = fixture.workspace.len();
    let active = fixture.workspace.active_document();
    let inactive = fixture.document(INACTIVE);
    let current = fixture.workspace.scene(inactive).unwrap().session();
    let unchanged_history = (
        current.revision(),
        current.content_state(),
        current.history_status(),
    );
    let mut owner = until(publication(&fixture), Phase::Ready);
    let original = owner.test_original_inventory();
    assert!(
        owner
            .installation_documents()
            .map(|(document, _)| document)
            .eq(hosts.iter().map(Host::document))
    );
    for ((_, source), host) in owner.installation_documents().zip(&hosts) {
        assert_eq!(
            (source.identity(), source.parent_identity(), source.digest()),
            host.accepted()
        );
    }
    assert!(
        owner
            .install(&fixture.authority, &mut fixture.workspace, &mut [])
            .is_err()
    );
    assert!(owner.failure().is_some());
    assert!(!ready_on_disk(&fixture));
    assert!(owner.request_retry());
    owner = advance(owner);
    assert!(owner.failure().is_none(), "{:?}", owner.failure());
    assert_eq!(owner.test_original_inventory(), original);
    let scratch = hosts.len() * size_of::<PrefabExtractionPersistenceDestination<'_>>();
    owner.admit_installation_storage(scratch).unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let installed = owner
        .install(
            &fixture.authority,
            &mut fixture.workspace,
            &mut destinations,
        )
        .unwrap();
    drop(destinations);
    // The completed facade still owns the actual context, so success has the same physical
    // binding-storage refund path as every refused installation and worker stage.
    owner.release_installation_storage(scratch);
    assert!(ready_on_disk(&fixture));
    assert_eq!(installed.session.startup_scene.schema_basis(), basis);
    assert_eq!(fixture.workspace.len(), open);
    assert_eq!(fixture.workspace.active_document(), active);
    assert!(
        fixture
            .workspace
            .prefab(fixture.document(GUARD))
            .unwrap()
            .is_dirty()
    );
    let current = fixture.workspace.scene(inactive).unwrap().session();
    assert_eq!(
        (
            current.revision(),
            current.content_state(),
            current.history_status()
        ),
        unchanged_history
    );
    for ((document, source), host) in installed.sources.iter().zip(&hosts) {
        assert_eq!(*document, host.document());
        assert_eq!(
            (source.identity(), source.parent_identity(), source.digest()),
            host.accepted()
        );
    }
    assert!(
        installed
            .receipts
            .iter()
            .any(|(_, receipt)| receipt.is_some())
    );
    assert_eq!(owner.test_original_inventory(), original);
}
