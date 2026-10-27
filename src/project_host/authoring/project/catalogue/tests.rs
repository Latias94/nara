use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, StableAssetId};
use nara_core::ItemLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};

use super::*;
use crate::project_host::authoring::AuthoringProjectAdmissionLimits;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn exact_directory_and_file_open_costs_are_admitted_before_opening() {
    let fixture = CatalogueFixture::new();
    let root = fixture.capability();
    let limits = AuthoringProjectAdmissionLimits {
        decode_work: ItemLimit::new(1).unwrap(),
        ..AuthoringProjectAdmissionLimits::default()
    };
    let mut budget = AuthoringProjectAdmissionBudget::new(limits);
    assert!(matches!(
        open_source_directory(
            &root,
            &ProjectPath::new("missing").unwrap(),
            AuthoringSourceKind::Scene,
            &mut budget
        ),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: 1 },
        )),
    ));
    let mut budget = AuthoringProjectAdmissionBudget::new(limits);
    assert!(matches!(
        open_source_file(
            &root,
            ProjectPath::new("missing.scene.json").unwrap(),
            "missing.scene.json",
            AuthoringSourceKind::Scene,
            0,
            &mut budget
        ),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: 1 },
        )),
    ));
}

#[test]
fn actual_prefab_metadata_read_rejects_changed_bytes_and_replaced_identity_after_lookup() {
    let fixture = CatalogueFixture::new();
    let root = fixture.capability();
    let mut catalogue = ReferenceCatalogue::new(root.share(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let path = catalogue
        .resolve(&AssetRef::StableId(fixture.id()), true, &mut budget)
        .unwrap()
        .unwrap();
    let directory = root
        .open_directory(&RelativePath::new("prefabs").unwrap())
        .unwrap();

    let changed = AssetMeta::new(
        AssetMetaIdentity::new(fixture.id(), path.clone(), AssetSourceKind::Prefab),
        "changed importer settings",
    )
    .to_json_string()
    .unwrap();
    fs::write(fixture.metadata_path(), changed).unwrap();
    let changed_bytes = catalogue.admit_prefab_metadata(&directory, &path, &mut budget);
    assert!(matches!(
        changed_bytes,
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { .. })
    ));

    fs::rename(
        fixture.metadata_path(),
        fixture.root.join("prefabs/prior-metadata"),
    )
    .unwrap();
    fs::write(fixture.metadata_path(), fixture.metadata()).unwrap();
    let replaced_identity = catalogue.admit_prefab_metadata(&directory, &path, &mut budget);
    assert!(matches!(
        replaced_identity,
        Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { .. })
    ));
}

#[test]
fn stable_lookup_directory_scan_uses_the_shared_admission_ceiling() {
    let fixture = CatalogueFixture::new();
    fs::write(fixture.root.join("prefabs/ordinary-a"), b"a").unwrap();
    fs::write(fixture.root.join("prefabs/ordinary-b"), b"b").unwrap();
    let mut catalogue = ReferenceCatalogue::new(fixture.capability(), fixture.directories());
    let limits = AuthoringProjectAdmissionLimits {
        unique_files: ItemLimit::new(3).unwrap(),
        ..AuthoringProjectAdmissionLimits::default()
    };
    let mut budget = AuthoringProjectAdmissionBudget::new(limits);
    let rejected = catalogue.resolve(&AssetRef::StableId(fixture.id()), true, &mut budget);
    assert!(matches!(
        rejected,
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::UniqueFileLimitExceeded { maximum: 3 }
        ))
    ));
}

#[test]
fn stable_lookup_observations_distinguish_requested_kind_and_deduplicate_repeated_requests() {
    let fixture = CatalogueFixture::new();
    let mut catalogue = ReferenceCatalogue::new(fixture.capability(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let reference = AssetRef::StableId(fixture.id());
    let path = catalogue.resolve(&reference, true, &mut budget).unwrap();
    let retained = budget.retained_candidate_bytes;
    assert_eq!(
        catalogue.resolve(&reference, true, &mut budget).unwrap(),
        path
    );
    assert_eq!(budget.retained_candidate_bytes, retained);
    assert!(
        catalogue
            .resolve(&reference, false, &mut budget)
            .unwrap()
            .is_none()
    );
    let observations = catalogue.into_lookup_observations(&mut budget).unwrap();
    assert_eq!(observations.len(), 2);
    let prefab = observations.iter().find(|lookup| lookup.prefab).unwrap();
    assert_eq!(prefab.stable_id, fixture.id());
    assert_eq!(prefab.target, path);
    assert!(
        observations
            .iter()
            .find(|lookup| !lookup.prefab)
            .unwrap()
            .target
            .is_none()
    );
    let directories = fixture.directories();
    ReferenceCatalogue::revalidate_lookups(
        &fixture.capability(),
        [&directories[0], &directories[1]],
        &observations,
        &mut budget,
    )
    .unwrap();
}

#[test]
fn stable_lookup_observation_is_admitted_before_target_copy_and_map_insertion() {
    let fixture = CatalogueFixture::new();
    let mut catalogue = ReferenceCatalogue::new(fixture.capability(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    catalogue.ensure_indexed(&mut budget).unwrap();
    let maximum = budget.retained_candidate_bytes;
    budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    let rejected = catalogue.resolve(&AssetRef::StableId(fixture.id()), true, &mut budget);
    assert!(matches!(
        rejected,
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert!(catalogue.lookups.is_empty());
}

#[test]
fn stable_lookup_observation_transfer_is_admitted_before_allocating_the_shared_slice() {
    let fixture = CatalogueFixture::new();
    let mut catalogue = ReferenceCatalogue::new(fixture.capability(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    catalogue
        .resolve(&AssetRef::StableId(fixture.id()), true, &mut budget)
        .unwrap();
    let maximum = budget.retained_candidate_bytes;
    budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    assert!(matches!(
        catalogue.into_lookup_observations(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: observed }
        )) if observed == maximum
    ));
}

#[test]
fn path_lookup_and_empty_observation_revalidation_do_not_scan_metadata() {
    let fixture = CatalogueFixture::new();
    fs::write(
        fixture.metadata_path(),
        b"invalid metadata must remain unused",
    )
    .unwrap();
    let root = fixture.capability();
    let directories = fixture.directories();
    let mut catalogue = ReferenceCatalogue::new(root.share(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let before = budget.usage();
    let path = AssetPath::new("enemy.prefab.json").unwrap();
    assert_eq!(
        catalogue
            .resolve(&AssetRef::Path(path.clone()), true, &mut budget)
            .unwrap(),
        Some(path),
    );
    assert!(!catalogue.is_indexed());
    assert!(catalogue.lookups.is_empty());
    ReferenceCatalogue::revalidate_lookups(
        &root,
        [&directories[0], &directories[1]],
        &[],
        &mut budget,
    )
    .unwrap();
    assert_eq!(budget.usage(), before);
}

#[test]
fn fresh_stable_lookup_revalidation_rejects_removed_and_rebound_targets() {
    let fixture = CatalogueFixture::new();
    let root = fixture.capability();
    let directories = fixture.directories();
    let mut catalogue = ReferenceCatalogue::new(root.share(), fixture.directories());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    catalogue
        .resolve(&AssetRef::StableId(fixture.id()), true, &mut budget)
        .unwrap();
    let observations = catalogue.into_lookup_observations(&mut budget).unwrap();
    fs::remove_file(fixture.metadata_path()).unwrap();
    for rebound in [false, true] {
        if rebound {
            fs::write(
                fixture.root.join("prefabs/rebound.prefab.json.meta"),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        fixture.id(),
                        AssetPath::new("rebound.prefab.json").unwrap(),
                        AssetSourceKind::Prefab,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        let mut budget =
            AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
        assert!(matches!(
            ReferenceCatalogue::revalidate_lookups(
                &root,
                [&directories[0], &directories[1]],
                &observations,
                &mut budget,
            ),
            Err(AuthoringProjectSourceAdmissionError::AssetMetadataChanged { path })
                if path.as_str() == "enemy.prefab.json"
        ));
    }
}

struct CatalogueFixture {
    root: PathBuf,
}

impl CatalogueFixture {
    fn new() -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_authoring_catalogue_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("prefabs")).unwrap();
        let fixture = Self { root };
        fs::write(fixture.metadata_path(), fixture.metadata()).unwrap();
        fixture
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn id(&self) -> StableAssetId {
        StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap()
    }

    fn directories(&self) -> [ProjectPath; 2] {
        [
            ProjectPath::new("assets").unwrap(),
            ProjectPath::new("prefabs").unwrap(),
        ]
    }

    fn metadata_path(&self) -> PathBuf {
        self.root.join("prefabs/enemy.prefab.json.meta")
    }

    fn metadata(&self) -> String {
        AssetMeta::new(
            AssetMetaIdentity::new(
                self.id(),
                AssetPath::new("enemy.prefab.json").unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        )
        .to_json_string()
        .unwrap()
    }
}

impl Drop for CatalogueFixture {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let resolved = self.root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(temporary_root)
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("nara_authoring_catalogue_")
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
