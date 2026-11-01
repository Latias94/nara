use super::*;

#[test]
fn failed_source_attempt_releases_outputs_and_preserves_prior_tokens_for_a_bounded_retry() {
    let fixture = Fixture::new();
    each_source!(
        document(false, 7),
        &fixture.previous,
        SceneFileLimits::default(),
        |session, source_key, candidate| {
            let before = snapshot!(session);
            let mut measured = AuthoredSchemaUpgradeWork::default();
            let prior = session
                .retain_authored_schema_upgrade_source(&mut measured)
                .unwrap();
            let prepared = session
                .prepare_authored_upgrade(
                    &source_key,
                    session.revision(),
                    candidate(document(true, 7)),
                    &fixture.upgrade,
                    &mut measured,
                )
                .unwrap();
            let required = measured.usage();
            drop((prior, prepared));

            let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
                ByteLimit::new(required.work_bytes * 3).unwrap(),
                ByteLimit::new(required.peak_retained_bytes).unwrap(),
            ));
            let identity = Arc::clone(&work.identity);
            let prior = session
                .retain_authored_schema_upgrade_source(&mut work)
                .unwrap();
            let entry = work.budget.retention_checkpoint();
            let refused = work.with_preparation_attempt(|work| {
                let _prepared = session
                    .prepare_authored_upgrade(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        work,
                    )
                    .expect("the measured allowance admits a complete source candidate");
                Err::<(), _>("late refusal")
            });
            assert_eq!(refused, Err("late refusal"));
            assert_eq!(work.budget.retention_checkpoint(), entry);
            assert_eq!(work.usage(), required);
            assert!(Arc::ptr_eq(&work.identity, &identity));
            assert_eq!(snapshot!(session), before);

            // This pre-entry token still belongs to the same Work and needs no second admission.
            let prepared = work
                .with_preparation_attempt(|work| {
                    session.prepare_authored_upgrade_with_retained_source(
                        &source_key,
                        session.revision(),
                        candidate(document(true, 7)),
                        &fixture.upgrade,
                        prior,
                        work,
                    )
                })
                .unwrap();
            assert!(work.usage().work_bytes > required.work_bytes);
            assert_eq!(
                work.usage().peak_retained_bytes,
                required.peak_retained_bytes
            );
            assert!(work.budget.retention_checkpoint().0 > entry.0);
            assert!(Arc::ptr_eq(&work.identity, &identity));
            assert_eq!(snapshot!(session), before);
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
fn nested_refusals_preserve_outer_outputs_and_outer_refusal_settles_inner_success() {
    let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
        ByteLimit::new(100).unwrap(),
        ByteLimit::new(20).unwrap(),
    ));
    work.retain_preparation_storage(4).unwrap();
    let identity = Arc::clone(&work.identity);

    let output = work
        .with_preparation_attempt(|work| {
            work.retain_preparation_storage(4).unwrap();
            let outer_output = vec![0_u8; 4];
            let refused = work.with_preparation_attempt(|work| {
                work.retain_preparation_storage(6).unwrap();
                let _discarded_output = vec![0_u8; 6];
                work.begin_step().unwrap();
                work.reserve("discarded-scratch", Some(6)).unwrap();
                Err::<(), _>("inner refusal")
            });
            assert_eq!(refused, Err("inner refusal"));
            assert_eq!(work.budget.retention_checkpoint(), (8, 0));
            Ok::<_, ()>(outer_output)
        })
        .unwrap();
    assert_eq!(output.len(), 4);
    assert_eq!(work.budget.retention_checkpoint(), (8, 0));
    assert_eq!(work.usage().work_bytes, 20);
    assert_eq!(work.usage().peak_retained_bytes, 20);

    let refused = work.with_preparation_attempt(|work| {
        work.retain_preparation_storage(2).unwrap();
        let _outer_output = vec![0_u8; 2];
        let _inner_output = work
            .with_preparation_attempt(|work| {
                work.retain_preparation_storage(3).unwrap();
                Ok::<_, ()>(vec![0_u8; 3])
            })
            .unwrap();
        work.begin_step().unwrap();
        work.reserve("outer-scratch", Some(7)).unwrap();
        Err::<(), _>("outer refusal")
    });
    assert_eq!(refused, Err("outer refusal"));
    assert_eq!(work.budget.retention_checkpoint(), (8, 0));
    assert_eq!(work.usage().work_bytes, 32);
    assert_eq!(work.usage().peak_retained_bytes, 20);
    assert!(Arc::ptr_eq(&work.identity, &identity));

    // Both the pre-entry reservation and the successful outer output remain charged.
    work.retain_preparation_storage(12).unwrap();
    assert!(work.retain_preparation_storage(1).is_err());
    assert_eq!(work.usage().work_bytes, 44);
    assert_eq!(work.usage().peak_retained_bytes, 20);
}
