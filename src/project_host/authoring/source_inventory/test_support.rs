//! Shared physical project fixture for neutral inventory and Schema-upgrade proof tests.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nara_asset::{AssetMeta, AssetMetaIdentity, StableAssetId};
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::{AuthoringSchemaBundle, AuthoringSchemaBundleOwner};
use nara_scene::{AuthoringSchemaBasis, PrefabDocument, SceneDocument};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost,
    document::{AuthoringDocumentAdmissionBasis, AuthoringDocumentKind},
    source_index::ProjectSourceIndexState,
};

pub(in crate::project_host::authoring) const FIRST_ID: &str =
    "ad322a15-01a2-4c04-847f-83e4df33f139";
pub(in crate::project_host::authoring) const SECOND_ID: &str =
    "fda28046-1e3b-4a26-bd80-bc50c7180fc0";
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(in crate::project_host::authoring) fn paths(inventory: &SourceInventory) -> Vec<&str> {
    inventory
        .participants()
        .map(|source| source.file().path())
        .collect()
}

pub(in crate::project_host::authoring) fn capture(
    schema: &AuthoringSchemaReceipt,
) -> SourceInventory {
    try_capture(schema).unwrap()
}

pub(in crate::project_host::authoring) fn try_capture(
    schema: &AuthoringSchemaReceipt,
) -> Result<SourceInventory, SourceInventoryError> {
    let mut budget = schema.budget;
    SourceInventory::capture(schema, &[], SceneFileLimits::default(), &mut budget)
}

pub(in crate::project_host::authoring) struct Fixture {
    pub(in crate::project_host::authoring) root: PathBuf,
}

impl Fixture {
    pub(in crate::project_host::authoring) fn new(identity: &str, extra_manifest: &str) -> Self {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_source_inventory_{}_{}",
            std::process::id(),
            sequence
        ));
        let fixture = Self { root };
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(fixture.root.join(directory)).unwrap();
        }
        fixture.write(
            "nara.toml",
            format!(
                r#"schema_version = 1
[project]
name = "Source inventory"
source_universe = "984edb80-00a9-45fd-9f6c-d2dbf903ed48"
native_document_identity = "{identity}"
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

    pub(in crate::project_host::authoring) fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    pub(in crate::project_host::authoring) fn scene(&self, path: &str) {
        self.write(
            path,
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        );
    }

    pub(in crate::project_host::authoring) fn prefab(&self, path: &str) {
        self.write(
            path,
            PrefabDocument::default()
                .to_ron_string()
                .unwrap()
                .as_bytes(),
        );
    }

    pub(in crate::project_host::authoring) fn metadata(
        &self,
        source: &str,
        local: &str,
        stable_id: &str,
        kind: AssetSourceKind,
        settings: u32,
    ) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(stable_id).unwrap(),
                AssetPath::new(local).unwrap(),
                kind,
            ),
            settings,
        );
        self.write(
            &format!("{source}.meta"),
            metadata.to_json_string().unwrap().as_bytes(),
        );
    }

    pub(in crate::project_host::authoring) fn receipt(
        &self,
        profile: Option<&str>,
    ) -> AuthoringSchemaReceipt {
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

    pub(in crate::project_host::authoring) fn open_source(
        &self,
        schema: &AuthoringSchemaReceipt,
        path: &str,
    ) -> AuthoringDocumentSourceFact {
        let pools = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
        let mut state = ProjectSourceIndexState::new(schema);
        state.request_refresh().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_work() {
            state.advance(&pools);
            assert!(Instant::now() < deadline, "source index did not finish");
            thread::sleep(Duration::from_millis(1));
        }
        let basis = AuthoringSchemaBasis::admit(schema.bundle(), &[]).unwrap();
        let document =
            AuthoringDocumentAdmissionBasis::new(schema, &basis, SceneFileLimits::default())
                .admit(
                    &state.snapshot().unwrap(),
                    path,
                    AuthoringDocumentKind::Scene,
                )
                .unwrap();
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        document.source().clone()
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
                .starts_with("nara_source_inventory_")
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
