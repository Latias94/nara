use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetPath, StableAssetId};
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};

use super::*;
use crate::project_host::authoring::AuthoringProjectAdmissionLimits;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn read_content_accepts_equal_scene_prefab_and_sidecar_replacements_without_weakening_open() {
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        let fixture = NativeFixture::new(kind);
        let root = fixture.capability();
        let policy = fixture.policy(NativeDocumentIdentity::StableMetadata);
        let path = RelativePath::new(fixture.path()).unwrap();
        let original = root.open_file_exact(&path).unwrap();
        let expected = policy
            .admit(&root, fixture.path(), kind, original.identity())
            .unwrap();
        let digest = original.digest(1024).unwrap();
        for relative in [
            fixture.path().to_owned(),
            format!("{}.meta", fixture.path()),
        ] {
            let source = fixture.root.join(&relative);
            let bytes = fs::read(&source).unwrap();
            fs::rename(&source, fixture.root.join(format!("{relative}.previous"))).unwrap();
            fs::write(source, bytes).unwrap();
        }
        let current = root.open_file_exact(&path).unwrap();
        assert!(!original.same_object(&current).unwrap());
        current.verify_digest(digest, digest.length()).unwrap();
        let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
        policy
            .revalidate_read_content(&root, fixture.path(), &current, &expected, 0, &mut budget)
            .unwrap();
        assert_eq!(budget.usage().retained_candidate_bytes(), 0);
        assert_eq!(budget.retained_open_handles, 0);
        assert!(budget.usage().peak_open_handles() < 64);
        assert!(matches!(
            policy.admit(&root, fixture.path(), kind, original.identity()),
            Err(AuthoringDocumentAdmissionError::SourceIdentityChanged)
        ));
        assert!(matches!(
            expected.revalidate(&root),
            Err(AuthoringDocumentAdmissionError::SourceIdentityChanged)
        ));
    }
}

#[test]
fn read_content_rejects_changed_native_metadata_and_releases_temporary_admission() {
    for change in ["id", "kind", "path", "content", "missing"] {
        let fixture = NativeFixture::new(SourceDocumentKind::Scene);
        let root = fixture.capability();
        let policy = fixture.policy(NativeDocumentIdentity::StableMetadata);
        let current = root
            .open_file_exact(&RelativePath::new(fixture.path()).unwrap())
            .unwrap();
        let expected = policy
            .admit(
                &root,
                fixture.path(),
                SourceDocumentKind::Scene,
                current.identity(),
            )
            .unwrap();
        let metadata_path = fixture.root.join(format!("{}.meta", fixture.path()));
        match change {
            "id" => fs::write(
                &metadata_path,
                fixture.metadata(
                    "1a284ca2-c86f-40d7-9111-13b202ea1349",
                    "source.scene.json",
                    AssetSourceKind::Scene,
                ),
            )
            .unwrap(),
            "kind" => fs::write(
                &metadata_path,
                fixture.metadata(
                    NativeFixture::ID,
                    "source.scene.json",
                    AssetSourceKind::Prefab,
                ),
            )
            .unwrap(),
            "path" => fs::write(
                &metadata_path,
                fixture.metadata(
                    NativeFixture::ID,
                    "elsewhere.scene.json",
                    AssetSourceKind::Scene,
                ),
            )
            .unwrap(),
            "content" => {
                let mut bytes = fs::read(&metadata_path).unwrap();
                bytes.push(b' ');
                fs::write(&metadata_path, bytes).unwrap();
            }
            "missing" => fs::remove_file(&metadata_path).unwrap(),
            _ => unreachable!(),
        }
        let mut budget = AuthoringProjectAdmissionBudget::new(Default::default());
        assert!(policy.revalidate_read_content(
            &root, fixture.path(), &current, &expected, 0, &mut budget,
        ).is_err(), "{change}");
        assert_eq!(budget.usage().retained_candidate_bytes(), 0, "{change}");
        assert_eq!(budget.retained_open_handles, 0, "{change}");
    }
}

#[test]
fn location_content_revalidation_keeps_the_canonical_path_without_reading_metadata() {
    let fixture = NativeFixture::new(SourceDocumentKind::Scene);
    let root = fixture.capability();
    let policy = fixture.policy(NativeDocumentIdentity::ProjectLocation);
    let current = root
        .open_file_exact(&RelativePath::new(fixture.path()).unwrap())
        .unwrap();
    let expected = policy
        .admit(
            &root,
            fixture.path(),
            SourceDocumentKind::Scene,
            current.identity(),
        )
        .unwrap();
    fs::write(
        fixture.root.join(format!("{}.meta", fixture.path())),
        b"unused metadata",
    )
    .unwrap();
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    policy
        .revalidate_read_content(&root, fixture.path(), &current, &expected, 0, &mut budget)
        .unwrap();
    assert_eq!(budget.usage().aggregate_encoded_bytes(), 0);
    assert!(
        policy
            .revalidate_read_content(
                &root,
                "scenes/another.scene.json",
                &current,
                &expected,
                0,
                &mut budget,
            )
            .is_err()
    );
    assert_eq!(budget.usage().retained_candidate_bytes(), 0);
}

struct NativeFixture {
    root: PathBuf,
    kind: SourceDocumentKind,
}

impl NativeFixture {
    const ID: &str = "672647f0-1bec-4706-9c45-6d3c5436306b";

    fn new(kind: SourceDocumentKind) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_native_read_content_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::create_dir_all(root.join("prefabs")).unwrap();
        let fixture = Self { root, kind };
        let (name, source_kind) = match kind {
            SourceDocumentKind::Scene => ("source.scene.json", AssetSourceKind::Scene),
            SourceDocumentKind::Prefab => ("source.prefab.json", AssetSourceKind::Prefab),
        };
        fs::write(fixture.root.join(fixture.path()), b"captured source body").unwrap();
        fs::write(
            fixture.root.join(format!("{}.meta", fixture.path())),
            fixture.metadata(Self::ID, name, source_kind),
        )
        .unwrap();
        fixture
    }

    fn path(&self) -> &'static str {
        match self.kind {
            SourceDocumentKind::Scene => "scenes/source.scene.json",
            SourceDocumentKind::Prefab => "prefabs/source.prefab.json",
        }
    }

    fn metadata(&self, id: &str, path: &str, kind: AssetSourceKind) -> String {
        AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(id).unwrap(),
                AssetPath::new(path).unwrap(),
                kind,
            ),
            (),
        )
        .to_json_string()
        .unwrap()
    }

    fn policy(&self, identity: NativeDocumentIdentity) -> NativeDocumentSourcePolicy {
        NativeDocumentSourcePolicy {
            identity,
            universe: "a3e5c207-14cb-43a3-a3f4-34ca64c179b6".parse().unwrap(),
            roots: EffectiveProjectPaths {
                assets: ProjectPath::new("assets").unwrap(),
                scenes: ProjectPath::new("scenes").unwrap(),
                prefabs: ProjectPath::new("prefabs").unwrap(),
                scripts: ProjectPath::new("scripts").unwrap(),
                import_cache: ProjectPath::new(".nara/import").unwrap(),
            },
        }
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for NativeFixture {
    fn drop(&mut self) {
        let resolved = self.root.canonicalize().unwrap();
        assert!(
            resolved.starts_with(std::env::temp_dir().canonicalize().unwrap())
                && resolved
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("nara_native_read_content_")
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
