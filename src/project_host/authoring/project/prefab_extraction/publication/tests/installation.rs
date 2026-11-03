//! Actual original persistence Hosts and Workspace destinations across the final Ready boundary.

use super::future_inventory::checkpoints_complete;
use super::*;
use crate::project_host::{
    authoring::project::prefab_extraction::publication::{
        ExtractionPersistenceDestination, PrefabExtractionInstallationError,
        PreparedPrefabExtractionWorkspaceInstallation,
    },
    persistence::{PrefabPersistenceHost, ScenePersistenceHost},
};

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

    fn destination(&mut self) -> ExtractionPersistenceDestination<'_> {
        match self {
            Self::Scene(document, path, host) => ExtractionPersistenceDestination::Scene {
                document: *document,
                project_path: path,
                host,
            },
            Self::Prefab(document, path, host) => ExtractionPersistenceDestination::Prefab {
                document: *document,
                project_path: path,
                host,
            },
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
            let scoped_root = (binding.locator() != source.relative_path()).then(|| {
                root.open_directory(
                    &RelativePath::new(source.relative_path().rsplit_once('/').unwrap().0).unwrap(),
                )
                .unwrap()
            });
            let host_root = scoped_root.as_ref().unwrap_or(&root);
            match source.source_key().kind() {
                SourceDocumentKind::Scene => Host::Scene(
                    *document,
                    source.relative_path().to_owned(),
                    ScenePersistenceHost::bind_authoring_source(
                        host_root,
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
                        host_root,
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

fn prepared(fixture: &Fixture) -> PreparedPrefabExtractionWorkspaceInstallation {
    prepared_with_work(fixture, 200_000_000)
}

fn prepared_with_work(
    fixture: &Fixture,
    work: usize,
) -> PreparedPrefabExtractionWorkspaceInstallation {
    let request = fixture.request_with_host_limits(
        SceneCompilationLimits::default(),
        ItemLimit::new(128).unwrap(),
        ItemLimit::new(work).unwrap(),
    );
    checkpoints_complete(
        fixture::confirmed_request(fixture, request)
            .into_publication()
            .unwrap(),
    )
    .into_future_inventory()
    .unwrap()
    .prepare_project(&mut AuthoringProjectAdmissionHost::default())
    .unwrap()
    .bind()
    .unwrap()
    .into_workspace_installation()
    .unwrap()
}

#[test]
fn ready_publication_installs_the_actual_workspace_and_original_hosts_with_selective_history() {
    let mut fixture = Fixture::new();
    add_closed_rewrite(&fixture);
    let mut hosts = original_hosts(&fixture);
    let before = live_state(&fixture);
    let old_hosts: Vec<_> = hosts.iter().map(Host::accepted).collect();
    let old_count = fixture.workspace.len();
    let active = fixture.workspace.active_document();
    let p = fixture.document(P);
    let inactive = fixture.document(INACTIVE);
    let guard = fixture.document(GUARD);
    let current = fixture.workspace.scene(inactive).unwrap().session();
    let preserved = (
        current.revision(),
        current.content_state(),
        current.history_status(),
    );
    let mut installation = prepared(&fixture);
    let saved: Vec<_> = installation
        .persistence()
        .documents()
        .filter_map(|(document, disposition)| {
            (disposition != EditorPrefabExtractionDisposition::Guard).then_some(document)
        })
        .collect();
    let sources = installation
        .persistence()
        .sources()
        .prepared()
        .corpus()
        .sources()
        .as_ptr();
    assert!(!sources.is_null());
    let basis = installation
        .persistence()
        .sources()
        .prepared()
        .corpus()
        .schema_basis()
        .clone();
    let expected_prefab = Arc::clone(installation.persistence().saved_prefab(p).unwrap());
    installation.project_mut().validate().unwrap();
    assert_pending(&fixture);
    assert_eq!(live_state(&fixture), before);
    assert_eq!(
        hosts.iter().map(Host::accepted).collect::<Vec<_>>(),
        old_hosts
    );
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let installed = bound.publish(&mut destinations).unwrap();
    drop(destinations);
    assert!(matches!(
        installed.session.settings().authoring_state,
        ProjectAuthoringState::Ready
    ));
    assert_eq!(installed.session.startup_scene.schema_basis(), basis);
    assert_eq!(fixture.workspace.len(), old_count);
    assert_eq!(fixture.workspace.active_document(), active);
    assert!(Arc::ptr_eq(
        &fixture
            .workspace
            .prefab(p)
            .unwrap()
            .session()
            .shared_document(),
        &expected_prefab
    ));
    assert!(!fixture.workspace.prefab(p).unwrap().is_dirty());
    assert!(!fixture.workspace.scene(inactive).unwrap().is_dirty());
    assert!(fixture.workspace.prefab(guard).unwrap().is_dirty());
    let after = live_state(&fixture);
    for (index, ((document, _, _), state)) in fixture.open.iter().zip(&after).enumerate() {
        if *document == guard {
            assert_eq!(state, &before[index]);
        }
    }
    let current = fixture.workspace.scene(inactive).unwrap().session();
    assert_eq!(
        (
            current.revision(),
            current.content_state(),
            current.history_status()
        ),
        preserved
    );
    for (host, previous) in hosts.iter().zip(&old_hosts) {
        if saved.contains(&host.document()) {
            assert_ne!(host.accepted().0, previous.0);
        } else {
            assert_eq!(&host.accepted(), previous);
        }
    }
    assert!(
        installed
            .receipts
            .iter()
            .all(|(_, receipt)| receipt.is_some())
    );
    assert_eq!(installed.receipts.len(), saved.len());
    let ready = fs::read_to_string(fixture.root.join("nara.toml")).unwrap();
    assert!(matches!(
        ProjectManifest::parse_toml_bytes(ready.as_bytes())
            .manifest
            .unwrap()
            .authoring
            .state,
        ProjectAuthoringState::Ready
    ));
    // Ordinary admission independently reads the actual Ready and extracted source generation.
    let reopen = admit_project(&fixture.root);
    assert_eq!(
        reopen.settings().project.source_universe,
        fixture.project.settings().project.source_universe
    );
}

#[test]
fn a_late_original_host_refusal_restores_all_earlier_leaves_and_the_same_workspace_stage() {
    let mut fixture = Fixture::new();
    let mut hosts = original_hosts(&fixture);
    let installation = prepared(&fixture);
    let corpus = installation.persistence().sources().prepared().corpus();
    let allocation = corpus.sources().as_ptr();
    let basis = corpus.schema_basis().clone();
    let before = live_state(&fixture);
    let files = fixture.disk_state();
    let old_hosts: Vec<_> = hosts.iter().map(Host::accepted).collect();
    let last = hosts.len() - 1;
    // An actual Host for another document cannot become the unchanged guard's destination.
    let (_, source, _) = fixture
        .open
        .iter()
        .find(|(document, _, _)| *document == fixture.document(P))
        .unwrap();
    let root = fixture.project.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(P).unwrap())
        .unwrap();
    let wrong = PrefabPersistenceHost::bind_authoring_source(
        &root,
        nara_tooling::EditorDocumentSourceBinding::new(P),
        file,
        ContentDigest::of_bytes(&fs::read(fixture.root.join(P)).unwrap()),
    )
    .unwrap()
    .0;
    assert_ne!(
        wrong.accepted_source_observation().unwrap().0,
        source.identity()
    );
    let document = hosts[last].document();
    let original = std::mem::replace(
        &mut hosts[last],
        Host::Prefab(document, GUARD.to_owned(), wrong),
    );
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let refusal = match bound.publish(&mut destinations) {
        Err(refusal) => refusal,
        Ok(_) => panic!("a complete destination mismatch must refuse before Ready"),
    };
    drop(destinations);
    let (mut installation, error) = refusal.into_parts();
    assert!(matches!(
        error,
        PrefabExtractionInstallationError::Persistence(
            nara_tooling::EditorPersistenceRejection::TargetChanged
        )
    ));
    hosts[last] = original;
    assert_eq!(
        hosts.iter().map(Host::accepted).collect::<Vec<_>>(),
        old_hosts
    );
    assert_eq!(fixture.disk_state(), files);
    assert_eq!(live_state(&fixture), before);
    assert_pending(&fixture);
    let corpus = installation.persistence().sources().prepared().corpus();
    assert_eq!(corpus.sources().as_ptr(), allocation);
    assert_eq!(corpus.schema_basis(), &basis);
    installation.project_mut().validate().unwrap();
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    assert!(bound.publish(&mut destinations).is_ok());
}

#[test]
fn final_ready_write_failure_returns_all_original_destinations_before_budgeted_cleanup_and_retry() {
    let mut fixture = Fixture::new();
    let mut hosts = original_hosts(&fixture);
    let mut installation = prepared(&fixture);
    let gate = installation.project_mut().test_fail_next_write();
    let corpus = installation.persistence().sources().prepared().corpus();
    let allocation = corpus.sources().as_ptr();
    let basis = corpus.schema_basis().clone();
    let before = live_state(&fixture);
    let files = fixture.disk_state();
    let old_hosts: Vec<_> = hosts.iter().map(Host::accepted).collect();
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let refusal = match bound.publish(&mut destinations) {
        Err(refusal) => refusal,
        Ok(_) => panic!("the actual final Ready publisher must reach the controlled write failure"),
    };
    drop(destinations);
    let (mut installation, error) = refusal.into_parts();
    assert!(matches!(
        error,
        PrefabExtractionInstallationError::Failed(FilePublicationFailureStage::WriteTemporary)
    ));
    assert!(installation.project_mut().cleanup_pending());
    assert!(!installation.project_mut().retry_cleanup().unwrap());
    assert_eq!(
        hosts.iter().map(Host::accepted).collect::<Vec<_>>(),
        old_hosts
    );
    assert_eq!(live_state(&fixture), before);
    let during_cleanup = fixture.disk_state();
    for (path, bytes) in &files {
        let actual = during_cleanup
            .iter()
            .find(|(candidate, _)| candidate == path)
            .unwrap();
        assert_eq!(
            ContentDigest::of_bytes(&actual.1),
            ContentDigest::of_bytes(bytes),
            "{path:?}"
        );
    }
    let new_files: Vec<_> = during_cleanup
        .iter()
        .filter(|(path, _)| !files.iter().any(|(original, _)| original == path))
        .collect();
    assert_eq!(new_files.len(), 1);
    assert!(
        new_files[0]
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(".nara-publication-")
    );
    assert_pending(&fixture);
    let corpus = installation.persistence().sources().prepared().corpus();
    assert_eq!(corpus.sources().as_ptr(), allocation);
    assert_eq!(corpus.schema_basis(), &basis);
    gate.release();
    assert!(installation.project_mut().retry_cleanup().unwrap());
    assert_eq!(fixture.disk_state(), files);
    installation.project_mut().validate().unwrap();
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    assert!(bound.publish(&mut destinations).is_ok());
}

#[test]
fn root_relative_host_bindings_remain_distinct_from_actual_project_destination_paths() {
    let mut fixture = Fixture::with_scoped_bindings();
    let mut hosts = original_hosts(&fixture);
    let p = fixture.document(P);
    assert_eq!(
        fixture
            .workspace
            .document_source_binding(p)
            .unwrap()
            .locator(),
        ALIAS
    );
    let installation = prepared(&fixture);
    let corpus = installation.persistence().sources().prepared().corpus();
    let allocation = corpus.sources().as_ptr();
    let basis = corpus.schema_basis().clone();
    let disk = fixture.disk_state();
    let working = live_state(&fixture);
    let observations: Vec<_> = hosts.iter().map(Host::accepted).collect();
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    // A valid local locator cannot be substituted for the lane's full project path.
    if let ExtractionPersistenceDestination::Prefab { project_path, .. } = &mut destinations[0] {
        *project_path = ALIAS;
    } else {
        panic!("the first captured document is the selected Prefab");
    }
    let refusal = match bound.publish(&mut destinations) {
        Err(refusal) => refusal,
        Ok(_) => panic!("a root-relative alias is not the captured project location"),
    };
    drop(destinations);
    let (installation, error) = refusal.into_parts();
    assert!(matches!(
        error,
        PrefabExtractionInstallationError::Destinations
    ));
    let corpus = installation.persistence().sources().prepared().corpus();
    assert_eq!(corpus.sources().as_ptr(), allocation);
    assert_eq!(corpus.schema_basis(), &basis);
    assert_eq!(
        hosts.iter().map(Host::accepted).collect::<Vec<_>>(),
        observations
    );
    assert_eq!(fixture.disk_state(), disk);
    assert_eq!(live_state(&fixture), working);
    assert_pending(&fixture);
    let bound = installation
        .bind_workspace(&fixture.authority, &mut fixture.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let installed = bound.publish(&mut destinations).unwrap();
    drop(destinations);
    assert!(matches!(
        installed.session.settings().authoring_state,
        ProjectAuthoringState::Ready
    ));
    assert!(fixture.workspace.len() == 4 && fixture.workspace.prefab(p).is_some());
}

#[test]
fn complete_live_installation_consumes_the_original_allowance_at_exact_and_one_short_work() {
    let mut probe = Fixture::new();
    let mut hosts = original_hosts(&probe);
    let prepared = prepared(&probe);
    let bound = prepared
        .bind_workspace(&probe.authority, &mut probe.workspace)
        .unwrap();
    let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
    let mut installed = bound.publish(&mut destinations).unwrap();
    let required = installed.host_usage().decode_work();
    drop(destinations);
    drop(installed);
    for limit in [required, required - 1] {
        let mut fixture = Fixture::new();
        let mut hosts = original_hosts(&fixture);
        let mut prepared = prepared_with_work(&fixture, limit);
        let corpus = prepared.persistence().sources().prepared().corpus();
        let allocation = corpus.sources().as_ptr();
        let basis = corpus.schema_basis().clone();
        let before = prepared.project_mut().host_usage().unwrap();
        let working = live_state(&fixture);
        let disk = fixture.disk_state();
        let old_hosts: Vec<_> = hosts.iter().map(Host::accepted).collect();
        let bound = prepared
            .bind_workspace(&fixture.authority, &mut fixture.workspace)
            .unwrap();
        let mut destinations: Vec<_> = hosts.iter_mut().map(Host::destination).collect();
        let result = bound.publish(&mut destinations);
        drop(destinations);
        match result {
            Ok(mut installed) => {
                assert_eq!(limit, required);
                assert_eq!(installed.host_usage().decode_work(), required);
            }
            Err(refusal) => {
                assert_eq!(limit, required - 1);
                let (mut prepared, error) = refusal.into_parts();
                assert!(matches!(error, PrefabExtractionInstallationError::Project(
                    super::super::super::publication::PrefabExtractionProjectError::Admission(
                        crate::project_host::authoring::AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
                    )
                )), "{error:?}");
                let after = prepared.project_mut().host_usage().unwrap();
                assert!(after.decode_work() > before.decode_work());
                assert_eq!(
                    after.retained_candidate_bytes(),
                    before.retained_candidate_bytes()
                );
                assert_eq!(
                    after.retained_open_handles(),
                    before.retained_open_handles()
                );
                let corpus = prepared.persistence().sources().prepared().corpus();
                assert_eq!(corpus.sources().as_ptr(), allocation);
                assert_eq!(corpus.schema_basis(), &basis);
                assert_eq!(
                    hosts.iter().map(Host::accepted).collect::<Vec<_>>(),
                    old_hosts
                );
                assert_eq!(live_state(&fixture), working);
                assert_eq!(fixture.disk_state(), disk);
                assert_pending(&fixture);
            }
        }
    }
}
