use nara_core::ItemLimit;

use super::*;

#[test]
fn same_and_reordered_complete_inputs_use_captured_facts_and_scene_limits() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/first.scene.json");
    project.scene("scenes/second.scene.json");
    project.prefab("prefabs/closed.prefab.ron");
    let schema = project.receipt(None);
    let first = project.open_source(&schema, "scenes/first.scene.json");
    let second = project.open_source(&schema, "scenes/second.scene.json");
    let inputs = [
        SourceUpgradeOpenSource::Bound(&first),
        SourceUpgradeOpenSource::Bound(&second),
    ];
    let limits = SceneFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(65_536).unwrap())
        .with_components(ItemLimit::new(128).unwrap());
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(&schema, &inputs, limits, &mut budget).unwrap();
    assert_eq!(inventory.scene_limits(), limits);

    // Preparation consumes the captured baseline. Only confirmation re-enumerates current disk.
    project.write("prefabs/closed.prefab.ron", b"external edit");
    project.scene("scenes/late.scene.json");
    inventory
        .validate_preparation_inputs(&schema, &inputs, &mut budget)
        .unwrap();
    inventory
        .validate_preparation_inputs(&schema, &[inputs[1], inputs[0]], &mut budget)
        .unwrap();
}

#[test]
fn empty_complete_inputs_are_valid_only_when_none_were_captured_open() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/closed.scene.json");
    let schema = project.receipt(None);
    let open = project.open_source(&schema, "scenes/closed.scene.json");
    let mut budget = schema.budget;
    let inventory =
        SourceUpgradeInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
            .unwrap();
    inventory
        .validate_preparation_inputs(&schema, &[], &mut budget)
        .unwrap();
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&open)],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn omitted_sources_and_repeated_sources_replacing_an_omission_refuse() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/first.scene.json");
    project.scene("scenes/second.scene.json");
    let schema = project.receipt(None);
    let first = project.open_source(&schema, "scenes/first.scene.json");
    let second = project.open_source(&schema, "scenes/second.scene.json");
    let inputs = [
        SourceUpgradeOpenSource::Bound(&first),
        SourceUpgradeOpenSource::Bound(&second),
    ];
    let mut budget = schema.budget;
    let inventory =
        SourceUpgradeInventory::capture(&schema, &inputs, SceneFileLimits::default(), &mut budget)
            .unwrap();
    for incomplete in [&[][..], &inputs[..1], &[inputs[0], inputs[0]][..]] {
        assert!(matches!(
            inventory.validate_preparation_inputs(&schema, incomplete, &mut budget),
            Err(SourceUpgradeInventoryError::MembershipChanged)
        ));
    }
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[inputs[0], SourceUpgradeOpenSource::Unbound],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::UnboundOpenSource)
    ));
}

#[test]
fn changed_accepted_identity_parent_and_digest_refuse() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/original.scene.json");
    project.scene("scenes/other.scene.json");
    let schema = project.receipt(None);
    let original = project.open_source(&schema, "scenes/original.scene.json");
    let other = project.open_source(&schema, "scenes/other.scene.json");
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(
        &schema,
        &[SourceUpgradeOpenSource::Bound(&original)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    assert_ne!(original.identity(), other.identity());
    assert_ne!(original.parent_identity(), Some(schema.root_identity()));
    for (identity, parent, digest) in [
        (
            other.identity(),
            original.parent_identity(),
            original.digest(),
        ),
        (
            original.identity(),
            Some(schema.root_identity()),
            original.digest(),
        ),
        (
            original.identity(),
            original.parent_identity(),
            ContentDigest::of_bytes(b"changed accepted bytes"),
        ),
    ] {
        let changed = original
            .with_persistence_observation(identity, parent, digest)
            .unwrap();
        assert!(matches!(
            inventory.validate_preparation_inputs(
                &schema,
                &[SourceUpgradeOpenSource::Bound(&changed)],
                &mut budget,
            ),
            Err(SourceUpgradeInventoryError::OpenSourceChanged)
        ));
    }
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&other)],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn reforged_native_key_with_the_same_path_and_accepted_observation_refuses() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/original.scene.json");
    let schema = project.receipt(None);
    let original = project.open_source(&schema, "scenes/original.scene.json");
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(
        &schema,
        &[SourceUpgradeOpenSource::Bound(&original)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let mut settings = schema.settings().clone();
    settings.project.source_universe = "dd84209a-c14c-49e4-8b13-b3738daf4f53".parse().unwrap();
    let native = NativeDocumentSourcePolicy::new(&settings)
        .admit(
            &schema.project_root(),
            original.relative_path(),
            SourceDocumentKind::Scene,
            original.identity(),
        )
        .unwrap();
    let reforged = AuthoringDocumentSourceFact::from_native_observation(
        original.relative_path(),
        original.identity(),
        original.parent_identity(),
        original.digest(),
        &native,
    )
    .unwrap();
    assert_ne!(reforged.source_key(), original.source_key());
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&reforged)],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::OpenSourceChanged)
    ));
}

#[test]
fn changed_native_metadata_with_the_same_key_and_accepted_observation_refuses() {
    let project = Fixture::new("stable_metadata", "");
    let path = "scenes/original.scene.json";
    project.scene(path);
    project.metadata(
        path,
        "original.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(None);
    let original = project.open_source(&schema, path);
    let mut budget = schema.budget;
    let inventory = SourceUpgradeInventory::capture(
        &schema,
        &[SourceUpgradeOpenSource::Bound(&original)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    project.metadata(
        path,
        "original.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        1,
    );
    let native = NativeDocumentSourcePolicy::new(schema.settings())
        .admit(
            &schema.project_root(),
            path,
            SourceDocumentKind::Scene,
            original.identity(),
        )
        .unwrap();
    let changed = original.with_native_source(native).unwrap();
    assert_eq!(changed.source_key(), original.source_key());
    assert_eq!(changed.identity(), original.identity());
    assert_eq!(changed.parent_identity(), original.parent_identity());
    assert_eq!(changed.digest(), original.digest());
    assert_eq!(changed.encoded_bytes(), original.encoded_bytes());
    assert_ne!(changed.native_source(), original.native_source());
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&changed)],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::OpenSourceChanged)
    ));
    inventory
        .validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&original)],
            &mut budget,
        )
        .unwrap();
}

#[test]
fn another_captured_authority_refuses_even_with_equal_manifest_and_bundle_bytes() {
    let project = Fixture::new("project_location", "");
    let other_project = Fixture::new("project_location", "");
    let schema = project.receipt(None);
    let other_schema = other_project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceUpgradeInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
            .unwrap();
    assert_eq!(schema.manifest_digest(), other_schema.manifest_digest());
    assert_eq!(schema.bundle_digest(), other_schema.bundle_digest());
    assert!(matches!(
        inventory.validate_preparation_inputs(&other_schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::AuthorityChanged)
    ));
}

#[test]
fn comparisons_are_cumulative_and_never_charge_existing_storage_or_handles_again() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/original.scene.json");
    let schema = project.receipt(None);
    let original = project.open_source(&schema, "scenes/original.scene.json");
    let inputs = [SourceUpgradeOpenSource::Bound(&original)];
    let mut budget = schema.budget;
    let inventory =
        SourceUpgradeInventory::capture(&schema, &inputs, SceneFileLimits::default(), &mut budget)
            .unwrap();
    // Exact incumbent ceilings leave no room for reacquisition or a temporary comparison set.
    budget.limits.retained_candidate_bytes =
        ByteLimit::new(budget.usage().retained_candidate_bytes()).unwrap();
    budget.limits.peak_open_handles = ItemLimit::new(budget.usage().peak_open_handles()).unwrap();
    let changed = original
        .with_persistence_observation(
            original.identity(),
            original.parent_identity(),
            ContentDigest::of_bytes(b"changed accepted bytes"),
        )
        .unwrap();
    let before = budget.usage();
    let mut comparison_work = 0;
    for attempt in 0..4 {
        let previous = budget.usage().decode_work();
        if attempt < 2 {
            inventory
                .validate_preparation_inputs(&schema, &inputs, &mut budget)
                .unwrap();
        } else {
            assert!(matches!(
                inventory.validate_preparation_inputs(
                    &schema,
                    &[SourceUpgradeOpenSource::Bound(&changed)],
                    &mut budget,
                ),
                Err(SourceUpgradeInventoryError::OpenSourceChanged)
            ));
        }
        let work = budget.usage().decode_work() - previous;
        assert!(work > 0);
        if attempt == 0 {
            comparison_work = work;
        } else {
            assert_eq!(work, comparison_work);
        }
        let mut usage = budget.usage();
        usage.decode_work = before.decode_work();
        assert_eq!(usage, before);
    }
    budget.limits.decode_work =
        ItemLimit::new(budget.usage().decode_work() + comparison_work - 1).unwrap();
    assert!(matches!(
        inventory.validate_preparation_inputs(
            &schema,
            &[SourceUpgradeOpenSource::Bound(&changed)],
            &mut budget,
        ),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    let mut usage = budget.usage();
    usage.decode_work = before.decode_work();
    assert_eq!(usage, before);
}
