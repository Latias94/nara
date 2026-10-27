#![cfg(feature = "snap-child-experiment")]

use nara::{
    project_host::{EditorEditPreparationError, EditorProjectSession},
    scene::SceneDocumentCandidate,
};
use nara_editor_panel_module_consumer::snap_local_position::{SnapInput, SnapPlan, read_requests};
use nara_tooling::{
    EditorAction, EditorDocumentMutation, EditorDocumentQueryRejection, EditorNavigationAction,
    EditorPersistenceAction, EditorPersistenceResult, EditorReopenTarget,
};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
#[path = "support/snap_child_process.rs"]
mod process;
#[path = "support/snap_project.rs"]
#[allow(dead_code)]
mod project;
#[path = "support/snap_child_wire.rs"]
#[allow(dead_code)]
mod wire;
use process::{Admission, Cleanup, Completion, Failure, Lane, RequestId, WorkerGate};
use project::*;

struct Session(EditorProjectSession);
impl std::ops::Deref for Session {
    type Target = EditorProjectSession;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        close(&mut self.0);
    }
}

fn input(session: &EditorProjectSession, document: nara_tooling::EditorDocumentId) -> SnapInput {
    let [x, y] = observations(session, document);
    SnapInput::capture(&x, &y, 16.0).unwrap()
}

fn lane(cwd: PathBuf, timeout: Duration) -> Lane {
    fs::create_dir(&cwd).unwrap();
    Lane::new(
        PathBuf::from(env!("CARGO_BIN_EXE_snap_local_position_child")),
        cwd,
        timeout,
    )
    .unwrap()
}

fn finish(lane: &mut Lane, request: &RequestId) -> Completion {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(done) = lane.take(request) {
            assert_eq!(&done.request, request);
            assert!(
                matches!(
                    done.cleanup,
                    Cleanup::Reaped {
                        io_joined: true,
                        ..
                    }
                ),
                "{done:?}"
            );
            eprintln!(
                "finite child spawn {:?}, reap {:?}, joined {:?}; {:?}",
                done.spawned_after, done.reaped_after, done.elapsed, done.cleanup
            );
            assert!(done.spawned_after.unwrap() <= done.reaped_after.unwrap());
            assert!(done.reaped_after.unwrap() <= done.elapsed);
            return done;
        }
        assert!(Instant::now() < deadline, "child did not physically return");
        thread::sleep(Duration::from_millis(1));
    }
}

fn ready(cwd: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cwd.join("ready").exists() {
        assert!(
            Instant::now() < deadline,
            "helper did not reach its controlled barrier"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_child_proposal_uses_inactive_source_owner_history_and_exact_persistence() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let active = session.current_frame().workspace().active_document.unwrap();
    let document = open_source(&mut session);
    applied(dispatch(
        &mut session,
        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument { document: active }),
    ));
    let before = session.current_frame();
    let original = fs::read(project.root.join(SOURCE)).unwrap();
    let cwd = project.root.join("child-success");
    let mut child = lane(cwd, Duration::from_secs(5));
    let request = child.start(input(&session, document), "normal").unwrap();
    let completed = finish(&mut child, &request);
    let SnapPlan::Change(proposal) = completed.result.unwrap() else {
        panic!("expected a change");
    };
    assert_eq!(proposal.before(), [23.0, -25.0]);
    assert_eq!(proposal.after(), [16.0, -32.0]);
    let candidate = session
        .prepare_authoring_property_edit(proposal.basis(), &proposal.property_edits())
        .unwrap();
    assert_eq!(candidate.patch(), &expected_patch(false));
    assert_eq!(candidate.inverse(), &expected_patch(true));
    assert_eq!(session.current_frame().workspace(), before.workspace());
    assert_eq!(fs::read(project.root.join(SOURCE)).unwrap(), original);
    let retained = session.commit_authoring_edit(candidate);
    applied(retained.clone());
    session.drive_editor_frame(Duration::ZERO);
    let frame = session.current_frame();
    assert_eq!(frame.workspace().active_document, Some(active));
    assert_eq!(
        frame.workspace().document(active),
        before.workspace().document(active)
    );
    assert_eq!(position(&session, document), [16.0, -32.0]);
    assert_eq!(
        frame
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1
    );
    for (redo, expected) in [(false, [23.0, -25.0]), (true, [16.0, -32.0])] {
        let base_revision = basis(&session, document).revision;
        applied(dispatch(
            &mut session,
            EditorAction::Mutate(if redo {
                EditorDocumentMutation::Redo {
                    document,
                    base_revision,
                }
            } else {
                EditorDocumentMutation::Undo {
                    document,
                    base_revision,
                }
            }),
        ));
        assert_eq!(position(&session, document), expected);
    }
    let revision = basis(&session, document).revision;
    let save = accepted(dispatch(
        &mut session,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision: revision,
        }),
    ));
    assert!(
        matches!(terminal(&mut session, save), EditorPersistenceResult::Saved { document: d, revision: r, .. } if d == document && r == revision)
    );
    acknowledge(&mut session, save);
    let reopen = accepted(dispatch(
        &mut session,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: revision,
            },
        }),
    ));
    assert_ne!(save, reopen);
    assert!(
        matches!(terminal(&mut session, reopen), EditorPersistenceResult::Opened { document: d, .. } if d == document)
    );
    acknowledge(&mut session, reopen);
    assert_eq!(position(&session, document), [16.0, -32.0]);
    assert_eq!(
        SceneDocumentCandidate::decode_json_bytes(&fs::read(project.root.join(SOURCE)).unwrap())
            .unwrap()
            .to_json_string()
            .unwrap(),
        scene([16.0, -32.0]).to_json_string().unwrap()
    );
    applied(retained);
}

#[test]
fn process_refusals_and_no_change_never_publish_or_retain_raw_diagnostics() {
    let project = fixture([16.0, -32.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let before = session.current_frame();
    let mut child = lane(project.root.join("child-failures"), Duration::from_secs(5));
    let request = child.start(input(&session, document), "normal").unwrap();
    assert_eq!(finish(&mut child, &request).result, Ok(SnapPlan::NoChange));
    for (mode, expected) in [
        ("short", Failure::Protocol),
        ("extra", Failure::StdoutLimit),
        ("stdout-limit", Failure::StdoutLimit),
        ("stderr-limit", Failure::StderrLimit),
        ("wrong-request", Failure::Protocol),
        ("invalid-number", Failure::Protocol),
        ("reserved", Failure::Protocol),
        ("version", Failure::Protocol),
        ("nonzero", Failure::NonzeroExit),
        ("panic-stdin", Failure::WorkerPanic),
    ] {
        let request = child.start(input(&session, document), mode).unwrap();
        assert_eq!(finish(&mut child, &request).result, Err(expected), "{mode}");
        assert_eq!(session.current_frame().workspace(), before.workspace());
    }
    // This legal source input rounds outside the Transform decode range in the helper.
    let max = f64::from(f32::MAX);
    let [mut x, mut y] = observations(&session, document);
    x.value = nara_tooling::EditorPropertyValue::Present(
        nara::reflect::ComponentValue::f64(max).unwrap(),
    );
    y.value = nara_tooling::EditorPropertyValue::Present(
        nara::reflect::ComponentValue::f64(1.0).unwrap(),
    );
    let request = child
        .start(
            SnapInput::capture(&x, &y, 2.0f64.powi(105)).unwrap(),
            "normal",
        )
        .unwrap();
    assert_eq!(
        finish(&mut child, &request).result,
        Err(Failure::Semantic(wire::Refusal::ResultOutOfRange))
    );
    assert_eq!(session.current_frame().workspace(), before.workspace());
}

#[test]
fn cancelled_late_and_foreign_results_cannot_reenter_a_successor_lane() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let cwd = project.root.join("child-cancel");
    let mut child = lane(cwd.clone(), Duration::from_secs(5));
    let old = child
        .start(input(&session, document), "hold-after-response")
        .unwrap();
    ready(&cwd);
    assert!(
        !child.poll(),
        "a frame alone must not settle before process exit and I/O return"
    );
    assert_eq!(
        child.start(input(&session, document), "normal"),
        Err(Admission::Busy)
    );
    assert!(child.cancel(&old));
    assert_eq!(finish(&mut child, &old).result, Err(Failure::Cancelled));
    let next = child.start(input(&session, document), "normal").unwrap();
    assert_ne!(old, next);
    assert!(!child.cancel(&old));
    assert!(child.take(&old).is_none());
    let mut other = lane(project.root.join("child-other"), Duration::from_secs(5));
    let foreign = other.start(input(&session, document), "normal").unwrap();
    assert_ne!(next, foreign);
    assert!(!child.cancel(&foreign));
    assert!(finish(&mut child, &next).result.is_ok());
    assert!(finish(&mut other, &foreign).result.is_ok());
    assert_eq!(
        session
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
}

#[test]
fn late_spawn_observation_and_timeout_keep_custody_until_real_return() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let gate = Arc::new(WorkerGate::default());
    let mut child = lane(
        project.root.join("child-late-spawn"),
        Duration::from_secs(5),
    );
    child.spawn_gate = Some(Arc::clone(&gate));
    let request = child.start(input(&session, document), "normal").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !gate.entered.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        thread::yield_now();
    }
    assert_eq!(
        child.start(input(&session, document), "normal"),
        Err(Admission::Busy)
    );
    assert!(child.cancel(&request));
    gate.release.store(true, Ordering::Release);
    assert_eq!(finish(&mut child, &request).result, Err(Failure::Cancelled));
    let cwd = project.root.join("child-timeout");
    let mut timed = lane(cwd, Duration::from_millis(250));
    let request = timed.start(input(&session, document), "hold").unwrap();
    assert_eq!(finish(&mut timed, &request).result, Err(Failure::TimedOut));
    assert!(!timed.cleanup_incomplete());
}

#[test]
fn child_exit_cannot_release_a_lane_while_an_io_worker_has_not_returned() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let gate = Arc::new(WorkerGate::default());
    let mut child = lane(
        project.root.join("child-held-reader"),
        Duration::from_secs(5),
    );
    child.io_return_gate = Some(Arc::clone(&gate));
    let request = child.start(input(&session, document), "normal").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !child.cleanup_incomplete() {
        assert!(Instant::now() < deadline);
        assert!(!child.poll());
        thread::sleep(Duration::from_millis(1));
    }
    assert!(gate.entered.load(Ordering::Acquire));
    assert_eq!(
        child.start(input(&session, document), "normal"),
        Err(Admission::Busy)
    );
    assert!(child.take(&request).is_none());
    gate.release.store(true, Ordering::Release);
    let completed = finish(&mut child, &request);
    assert_eq!(completed.result, Err(Failure::CleanupIncomplete));
    assert!(
        completed
            .elapsed
            .saturating_sub(completed.reaped_after.unwrap())
            >= Duration::from_millis(500)
    );
    let successor = child.start(input(&session, document), "normal").unwrap();
    assert!(finish(&mut child, &successor).result.is_ok());
    assert_eq!(
        session
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
}

#[test]
fn cancellation_preserves_an_already_observed_failure_and_revokes_only_success() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let mut child = lane(
        project.root.join("child-known-failure"),
        Duration::from_secs(5),
    );
    for (mode, expected) in [
        ("wrong-request", Failure::Protocol),
        ("normal", Failure::Cancelled),
    ] {
        let request = child.start(input(&session, document), mode).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !child.poll() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(child.cancel(&request));
        assert_eq!(finish(&mut child, &request).result, Err(expected));
    }
}

#[test]
fn original_basis_refuses_a_real_child_response_after_edit_and_reopen() {
    let project = fixture([23.0, -25.0]);
    let mut session = Session(open(&project));
    let document = open_source(&mut session);
    let cwd = project.root.join("child-stale");
    let mut child = lane(cwd.clone(), Duration::from_secs(5));
    let captured = input(&session, document);
    let original_basis = captured.basis().clone();
    let request = child.start(captured, "hold").unwrap();
    ready(&cwd);
    let local = proposal(&session, document);
    let candidate = session
        .prepare_authoring_property_edit(local.basis(), &local.property_edits())
        .unwrap();
    applied(session.commit_authoring_edit(candidate));
    session.drive_editor_frame(Duration::ZERO);
    fs::write(cwd.join("release"), b"go").unwrap();
    let done = finish(&mut child, &request);
    let SnapPlan::Change(result) = done.result.unwrap() else {
        panic!("expected a historical proposal");
    };
    assert_eq!(result.basis(), &original_basis);
    assert!(
        session
            .validate_authoring_query_basis(result.basis())
            .is_err()
    );
    let before = session.current_frame();
    assert!(matches!(
        session.prepare_authoring_property_edit(result.basis(), &result.property_edits()),
        Err(EditorEditPreparationError::QueryBasis(
            EditorDocumentQueryRejection::StaleRevision
        ))
    ));
    assert_eq!(session.current_frame().workspace(), before.workspace());
    let revision = basis(&session, document).revision;
    applied(dispatch(
        &mut session,
        EditorAction::Mutate(EditorDocumentMutation::Undo {
            document,
            base_revision: revision,
        }),
    ));
    assert_eq!(position(&session, document), [23.0, -25.0]);
    let reopen_cwd = project.root.join("child-held-reopen");
    let mut reopen_child = lane(reopen_cwd.clone(), Duration::from_secs(5));
    let captured = input(&session, document);
    let old_basis = captured.basis().clone();
    let request = reopen_child.start(captured, "hold").unwrap();
    ready(&reopen_cwd);
    let revision = basis(&session, document).revision;
    let reopen = accepted(dispatch(
        &mut session,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document,
                base_revision: revision,
            },
        }),
    ));
    assert!(
        matches!(terminal(&mut session, reopen), EditorPersistenceResult::Opened { document: d, .. } if d == document)
    );
    acknowledge(&mut session, reopen);
    let reopened = session.current_frame();
    assert!(!reopen_child.poll());
    fs::write(reopen_cwd.join("release"), b"go").unwrap();
    let SnapPlan::Change(late) = finish(&mut reopen_child, &request).result.unwrap() else {
        panic!("the child must return a changing proposal after completed Reopen");
    };
    assert_eq!(late.basis(), &old_basis);
    assert_eq!(late.before(), [23.0, -25.0]);
    assert_eq!(late.after(), [16.0, -32.0]);
    assert!(
        session
            .validate_authoring_query_basis(late.basis())
            .is_err()
    );
    assert!(matches!(
        session.prepare_authoring_property_edit(late.basis(), &late.property_edits()),
        Err(EditorEditPreparationError::QueryBasis(
            EditorDocumentQueryRejection::StaleIncarnation
        ))
    ));
    assert_eq!(session.current_frame().workspace(), reopened.workspace());
    assert!(session.validate_authoring_query_basis(&old_basis).is_err());
    assert_eq!(position(&session, document), [23.0, -25.0]);
    assert_eq!(
        session
            .current_frame()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    let old_reads = read_requests(&old_basis, &id("actor")).unwrap();
    assert!(
        session
            .read_document_property(old_reads[0].clone())
            .is_err()
    );
}
