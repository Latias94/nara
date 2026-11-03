use super::*;

#[test]
fn disposal_releases_only_its_captured_corpus_and_keeps_every_native_pin_charged() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let open = fixture.future_open(schema, ready.basis());
    let mut budget = operation_budget();
    let earlier_bytes = 4_096;
    reserve(&mut budget, earlier_bytes).unwrap();
    let proof = verified(&fixture, schema, &[OpenSource::Bound(&open)], &mut budget).unwrap();
    let own_bytes = proof.retained_bytes;
    let custody_bytes = proof.inventory.shared_custody().retained_bytes();
    assert!(own_bytes > custody_bytes);
    assert!(custody_bytes > 0);
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        earlier_bytes + own_bytes,
    );
    let handles = budget.usage().retained_open_handles();
    let first_leaf = proof.inventory.shared_file(CLOSED).unwrap();
    let first_parent = proof.inventory.shared_parent(CLOSED).unwrap();
    let shared = proof.inventory.shared_custody();
    let later_bytes = 8_192;
    budget.charge_source_bytes(0, 0, 113, later_bytes).unwrap();
    let disposal = proof.prepare_disposal(&mut budget).unwrap();
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        earlier_bytes
            + own_bytes
            + later_bytes
            + size_of::<PreparedSourceUpgradeInventoryDisposal>(),
    );
    let before = budget.usage();
    // Admission would refuse this exhausted allowance. The prepared transfer must only drop
    // its corpus and move existing ownership; it neither admits work nor reopens any source.
    budget.limits.decode_work = nara_core::ItemLimit::new(1).unwrap();
    let custody = proof.into_custody(disposal, &mut budget);
    let mut expected = before;
    expected.retained_candidate_bytes = earlier_bytes + later_bytes + custody_bytes;
    assert_eq!(budget.usage(), expected);
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert!(custody.same_owner(&shared));
    assert!(
        custody
            .file(CLOSED)
            .unwrap()
            .same_object(&first_leaf)
            .unwrap()
    );
    assert!(
        custody
            .parent(CLOSED)
            .unwrap()
            .same_object(&first_parent)
            .unwrap()
    );
    assert_eq!(
        custody.file_count(),
        fixture.original.participants().len() + fixture.original.metadata().len(),
    );
    for directory in [
        "schema",
        "scenes",
        "prefabs",
        "assets",
        "other",
        "other/scenes",
    ] {
        assert!(custody.has_directory(directory));
    }
    assert!(custody.parent(BUNDLE).is_some());
    drop(custody);
    assert_eq!(budget.usage(), expected);
    assert_eq!(
        shared.file(CLOSED).unwrap().parent_identity(),
        Some(shared.parent(CLOSED).unwrap().identity()),
    );
}

#[test]
fn disposal_record_preflight_is_exact_and_refusal_preserves_the_same_inventory() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
    let custody_bytes = proof.inventory.shared_custody().retained_bytes();
    let before = budget.usage();
    let record_bytes = size_of::<PreparedSourceUpgradeInventoryDisposal>();
    let exact = before.retained_candidate_bytes() + record_bytes;
    budget.limits.retained_candidate_bytes = ByteLimit::new(exact - 1).unwrap();
    assert!(matches!(
        proof.prepare_disposal(&mut budget),
        Err(SourceUpgradeInventoryError::Inventory(SourceInventoryError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum },
        ))) if maximum == exact - 1,
    ));
    assert_eq!(budget.usage(), before);
    assert_eq!(
        proof.inventory.shared_file(CLOSED).unwrap().identity(),
        proof
            .inventory
            .participant(CLOSED)
            .unwrap()
            .file()
            .identity(),
    );
    budget.limits.retained_candidate_bytes = ByteLimit::new(exact).unwrap();
    let disposal = proof.prepare_disposal(&mut budget).unwrap();
    assert_eq!(budget.usage().retained_candidate_bytes(), exact);
    let custody = proof.into_custody(disposal, &mut budget);
    let mut expected = before;
    expected.retained_candidate_bytes = custody_bytes;
    assert_eq!(budget.usage(), expected);
    assert!(custody.file(CLOSED).is_some());
}
