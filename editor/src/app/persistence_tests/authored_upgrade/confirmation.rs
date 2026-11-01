//! Application confirmation uses the real registered custodian, including an Idle owner.

#[path = "../../../../../tests/fixtures/behavior-languages/tests/support/authored_upgrade_command.rs"]
#[allow(
    dead_code,
    reason = "This custodian fixture uses only the independent command's catalogs; its value transformations have separate consumers"
)]
mod project_command;

use nara::{
    core::ItemLimit,
    project_host::{
        AuthoredSchemaUpgradeHostLimits, AuthoredSchemaUpgradeProposal,
        AuthoredSchemaUpgradeSourceProposal, EditorAuthoredUpgradeWorkingSource,
    },
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentTypeId,
        PersistentComponentSchemaOwnerId,
    },
    scene::{
        PrefabDocumentCandidate, SceneDocumentCandidate, SceneFileEncoding, SourceDocumentKind,
    },
};

use super::*;

fn command_data(
    application: &EditorApplication,
    operation: EditorAuthoredSchemaUpgradeId,
) -> AuthoredSchemaUpgradeProposal {
    let inputs = application
        .authored_schema_upgrade_inputs(operation)
        .unwrap();
    let bundle = AuthoringSchemaBundle::new(inputs.schema_bundle().owners().iter().map(|owner| {
        if owner.owner_id().as_str() == project_command::OWNER {
            AuthoringSchemaBundleOwner::new(
                owner.owner_id().clone(),
                project_command::successor_catalog(owner.current()).unwrap(),
                Some(owner.current().clone()),
            )
        } else {
            AuthoringSchemaBundleOwner::from(owner)
        }
    }))
    .unwrap();
    let sources = inputs
        .sources()
        .map(|source| {
            let bytes = match source.working {
                EditorAuthoredUpgradeWorkingSource::Scene(document) => match source.encoding {
                    SceneFileEncoding::Json => document.to_json_string(),
                    SceneFileEncoding::Ron => document.to_ron_string(),
                }
                .unwrap()
                .into_bytes(),
                EditorAuthoredUpgradeWorkingSource::Prefab(document) => match source.encoding {
                    SceneFileEncoding::Json => document.to_json_string(),
                    SceneFileEncoding::Ron => document.to_ron_string(),
                }
                .unwrap()
                .into_bytes(),
                EditorAuthoredUpgradeWorkingSource::Saved(bytes) => match source.key.kind() {
                    SourceDocumentKind::Scene => {
                        let document = match source.encoding {
                            SceneFileEncoding::Json => {
                                SceneDocumentCandidate::decode_json_bytes(bytes)
                            }
                            SceneFileEncoding::Ron => {
                                SceneDocumentCandidate::decode_ron_bytes(bytes)
                            }
                        }
                        .unwrap()
                        .into_composition_source()
                        .unwrap();
                        match source.encoding {
                            SceneFileEncoding::Json => document.to_json_string(),
                            SceneFileEncoding::Ron => document.to_ron_string(),
                        }
                        .unwrap()
                        .into_bytes()
                    }
                    SourceDocumentKind::Prefab => {
                        let document = match source.encoding {
                            SceneFileEncoding::Json => {
                                PrefabDocumentCandidate::decode_json_bytes(bytes)
                            }
                            SceneFileEncoding::Ron => {
                                PrefabDocumentCandidate::decode_ron_bytes(bytes)
                            }
                        }
                        .unwrap()
                        .into_composition_source()
                        .unwrap();
                        match source.encoding {
                            SceneFileEncoding::Json => document.to_json_string(),
                            SceneFileEncoding::Ron => document.to_ron_string(),
                        }
                        .unwrap()
                        .into_bytes()
                    }
                },
            };
            // This custodian integration fixture has no EnemyStats occurrences. Root's complete
            // consumer separately transforms dirty/direct/dormant records and checks their values.
            AuthoredSchemaUpgradeSourceProposal {
                key: source.key.clone(),
                project_path: source.project_path.into(),
                reviewed_working: bytes.clone().into_boxed_slice(),
                successor: bytes.into_boxed_slice(),
            }
        })
        .collect();
    AuthoredSchemaUpgradeProposal {
        successor_bundle: bundle
            .to_json_string()
            .unwrap()
            .into_bytes()
            .into_boxed_slice(),
        component: ComponentTypeId::new(project_command::COMPONENT),
        field: ComponentFieldId::new("health"),
        successor_property: project_command::health_property_v2(),
        sources,
    }
}

fn fixture_with_command() -> PersistenceFixture {
    let fixture = PersistenceFixture::authored_upgrade();
    let path = fixture.root.join("catalogs/authoring.bundle.json");
    let builtins = AuthoringSchemaBundle::from_json_bytes(&fs::read(&path).unwrap()).unwrap();
    let bundle = AuthoringSchemaBundle::new(
        builtins
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::new(
                PersistentComponentSchemaOwnerId::try_new(project_command::OWNER).unwrap(),
                project_command::v1_catalog(),
                None,
            )]),
    )
    .unwrap();
    fs::write(path, bundle.to_json_string().unwrap()).unwrap();
    fixture
}

#[test]
fn application_confirmation_publishes_ready_and_replaces_its_actual_registered_play_custodian() {
    let fixture = fixture_with_command();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    settle_background_imports(&mut application);
    let (old_owner, generation) = {
        let project = application.state.published().unwrap();
        (
            std::ptr::from_ref(project.supervised_play.as_ref()),
            project
                .supervised_play
                .binding()
                .unwrap()
                .published_generation(),
        )
    };
    let operation = application
        .begin_authored_schema_upgrade_with_limits(
            AuthoredSchemaUpgradeHostLimits::default()
                .with_open_handles(ItemLimit::new(128).unwrap()),
        )
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while application.authored_schema_upgrade_view().unwrap().phase
        != EditorAuthoredSchemaUpgradePhase::AwaitingProposal
    {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            application.authored_schema_upgrade_view()
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    application
        .prepare_authored_schema_upgrade(operation, command_data(&application, operation))
        .unwrap();
    while application.authored_schema_upgrade_view().unwrap().phase
        != EditorAuthoredSchemaUpgradePhase::Review
    {
        assert!(
            Instant::now() < deadline,
            "{:?}; {:?}",
            application.authored_schema_upgrade_view(),
            application.authored_schema_upgrade_diagnostics()
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    application
        .confirm_authored_schema_upgrade(operation)
        .unwrap();
    while application
        .authored_schema_upgrade_view()
        .unwrap()
        .work_pending
    {
        assert!(
            Instant::now() < deadline,
            "{:?}",
            application.authored_schema_upgrade_view()
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        application.authored_schema_upgrade_view().unwrap().outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
        "{:?}",
        application.authored_schema_upgrade_view()
    );
    let project = application.state.published().unwrap();
    assert_ne!(
        std::ptr::from_ref(project.supervised_play.as_ref()),
        old_owner
    );
    assert_eq!(
        project
            .supervised_play
            .binding()
            .unwrap()
            .published_generation(),
        generation
    );
    assert!(!project.supervised_play.close_complete());
    assert!(!project.session.supervised_play_retirement_pending());
    assert!(matches!(
        nara::project::ProjectManifest::parse_toml_bytes(
            &fs::read(fixture.root.join("nara.toml")).unwrap()
        )
        .manifest
        .unwrap()
        .authoring
        .state,
        nara::project::ProjectAuthoringState::Ready
    ));
    close(&mut application);
}

#[test]
fn application_close_and_exit_can_retry_the_retained_pending_operation_after_a_real_source_lock() {
    use nara::{
        fs::{LockMode, RelativePath},
        project::{ProjectAuthoringState, ProjectManifest},
    };

    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = fixture_with_command();
        let mut application =
            fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
        settle_background_imports(&mut application);
        edit_save_source(&mut application);
        let operation = application
            .begin_authored_schema_upgrade_with_limits(
                AuthoredSchemaUpgradeHostLimits::default()
                    .with_open_handles(ItemLimit::new(128).unwrap()),
            )
            .unwrap();
        let deadline = Instant::now() + WAIT;
        while application.authored_schema_upgrade_view().unwrap().phase
            != EditorAuthoredSchemaUpgradePhase::AwaitingProposal
        {
            assert!(
                Instant::now() < deadline,
                "capture must return: {:?}",
                application.authored_schema_upgrade_view()
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        application
            .prepare_authored_schema_upgrade(operation, command_data(&application, operation))
            .unwrap();
        while application.authored_schema_upgrade_view().unwrap().phase
            != EditorAuthoredSchemaUpgradePhase::Review
        {
            assert!(
                Instant::now() < deadline,
                "review must return: {:?}",
                application.authored_schema_upgrade_view()
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        let expected = application
            .authored_schema_upgrade_review(operation)
            .unwrap()
            .sources()
            .find(|source| source.project_path == SOURCE)
            .unwrap()
            .successor
            .to_vec();
        application
            .confirm_authored_schema_upgrade(operation)
            .unwrap();
        while application.authored_schema_upgrade_view().unwrap().phase
            != EditorAuthoredSchemaUpgradePhase::PublishingPending
        {
            assert!(
                Instant::now() < deadline,
                "Pending work must start: {:?}",
                application.authored_schema_upgrade_view()
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        // The admitted worker can only publish Pending. Do not drive its return and enqueue
        // a source step until the actual source has been exclusively locked by this consumer.
        loop {
            let bytes = match fs::read(fixture.root.join("nara.toml")) {
                Ok(bytes) => bytes,
                Err(error)
                    if cfg!(windows)
                        && matches!(error.raw_os_error(), Some(32 | 33))
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(error) => panic!("manifest observation failed: {error}"),
            };
            let manifest = ProjectManifest::parse_toml_bytes(&bytes).manifest.unwrap();
            if matches!(manifest.authoring.state, ProjectAuthoringState::Pending(_)) {
                break;
            }
            assert!(Instant::now() < deadline, "Pending must become visible");
            std::thread::sleep(Duration::from_millis(1));
        }
        let root = crate::app::open_project_root(&fixture.root).unwrap();
        let file = root
            .open_file_exact(&RelativePath::new(SOURCE).unwrap())
            .unwrap();
        let lock = file.try_lock(LockMode::Exclusive).unwrap();
        while application.authored_schema_upgrade_view().unwrap().phase
            != EditorAuthoredSchemaUpgradePhase::RequiresReconcile
        {
            assert!(
                Instant::now() < deadline,
                "locked source must refuse: {:?}",
                application.authored_schema_upgrade_view()
            );
            application.advance(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            application
                .authored_schema_upgrade_view()
                .unwrap()
                .can_retry()
        );
        assert!(
            !application
                .authored_schema_upgrade_view()
                .unwrap()
                .can_cancel()
        );
        request_retirement(&mut application, retirement);
        assert_eq!(application.session_retirement, Some(retirement));
        drop(lock);
        application
            .retry_authored_schema_upgrade(operation)
            .unwrap();
        finish_retirement(&mut application, retirement);
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), expected);
        let manifest =
            ProjectManifest::parse_toml_bytes(&fs::read(fixture.root.join("nara.toml")).unwrap())
                .manifest
                .unwrap();
        assert_eq!(manifest.authoring.state, ProjectAuthoringState::Ready);
    }
}
