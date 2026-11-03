//! Actual custodian handoff using public Session registration and real cleanup owners.

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::Arc,
    thread,
};

use nara::{
    project::ProjectAuthoringRecoveryDigest,
    project_host::{
        EditorAuthoredSchemaUpgradeId, EditorProjectCloseProgress, EditorProjectOpenHost,
        EditorProjectOpenProgress, EditorProjectSession,
    },
};

use super::*;

const WAIT: Duration = Duration::from_secs(15);

fn open(
    root: &std::path::Path,
) -> (
    EditorProjectOpenHost,
    EditorProjectSession,
    EditorSupervisedPlayBinding,
) {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(
        crate::app::open_project_root(root).unwrap(),
        Some("desktop".to_owned()),
    );
    let deadline = Instant::now() + WAIT;
    let mut session = loop {
        assert!(
            Instant::now() < deadline,
            "the retirement test project did not open"
        );
        match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(pending) => {
                attempt = pending;
                thread::sleep(Duration::from_millis(1));
            }
            EditorProjectOpenProgress::Opened(session) => break session,
            EditorProjectOpenProgress::Rejected(error) => panic!("test Open failed: {error:?}"),
        }
    };
    let binding = session.register_supervised_play_owner(17).unwrap();
    (host, session, binding)
}

fn with_session(test: impl FnOnce(&mut EditorProjectSession, EditorSupervisedPlayBinding)) {
    let fixture = crate::app::PersistenceFixture::authored_upgrade();
    let (mut host, mut session, binding) = open(fixture.project_root());
    let result = catch_unwind(AssertUnwindSafe(|| test(&mut session, binding)));
    let deadline = Instant::now() + WAIT;
    while !matches!(
        session.advance_close(),
        EditorProjectCloseProgress::Complete(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "the retirement test Session did not close"
        );
        thread::sleep(Duration::from_millis(1));
    }
    while !host.poll_shutdown().is_complete() {
        assert!(
            Instant::now() < deadline,
            "the retirement test Open Host did not close"
        );
        thread::sleep(Duration::from_millis(1));
    }
    drop(session);
    drop(host);
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

fn bound_owner(binding: EditorSupervisedPlayBinding) -> Box<SupervisedDesktopPlayOwner> {
    Box::new(SupervisedDesktopPlayOwner::with_binding(
        binding,
        SupervisedPlayWake::default(),
    ))
}

fn begin(
    session: &mut EditorProjectSession,
    owner: &SupervisedDesktopPlayOwner,
) -> (
    EditorAuthoredSchemaUpgradeId,
    EditorSupervisedPlayRetirementRequest,
) {
    let deadline = Instant::now() + WAIT;
    let operation = loop {
        match session.begin_authored_schema_upgrade() {
            Ok(operation) => break operation,
            Err(nara::project_host::EditorAuthoredSchemaUpgradeError::Busy) => {
                assert!(
                    Instant::now() < deadline,
                    "initial source owners did not settle"
                );
                session.drive_editor_frame(Duration::ZERO);
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("upgrade acceptance failed: {error:?}"),
        }
    };
    let digest = ContentDigest::of_bytes(b"reviewed custodian recovery");
    session
        .begin_supervised_play_retirement(
            nara::project_host::EditorSourceOperationId::AuthoredSchemaUpgrade(operation),
            ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes()),
        )
        .unwrap();
    let request = session
        .supervised_play_retirement_request(owner.binding().unwrap())
        .unwrap()
        .unwrap();
    (operation, request)
}

fn accept(
    session: &mut EditorProjectSession,
    owner: &SupervisedDesktopPlayOwner,
    acknowledgment: EditorSupervisedPlayRetirementAck,
) {
    session
        .apply_supervised_play_retirement_ack(owner.binding().unwrap(), acknowledgment)
        .unwrap();
    assert!(!session.supervised_play_retirement_pending());
    session.finish_supervised_play_retirement().unwrap();
}

#[test]
fn even_an_idle_registered_owner_installs_its_prepared_empty_replacement_before_ack() {
    with_session(|session, binding| {
        let mut owner = bound_owner(binding);
        let (_, request) = begin(session, &owner);
        assert!(!owner.close_complete());
        assert!(
            owner
                .begin_authored_upgrade_retirement(request, Instant::now())
                .unwrap()
        );
        assert!(owner.close_complete());
        let replacement = owner
            .authored_upgrade_retirement
            .as_ref()
            .unwrap()
            .replacement
            .as_ref() as *const SupervisedDesktopPlayOwner;
        assert!(
            owner
                .authored_upgrade_retirement
                .as_ref()
                .unwrap()
                .replacement
                .binding()
                .is_none()
        );
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        assert!(std::ptr::eq(owner.as_ref(), replacement));
        assert!(!owner.close_requested);
        assert!(!owner.close_complete());
        assert!(owner.authored_upgrade_retirement.is_none());
        assert!(
            owner.binding().is_some(),
            "handoff moves the original non-Clone binding"
        );
        assert!(
            session.supervised_play_retirement_pending(),
            "installation alone is not a Root ack"
        );
        accept(session, &owner, acknowledgment);
        assert!(
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .is_none()
        );

        // The installed owner can accept a new request; it is not the permanently closed owner.
        owner.submit(
            tests::request(71, EditorPlayOperation::Play, Some(vec![0])),
            std::env::temp_dir(),
            Instant::now(),
        );
        let rejected = owner.advance(Instant::now()).unwrap();
        assert!(matches!(
            rejected.result(),
            Some(EditorPlayOperationResult::Failed {
                failure: EditorPlayFailure::Start,
                ..
            })
        ));
        owner.request_close(Instant::now());
        assert!(owner.close_complete());
    });
}

#[test]
fn a_foreign_registered_owner_cannot_retire_another_projects_exact_request() {
    with_session(|session, binding| {
        with_session(|_, foreign| {
            let mut owner = bound_owner(binding);
            let mut foreign_owner = bound_owner(foreign);
            assert_eq!(
                owner.binding().unwrap().published_generation(),
                foreign_owner.binding().unwrap().published_generation()
            );
            let (_, request) = begin(session, &owner);
            assert!(matches!(
                foreign_owner.begin_authored_upgrade_retirement(request, Instant::now()),
                Err(EditorSupervisedPlayOwnerError::ForeignOwner)
            ));
            assert!(!foreign_owner.close_requested);
            assert!(foreign_owner.authored_upgrade_retirement.is_none());
            let request = session
                .supervised_play_retirement_request(owner.binding().unwrap())
                .unwrap()
                .unwrap();
            owner
                .begin_authored_upgrade_retirement(request, Instant::now())
                .unwrap();
            let acknowledgment =
                SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                    .unwrap()
                    .unwrap();
            accept(session, &owner, acknowledgment);
            owner.request_close(Instant::now());
            foreign_owner.request_close(Instant::now());
        });
    });
}

#[test]
fn a_real_preparation_worker_must_return_before_replacement_and_ack() {
    with_session(|session, binding| {
        let base = tests::unique_test_root("upgrade-held-preparation");
        fs::create_dir_all(&base).unwrap();
        let store = ExecutableGenerationStore::create(&base).unwrap();
        let (preparation, started, release) = GenerationPreparationOwner::held_for_test(store);
        started.recv_timeout(WAIT).unwrap();
        let mut owner = bound_owner(binding);
        owner.preparation = Some(preparation);
        owner.pending = Some(PendingRequest {
            request: tests::request(81, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: base.clone(),
            source: None,
            source_receipt_digest: Some(ContentDigest::of_bytes(b"held source")),
            phase: PendingPhase::PreparingGeneration,
            completion: None,
            ready_reported: false,
        });
        let (_, request) = begin(session, &owner);
        let begun = owner.begin_authored_upgrade_retirement(request, Instant::now());
        let incomplete =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner);
        let retained = owner.preparation.is_some() && owner.authored_upgrade_retirement.is_some();
        // Release before asserting results so a failed assertion cannot leave the held worker.
        release.send(()).unwrap();
        assert!(begun.unwrap());
        assert!(incomplete.unwrap().is_none());
        assert!(retained);
        assert!(session.supervised_play_retirement_pending());
        let deadline = Instant::now() + WAIT;
        while !owner.close_complete() {
            let _ = owner.advance(Instant::now());
            assert!(
                Instant::now() < deadline,
                "the actual preparation worker did not retire"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        assert!(owner.preparation.is_none());
        assert!(!owner.close_requested);
        accept(session, &owner, acknowledgment);
        owner.request_close(Instant::now());
        assert!(owner.close_complete());
        drop(owner);
        fs::remove_dir_all(base).unwrap();
    });
}

#[test]
fn actual_build_process_cleanup_precedes_ack_and_replacement() {
    with_session(|session, binding| {
        let mut owner = bound_owner(binding);
        owner.build = Some(DesktopBuildAttempt::waiting_test_attempt());
        owner.pending = Some(PendingRequest {
            request: tests::request(91, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: std::env::temp_dir(),
            source: None,
            source_receipt_digest: Some(ContentDigest::of_bytes(b"building source")),
            phase: PendingPhase::Building,
            completion: None,
            ready_reported: false,
        });
        let (_, request) = begin(session, &owner);
        owner
            .begin_authored_upgrade_retirement(request, Instant::now())
            .unwrap();
        assert!(!owner.close_complete());
        assert!(owner.build.is_some());
        assert!(
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .is_none()
        );
        let deadline = Instant::now() + WAIT;
        while !owner.close_complete() {
            let _ = owner.advance(Instant::now());
            assert!(
                Instant::now() < deadline,
                "the actual build process did not retire"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        assert!(owner.build.is_none());
        assert!(!owner.close_requested);
        accept(session, &owner, acknowledgment);
        owner.request_close(Instant::now());
        assert!(owner.close_complete());
    });
}

#[test]
fn a_completed_handoff_preserves_the_same_registration_across_cancellation() {
    with_session(|session, binding| {
        let mut owner = bound_owner(binding);
        let (operation, request) = begin(session, &owner);
        owner
            .begin_authored_upgrade_retirement(request, Instant::now())
            .unwrap();
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        assert!(!owner.close_requested);
        accept(session, &owner, acknowledgment);
        session.cancel_authored_schema_upgrade(operation).unwrap();
        let deadline = Instant::now() + WAIT;
        while session.authored_schema_upgrade_view().writes_fenced {
            assert!(
                Instant::now() < deadline,
                "cancellation did not resume after the exact ack"
            );
            let _ = session.drive_editor_frame(Duration::ZERO);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(owner.binding().unwrap().published_generation(), 17);
        assert!(matches!(
            session.register_supervised_play_owner(17),
            Err(EditorSupervisedPlayOwnerError::AlreadyRegistered)
        ));
        // The concrete replacement remains usable after cancellation, without re-registration.
        owner.submit(
            tests::request(96, EditorPlayOperation::Pause, None),
            std::env::temp_dir(),
            Instant::now(),
        );
        assert_eq!(owner.advance(Instant::now()).unwrap().attempt(), 96);
        assert!(!owner.close_requested);
        owner.request_close(Instant::now());
        assert!(owner.close_complete());
    });
}

#[test]
fn cleanup_failure_retains_the_exact_request_until_explicit_retry_succeeds() {
    with_session(|session, binding| {
        let base = tests::unique_test_root("upgrade-cleanup-failure");
        fs::create_dir_all(&base).unwrap();
        let source = b"cleanup failure source";
        let mut store = ExecutableGenerationStore::create(&base).unwrap();
        let generation = store
            .publish_candidate(
                &std::env::current_exe().unwrap(),
                tests::generation_manifest(source),
            )
            .unwrap();
        store.activate_candidate(generation.id()).unwrap();
        store.set_cleanup_incomplete_for_test(true);
        let mut owner = bound_owner(binding);
        owner.store = Some(store);
        owner.active_generation_label = Some(generation.sequence());
        owner.active_source_receipt_digest = Some(ContentDigest::of_bytes(source));
        let (_, request) = begin(session, &owner);
        owner
            .begin_authored_upgrade_retirement(request, Instant::now())
            .unwrap();
        assert!(owner.close_retry_required());
        assert!(
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .is_none()
        );
        assert!(session.supervised_play_retirement_pending());
        assert!(owner.authored_upgrade_retirement.is_some());
        owner
            .store
            .as_mut()
            .unwrap()
            .set_cleanup_incomplete_for_test(false);
        owner.submit(
            tests::request(101, EditorPlayOperation::RetryClose, None),
            base.clone(),
            Instant::now(),
        );
        assert!(owner.close_complete());
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        assert!(owner.store.is_none());
        assert!(!owner.close_requested);
        accept(session, &owner, acknowledgment);
        owner.request_close(Instant::now());
        drop(owner);
        drop(generation);
        fs::remove_dir_all(base).unwrap();
    });
}

#[test]
fn installing_a_new_wake_updates_the_prepared_replacement_before_handoff() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    with_session(|session, binding| {
        let mut owner = bound_owner(binding);
        let (_, request) = begin(session, &owner);
        owner
            .begin_authored_upgrade_retirement(request, Instant::now())
            .unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&wakes);
        owner.set_wake(SupervisedPlayWake::new(move || {
            observed.fetch_add(1, Ordering::Relaxed);
            true
        }));
        let acknowledgment =
            SupervisedDesktopPlayOwner::complete_authored_upgrade_retirement(&mut owner)
                .unwrap()
                .unwrap();
        owner.wake.notify();
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        accept(session, &owner, acknowledgment);
        owner.request_close(Instant::now());
    });
}
