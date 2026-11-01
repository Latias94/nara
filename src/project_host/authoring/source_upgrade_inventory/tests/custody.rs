use nara_core::ItemLimit;

use super::*;

fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
}

fn actual_capture(
    schema: &AuthoringSchemaReceipt,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceUpgradeInventory, SourceUpgradeInventoryError> {
    SourceUpgradeInventory::capture(schema, &[], SceneFileLimits::default(), budget)
}

fn replace_file(project: &Fixture, path: &str, bytes: &[u8]) {
    let temporary = project.root.join("replacement.tmp");
    fs::write(&temporary, bytes).unwrap();
    fs::remove_file(project.root.join(path)).unwrap();
    fs::rename(temporary, project.root.join(path)).unwrap();
}

#[test]
fn original_capture_retains_first_read_leaves_metadata_exact_parents_and_missing_ancestors() {
    let project = Fixture::new(
        "project_location",
        "[profiles.dormant.paths]\nscenes = 'dormant/deeper/scenes'",
    );
    fs::create_dir(project.root.join("dormant")).unwrap();
    project.scene("scenes/nested/one.scene.json");
    project.scene("scenes/nested/two.scene.json");
    project.prefab("prefabs/base.prefab.ron");
    project.metadata(
        "assets/image.png",
        "image.png",
        FIRST_ID,
        AssetSourceKind::Image,
        0,
    );
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let retained = budget.usage().retained_open_handles();
    assert_eq!(retained, inventory.retained_handle_count());
    assert_eq!(
        inventory.custody.files.len(),
        inventory.participants().len() + inventory.metadata().len()
    );
    for fact in inventory
        .participants()
        .map(SourceUpgradeParticipant::file)
        .chain(inventory.metadata().map(SourceUpgradeMetadata::file))
    {
        let file = inventory.shared_file(fact.path()).unwrap();
        assert!(Arc::ptr_eq(
            &file,
            &inventory.shared_file(fact.path()).unwrap()
        ));
        assert_eq!(file.identity(), fact.identity());
        assert_eq!(file.parent_identity(), fact.parent_identity());
        file.verify_digest(fact.digest(), fact.digest().length())
            .unwrap();
        let parent = inventory.shared_parent(fact.path()).unwrap();
        assert_eq!(Some(parent.identity()), fact.parent_identity());
    }
    let first_parent = inventory
        .shared_parent("scenes/nested/one.scene.json")
        .unwrap();
    let second_parent = inventory
        .shared_parent("scenes/nested/two.scene.json")
        .unwrap();
    assert!(first_parent.same_object(&second_parent).unwrap());
    assert_eq!(
        inventory
            .custody
            .directories
            .keys()
            .filter(|path| path.as_str() == "scenes/nested")
            .count(),
        1
    );
    assert!(
        inventory
            .shared_parent(schema.bundle_path().as_str())
            .is_some()
    );
    assert!(inventory.custody.directories.contains_key("dormant"));
    assert!(!inventory.custody.directories.contains_key("dormant/deeper"));
    assert_eq!(budget.usage().retained_open_handles(), retained);
    assert_eq!(
        inventory.shared_parent("nara.toml").unwrap().identity(),
        schema.root_identity()
    );
}

#[test]
fn shared_custody_keeps_every_pin_alive_after_the_reviewed_inventory_drops() {
    let project = Fixture::new(
        "project_location",
        "[profiles.dormant.paths]\nscenes = 'dormant/deeper/scenes'",
    );
    fs::create_dir(project.root.join("dormant")).unwrap();
    let path = "scenes/nested/deeper/closed.scene.json";
    project.scene(path);
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let handles = inventory.retained_handle_count();
    let token = inventory.shared_custody();
    let weak = Arc::downgrade(&inventory.custody);
    assert!(Arc::ptr_eq(&inventory.custody, &token.inner));
    drop(inventory);
    assert!(weak.upgrade().is_some());
    for directory in [
        "scenes",
        "scenes/nested",
        "scenes/nested/deeper",
        "schema",
        "dormant",
    ] {
        assert!(token.inner.directories.contains_key(directory));
    }
    let file = token.file(path).unwrap();
    assert_eq!(
        file.parent_identity(),
        Some(token.parent(path).unwrap().identity())
    );
    assert_eq!(
        file.read_to_end_bounded(4096).unwrap(),
        fs::read(project.root.join(path)).unwrap()
    );
    assert!(token.parent(schema.bundle_path().as_str()).is_some());
    let second = token.clone();
    assert!(Arc::ptr_eq(&second.inner, &token.inner));
    assert_eq!(budget.usage().retained_open_handles(), handles);
    drop(second);
    drop(token);
    assert!(weak.upgrade().is_none());
    // Only the last actual shared owner dropping ends the original retained allowance.
    budget.release_open_handles(handles);
    assert_eq!(budget.usage().retained_open_handles(), 0);
}

#[test]
fn continuously_retained_closed_source_rejects_same_bytes_at_a_new_native_leaf() {
    let project = Fixture::new("project_location", "");
    let path = "scenes/closed.scene.json";
    project.scene(path);
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let original = inventory.shared_file(path).unwrap();
    let bytes = original.read_to_end_bounded(4096).unwrap();
    replace_file(&project, path, &bytes);
    let current = schema
        .project_root()
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    // The original remains live even after unlink. Native identity reuse is excluded by that
    // lifetime, rather than by hoping the filesystem assigns a different unused ID.
    assert_eq!(original.read_to_end_bounded(4096).unwrap(), bytes);
    assert!(!original.same_object(&current).unwrap());
    assert_ne!(original.identity(), current.identity());
    let handles = budget.usage().retained_open_handles();
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
}

#[test]
fn metadata_native_identity_is_backed_by_the_same_continuously_live_first_read() {
    let project = Fixture::new("project_location", "");
    let path = "assets/image.png.meta";
    project.metadata(
        "assets/image.png",
        "image.png",
        FIRST_ID,
        AssetSourceKind::Image,
        0,
    );
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let original = inventory.shared_file(path).unwrap();
    let bytes = original.read_to_end_bounded(4096).unwrap();
    replace_file(&project, path, &bytes);
    assert_eq!(original.read_to_end_bounded(4096).unwrap(), bytes);
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
}

#[test]
fn replaced_missing_ancestor_is_a_conflict_even_when_the_child_is_still_missing() {
    let project = Fixture::new(
        "project_location",
        "[profiles.dormant.paths]\nscenes = 'dormant/scenes'",
    );
    fs::create_dir(project.root.join("dormant")).unwrap();
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let ancestor = inventory.shared_parent("dormant/unused.json").unwrap();
    fs::rename(
        project.root.join("dormant"),
        project.root.join("previous-dormant"),
    )
    .unwrap();
    fs::create_dir(project.root.join("dormant")).unwrap();
    let current = schema
        .project_root()
        .open_directory_exact(&RelativePath::new("dormant").unwrap())
        .unwrap();
    assert!(!ancestor.same_object(&current).unwrap());
    let handles = budget.usage().retained_open_handles();
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
}

#[test]
fn real_handle_peak_is_admitted_exactly_and_one_short_refuses_before_any_pending_write() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/nested/one.scene.json");
    project.metadata(
        "assets/image.png",
        "image.png",
        FIRST_ID,
        AssetSourceKind::Image,
        0,
    );
    let schema = project.receipt(None);
    let mut measured = budget();
    let inventory = actual_capture(&schema, &mut measured).unwrap();
    let exact = measured.usage().peak_open_handles();
    let retained = inventory.retained_handle_count();
    assert!(exact > retained);
    assert!(
        exact
            <= AuthoringProjectAdmissionLimits::default()
                .peak_open_handles()
                .get()
    );
    drop(inventory);
    measured.release_open_handles(retained);
    assert_eq!(measured.usage().retained_open_handles(), 0);
    for maximum in [exact - 1, exact] {
        let mut limited = budget();
        limited.limits.peak_open_handles = ItemLimit::new(maximum).unwrap();
        let result = actual_capture(&schema, &mut limited);
        if maximum == exact {
            let inventory = result.unwrap();
            assert_eq!(limited.usage().peak_open_handles(), exact);
            let handles = inventory.retained_handle_count();
            assert_eq!(limited.usage().retained_open_handles(), handles);
            drop(inventory);
            limited.release_open_handles(handles);
        } else {
            assert!(matches!(result, Err(SourceUpgradeInventoryError::Admission(
                AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: actual }
            )) if actual == maximum));
        }
        assert_eq!(limited.usage().retained_open_handles(), 0);
        let bytes = fs::read(project.root.join("nara.toml")).unwrap();
        assert_eq!(
            ProjectManifest::parse_toml_bytes(&bytes)
                .manifest
                .unwrap()
                .authoring
                .state,
            ProjectAuthoringState::Ready
        );
    }
}

#[test]
fn confirmation_drops_its_handles_and_storage_on_success_and_membership_refusal() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/one.scene.json");
    let schema = project.receipt(None);
    let mut budget = budget();
    let inventory = actual_capture(&schema, &mut budget).unwrap();
    let handles = budget.usage().retained_open_handles();
    let bytes = budget.usage().retained_candidate_bytes();
    let first_work = budget.usage().decode_work();
    inventory.validate(&schema, &[], &mut budget).unwrap();
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(budget.usage().retained_candidate_bytes(), bytes);
    assert!(budget.usage().decode_work() > first_work);
    project.scene("scenes/late.scene.json");
    let work = budget.usage().decode_work();
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(budget.usage().retained_candidate_bytes(), bytes);
    assert!(budget.usage().decode_work() > work);
}

#[test]
fn failed_capture_closes_all_new_handles_before_refunding_their_allowance() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/one.scene.json");
    project.write("assets/image.png.meta", b"invalid metadata");
    let schema = project.receipt(None);
    let mut budget = budget();
    assert!(matches!(
        actual_capture(&schema, &mut budget),
        Err(SourceUpgradeInventoryError::MetadataFormat { path, .. }) if path == "assets/image.png.meta"
    ));
    assert_eq!(budget.usage().retained_open_handles(), 0);
    assert_eq!(budget.usage().retained_candidate_bytes(), 0);
    assert!(budget.usage().decode_work() > 0);
}
