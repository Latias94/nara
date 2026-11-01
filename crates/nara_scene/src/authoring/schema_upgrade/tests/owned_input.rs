use super::*;

use std::mem::size_of_val;

#[test]
fn owned_and_direct_preparation_share_exact_inputs_results_and_successor_costs() {
    let fixture = Fixture::new();
    let limits = SceneFileLimits::default().with_entities(ItemLimit::new(17).unwrap());
    each_source!(
        document(false, 7),
        &fixture.previous,
        limits,
        |session, source_key, candidate| {
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            assert!(session.apply_authoring_patch(&edit(9)).applied);
            assert!(session.undo_authoring().applied);
            let before = snapshot!(session);
            assert_eq!(before.2.undo_depth, 1);
            assert_eq!(before.2.redo_depth, 1);
            let original = session.shared_document();
            let state = session.authority.published_shared().unwrap();

            let mut direct_work = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut direct_work)
                .unwrap();
            let direct_entry = direct_work.budget.retention_checkpoint();
            let direct_work_entry = direct_work.usage().work_bytes;
            let direct = session
                .prepare_authored_upgrade_with_retained_source(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    retained,
                    &mut direct_work,
                )
                .unwrap();

            let mut work = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let original_refs = Arc::strong_count(&original);
            let state_refs = Arc::strong_count(&state);
            let input = session
                .capture_authored_schema_upgrade_input(retained, &mut work)
                .unwrap();
            assert_eq!(Arc::strong_count(&original), original_refs + 1);
            assert_eq!(Arc::strong_count(&state), state_refs + 1);
            assert!(Arc::ptr_eq(&original, &input.shared_document()));
            assert!(std::ptr::eq(input.document(), original.as_ref()));
            assert_eq!(input.revision(), before.0);
            assert_eq!(input.content_state(), before.1);
            assert_eq!(input.schema_basis(), fixture.previous);
            assert_eq!(input.source(), session.authoring_source().unwrap());
            assert_eq!(input.file_limits(), limits);
            assert_eq!(input.history_limits(), session.history_limits());
            let entry = work.budget.retention_checkpoint();
            let work_entry = work.usage().work_bytes;
            let owned = input
                .prepare(
                    &source_key,
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    &mut work,
                )
                .unwrap();
            // Neither path charges a second incumbent; both use the same successor kernel.
            assert_eq!(
                work.usage().work_bytes - work_entry,
                direct_work.usage().work_bytes - direct_work_entry,
            );
            assert_eq!(
                work.budget.retention_checkpoint().0 - entry.0,
                direct_work.budget.retention_checkpoint().0 - direct_entry.0,
            );
            assert_eq!(owned.document(), direct.document());
            assert_eq!(owned.source_key(), direct.source_key());
            assert_eq!(owned.source(), direct.source());
            assert_eq!(owned.base_revision(), direct.base_revision());
            assert_eq!(owned.base_content_state(), direct.base_content_state());
            assert_eq!(owned.previous_basis(), direct.previous_basis());
            assert_eq!(owned.schema_basis(), direct.schema_basis());
            assert_eq!(owned.file_limits(), direct.file_limits());
            assert_eq!(owned.history_limits(), direct.history_limits());
            assert_eq!(owned.readiness(), direct.readiness());
            assert_eq!(
                owned.unavailable_record_manifest(),
                direct.unavailable_record_manifest()
            );
            assert_eq!(owned.diagnostics(), direct.diagnostics());
            assert!(Arc::ptr_eq(&original, &owned.original));
            assert!(Arc::ptr_eq(&state, &owned.checkpoint.original_state));
            assert_ne!(owned.revision().source_id(), before.0.source_id());
            assert_ne!(owned.content_state(), before.1);
            session
                .validate_authored_schema_upgrade(&source_key, &owned)
                .unwrap();
            session
                .validate_authored_schema_upgrade(&source_key, &direct)
                .unwrap();

            let direct_refusal = session
                .prepare_authored_schema_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 9)),
                    &fixture.upgrade,
                    &mut |_, _| Ok(()),
                )
                .unwrap_err();
            let owned_refusal = work
                .with_preparation_attempt(|work| {
                    input.prepare(
                        &source_key,
                        candidate(document(true, 9)),
                        &fixture.upgrade,
                        work,
                    )
                })
                .unwrap_err();
            assert_eq!(owned_refusal.diagnostics, direct_refusal.diagnostics);
            assert_eq!(snapshot!(session), before);
            assert!(session.redo_authoring().applied);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), input.document());
        }
    );
}

#[test]
fn background_input_survives_edit_undo_and_original_session_drop_but_bind_stays_exact() {
    let fixture = Arc::new(Fixture::new());
    // Unchanged, edit, Undo, edit-and-Undo back to the same content, and source Close/drop.
    for change in 0..5 {
        each_source!(
            document(false, 7),
            &fixture.previous,
            SceneFileLimits::default(),
            |session, source_key, candidate| {
                assert!(session.apply_authoring_patch(&edit(8)).applied);
                assert!(session.apply_authoring_patch(&edit(9)).applied);
                assert!(session.undo_authoring().applied);
                let captured = snapshot!(session);
                let original = Arc::downgrade(&session.shared_document());
                let original_state = Arc::downgrade(&session.authority.published_shared().unwrap());
                let mut work = AuthoredSchemaUpgradeWork::default();
                let retained = session
                    .retain_authored_schema_upgrade_source(&mut work)
                    .unwrap();
                let input = session
                    .capture_authored_schema_upgrade_input(retained, &mut work)
                    .unwrap();
                let worker_fixture = Arc::clone(&fixture);
                let worker_key = source_key.clone();
                let successor = candidate(document(true, 8));
                let (start, wait) = std::sync::mpsc::channel();
                let worker = std::thread::spawn(move || {
                    wait.recv().unwrap();
                    let prepared = input
                        .prepare(&worker_key, successor, &worker_fixture.upgrade, &mut work)
                        .unwrap();
                    (input, work, prepared)
                });
                match change {
                    0 => {}
                    1 => assert!(session.apply_authoring_patch(&edit(10)).applied),
                    2 => assert!(session.undo_authoring().applied),
                    3 => {
                        assert!(session.apply_authoring_patch(&edit(10)).applied);
                        assert!(session.undo_authoring().applied);
                        assert_eq!(session.content_state(), captured.1);
                    }
                    4 => {
                        // Source-level Close consumes the actual owner. An independently reopened
                        // source with the same key and bytes cannot accept its old preparation.
                        let replacement = session
                            .try_reopen_authoring_candidate(candidate(document(false, 8)))
                            .unwrap();
                        drop(session);
                        session = replacement;
                    }
                    _ => unreachable!(),
                }
                assert!(original.upgrade().is_some());
                assert!(original_state.upgrade().is_some());
                let current = snapshot!(session);
                let current_document = session.shared_document();
                let current_state = session.authority.published_shared().unwrap();
                start.send(()).unwrap();
                let (input, _work, prepared) = worker.join().unwrap();
                assert_eq!(input.revision(), captured.0);
                assert_eq!(input.content_state(), captured.1);
                assert_eq!(prepared.base_revision(), captured.0);
                assert_eq!(prepared.base_content_state(), captured.1);
                assert!(Arc::ptr_eq(
                    &original.upgrade().unwrap(),
                    &input.shared_document()
                ));
                assert!(Arc::ptr_eq(
                    &original.upgrade().unwrap(),
                    &prepared.original
                ));
                assert!(Arc::ptr_eq(
                    &original_state.upgrade().unwrap(),
                    &prepared.checkpoint.original_state
                ));
                if change == 0 {
                    let future = prepared.content_state();
                    session
                        .bind_authored_schema_upgrade(&source_key, prepared)
                        .unwrap()
                        .commit();
                    assert_eq!(session.content_state(), future);
                    assert_eq!(
                        session.authoring_schema_basis(),
                        Some(fixture.successor.clone())
                    );
                } else {
                    assert!(
                        session
                            .bind_authored_schema_upgrade(&source_key, prepared)
                            .is_err()
                    );
                    assert_eq!(snapshot!(session), current);
                    assert!(Arc::ptr_eq(&current_document, &session.shared_document()));
                    assert!(Arc::ptr_eq(
                        &current_state,
                        &session.authority.published_shared().unwrap()
                    ));
                }
            }
        );
    }
}

#[test]
fn capture_admits_its_exact_metadata_before_sharing_and_refusal_preserves_parent_storage() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let _ = (source_key, candidate);
            let before = snapshot!(session);
            let mut measured = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut measured)
                .unwrap();
            // Ensure capture, rather than an earlier incumbent scan, establishes the peak.
            let padding =
                measured.usage().peak_retained_bytes - measured.budget.retention_checkpoint().0 + 1;
            measured.retain_preparation_storage(padding).unwrap();
            let entry = measured.budget.retention_checkpoint();
            let work_entry = measured.usage().work_bytes;
            let input = session
                .capture_authored_schema_upgrade_input(retained, &mut measured)
                .unwrap();
            let metadata = size_of_val(&input);
            let required = measured.usage();
            assert_eq!(required.work_bytes - work_entry, metadata);
            assert_eq!(required.peak_retained_bytes, entry.0 + metadata);
            drop(input);

            for (work_bytes, retained_bytes, succeeds) in [
                (required.work_bytes, required.peak_retained_bytes, true),
                (required.work_bytes - 1, required.peak_retained_bytes, false),
                (required.work_bytes, required.peak_retained_bytes - 1, false),
            ] {
                let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
                    ByteLimit::new(work_bytes).unwrap(),
                    ByteLimit::new(retained_bytes).unwrap(),
                ));
                let retained = session
                    .retain_authored_schema_upgrade_source(&mut work)
                    .unwrap();
                work.retain_preparation_storage(padding).unwrap();
                let entry = work.budget.retention_checkpoint();
                let original = session.shared_document();
                let state = session.authority.published_shared().unwrap();
                let original_refs = Arc::strong_count(&original);
                let state_refs = Arc::strong_count(&state);
                let result = session.capture_authored_schema_upgrade_input(retained, &mut work);
                assert_eq!(result.is_ok(), succeeds);
                if succeeds {
                    let input = result.unwrap();
                    assert!(Arc::ptr_eq(&original, &input.shared_document()));
                    assert_eq!(work.usage(), required);
                } else {
                    assert_eq!(work.budget.retention_checkpoint(), entry);
                    assert_eq!(Arc::strong_count(&original), original_refs);
                    assert_eq!(Arc::strong_count(&state), state_refs);
                }
                assert_eq!(snapshot!(session), before);
            }

            let mut work = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let entry = work.budget.retention_checkpoint();
            let result = work.with_preparation_attempt(|work| {
                let _input = session
                    .capture_authored_schema_upgrade_input(retained, work)
                    .unwrap();
                Err::<(), _>("later capture refuses")
            });
            assert_eq!(result, Err("later capture refuses"));
            assert_eq!(work.budget.retention_checkpoint(), entry);
            assert_eq!(snapshot!(session), before);
        }
    );
}

#[test]
fn borrowed_input_keeps_its_original_token_for_a_retry_at_the_same_retained_ceiling() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            assert!(session.apply_authoring_patch(&edit(9)).applied);
            assert!(session.undo_authoring().applied);
            let before = snapshot!(session);
            let mut measured = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut measured)
                .unwrap();
            let input = session
                .capture_authored_schema_upgrade_input(retained, &mut measured)
                .unwrap();
            let capture_work = measured.usage().work_bytes;
            let prepared = input
                .prepare(
                    &source_key,
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    &mut measured,
                )
                .unwrap();
            let required = measured.usage();
            let attempt_work = required.work_bytes - capture_work;
            drop((input, prepared));

            let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
                ByteLimit::new(capture_work + 2 * attempt_work).unwrap(),
                ByteLimit::new(required.peak_retained_bytes).unwrap(),
            ));
            let identity = Arc::clone(&work.identity);
            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let input = session
                .capture_authored_schema_upgrade_input(retained, &mut work)
                .unwrap();
            let entry = work.budget.retention_checkpoint();
            let result = work.with_preparation_attempt(|work| {
                let _prepared = input
                    .prepare(
                        &source_key,
                        candidate(document(true, 8)),
                        &fixture.upgrade,
                        work,
                    )
                    .unwrap();
                Err::<(), _>("another source refuses")
            });
            assert_eq!(result, Err("another source refuses"));
            assert_eq!(work.budget.retention_checkpoint(), entry);
            assert_eq!(work.usage(), required);
            assert!(Arc::ptr_eq(&identity, &work.identity));
            assert_eq!(snapshot!(session), before);

            let prepared = work
                .with_preparation_attempt(|work| {
                    input.prepare(
                        &source_key,
                        candidate(document(true, 8)),
                        &fixture.upgrade,
                        work,
                    )
                })
                .unwrap();
            assert_eq!(work.usage().work_bytes, capture_work + 2 * attempt_work);
            assert_eq!(
                work.usage().peak_retained_bytes,
                required.peak_retained_bytes
            );
            assert!(Arc::ptr_eq(&identity, &work.identity));
            assert_eq!(snapshot!(session), before);
            let exhausted_entry = work.budget.retention_checkpoint();
            assert!(
                work.with_preparation_attempt(|work| {
                    input.prepare(
                        &source_key,
                        candidate(document(true, 8)),
                        &fixture.upgrade,
                        work,
                    )
                })
                .is_err()
            );
            assert_eq!(work.budget.retention_checkpoint(), exhausted_entry);
            assert_eq!(work.usage().work_bytes, capture_work + 2 * attempt_work);
            session
                .bind_authored_schema_upgrade(&source_key, prepared)
                .unwrap()
                .commit();
            assert_eq!(
                session.authoring_schema_basis(),
                Some(fixture.successor.clone())
            );
        }
    );
}

#[test]
fn capture_and_prepare_refuse_foreign_work_and_capture_refuses_a_stale_revision_or_basis() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let mut work = AuthoredSchemaUpgradeWork::default();
            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let mut foreign = AuthoredSchemaUpgradeWork::default();
            assert!(
                session
                    .capture_authored_schema_upgrade_input(retained, &mut foreign)
                    .is_err()
            );
            assert_eq!(foreign.usage(), AuthoredSchemaUpgradeUsage::default());

            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let input = session
                .capture_authored_schema_upgrade_input(retained, &mut work)
                .unwrap();
            assert!(
                input
                    .prepare(
                        &source_key,
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        &mut foreign,
                    )
                    .is_err()
            );
            assert_eq!(foreign.usage(), AuthoredSchemaUpgradeUsage::default());

            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            assert!(session.apply_authoring_patch(&edit(8)).applied);
            let before = snapshot!(session);
            let entry = work.budget.retention_checkpoint();
            assert!(
                session
                    .capture_authored_schema_upgrade_input(retained, &mut work)
                    .is_err()
            );
            assert_eq!(work.budget.retention_checkpoint(), entry);
            assert_eq!(snapshot!(session), before);

            let retained = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let independent_previous = admit_basis(previous_catalog(), None);
            let classification = session
                .prepare_schema_reclassification(
                    &independent_previous,
                    &SchemaReclassificationKind::DeclarationExtension,
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap();
            session
                .commit_schema_reclassification(classification)
                .unwrap();
            let before = snapshot!(session);
            let entry = work.budget.retention_checkpoint();
            assert!(
                session
                    .capture_authored_schema_upgrade_input(retained, &mut work)
                    .is_err()
            );
            assert_eq!(work.budget.retention_checkpoint(), entry);
            assert_eq!(snapshot!(session), before);
            // A snapshot prepared after a later classification still pins the original state.
            let prepared = input
                .prepare(
                    &source_key,
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut work,
                )
                .unwrap();
            assert!(
                session
                    .bind_authored_schema_upgrade(&source_key, prepared)
                    .is_err()
            );
            assert_eq!(snapshot!(session), before);
        }
    );
}
