//! Public Session capture/proposal/review using the independent project's real Rust command.

mod publication;
mod retry;

#[path = "../../../../../../tests/fixtures/behavior-languages/tests/support/authored_upgrade_command.rs"]
mod project_command;

use nara_reflect::{AuthoringSchemaBundleOwner, ComponentFieldId, ComponentTypeId};
use nara_scene::{PrefabDocumentCandidate, SceneDocumentCandidate};

use super::*;
use crate::project_host::{
    AuthoredSchemaUpgradeHistoryPolicy, AuthoredSchemaUpgradeProposal,
    AuthoredSchemaUpgradeSourceProposal,
};

fn enemy_fixture() -> Fixture {
    let fixture = Fixture::new();
    let builtins = AuthoringSchemaBundle::from_json_bytes(&fixture.bytes(BUNDLE)).unwrap();
    let catalog = project_command::v1_catalog();
    let owner = PersistentComponentSchemaOwnerId::try_new(project_command::OWNER).unwrap();
    let bundle = AuthoringSchemaBundle::new(
        builtins
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .chain([AuthoringSchemaBundleOwner::new(
                owner.clone(),
                catalog,
                None,
            )]),
    )
    .unwrap();
    fs::write(fixture.root.join(BUNDLE), bundle.to_json_string().unwrap()).unwrap();
    for (index, path) in SOURCES.into_iter().enumerate() {
        let record = named_entity(path).with_component(
            ComponentTypeId::new(project_command::COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_health", ComponentValue::I64(105 + index as i64)),
                    ("wire_armor", ComponentValue::I64(7)),
                ]),
            )
            .with_owner(owner.clone()),
        );
        let encoded = if path.ends_with(".prefab.json") {
            PrefabDocument::from_ordered_roots([record])
                .to_json_string()
                .unwrap()
        } else {
            SceneDocument::from_ordered_roots([record])
                .to_json_string()
                .unwrap()
        };
        fs::write(fixture.root.join(path), encoded).unwrap();
    }
    fixture
}

fn proposal(
    editor: &EditorProjectSession,
    operation: EditorAuthoredSchemaUpgradeId,
) -> AuthoredSchemaUpgradeProposal {
    let input = editor.authored_schema_upgrade_inputs(operation).unwrap();
    let bundle = AuthoringSchemaBundle::new(input.schema_bundle().owners().iter().map(|owner| {
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
    let sources = input
        .sources()
        .map(|source| {
            let (working, successor) = match source.working {
                EditorAuthoredUpgradeWorkingSource::Scene(document) => {
                    scene_output(document, source.encoding)
                }
                EditorAuthoredUpgradeWorkingSource::Prefab(document) => {
                    prefab_output(document, source.encoding)
                }
                EditorAuthoredUpgradeWorkingSource::Saved(bytes) => match source.key.kind() {
                    SourceDocumentKind::Scene => {
                        let candidate = match source.encoding {
                            SceneFileEncoding::Json => {
                                SceneDocumentCandidate::decode_json_bytes(bytes)
                            }
                            SceneFileEncoding::Ron => {
                                SceneDocumentCandidate::decode_ron_bytes(bytes)
                            }
                        }
                        .unwrap();
                        let document = candidate.into_composition_source().unwrap();
                        let (_, successor) = scene_output(&document, source.encoding);
                        (bytes.to_vec(), successor)
                    }
                    SourceDocumentKind::Prefab => {
                        let candidate = match source.encoding {
                            SceneFileEncoding::Json => {
                                PrefabDocumentCandidate::decode_json_bytes(bytes)
                            }
                            SceneFileEncoding::Ron => {
                                PrefabDocumentCandidate::decode_ron_bytes(bytes)
                            }
                        }
                        .unwrap();
                        let document = candidate.into_composition_source().unwrap();
                        let (_, successor) = prefab_output(&document, source.encoding);
                        (bytes.to_vec(), successor)
                    }
                },
            };
            AuthoredSchemaUpgradeSourceProposal {
                key: source.key.clone(),
                project_path: source.project_path.into(),
                reviewed_working: working.into_boxed_slice(),
                successor: successor.into_boxed_slice(),
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

fn scene_output(document: &SceneDocument, encoding: SceneFileEncoding) -> (Vec<u8>, Vec<u8>) {
    let next = project_command::plan_scene(document)
        .unwrap()
        .into_document();
    let encode = |document: &SceneDocument| {
        match encoding {
            SceneFileEncoding::Json => document.to_json_string(),
            SceneFileEncoding::Ron => document.to_ron_string(),
        }
        .unwrap()
        .into_bytes()
    };
    (encode(document), encode(&next))
}

fn prefab_output(document: &PrefabDocument, encoding: SceneFileEncoding) -> (Vec<u8>, Vec<u8>) {
    let next = project_command::plan_prefab(document)
        .unwrap()
        .into_document();
    let encode = |document: &PrefabDocument| {
        match encoding {
            SceneFileEncoding::Json => document.to_json_string(),
            SceneFileEncoding::Ron => document.to_ron_string(),
        }
        .unwrap()
        .into_bytes()
    };
    (encode(document), encode(&next))
}

fn reviewed(editor: &mut EditorProjectSession) {
    wait(editor, |editor| {
        editor.authored_schema_upgrade_view().phase != EditorAuthoredSchemaUpgradePhase::Preparing
    });
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(
        view.phase,
        EditorAuthoredSchemaUpgradePhase::Review,
        "proposal must prove the complete transition: {view:?}; {:?}; {:?}",
        editor.authored_schema_upgrade_diagnostics(),
        editor.authored_schema_upgrade.proposal_failure
    );
    assert!(view.writes_fenced && !view.work_pending);
    assert_eq!(view.refusal, None);
}

fn captured_input_pointers(
    editor: &EditorProjectSession,
    operation: EditorAuthoredSchemaUpgradeId,
) -> Vec<(*const u8, *const ())> {
    editor
        .authored_schema_upgrade_inputs(operation)
        .unwrap()
        .sources()
        .map(|source| {
            let working = match source.working {
                EditorAuthoredUpgradeWorkingSource::Scene(document) => {
                    std::ptr::from_ref(document).cast()
                }
                EditorAuthoredUpgradeWorkingSource::Prefab(document) => {
                    std::ptr::from_ref(document).cast()
                }
                EditorAuthoredUpgradeWorkingSource::Saved(bytes) => bytes.as_ptr().cast(),
            };
            (source.original_disk.as_ptr(), working)
        })
        .collect()
}

#[test]
fn full_preparation_queue_returns_the_original_capture_and_public_retry_reaches_review() {
    use std::sync::{Condvar, Mutex};

    use nara_tasks::{TaskPoolKind, TaskSpawnRequest};

    // Release real IO workers on assertion failure as well as on the successful retry path.
    struct ReleaseWorkers(Arc<(Mutex<bool>, Condvar)>);
    impl Drop for ReleaseWorkers {
        fn drop(&mut self) {
            let (released, wake) = &*self.0;
            *released.lock().unwrap() = true;
            wake.notify_all();
        }
    }

    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "retain-through-preparation-queue-refusal");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        let original_inputs = captured_input_pointers(editor, operation);
        let baseline = editor.authored_schema_upgrade.baseline.as_ref().unwrap();
        let original_usage = baseline.usage();
        let original_work = baseline.work_usage();
        let original_basis = baseline.input().schema_basis().clone();

        let (release, handles) = {
            let pools = editor.authoring_runtime.persistence_tasks();
            let config = pools.config().kind(TaskPoolKind::Io);
            let latch = Arc::new((Mutex::new(false), Condvar::new()));
            let release = ReleaseWorkers(Arc::clone(&latch));
            let (started_tx, started) = mpsc::channel();
            let mut handles = Vec::with_capacity(config.workers().get() + config.pending().get());
            for _ in 0..config.workers().get() {
                let latch = Arc::clone(&latch);
                let started = started_tx.clone();
                handles.push(
                    pools
                        .spawn(
                            TaskPoolKind::Io,
                            TaskSpawnRequest::new(operation.0, super::super::DOMAIN),
                            move |_| {
                                let (released, wake) = &*latch;
                                let guard = released.lock().unwrap();
                                started.send(()).unwrap();
                                let (guard, _) = wake
                                    .wait_timeout_while(guard, WAIT * 2, |released| !*released)
                                    .unwrap();
                                assert!(*guard, "release the queue-pressure workers");
                            },
                        )
                        .into_handle()
                        .unwrap(),
                );
            }
            for _ in 0..config.workers().get() {
                started.recv_timeout(WAIT).unwrap();
            }
            for _ in 0..config.pending().get() {
                handles.push(
                    pools
                        .spawn(
                            TaskPoolKind::Io,
                            TaskSpawnRequest::new(operation.0, super::super::DOMAIN),
                            |_| (),
                        )
                        .into_handle()
                        .unwrap(),
                );
            }
            assert_eq!(
                pools.stats().for_kind(TaskPoolKind::Io).queued,
                config.pending().get()
            );
            (release, handles)
        };

        let mut consumed = original_usage.decode_work();
        for _ in 0..2 {
            let rejected = editor
                .authoring_runtime
                .persistence_tasks()
                .stats()
                .for_kind(TaskPoolKind::Io)
                .rejected;
            assert_eq!(
                editor.prepare_authored_schema_upgrade(operation, proposal(editor, operation)),
                Err(EditorAuthoredSchemaUpgradeError::Task)
            );
            assert_eq!(
                editor
                    .authoring_runtime
                    .persistence_tasks()
                    .stats()
                    .for_kind(TaskPoolKind::Io)
                    .rejected,
                rejected + 1
            );
            let view = editor.authored_schema_upgrade_view();
            assert_eq!(view.operation, Some(operation));
            assert_eq!(
                view.phase,
                EditorAuthoredSchemaUpgradePhase::AwaitingProposal
            );
            assert_eq!(view.refusal, Some(EditorAuthoredSchemaUpgradeError::Task));
            assert!(view.writes_fenced && !view.work_pending);
            assert!(editor.authored_schema_upgrade.pending.is_none());
            assert_eq!(captured_input_pointers(editor, operation), original_inputs);
            let baseline = editor.authored_schema_upgrade.baseline.as_ref().unwrap();
            let usage = baseline.usage();
            assert_eq!(baseline.input().schema_basis(), &original_basis);
            assert_eq!(baseline.work_usage(), original_work);
            assert_eq!(
                usage.retained_candidate_bytes(),
                original_usage.retained_candidate_bytes()
            );
            assert_eq!(
                usage.retained_open_handles(),
                original_usage.retained_open_handles()
            );
            assert!(
                usage.decode_work() > consumed,
                "queue retries retain performed admission work"
            );
            consumed = usage.decode_work();
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);
        }

        drop(release);
        let deadline = Instant::now() + WAIT;
        while handles
            .iter()
            .any(|task| task.return_observer().state() == TaskReturnState::Pending)
        {
            assert!(
                Instant::now() < deadline,
                "the pressure tasks must physically return"
            );
            thread::sleep(Duration::from_millis(1));
        }
        drop(handles);
        editor
            .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
            .unwrap();
        reviewed(editor);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
    });
}

#[test]
fn closed_preparation_pool_returns_the_original_capture_without_retaining_proposal_storage() {
    use nara_tasks::{TaskPoolConfig, TaskPools, TaskSpawnRequest};

    use super::super::{DOMAIN, PreparationStartFailure, PreparationWork};

    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        let original_inputs = captured_input_pointers(editor, operation);
        let baseline = editor.authored_schema_upgrade.baseline.as_ref().unwrap();
        let original_usage = baseline.usage();
        let original_work = baseline.work_usage();
        // The Runtime owns its real pool's close authority. This private owner control uses
        // an independently closed pool; the preceding QueueFull test covers the public API.
        let one = nara_core::ItemLimit::new(1).unwrap();
        let pools =
            TaskPools::inline_for_tests(TaskPoolConfig::threaded(one, one, one).unwrap()).unwrap();
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        let data = proposal(editor, operation);
        let baseline = editor.authored_schema_upgrade.baseline.take().unwrap();
        let failure = PreparationWork::start(
            &pools,
            TaskSpawnRequest::new(operation.0, DOMAIN),
            baseline,
            data,
            None,
        );
        let Err(PreparationStartFailure::Task(baseline)) = failure else {
            panic!("a closed pool must return the original preparation baseline");
        };
        editor.restore_unstarted_authored_schema_upgrade_proposal(baseline);
        assert_eq!(captured_input_pointers(editor, operation), original_inputs);
        let baseline = editor.authored_schema_upgrade.baseline.as_ref().unwrap();
        let usage = baseline.usage();
        assert_eq!(baseline.work_usage(), original_work);
        assert_eq!(
            usage.retained_candidate_bytes(),
            original_usage.retained_candidate_bytes()
        );
        assert_eq!(
            usage.retained_open_handles(),
            original_usage.retained_open_handles()
        );
        assert!(usage.decode_work() > original_usage.decode_work());
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(
            view.phase,
            EditorAuthoredSchemaUpgradePhase::AwaitingProposal
        );
        assert!(view.writes_fenced && !view.work_pending);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
    });
}

#[test]
fn project_command_prepares_a_complete_review_without_changing_live_sources_history_or_disk() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
        let prefab = open(editor, PREFAB, AuthoringDocumentKind::Prefab);
        let documents = [startup, inactive, prefab];
        for document in documents {
            edit(editor, document, "dirty-working-source");
        }
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        editor
            .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
            .unwrap();
        reviewed(editor);
        let review = editor.authored_schema_upgrade_review(operation).unwrap();
        assert_eq!(review.source_count(), SOURCES.len());
        assert_eq!(review.document_count(), documents.len());
        assert_eq!(
            review.history_policy(),
            AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
        );
        assert_eq!(review.component().as_str(), project_command::COMPONENT);
        assert_eq!(review.field().as_str(), "health");
        assert_eq!(
            review.successor_property(),
            &project_command::health_property_v2()
        );
        let sources: Vec<_> = review.sources().collect();
        assert_eq!(sources.len(), SOURCES.len());
        for source in sources {
            assert_ne!(source.reviewed_working, source.successor);
            assert_eq!(source.original_disk, fixture.bytes(source.project_path));
            assert_eq!(
                source.document,
                documents
                    .into_iter()
                    .zip([STARTUP, INACTIVE, PREFAB])
                    .find_map(|(document, path)| (path == source.project_path).then_some(document))
            );
        }
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        assert_writes_rejected(editor, startup);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        for document in documents {
            let report = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: editor.document_query_basis(document).unwrap().revision,
            });
            assert!(report.applied, "{report:?}");
        }
    });
}

#[test]
fn refused_complete_proposal_returns_the_original_capture_for_correction_and_retry() {
    let fixture = enemy_fixture();
    let mut measured_work = None;
    for bounded in [false, true] {
        // The complete built-in Schema depends on enabled features. Measure this same
        // input before selecting an exact cumulative ceiling for the second operation.
        let work_limit = bounded.then(|| {
            nara_core::ItemLimit::new(measured_work.expect("the default-limit control runs first"))
                .unwrap()
        });
        fixture.run(|editor| {
            let document = editor.workspace().active_document().unwrap();
            edit(editor, document, "dirty-before-refused-proposal");
            let before = state(editor, document);
            let disk = fixture.disk();
            let operation = match work_limit {
                Some(limit) => editor.begin_authored_schema_upgrade_with_limits(
                    crate::project_host::AuthoredSchemaUpgradeHostLimits::default()
                        .with_host_work(limit),
                ),
                None => editor.begin_authored_schema_upgrade(),
            }
            .unwrap();
            if let Some(limit) = work_limit {
                assert_eq!(
                    editor.authored_schema_upgrade_view().limits.host_work(),
                    limit
                );
            }
            awaiting(editor, operation);
            let original_work = editor
                .authored_schema_upgrade
                .baseline
                .as_ref()
                .unwrap()
                .work_usage();
            let mut incomplete = proposal(editor, operation);
            incomplete.sources = incomplete.sources.into_vec().into_iter().skip(1).collect();
            assert_eq!(
                editor.prepare_authored_schema_upgrade(operation, incomplete),
                Err(EditorAuthoredSchemaUpgradeError::Preparation)
            );
            assert_eq!(
                editor.authored_schema_upgrade_view().phase,
                EditorAuthoredSchemaUpgradePhase::AwaitingProposal
            );
            assert_eq!(
                editor
                    .authored_schema_upgrade_inputs(operation)
                    .unwrap()
                    .source_count(),
                SOURCES.len()
            );
            assert_eq!(
                editor
                    .authored_schema_upgrade
                    .baseline
                    .as_ref()
                    .unwrap()
                    .work_usage(),
                original_work
            );

            let mut invalid = proposal(editor, operation);
            let source = invalid
                .sources
                .iter_mut()
                .find(|source| source.project_path.as_ref() == STARTUP)
                .unwrap();
            let mut changed = SceneDocumentCandidate::decode_json_bytes(&source.successor)
                .unwrap()
                .into_composition_source()
                .unwrap();
            let record = changed.entities[0]
                .components
                .get_mut(&ComponentTypeId::new(project_command::COMPONENT))
                .unwrap();
            let ComponentValue::Map(fields) = &mut record.value else {
                unreachable!()
            };
            fields.insert("wire_armor".into(), ComponentValue::I64(999));
            source.successor = changed
                .to_json_string()
                .unwrap()
                .into_bytes()
                .into_boxed_slice();
            editor
                .prepare_authored_schema_upgrade(operation, invalid)
                .unwrap();
            wait(editor, |editor| {
                editor.authored_schema_upgrade_view().phase
                    != EditorAuthoredSchemaUpgradePhase::Preparing
            });
            let view = editor.authored_schema_upgrade_view();
            assert_eq!(
                view.phase,
                EditorAuthoredSchemaUpgradePhase::AwaitingProposal
            );
            assert_eq!(
                view.refusal,
                Some(EditorAuthoredSchemaUpgradeError::Preparation)
            );
            assert!(view.writes_fenced && !view.work_pending);
            assert!(
                editor
                    .authored_schema_upgrade_diagnostics()
                    .unwrap()
                    .has_errors()
            );
            assert_eq!(
                editor
                    .authored_schema_upgrade_inputs(operation)
                    .unwrap()
                    .source_count(),
                SOURCES.len()
            );
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);

            let refused_work = editor
                .authored_schema_upgrade
                .baseline
                .as_ref()
                .unwrap()
                .usage()
                .decode_work();
            if let Some(limit) = work_limit {
                assert_eq!(refused_work, limit.get());
            } else {
                measured_work = Some(refused_work);
            }
            let corrected =
                editor.prepare_authored_schema_upgrade(operation, proposal(editor, operation));
            if work_limit.is_some() {
                assert_eq!(corrected, Err(EditorAuthoredSchemaUpgradeError::WorkLimit));
                let view = editor.authored_schema_upgrade_view();
                assert_eq!(
                    view.phase,
                    EditorAuthoredSchemaUpgradePhase::AwaitingProposal
                );
                assert_eq!(
                    view.refusal,
                    Some(EditorAuthoredSchemaUpgradeError::WorkLimit)
                );
                assert!(view.writes_fenced && !view.work_pending);
                assert_eq!(
                    editor
                        .authored_schema_upgrade_inputs(operation)
                        .unwrap()
                        .source_count(),
                    SOURCES.len()
                );
            } else {
                corrected.unwrap();
                reviewed(editor);
            }
            editor.cancel_authored_schema_upgrade(operation).unwrap();
            cancelled(editor, operation);
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);
        });
    }
}

fn preparation_cancellation_waits_for_actual_return(cancel_operation: bool) {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "retained-while-preparing");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        editor.authored_schema_upgrade.next_preparation_gate = Some(Box::new(move || {
            let _ = entered_tx.send(());
            release_rx
                .recv_timeout(WAIT * 2)
                .expect("release the actual prepared result");
        }));
        editor
            .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
            .unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            if entered.try_recv().is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the project proposal must finish preparation"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let returned = editor
            .authored_schema_upgrade
            .pending
            .as_ref()
            .unwrap()
            .return_observer();
        assert_eq!(returned.state(), TaskReturnState::Pending);
        if cancel_operation {
            editor.cancel_authored_schema_upgrade(operation).unwrap();
        } else {
            editor
                .authored_schema_upgrade
                .pending
                .as_ref()
                .unwrap()
                .cancel();
        }
        for _ in 0..3 {
            editor.drive_editor_frame(Duration::ZERO);
            let view = editor.authored_schema_upgrade_view();
            assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Preparing);
            assert!(view.writes_fenced && view.work_pending);
            assert_eq!(returned.state(), TaskReturnState::Pending);
            assert_eq!(state(editor, document), before);
            assert_eq!(fixture.disk(), disk);
        }
        release.send(()).unwrap();
        if cancel_operation {
            wait(editor, |editor| {
                editor.authored_schema_upgrade_view().phase
                    == EditorAuthoredSchemaUpgradePhase::Terminal
            });
        } else {
            reviewed(editor);
            editor.cancel_authored_schema_upgrade(operation).unwrap();
        }
        assert_eq!(returned.state(), TaskReturnState::Returned);
        cancelled(editor, operation);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn cancelling_preparation_retains_the_fence_until_the_actual_candidate_worker_returns() {
    preparation_cancellation_waits_for_actual_return(true);
}

#[test]
fn cancelled_preparation_task_preserves_its_actual_review_after_physical_return() {
    preparation_cancellation_waits_for_actual_return(false);
}
