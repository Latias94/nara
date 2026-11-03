use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetProductRef, ImportedProductId};
use nara_core::ItemLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::AuthoringSchemaBundleOwner;
use nara_scene::SourceDocumentKind;

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringSchemaReceipt, document::AuthoringDocumentSourceFact,
    source_inventory::OpenSource,
};

const DEFAULT_PREFAB: &str = "9cefa463-fc82-46ad-b61b-77283280533f";
const ALTERNATE_PREFAB: &str = "8f899409-fd2d-494b-93fb-8edbe012032b";
const DEFAULT_IMAGE: &str = "7e786936-d69e-4f34-ae57-1f20bde32b94";
const ALTERNATE_IMAGE: &str = "c1bc9d64-d5cb-40ea-bb3e-c16719f43b42";
const SHARED_SCENE: &str = "ad322a15-01a2-4c04-847f-83e4df33f139";
const PLANNED: &str = "94692c80-85a1-49fb-bcb8-4ae59ea50d9a";
const ALIAS: &str = "item.prefab.ron";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn actual_configuration_lookups_keep_captured_facts_after_live_metadata_changes_without_io() {
    let project = Fixture::profiled();
    let schema = project.receipt(Some("alternate"));
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(inventory.configurations()[0].profile_name(), None);
    assert_eq!(
        inventory.configurations()[1].profile_name(),
        Some("alternate"),
    );
    // Each baseline catalogue is valid independently. An alternate source outside default
    // roots does not acquire default visibility through its stable identity.
    let alternate_image = inventory
        .metadata()
        .find(|metadata| metadata.record().path().as_str() == "alternate/assets/image.png")
        .unwrap();
    assert_eq!(alternate_image.visibility().len(), 1);
    assert_eq!(alternate_image.visibility()[0].configuration_index(), 1);
    assert!(
        inventory.configurations()[0].catalogue_bindings()[0]
            .scope()
            .relative_path(alternate_image.record().path())
            .is_none()
    );
    let before = budget.usage();
    // Exhaust the file-observation allowance. Construction and every lookup must use captured
    // facts even though the live sidecars are now invalid and a new identity is discoverable.
    budget.limits.unique_files = ItemLimit::new(before.unique_files()).unwrap();
    for metadata in inventory.metadata() {
        project.write(metadata.file().path(), b"{");
    }
    project.metadata(
        "assets/late.png",
        "late.png",
        PLANNED,
        AssetSourceKind::Image,
    );
    for (configuration, prefix, prefab, image, hidden_prefab, hidden_image) in [
        (
            0,
            "",
            DEFAULT_PREFAB,
            DEFAULT_IMAGE,
            ALTERNATE_PREFAB,
            ALTERNATE_IMAGE,
        ),
        (
            1,
            "alternate/",
            ALTERNATE_PREFAB,
            ALTERNATE_IMAGE,
            DEFAULT_PREFAB,
            DEFAULT_IMAGE,
        ),
    ] {
        let mut catalogue = ReferenceCatalogue::from_inventory(
            schema.project_root(),
            &inventory,
            configuration,
            &mut budget,
        )
        .unwrap();
        assert!(catalogue.is_indexed());
        assert_eq!(catalogue.database().len(), 3);
        assert_eq!(catalogue.metadata.len(), 3);
        let expected_prefab = format!("{prefix}prefabs/{ALIAS}");
        for reference in [
            AssetRef::path(ALIAS).unwrap(),
            stable(prefab),
            primary(prefab),
        ] {
            let selected = catalogue
                .resolve_prefab(&reference, &mut budget)
                .unwrap()
                .unwrap();
            assert_eq!(selected.path.as_str(), ALIAS);
            assert_eq!(
                selected.binding.scope().resolve(&selected.path).as_str(),
                expected_prefab,
            );
            let participant = inventory.participant(&expected_prefab).unwrap();
            let source = ResolvedPrefabSource::bound(
                Arc::new(PrefabDocument::default()),
                participant.source_key().clone(),
            );
            assert_eq!(
                catalogue
                    .bind_prefab_source(&reference, source)
                    .unwrap()
                    .key(),
                participant.source_key(),
            );
            selected.release(&mut budget).unwrap();
        }
        for reference in [stable(image), primary(image)] {
            let selected = catalogue
                .resolve_resource(&reference, &mut budget)
                .unwrap()
                .unwrap();
            assert_eq!(selected.path.as_str(), "image.png");
            let expected_assets = if configuration == 0 {
                "assets"
            } else {
                "alternate/assets"
            };
            assert_eq!(selected.binding.directory().as_str(), expected_assets,);
            selected.release(&mut budget).unwrap();
        }
        for id in [hidden_prefab, hidden_image, PLANNED] {
            for reference in [stable(id), primary(id)] {
                assert!(
                    catalogue
                        .resolve_resource(&reference, &mut budget)
                        .unwrap()
                        .is_none()
                );
            }
        }
        assert!(
            catalogue
                .resolve_prefab(&primary(hidden_prefab), &mut budget)
                .unwrap()
                .is_none()
        );
        catalogue
            .resolve_resource(&stable(SHARED_SCENE), &mut budget)
            .unwrap()
            .unwrap()
            .release(&mut budget)
            .unwrap();
        for (path, fact) in &catalogue.metadata {
            let captured = inventory
                .metadata()
                .find(|metadata| metadata.record().path() == path)
                .unwrap();
            let file = captured.file();
            assert_eq!(fact.kind(), AuthoringSourceKind::AssetMetadata);
            assert_eq!(fact.project_path().as_str(), file.path());
            assert_eq!(fact.identity(), file.identity());
            assert_eq!(fact.parent_identity(), file.parent_identity());
            assert_eq!(fact.digest(), file.digest());
            assert_eq!(fact.encoded_bytes() as u64, file.encoded_bytes());
            assert_eq!(fact.digest(), ContentDigest::of_bytes(captured.bytes()));
        }
        assert_eq!(catalogue.lookup_sources().count(), 0);
        assert!(catalogue.lookups.is_empty());
        project.write(&format!("{expected_prefab}.meta"), b"still invalid");
        catalogue
            .resolve_prefab(&stable(prefab), &mut budget)
            .unwrap()
            .unwrap()
            .release(&mut budget)
            .unwrap();
    }
    let after = budget.usage();
    assert_eq!(after.unique_files(), before.unique_files());
    assert_eq!(after.peak_open_handles(), before.peak_open_handles());
    assert_eq!(
        after.retained_open_handles(),
        before.retained_open_handles()
    );
    assert_eq!(
        after.aggregate_encoded_bytes(),
        before.aggregate_encoded_bytes(),
    );
    assert_eq!(after.decoded_bytes(), before.decoded_bytes());
}

#[test]
fn overlapping_roots_keep_the_existing_actual_scope_kind_and_deduplication_rules() {
    let project = Fixture::new(
        r#"
[paths]
assets = "content"
scenes = "content/scenes"
prefabs = "content/prefabs"
"#,
    );
    project.prefab("content/prefabs/nested/item.prefab.ron");
    project.metadata(
        "content/prefabs/nested/item.prefab.ron",
        "prefabs/nested/item.prefab.ron",
        DEFAULT_PREFAB,
        AssetSourceKind::Prefab,
    );
    project.metadata(
        "content/prefabs/nested/image.bin",
        "nested/image.bin",
        DEFAULT_IMAGE,
        AssetSourceKind::Image,
    );
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(inventory.metadata().len(), 2);
    let mut frozen =
        ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 0, &mut budget)
            .unwrap();
    let mut live = ReferenceCatalogue::new(
        schema.project_root(),
        ReferenceCatalogue::project_bindings(schema.settings()),
        &mut budget,
    )
    .unwrap();
    live.ensure_indexed(&mut budget).unwrap();
    assert_eq!(frozen.database(), live.database());
    assert_eq!(frozen.source_bindings, live.source_bindings);
    assert_eq!(frozen.metadata.len(), live.metadata.len());
    let selected = frozen
        .resolve_prefab(&primary(DEFAULT_PREFAB), &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(selected.binding.directory().as_str(), "content");
    assert_eq!(selected.path.as_str(), "prefabs/nested/item.prefab.ron");
    selected.release(&mut budget).unwrap();
    let selected = frozen
        .resolve_resource(&stable(DEFAULT_IMAGE), &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(selected.binding.directory().as_str(), "content/prefabs");
    assert_eq!(selected.binding.expected_kind, Some(AssetSourceKind::Image));
    assert_eq!(selected.path.as_str(), "nested/image.bin");
    selected.release(&mut budget).unwrap();
    assert!(matches!(
        frozen.resolve_prefab(&stable(DEFAULT_IMAGE), &mut budget),
        Err(AuthoringProjectSourceAdmissionError::NativeSource(_)),
    ));
    let auxiliary = AssetRef::Product(
        AssetProductRef::new(
            DEFAULT_PREFAB.parse().unwrap(),
            ImportedProductId::parse_subproduct("dd195b6a-449c-4e1a-bec5-24aa8d83870c").unwrap(),
        )
        .unwrap(),
    );
    assert!(
        frozen
            .resolve_prefab(&auxiliary, &mut budget)
            .unwrap()
            .is_none()
    );
    for (path, fact) in &frozen.metadata {
        assert!(same_metadata_content(fact, &live.metadata[path]));
        assert!(fact.dependency_depth() > 0);
    }
}

#[test]
fn native_only_scripts_visibility_never_becomes_a_catalogue_binding() {
    let project = Fixture::new(
        r#"
[paths]
scripts = "scripts/default"
[profiles.alternate.paths]
scripts = "scripts/alternate"
"#,
    );
    let path = "scripts/alternate/open.scene.json";
    project.scene(path);
    project.metadata(
        path,
        "open.scene.json",
        SHARED_SCENE,
        AssetSourceKind::Scene,
    );
    let schema = project.receipt(Some("alternate"));
    let root = schema.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let native = NativeDocumentSourcePolicy::new(schema.settings())
        .admit(&root, path, SourceDocumentKind::Scene, file.identity())
        .unwrap();
    let source = AuthoringDocumentSourceFact::from_native_observation(
        path,
        file.identity(),
        file.parent_identity(),
        ContentDigest::of_bytes(&fs::read(project.root.join(path)).unwrap()),
        &native,
    )
    .unwrap();
    let mut budget = schema.budget;
    let inventory = SourceInventory::capture(
        &schema,
        &[OpenSource::Bound(&source)],
        SceneFileLimits::default(),
        &mut budget,
    )
    .unwrap();
    let metadata = inventory.metadata().next().unwrap();
    assert_eq!(metadata.visibility().len(), 1);
    assert_eq!(metadata.visibility()[0].configuration_index(), 1);
    assert_eq!(metadata.visibility()[0].catalogue_binding_index(), None);
    for configuration in 0..inventory.configurations().len() {
        let mut catalogue = ReferenceCatalogue::from_inventory(
            schema.project_root(),
            &inventory,
            configuration,
            &mut budget,
        )
        .unwrap();
        assert!(catalogue.is_indexed());
        assert!(catalogue.database().is_empty());
        assert!(catalogue.metadata.is_empty());
        assert!(
            catalogue
                .resolve_resource(&stable(SHARED_SCENE), &mut budget)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn planned_sidecar_uses_actual_configuration_scopes_without_fabricating_observed_facts() {
    let project = Fixture::profiled();
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let canonical = AssetSourcePath::new("prefabs/new.prefab.ron").unwrap();
    let alias = AssetPath::new("new.prefab.ron").unwrap();
    let proposed = AssetRecord::new(
        PLANNED.parse().unwrap(),
        canonical.clone(),
        AssetSourceKind::Prefab,
    );
    for configuration in 0..inventory.configurations().len() {
        let catalogue = ReferenceCatalogue::from_inventory(
            schema.project_root(),
            &inventory,
            configuration,
            &mut budget,
        )
        .unwrap();
        let visible = ReferenceCatalogue::planned_metadata_binding(
            &catalogue.bindings,
            &canonical,
            &alias,
            &mut budget,
        )
        .unwrap()
        .is_some();
        assert_eq!(visible, configuration == 0);
        let malformed = ReferenceCatalogue::planned_metadata_binding(
            &catalogue.bindings,
            &canonical,
            &AssetPath::new("prefabs/new.prefab.ron").unwrap(),
            &mut budget,
        );
        if configuration == 0 {
            assert!(matches!(
                malformed,
                Err(AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. })
            ));
        } else {
            assert_eq!(malformed.unwrap(), None);
        }
        assert!(catalogue.database().record_for_path(&canonical).is_none());
        assert!(!catalogue.metadata.contains_key(&canonical));
        let prefabs = catalogue.prefabs();
        let existing = catalogue.database().records().next().unwrap();
        let existing_path = existing.path().clone();
        let original_allocation = std::ptr::from_ref(existing);
        let usage = budget.usage();
        let temporary_bytes = catalogue.frozen_storage.as_ref().unwrap().temporary_bytes;
        let mut after = catalogue.into_database(&mut budget).unwrap();
        assert_eq!(
            budget.retained_candidate_bytes,
            usage.retained_candidate_bytes() - temporary_bytes,
        );
        assert_eq!(budget.usage().decode_work(), usage.decode_work());
        assert_eq!(
            std::ptr::from_ref(after.record_for_path(&existing_path).unwrap()),
            original_allocation,
        );
        if visible {
            admit_asset_record(&mut after, &proposed, &mut budget).unwrap();
        }
        for reference in [
            AssetRef::Path(alias.clone()),
            stable(PLANNED),
            primary(PLANNED),
        ] {
            let resolved = after.resolver(prefabs.scope()).resolve_ref(&reference);
            if visible {
                assert_eq!(resolved.unwrap(), &proposed);
            } else {
                assert!(resolved.is_err());
            }
        }
    }
}

#[test]
fn planned_sidecar_distinguishes_malformed_covered_and_outside_empty_configurations() {
    let project = Fixture::new(
        r#"
[paths]
assets = "source-assets"
prefabs = "content/prefabs"
[profiles.alternate.paths]
assets = "content"
scenes = "alternate/scenes"
prefabs = "alternate/prefabs"
"#,
    );
    fs::create_dir_all(project.root.join("content/prefabs")).unwrap();
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert_eq!(inventory.configurations().len(), 2);
    assert!(
        inventory
            .configurations()
            .iter()
            .all(|policy| policy.participant_paths().len() == 0)
    );
    let canonical = AssetSourcePath::new("content/prefabs/new.prefab.ron").unwrap();
    let authored = AssetPath::new("new.prefab.ron").unwrap();
    let before = budget.usage();
    assert_eq!(
        ReferenceCatalogue::planned_metadata_binding(
            inventory.configurations()[0].catalogue_bindings(),
            &canonical,
            &authored,
            &mut budget,
        )
        .unwrap(),
        Some(1),
    );
    for policy in inventory.configurations() {
        // A shared string prefix is not containment in a configured directory.
        assert_eq!(
            ReferenceCatalogue::planned_metadata_binding(
                policy.catalogue_bindings(),
                &AssetSourcePath::new("contentious/new.prefab.ron").unwrap(),
                &authored,
                &mut budget,
            )
            .unwrap(),
            None,
        );
    }
    assert_eq!(
        budget.retained_candidate_bytes,
        before.retained_candidate_bytes()
    );
    assert!(matches!(
        ReferenceCatalogue::planned_metadata_binding(
            inventory.configurations()[1].catalogue_bindings(),
            &canonical,
            &authored,
            &mut budget,
        ),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { requested, declared })
            if requested.as_str() == "prefabs/new.prefab.ron" && declared == authored
    ));
    assert_eq!(budget.usage().unique_files(), before.unique_files());
    assert_eq!(
        budget.usage().aggregate_encoded_bytes(),
        before.aggregate_encoded_bytes()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
    assert_eq!(inventory.metadata().len(), 0);
    assert!(!project.root.join(canonical.as_str()).exists());
    assert!(
        !project
            .root
            .join(format!("{}.meta", canonical.as_str()))
            .exists()
    );

    // The planned refusal agrees with the actual indexer's declaration rules.
    project.prefab(canonical.as_str());
    project.metadata(
        canonical.as_str(),
        authored.as_str(),
        PLANNED,
        AssetSourceKind::Prefab,
    );
    let mut live = ReferenceCatalogue::new(
        schema.project_root(),
        inventory.configurations()[1].catalogue_bindings().to_vec(),
        &mut budget,
    )
    .unwrap();
    assert!(matches!(
        live.ensure_indexed(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. }),
    ));
}

#[test]
fn captured_metadata_must_also_be_valid_in_every_catalogue_that_scans_it() {
    let project = Fixture::new(
        r#"
[paths]
assets = "source-assets"
prefabs = "content/prefabs"
[profiles.alternate.paths]
assets = "content"
scenes = "alternate/scenes"
prefabs = "alternate/prefabs"
"#,
    );
    project.metadata(
        "content/prefabs/image.bin",
        "image.bin",
        DEFAULT_IMAGE,
        AssetSourceKind::Image,
    );
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    assert!(
        inventory
            .configurations()
            .iter()
            .all(|policy| policy.participant_paths().len() == 0)
    );
    assert_eq!(
        inventory
            .metadata()
            .next()
            .unwrap()
            .authored_path()
            .as_str(),
        "image.bin"
    );
    let frozen =
        ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 0, &mut budget)
            .unwrap();
    assert_eq!(frozen.database().len(), 1);
    let default_database = frozen.into_database(&mut budget).unwrap();
    let before = budget.retained_candidate_bytes;
    assert!(matches!(
        ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 1, &mut budget),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. })
    ));
    assert_eq!(budget.retained_candidate_bytes, before);
    let mut live = ReferenceCatalogue::new(
        schema.project_root(),
        inventory.configurations()[1].catalogue_bindings().to_vec(),
        &mut budget,
    )
    .unwrap();
    assert!(matches!(
        live.ensure_indexed(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. })
    ));
    assert_eq!(default_database.len(), 1);
}

#[test]
fn frozen_resolved_aliases_release_only_their_own_storage_without_lookup_receipts() {
    let project = Fixture::profiled();
    project.metadata(
        "assets/custom.bin",
        "custom.bin",
        PLANNED,
        AssetSourceKind::Other("custom-source".to_owned()),
    );
    let schema = project.receipt(None);
    let mut budget = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut budget).unwrap();
    let mut catalogue =
        ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 0, &mut budget)
            .unwrap();
    let original = budget.retained_candidate_bytes;
    for reference in [
        AssetRef::path(ALIAS).unwrap(),
        stable(DEFAULT_PREFAB),
        primary(DEFAULT_PREFAB),
    ] {
        let resolved = catalogue
            .resolve_prefab(&reference, &mut budget)
            .unwrap()
            .unwrap();
        assert_eq!(resolved.path.as_str(), ALIAS);
        assert_eq!(
            budget.retained_candidate_bytes,
            original + resolved.retained_bytes
        );
        assert!(catalogue.lookups.is_empty());
        let unrelated_bytes = 37;
        budget
            .charge_source_bytes(0, 0, 11, unrelated_bytes)
            .unwrap();
        let spent = budget.usage().decode_work();
        resolved.release(&mut budget).unwrap();
        assert_eq!(budget.retained_candidate_bytes, original + unrelated_bytes);
        assert_eq!(budget.usage().decode_work(), spent);
        budget
            .reconcile_retained_reservation(unrelated_bytes, 0)
            .unwrap();
    }
    let resolved = catalogue
        .resolve_resource(&stable(DEFAULT_IMAGE), &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.path.as_str(), "image.png");
    resolved.release(&mut budget).unwrap();
    let resolved = catalogue
        .resolve_resource(&stable(PLANNED), &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(
        resolved.binding.expected_kind,
        Some(AssetSourceKind::Other("custom-source".to_owned()))
    );
    assert_eq!(
        budget.retained_candidate_bytes,
        original + resolved.retained_bytes
    );
    resolved.release(&mut budget).unwrap();
    catalogue
        .resolve_resource(&AssetRef::path("image.png").unwrap(), &mut budget)
        .unwrap()
        .unwrap()
        .release(&mut budget)
        .unwrap();
    assert!(matches!(
        catalogue.resolve_prefab(&stable(DEFAULT_IMAGE), &mut budget),
        Err(AuthoringProjectSourceAdmissionError::NativeSource(_)),
    ));
    assert!(
        catalogue
            .resolve_resource(&stable(ALTERNATE_IMAGE), &mut budget)
            .unwrap()
            .is_none()
    );
    assert_eq!(budget.retained_candidate_bytes, original);
    assert!(catalogue.lookups.is_empty());
}

#[test]
fn two_frozen_configurations_keep_databases_and_release_only_constructor_reservations() {
    let project = Fixture::profiled();
    let schema = project.receipt(None);
    let mut original = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut original).unwrap();
    let capture = |budget: &mut AuthoringProjectAdmissionBudget, peak: &mut usize| {
        let mut databases = Vec::new();
        let mut continuing = original.retained_candidate_bytes;
        for configuration in 0..2 {
            let catalogue = ReferenceCatalogue::from_inventory(
                schema.project_root(),
                &inventory,
                configuration,
                budget,
            );
            let catalogue = match catalogue {
                Ok(catalogue) => catalogue,
                Err(error) => {
                    // A refused constructor cannot release earlier databases or another owner.
                    assert_eq!(budget.retained_candidate_bytes, continuing);
                    return Err(error);
                }
            };
            *peak = (*peak).max(budget.retained_candidate_bytes);
            let storage = catalogue.frozen_storage.as_ref().unwrap();
            let database_bytes = storage.database_bytes;
            let temporary_bytes = storage.temporary_bytes;
            assert!(database_bytes > 0);
            assert!(temporary_bytes > 0);
            assert_eq!(
                budget.retained_candidate_bytes,
                continuing + database_bytes + temporary_bytes
            );
            continuing += database_bytes;
            if configuration == 0 {
                // This later reservation is outside the constructor's accounting interval.
                budget.charge_source_bytes(0, 0, 13, 37)?;
                continuing += 37;
                *peak = (*peak).max(budget.retained_candidate_bytes);
            }
            let record = catalogue.database().records().next().unwrap();
            let path = record.path().clone();
            let allocation = std::ptr::from_ref(record);
            let spent = budget.usage().decode_work();
            let database = catalogue.into_database(budget)?;
            assert_eq!(
                std::ptr::from_ref(database.record_for_path(&path).unwrap()),
                allocation
            );
            assert_eq!(budget.retained_candidate_bytes, continuing);
            assert_eq!(budget.usage().decode_work(), spent);
            databases.push(database);
        }
        Ok::<_, AuthoringProjectSourceAdmissionError>(databases)
    };
    let mut measured = original;
    let mut exact = 0;
    let databases = capture(&mut measured, &mut exact).unwrap();
    assert_eq!(databases.len(), 2);
    for maximum in [exact, exact - 1] {
        let mut budget = original;
        budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let mut peak = 0;
        let result = capture(&mut budget, &mut peak);
        if maximum == exact {
            assert_eq!(result.unwrap().len(), 2);
            assert_eq!(peak, exact);
            assert_eq!(
                budget.retained_candidate_bytes,
                measured.retained_candidate_bytes
            );
        } else {
            assert!(
                matches!(result, Err(AuthoringProjectSourceAdmissionError::Budget(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: actual }
            )) if actual == maximum)
            );
        }
    }
}

#[test]
fn frozen_storage_is_admitted_before_copying_and_refused_allocations_are_released() {
    let project = Fixture::profiled();
    let schema = project.receipt(None);
    let mut original = schema.budget;
    let inventory =
        SourceInventory::capture(&schema, &[], SceneFileLimits::default(), &mut original).unwrap();
    let mut measured = original;
    let catalogue =
        ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 1, &mut measured)
            .unwrap();
    let exact = measured.usage().retained_candidate_bytes();
    drop(catalogue);
    for maximum in [original.retained_candidate_bytes + 1, exact - 1, exact] {
        let mut budget = original;
        budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let result =
            ReferenceCatalogue::from_inventory(schema.project_root(), &inventory, 1, &mut budget);
        if maximum == exact {
            assert_eq!(result.unwrap().database().len(), 3);
            assert_eq!(budget.usage().retained_candidate_bytes(), exact);
        } else {
            assert!(matches!(result,
                Err(AuthoringProjectSourceAdmissionError::Budget(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                        maximum: actual,
                    }
                )) if actual == maximum
            ));
            assert_eq!(
                budget.retained_candidate_bytes,
                original.retained_candidate_bytes,
            );
        }
        assert_eq!(
            budget.usage().unique_files(),
            original.usage().unique_files()
        );
        assert_eq!(
            budget.usage().retained_open_handles(),
            original.usage().retained_open_handles(),
        );
    }
    for configuration in [inventory.configurations().len(), usize::MAX] {
        let mut budget = original;
        assert!(matches!(
            ReferenceCatalogue::from_inventory(
                schema.project_root(),
                &inventory,
                configuration,
                &mut budget,
            ),
            Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch),
        ));
        assert_eq!(budget.usage(), original.usage());
    }
}

fn stable(id: &str) -> AssetRef {
    AssetRef::StableId(id.parse().unwrap())
}

fn primary(id: &str) -> AssetRef {
    AssetRef::Product(
        AssetProductRef::new(id.parse().unwrap(), ImportedProductId::PRIMARY).unwrap(),
    )
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(extra_manifest: &str) -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let fixture = Self {
            root: std::env::temp_dir().join(format!(
                "nara_frozen_catalogue_{}_{}",
                std::process::id(),
                sequence,
            )),
        };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(fixture.root.join(directory)).unwrap();
        }
        fixture.write(
            "nara.toml",
            format!(
                r#"schema_version = 1
[project]
name = "Frozen catalogue"
source_universe = "984edb80-00a9-45fd-9f6c-d2dbf903ed48"
native_document_identity = "stable_metadata"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
{extra_manifest}
"#
            )
            .as_bytes(),
        );
        let bundle =
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
        fixture.write(
            "schema/authoring.bundle.json",
            bundle.to_json_string().unwrap().as_bytes(),
        );
        fixture
    }

    fn profiled() -> Self {
        let fixture = Self::new(
            r#"
[profiles.alternate.paths]
assets = "alternate/assets"
prefabs = "alternate/prefabs"
"#,
        );
        for path in [
            "prefabs/item.prefab.ron",
            "alternate/prefabs/item.prefab.ron",
        ] {
            fixture.prefab(path);
        }
        fixture.scene("scenes/shared.scene.json");
        for (path, alias, id, kind) in [
            (
                "prefabs/item.prefab.ron",
                ALIAS,
                DEFAULT_PREFAB,
                AssetSourceKind::Prefab,
            ),
            (
                "alternate/prefabs/item.prefab.ron",
                ALIAS,
                ALTERNATE_PREFAB,
                AssetSourceKind::Prefab,
            ),
            (
                "assets/image.png",
                "image.png",
                DEFAULT_IMAGE,
                AssetSourceKind::Image,
            ),
            (
                "alternate/assets/image.png",
                "image.png",
                ALTERNATE_IMAGE,
                AssetSourceKind::Image,
            ),
            (
                "scenes/shared.scene.json",
                "shared.scene.json",
                SHARED_SCENE,
                AssetSourceKind::Scene,
            ),
        ] {
            fixture.metadata(path, alias, id, kind);
        }
        fixture
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn metadata(&self, source: &str, local: &str, id: &str, kind: AssetSourceKind) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(id.parse().unwrap(), AssetPath::new(local).unwrap(), kind),
            (),
        );
        self.write(
            &format!("{source}.meta"),
            metadata.to_json_string().unwrap().as_bytes(),
        );
    }

    fn prefab(&self, path: &str) {
        self.write(
            path,
            PrefabDocument::default()
                .to_ron_string()
                .unwrap()
                .as_bytes(),
        );
    }

    fn scene(&self, path: &str) {
        self.write(
            path,
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        );
    }

    fn receipt(&self, profile: Option<&str>) -> AuthoringSchemaReceipt {
        let root = DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();
        AuthoringProjectAdmissionHost::default()
            .prepare(root, profile)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved.starts_with(temporary_root));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_frozen_catalogue_")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

#[cfg(windows)]
fn host_directory(path: &Path) -> io::Result<File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
}

#[cfg(not(windows))]
fn host_directory(path: &Path) -> io::Result<File> {
    File::open(path)
}
