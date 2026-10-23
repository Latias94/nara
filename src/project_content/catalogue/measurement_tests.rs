//! Finite operation counts for the real stable-ID delivery and Check paths.

use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[cfg(not(windows))]
use std::fs::File;

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_fs::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode,
};
use nara_identity::SceneEntityId;
use nara_scene::{
    PrefabDocument, PrefabInstance, SceneDocument, SceneEntityRecord, ScenePatchDocument,
};

use crate::{
    ProductRecipe,
    project_host::{ingest_project_manifest, resolve_product_recipe},
    project_workflow::{ProjectCheckRequest, check_project},
};

use super::super::{
    ProjectContentBudgetKind, ProjectContentDeliveryReceipt, ProjectContentErrorKind,
    ProjectContentLoader,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct CatalogueCounts {
    pub(super) catalogue_builds: usize,
    pub(super) directory_enumerations: usize,
    pub(super) directory_entries: usize,
    pub(super) identity_metadata_reads: usize,
    pub(super) metadata_revalidations: usize,
}

thread_local! {
    static COUNTS: RefCell<Option<CatalogueCounts>> = const { RefCell::new(None) };
}

pub(super) fn record(update: impl FnOnce(&mut CatalogueCounts)) {
    COUNTS.with_borrow_mut(|counts| {
        if let Some(counts) = counts {
            update(counts);
        }
    });
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, CatalogueCounts, Duration) {
    COUNTS.with_borrow_mut(|counts| {
        assert!(counts.replace(CatalogueCounts::default()).is_none());
    });
    let start = Instant::now();
    let result = operation();
    let elapsed = start.elapsed();
    let counts = COUNTS.with_borrow_mut(|counts| counts.take().unwrap());
    (result, counts, elapsed)
}

#[test]
#[ignore = "bounded filesystem measurement; run explicitly with --nocapture"]
fn measure_stable_id_multi_scene_delivery_and_check() {
    const SAMPLES: usize = 3;
    for (label, scenes, identities) in [("tiny", 2, 4), ("larger", 12, 256)] {
        let project = CatalogueProject::new(scenes, identities);
        let root = project.capability();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        drop(manifest);
        for workflow in ["load_delivery", "check_project"] {
            let mut timings = Vec::new();
            let mut observed = None;
            let mut peak_entries = None;
            let mut peak_work = None;
            let mut peak_files = None;
            for _ in 0..SAMPLES {
                let loader = ProjectContentLoader::new(root.share()).unwrap();
                let (delivery, counts, elapsed) = measure(|| match workflow {
                    "load_delivery" => loader.load_delivery(&candidate, &plan).unwrap(),
                    "check_project" => {
                        check_project(ProjectCheckRequest::new(root.share(), ProductRecipe::new()))
                            .unwrap()
                    }
                    _ => unreachable!(),
                });
                project.assert_delivery(&delivery);
                assert_eq!(counts, project.expected_counts());
                assert_eq!(*observed.get_or_insert(counts), counts);
                if workflow == "load_delivery" {
                    let budget = loader.budget_snapshot();
                    assert_eq!(budget.active_generations(), 0);
                    peak_entries =
                        Some(budget.high_water(ProjectContentBudgetKind::DirectoryEntries));
                    peak_work = Some(budget.high_water(ProjectContentBudgetKind::WorkBytes));
                    peak_files = Some(budget.high_water(ProjectContentBudgetKind::Files));
                }
                drop(delivery);
                assert_eq!(loader.budget_snapshot().active_reservations(), 0);
                timings.push(elapsed.as_micros());
            }
            timings.sort_unstable();
            let counts = observed.unwrap();
            println!(
                "CATALOGUE_MEASUREMENT {}",
                serde_json::json!({
                    "workload": label,
                    "workflow": workflow,
                    "scenes": scenes,
                    "identities": identities,
                    "samples": SAMPLES,
                    "catalogue_builds": counts.catalogue_builds,
                    "directory_enumerations": counts.directory_enumerations,
                    "directory_entries": counts.directory_entries,
                    "identity_metadata_reads": counts.identity_metadata_reads,
                    "metadata_revalidations": counts.metadata_revalidations,
                    "budget_peak_directory_entries": peak_entries,
                    "budget_peak_work_bytes": peak_work,
                    "budget_peak_files": peak_files,
                    "elapsed_us_min": timings[0],
                    "elapsed_us_median": timings[SAMPLES / 2],
                    "elapsed_us_max": timings[SAMPLES - 1],
                })
            );
        }
    }
}

#[test]
fn warmed_loader_rechecks_duplicate_identity_and_changed_delivery_source() {
    let project = CatalogueProject::new(2, 4);
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    drop(manifest);
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let before = loader.load_delivery(&candidate, &plan).unwrap();
    let original_digest = before.delivery_digest();
    let source = format!("prefabs/{}", prefab_path(1));
    let original_source = before
        .members()
        .iter()
        .find(|member| member.path().as_str() == source)
        .unwrap()
        .digest();
    drop(before);

    project.write_prefab(1, "changed-source");
    let (changed, counts, _) = measure(|| loader.load_delivery(&candidate, &plan).unwrap());
    assert_eq!(counts, project.expected_counts());
    assert_ne!(changed.delivery_digest(), original_digest);
    assert_ne!(
        changed
            .members()
            .iter()
            .find(|member| member.path().as_str() == source)
            .unwrap()
            .digest(),
        original_source
    );
    let checked =
        check_project(ProjectCheckRequest::new(root.share(), ProductRecipe::new())).unwrap();
    assert_eq!(checked.delivery_digest(), changed.delivery_digest());
    let changed_source_digest = changed.delivery_digest();
    drop((changed, checked));

    // Capture the catalogue, then observe different valid metadata bytes during admission.
    let mut context = super::super::LoadContext::new(
        &loader,
        plan.schema_validation(),
        &plan.asset_domains,
        None,
        Default::default(),
        candidate.manifest_digest(),
        None,
        None,
        false,
    )
    .unwrap();
    context.catalogue_roots = Some([
        candidate.settings().paths.assets.clone(),
        candidate.settings().paths.prefabs.clone(),
    ]);
    let path = context
        .resolve_reference_path(&AssetRef::StableId(stable_id(0)), &AssetSourceKind::Prefab)
        .unwrap();
    let metadata_path = project.root.join(format!("prefabs/{}.meta", path.as_str()));
    let mut metadata = fs::read(&metadata_path).unwrap();
    metadata.push(b'\n');
    fs::write(&metadata_path, &metadata).unwrap();
    let error = context
        .verify_catalogued_metadata(
            &path,
            &candidate.settings().paths.prefabs,
            nara_fs::ContentDigest::of_bytes(&metadata),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::HostAuthorityRejected);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-metadata-source-changed"
    }));
    drop(context);
    let refreshed = loader.load_delivery(&candidate, &plan).unwrap();
    assert_ne!(refreshed.delivery_digest(), changed_source_digest);
    drop(refreshed);

    project.write_metadata("group-000/duplicate.prefab.json", 0);
    let (error, counts, _) = measure(|| loader.load_delivery(&candidate, &plan).unwrap_err());
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-identity-conflict"
    }));
    assert_eq!(counts.catalogue_builds, 1);
    assert!((2..=5).contains(&counts.identity_metadata_reads));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    let error = check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap_err();
    assert_eq!(
        error.content_error().unwrap().kind(),
        ProjectContentErrorKind::AssetMetaMismatch
    );
    println!(
        "CATALOGUE_REVALIDATION changed_source=fresh-digest duplicate_identity=rejected builds={} identity_metadata_reads={}",
        counts.catalogue_builds, counts.identity_metadata_reads
    );
}

#[test]
fn larger_delivery_and_check_discover_one_complete_catalogue() {
    let project = CatalogueProject::new(12, 256);
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    drop(manifest);
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let (loaded, counts, _) = measure(|| loader.load_delivery(&candidate, &plan).unwrap());
    assert_eq!(counts, project.expected_counts());
    project.assert_delivery(&loaded);
    let (checked, counts, _) =
        measure(|| check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap());
    assert_eq!(counts, project.expected_counts());
    assert_eq!(loaded.scene_roots(), checked.scene_roots());
    assert_eq!(loaded.members(), checked.members());
    assert_eq!(loaded.delivery_digest(), checked.delivery_digest());
    drop((loaded, checked));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn catalogue_stays_lazy_until_a_later_delivery_scene_needs_stable_ids() {
    let project = CatalogueProject::new(2, 4);
    fs::write(
        project.root.join("scenes/scene-000.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    drop(manifest);
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let (startup, counts, _) = measure(|| loader.load(&candidate, &plan).unwrap());
    assert_eq!(counts, CatalogueCounts::default());
    drop(startup);
    let (loaded, counts, _) = measure(|| loader.load_delivery(&candidate, &plan).unwrap());
    assert_eq!(
        counts,
        CatalogueCounts {
            metadata_revalidations: 2,
            ..project.expected_counts()
        }
    );
    assert_eq!(loaded.members().len(), 8);
    let (checked, counts, _) =
        measure(|| check_project(ProjectCheckRequest::new(root, ProductRecipe::new())).unwrap());
    assert_eq!(counts.catalogue_builds, 1);
    assert_eq!(loaded.members(), checked.members());
    assert_eq!(loaded.delivery_digest(), checked.delivery_digest());
    drop((loaded, checked));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

#[test]
fn catalogue_capture_and_scene_copy_reject_budget_pressure_without_retaining_work() {
    let project = CatalogueProject::new(2, 4);
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    drop(manifest);
    let loader = ProjectContentLoader::new(root).unwrap();
    let mut context = super::super::LoadContext::new(
        &loader,
        plan.schema_validation(),
        &plan.asset_domains,
        None,
        Default::default(),
        candidate.manifest_digest(),
        None,
        None,
        false,
    )
    .unwrap();
    context.catalogue_roots = Some([
        candidate.settings().paths.assets.clone(),
        candidate.settings().paths.prefabs.clone(),
    ]);
    context.ensure_reference_catalogue().unwrap();
    let host = loader.budget_host();
    let mut pressure = host.reserve();
    let available = loader.limits.limit(ProjectContentBudgetKind::WorkBytes)
        - loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::WorkBytes);
    pressure
        .set(ProjectContentBudgetKind::WorkBytes, available)
        .unwrap();
    context.capture_delivery_catalogue = true;
    assert!(context.capture_delivery_catalogue().is_err());
    drop((pressure, context));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
        &loader.root,
        &candidate,
        &plan,
    )
    .unwrap();
    let (startup, catalogue) = loader
        .load_startup_for_delivery(&candidate, &plan, None, agreement)
        .unwrap();
    let mut pressure = host.reserve();
    let available = loader.limits.limit(ProjectContentBudgetKind::WorkBytes)
        - loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::WorkBytes);
    pressure
        .set(ProjectContentBudgetKind::WorkBytes, available)
        .unwrap();
    let error = loader
        .load_delivery_with_startup_and_catalogue(&candidate, &plan, startup, catalogue)
        .unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::BudgetExceeded);
    drop(pressure);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
}

#[test]
fn delivery_revalidates_later_references_and_rediscovers_duplicates_next_operation() {
    let project = CatalogueProject::new(2, 4);
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    drop(manifest);
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
        &root, &candidate, &plan,
    )
    .unwrap();
    let (startup, catalogue) = loader
        .load_startup_for_delivery(&candidate, &plan, None, agreement)
        .unwrap();
    let metadata_path = project
        .root
        .join(format!("prefabs/{}.meta", prefab_path(2)));
    let mut metadata = fs::read(&metadata_path).unwrap();
    metadata.push(b'\n');
    fs::write(&metadata_path, metadata).unwrap();
    let error = loader
        .load_delivery_with_startup_and_catalogue(&candidate, &plan, startup, catalogue)
        .unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.content.asset-metadata-source-changed"
    }));
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);

    let agreement = crate::project_host::runtime_agreement::prepare_runtime_schema_agreement(
        &root, &candidate, &plan,
    )
    .unwrap();
    let (startup, catalogue) = loader
        .load_startup_for_delivery(&candidate, &plan, None, agreement)
        .unwrap();
    project.write_metadata("group-000/duplicate.prefab.json", 0);
    // Unreferenced additions do not revise the catalogue captured by this operation.
    let delivery = loader
        .load_delivery_with_startup_and_catalogue(&candidate, &plan, startup, catalogue)
        .unwrap();
    project.assert_delivery(&delivery);
    drop(delivery);
    let error = loader.load_delivery(&candidate, &plan).unwrap_err();
    assert_eq!(error.kind(), ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct CatalogueProject {
    root: PathBuf,
    temporary_parent: PathBuf,
    scenes: usize,
    identities: usize,
}

impl CatalogueProject {
    fn new(scenes: usize, identities: usize) -> Self {
        assert!(scenes + 1 <= identities);
        let temporary_parent = std::env::temp_dir().canonicalize().unwrap();
        let root = temporary_parent.join(format!(
            "nara_catalogue_measurement_{}_{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let project = Self {
            root,
            temporary_parent,
            scenes,
            identities,
        };
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(project.root.join(directory)).unwrap();
        }
        let declared = (1..scenes)
            .map(|index| format!("\"scene-{index:03}.scene.json\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            project.root.join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Catalogue measurement"
[paths]
assets = "assets"
prefabs = "prefabs"
scenes = "scenes"
[startup]
default_scene = "scene-000.scene.json"
[content]
scenes = [{declared}]
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[capabilities]
requested = ["runtime-2d"]
"#
            ),
        )
        .unwrap();
        for index in 0..identities {
            project.write_prefab(index, "source");
            project.write_metadata(&prefab_path(index), index);
        }
        for index in 0..scenes {
            let roots = [0, index + 1].map(|identity| {
                let mut entity = SceneEntityRecord::new(
                    SceneEntityId::new(format!("anchor-{identity}")).unwrap(),
                );
                entity.prefab = Some(PrefabInstance {
                    source: AssetRef::StableId(stable_id(identity)),
                    overrides: ScenePatchDocument::default(),
                });
                entity
            });
            fs::write(
                project
                    .root
                    .join(format!("scenes/scene-{index:03}.scene.json")),
                SceneDocument::from_ordered_roots(roots)
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
        }
        let root = project.capability();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
            OpenOptions::new()
                .read(true)
                .share_mode(0x1 | 0x2 | 0x4)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn write_prefab(&self, index: usize, entity: &str) {
        let path = self.root.join("prefabs").join(prefab_path(index));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new(entity).unwrap(),
        )]);
        fs::write(path, prefab.to_json_string().unwrap()).unwrap();
    }

    fn write_metadata(&self, path: &str, identity: usize) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id(identity),
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        );
        fs::write(
            self.root.join(format!("prefabs/{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    fn expected_counts(&self) -> CatalogueCounts {
        let groups = self.identities.div_ceil(32);
        CatalogueCounts {
            catalogue_builds: 1,
            directory_enumerations: 2 + groups,
            directory_entries: 2 * self.identities + groups,
            identity_metadata_reads: self.identities,
            metadata_revalidations: self.scenes * 2,
        }
    }

    fn assert_delivery(&self, delivery: &ProjectContentDeliveryReceipt) {
        assert_eq!(delivery.scene_roots().len(), self.scenes);
        assert_eq!(
            delivery.members().len(),
            self.scenes + 2 + 2 * (self.scenes + 1)
        );
        assert!(delivery.members().iter().all(|member| {
            !member
                .path()
                .as_str()
                .ends_with(&prefab_path(self.identities - 1))
        }));
    }
}

impl Drop for CatalogueProject {
    fn drop(&mut self) {
        let relative = self.root.strip_prefix(&self.temporary_parent).unwrap();
        assert_eq!(relative.components().count(), 1);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn prefab_path(index: usize) -> String {
    format!("group-{:03}/prefab-{index:03}.prefab.json", index / 32)
}

fn stable_id(index: usize) -> StableAssetId {
    StableAssetId::parse_str(&format!("3eaca099-2000-4000-8000-{index:012x}")).unwrap()
}
