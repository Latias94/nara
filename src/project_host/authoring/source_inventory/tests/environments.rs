use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_scene::{PrefabInstance, SceneEntityId, SceneEntityRecord};

use super::*;

const SHARED: &str = "scenes/shared.scene.json";
const ALIAS: &str = "item.prefab.ron";
const DEFAULT_PREFAB: &str = "prefabs/item.prefab.ron";
const ALTERNATE_PREFAB: &str = "alternate/prefabs/item.prefab.ron";

fn shared_profile_fixture() -> Fixture {
    let project = Fixture::new(
        "stable_metadata",
        r#"
[profiles.alternate.paths]
assets = "alternate/assets"
prefabs = "alternate/prefabs"
"#,
    );
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("item").unwrap());
    anchor.prefab = Some(PrefabInstance::forest(AssetRef::path(ALIAS).unwrap()));
    project.write(
        SHARED,
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap()
            .as_bytes(),
    );
    for path in [DEFAULT_PREFAB, ALTERNATE_PREFAB] {
        project.prefab(path);
    }
    for (path, alias, id, kind) in [
        (
            SHARED,
            "shared.scene.json",
            FIRST_ID,
            AssetSourceKind::Scene,
        ),
        (DEFAULT_PREFAB, ALIAS, SECOND_ID, AssetSourceKind::Prefab),
        (
            ALTERNATE_PREFAB,
            ALIAS,
            "94692c80-85a1-49fb-bcb8-4ae59ea50d9a",
            AssetSourceKind::Prefab,
        ),
        (
            "assets/image.png",
            "image.png",
            "7e786936-d69e-4f34-ae57-1f20bde32b94",
            AssetSourceKind::Image,
        ),
        (
            "alternate/assets/image.png",
            "image.png",
            "c1bc9d64-d5cb-40ea-bb3e-c16719f43b42",
            AssetSourceKind::Image,
        ),
    ] {
        project.metadata(path, alias, id, kind, 0);
    }
    project
}

#[test]
fn actual_all_profile_capture_retains_shared_physical_sources_and_distinct_alias_scopes() {
    let project = shared_profile_fixture();
    let schema = project.receipt(Some("alternate"));
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(
        paths(&inventory),
        [ALTERNATE_PREFAB, DEFAULT_PREFAB, SHARED]
    );
    assert_eq!(inventory.metadata().len(), 5);
    assert_eq!(
        inventory
            .configurations()
            .iter()
            .map(ConfigurationPolicy::profile_name)
            .collect::<Vec<_>>(),
        [None, Some("alternate")],
    );
    for (index, expected) in [DEFAULT_PREFAB, ALTERNATE_PREFAB].into_iter().enumerate() {
        let configuration = &inventory.configurations()[index];
        assert_eq!(
            configuration.participant_paths().collect::<Vec<_>>(),
            [expected, SHARED],
        );
        assert_eq!(configuration.native_scopes().len(), 4);
        let metadata = &inventory.metadata[expected];
        assert_eq!(metadata.visibility().len(), 1);
        let visibility = &metadata.visibility()[0];
        assert_eq!(visibility.configuration_index(), index);
        let binding =
            &configuration.catalogue_bindings()[visibility.catalogue_binding_index().unwrap()];
        assert_eq!(visibility.scope(), binding.scope());
        assert_eq!(
            binding.scope().relative_path(metadata.record().path()),
            Some(ALIAS),
        );
        // These are the existing catalogue/native owners' actual captured bindings and policy,
        // rather than a new extraction resolver or a selected-profile alias table.
        let path = binding.scope().resolve(&AssetPath::new(ALIAS).unwrap());
        assert_eq!(path.as_str(), expected);
        let participant = inventory.participant(expected).unwrap();
        let native = configuration
            .native_policy()
            .admit(
                &schema.project_root(),
                path.as_str(),
                SourceDocumentKind::Prefab,
                participant.file().identity(),
            )
            .unwrap();
        assert_eq!(native.key(), participant.source_key());
    }
    assert_ne!(
        inventory.participants[DEFAULT_PREFAB].source_key(),
        inventory.participants[ALTERNATE_PREFAB].source_key(),
    );
    let shared = &inventory.metadata[SHARED];
    assert_eq!(shared.policy, 0);
    assert_eq!(
        shared
            .visibility()
            .iter()
            .map(ConfigurationRecordPolicy::configuration_index)
            .collect::<Vec<_>>(),
        [0, 1],
    );
    for (path, configuration) in [("assets/image.png", 0), ("alternate/assets/image.png", 1)] {
        let metadata = &inventory.metadata[path];
        assert_eq!(metadata.visibility().len(), 1);
        assert_eq!(
            metadata.visibility()[0].configuration_index(),
            configuration
        );
    }
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    inventory.validate(&schema, &[], &mut budget).unwrap();
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert_eq!(budget.usage().retained_open_handles(), handles);
}

#[cfg(target_os = "linux")]
#[test]
fn external_native_sidecar_spelling_participates_in_namespace_revalidation() {
    let project = Fixture::new("stable_metadata", "[paths]\nassets = \"Scripts\"\n");
    let path = "scripts/level.scene.json";
    project.scene(path);
    project.metadata(
        path,
        "level.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    fs::create_dir_all(project.root.join("Scripts")).unwrap();
    let schema = project.receipt(None);
    let root = schema.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let bytes = file.read_to_end_bounded(65_536).unwrap();
    let native = NativeDocumentSourcePolicy::new(schema.settings())
        .admit(&root, path, SourceDocumentKind::Scene, file.identity())
        .unwrap();
    let source = AuthoringDocumentSourceFact::from_native_observation(
        path,
        file.identity(),
        file.parent_identity(),
        ContentDigest::of_bytes(&bytes),
        &native,
    )
    .unwrap();
    let inputs = [OpenSource::Bound(&source)];
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &inputs, SceneFileLimits::default(), &mut budget)
            .unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    project.write("Scripts/level.scene.json.META", b"independent regular file");
    assert!(matches!(
        inventory.validate(&schema, &inputs, &mut budget),
        Err(SourceInventoryError::PathConflict)
    ));
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert_eq!(budget.usage().retained_open_handles(), handles);
}

#[test]
fn original_work_budget_is_exact_for_all_profile_capture_and_context_comparison() {
    let project = shared_profile_fixture();
    let schema = project.receipt(None);
    let mut measured = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut measured).unwrap();
    let exact = measured.usage().decode_work();
    drop(inventory);
    for maximum in [exact - 1, exact] {
        let mut budget = schema.budget;
        budget.limits.decode_work = ItemLimit::new(maximum).unwrap();
        let result =
            SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget);
        if maximum == exact {
            result.unwrap();
            assert_eq!(budget.usage().decode_work(), exact);
        } else {
            // A final digest caps its read at remaining work; that existing owner reports
            // ByteLimitExceeded when the last byte is unavailable under the original meter.
            match result {
                Err(SourceInventoryError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: actual },
                )) => assert_eq!(actual, maximum),
                Err(SourceInventoryError::Host(FsError::ByteLimitExceeded { .. })) => {
                    assert!(budget.usage().decode_work() <= maximum);
                }
                other => panic!("one-short capture did not refuse its work bound: {other:?}"),
            }
            assert_eq!(
                budget.usage().retained_candidate_bytes(),
                schema.budget.retained_candidate_bytes,
            );
            assert_eq!(
                budget.usage().retained_open_handles(),
                schema.budget.retained_open_handles,
            );
        }
    }
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let before = budget;
    inventory.validate(&schema, &[], &mut budget).unwrap();
    let exact = budget.usage().decode_work();
    for maximum in [exact - 1, exact] {
        let mut budget = before;
        budget.limits.decode_work = ItemLimit::new(maximum).unwrap();
        let result = inventory.validate(&schema, &[], &mut budget);
        if maximum == exact {
            result.unwrap();
        } else {
            assert!(matches!(
                result,
                Err(SourceInventoryError::Admission(
                    AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: actual },
                )) if actual == maximum,
            ));
        }
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes,
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles,
        );
    }
}

#[test]
fn open_native_scripts_visibility_uses_every_admitting_configuration_and_keeps_others_out() {
    let project = Fixture::new(
        "stable_metadata",
        r#"
[paths]
scripts = "scripts/default"
[profiles.alternate.paths]
scripts = "scripts/alternate"
[profiles.shared.paths]
scripts = "scripts/alternate"
"#,
    );
    let path = "scripts/alternate/level.scene.json";
    project.scene(path);
    project.metadata(
        path,
        "level.scene.json",
        FIRST_ID,
        AssetSourceKind::Scene,
        0,
    );
    let schema = project.receipt(Some("alternate"));
    let source = project.open_source(&schema, path);
    let inputs = [OpenSource::Bound(&source)];
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &inputs, SceneFileLimits::default(), &mut budget)
            .unwrap();
    assert_eq!(paths(&inventory), [path]);
    assert_eq!(inventory.configurations()[0].participant_paths().len(), 0);
    for index in [1, 2] {
        assert_eq!(
            inventory.configurations()[index]
                .participant_paths()
                .collect::<Vec<_>>(),
            [path],
        );
    }
    let metadata = &inventory.metadata[path];
    assert_eq!(metadata.policy, 1);
    assert_eq!(metadata.visibility().len(), 2);
    for (policy, index) in metadata.visibility().iter().zip([1, 2]) {
        assert_eq!(policy.configuration_index(), index);
        assert_eq!(policy.catalogue_binding_index(), None);
        assert_eq!(
            policy.scope().relative_path(metadata.record().path()),
            Some("level.scene.json"),
        );
        assert_eq!(
            policy.scope().prefix().unwrap().as_str(),
            "scripts/alternate"
        );
    }
    inventory.validate(&schema, &inputs, &mut budget).unwrap();
}

#[test]
fn open_project_locations_keep_the_selected_context_without_implying_unrelated_profiles() {
    let project = Fixture::new(
        "project_location",
        r#"
[profiles.alternate.paths]
assets = "alternate/assets"
scenes = "alternate/scenes"
prefabs = "alternate/prefabs"
[profiles.unrelated.paths]
assets = "unrelated/assets"
scenes = "unrelated/scenes"
prefabs = "unrelated/prefabs"
"#,
    );
    let schema = project.receipt(Some("alternate"));
    for (path, in_default) in [
        ("unscoped/level.scene.json", false),
        ("scenes/default.scene.json", true),
    ] {
        project.scene(path);
        // An actual accepted native/file observation may outlive a change in Browser roots.
        let file = schema
            .project_root()
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let bytes = file
            .read_to_end_bounded(SceneFileLimits::default().encoded_bytes().get() as u64)
            .unwrap();
        let native = NativeDocumentSourcePolicy::new(schema.settings())
            .admit(
                &schema.project_root(),
                path,
                SourceDocumentKind::Scene,
                file.identity(),
            )
            .unwrap();
        let source = AuthoringDocumentSourceFact::from_native_observation(
            path,
            file.identity(),
            file.parent_identity(),
            ContentDigest::of_bytes(&bytes),
            &native,
        )
        .unwrap();
        let inputs = [OpenSource::Bound(&source)];
        let mut budget = schema.budget;
        let inventory =
            SourceInventory::capture(&schema, &inputs, SceneFileLimits::default(), &mut budget)
                .unwrap();
        assert_eq!(paths(&inventory), [path]);
        assert_eq!(
            inventory.configurations()[0].participant_paths().len(),
            usize::from(in_default),
        );
        assert_eq!(
            inventory.configurations()[1]
                .participant_paths()
                .collect::<Vec<_>>(),
            [path],
        );
        assert_eq!(inventory.configurations()[2].participant_paths().len(), 0);
        assert_eq!(
            inventory.participant(path).unwrap().source_key(),
            source.source_key(),
        );
        inventory.validate(&schema, &inputs, &mut budget).unwrap();
        if !in_default {
            // A real receipt selecting another configuration has the same manifest, physical
            // members and location key, but changes this unscoped source's actual context.
            let unrelated_authority = project.receipt(None);
            assert!(matches!(
                inventory.validate(&unrelated_authority, &inputs, &mut budget),
                Err(SourceInventoryError::AuthorityChanged),
            ));
            // Select another profile through the same live capability session, so only
            // configuration membership changes rather than the filesystem authority itself.
            let default_schema = AuthoringProjectAdmissionHost::default()
                .prepare(schema.project_root(), None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish();
            let changed = inventory.validate(&default_schema, &inputs, &mut budget);
            assert!(
                matches!(changed, Err(SourceInventoryError::MembershipChanged)),
                "{changed:?}; {:?}",
                budget.usage()
            );
        }
    }
}

#[test]
fn frozen_revalidation_reuses_admitted_wire_storage_and_keeps_cumulative_work() {
    let project = Fixture::new("project_location", "");
    let path = "scenes/frozen.scene.json";
    let bytes = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("original").unwrap(),
    )])
    .to_json_string()
    .unwrap();
    project.write(path, bytes.as_bytes());
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let original = inventory.participant(path).unwrap().bytes().as_ptr();
    let before = budget.usage();
    // No new wire body may be acquired under this allowance. Revalidation still reads/hash
    // checks current files and spends work; it does not construct another encoded corpus.
    budget.limits.aggregate_encoded_bytes =
        ByteLimit::new(before.aggregate_encoded_bytes()).unwrap();
    let mut spent = before.decode_work();
    for _ in 0..3 {
        inventory.validate(&schema, &[], &mut budget).unwrap();
        assert_eq!(
            budget.usage().aggregate_encoded_bytes(),
            before.aggregate_encoded_bytes()
        );
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(budget.usage().decode_work() > spent);
        spent = budget.usage().decode_work();
        assert_eq!(
            inventory.participant(path).unwrap().bytes().as_ptr(),
            original
        );
    }
    let changed = bytes.replace("original", "modified");
    assert_eq!(changed.len(), bytes.len());
    project.write(path, changed.as_bytes());
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceInventoryError::SourceChanged)
    ));
    assert_eq!(
        budget.usage().aggregate_encoded_bytes(),
        before.aggregate_encoded_bytes()
    );
    assert_eq!(
        inventory.participant(path).unwrap().bytes(),
        bytes.as_bytes()
    );
}

#[test]
fn unrelated_regular_files_do_not_acquire_source_membership_during_revalidation() {
    let project = Fixture::new("project_location", "");
    project.scene("scenes/level.scene.json");
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    project.write("assets/notes.txt", b"not an authored source");
    inventory.validate(&schema, &[], &mut budget).unwrap();
    assert_eq!(paths(&inventory), ["scenes/level.scene.json"]);
    project.scene("scenes/new.scene.json");
    assert!(matches!(
        inventory.validate(&schema, &[], &mut budget),
        Err(SourceInventoryError::MembershipChanged)
    ));
}

#[test]
fn ready_revalidation_checks_retained_manifest_and_bundle_names_and_contents() {
    for (path, mode) in [
        ("nara.toml", 0),
        ("nara.toml", 1),
        ("nara.toml", 2),
        ("schema/authoring.bundle.json", 0),
        ("schema/authoring.bundle.json", 1),
        ("schema/authoring.bundle.json", 2),
    ] {
        let project = Fixture::new("project_location", "");
        project.scene("scenes/level.scene.json");
        let schema = project.receipt(None);
        let mut budget = schema.budget;
        let inventory =
            SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget)
                .unwrap();
        let before = budget.usage();
        let mut bytes = fs::read(project.root.join(path)).unwrap();
        if mode == 1 {
            let retired = project.root.join("retired-schema-source");
            fs::rename(project.root.join(path), retired).unwrap();
        } else if mode == 0 {
            bytes.push(b' ');
        } else {
            let index = bytes
                .iter()
                .position(|byte| byte.is_ascii_alphabetic())
                .unwrap();
            bytes[index] = if bytes[index] == b'x' { b'y' } else { b'x' };
        }
        fs::write(project.root.join(path), bytes).unwrap();
        let result = inventory.validate(&schema, &[], &mut budget);
        if mode == 2 {
            assert!(
                matches!(result, Err(SourceInventoryError::SourceChanged)),
                "{path}: {result:?}"
            );
        } else {
            assert!(result.is_err(), "{path}; mode={mode}");
        }
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            before.retained_open_handles()
        );
        assert!(budget.usage().decode_work() > before.decode_work());
    }
}
