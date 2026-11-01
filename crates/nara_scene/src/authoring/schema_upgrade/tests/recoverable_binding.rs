use super::*;

#[test]
fn refused_binding_and_guard_rollback_return_the_exact_checkpoint_for_retry() {
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
            assert_eq!(before.2.undo_depth, 1);
            assert_eq!(before.2.redo_depth, 1);
            let original = session.shared_document();
            let original_state = session.authority.published_shared().unwrap();
            let mut work = AuthoredSchemaUpgradeWork::default();
            let prepared = session
                .prepare_authored_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 8)),
                    &fixture.upgrade,
                    &mut work,
                )
                .unwrap();
            let future = prepared.shared_document();
            let future_state = Arc::downgrade(&prepared.checkpoint.state);
            let revision = prepared.revision();
            let content_state = prepared.content_state();
            let usage = work.usage();
            let retention = work.budget.retention_checkpoint();
            let foreign_key = SourceDocumentKey::named(
                "04deef6c-d2c5-4fcf-905c-2793204b9512".parse().unwrap(),
                "another-source",
                source_key.kind(),
            )
            .unwrap();
            let failure = session
                .bind_authored_schema_upgrade(&foreign_key, prepared)
                .unwrap_err();
            assert!(!failure.report().applied);
            assert!(failure.report().diagnostics.has_errors());
            let (mut prepared, report) = failure.into_parts();
            assert!(report.diagnostics.has_errors());
            assert_eq!(snapshot!(session), before);
            assert!(Arc::ptr_eq(&original, &session.shared_document()));
            assert!(Arc::ptr_eq(
                &original_state,
                &session.authority.published_shared().unwrap()
            ));
            for _ in 0..3 {
                assert!(Arc::ptr_eq(&future, &prepared.shared_document()));
                assert!(Arc::ptr_eq(
                    &future_state.upgrade().unwrap(),
                    &prepared.checkpoint.state
                ));
                assert_eq!(prepared.revision(), revision);
                assert_eq!(prepared.content_state(), content_state);
                assert_eq!(prepared.base_revision(), before.0);
                assert_eq!(prepared.base_content_state(), before.1);
                assert_eq!(prepared.previous_basis(), fixture.previous);
                assert_eq!(prepared.schema_basis(), fixture.successor);
                prepared = session
                    .bind_authored_schema_upgrade(&source_key, prepared)
                    .unwrap()
                    .into_prepared();
                assert_eq!(snapshot!(session), before);
                assert!(Arc::ptr_eq(&original, &session.shared_document()));
                assert_eq!(work.usage(), usage);
                assert_eq!(work.budget.retention_checkpoint(), retention);
            }
            session
                .bind_authored_schema_upgrade(&source_key, prepared)
                .unwrap()
                .commit();
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content_state);
            assert!(Arc::ptr_eq(&future, &session.shared_document()));
            assert!(Arc::ptr_eq(
                &future_state.upgrade().unwrap(),
                &session.authority.published_shared().unwrap()
            ));
            assert_eq!(session.history_status(), Default::default());
            assert_eq!(work.usage(), usage);
        }
    );
}
