use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nara_core::ByteLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_identity::SceneEntityId;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentReadiness, PrefabDocument, SceneComponentRecord, SceneDocument,
    SceneEntityRecord,
};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, source_index::ProjectSourceIndexState,
};

const SCENE_PATH: &str = "scenes/external.scene.json";
const PREFAB_PATH: &str = "prefabs/external.prefab.ron";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn owned_requests_share_the_basis_and_prepare_both_kinds_after_the_owner_is_dropped() {
    let project = TestProject::new();
    let record = SceneEntityRecord::new(SceneEntityId::new("opaque").unwrap()).with_component(
        ComponentTypeId::new("example.Unknown"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "payload",
                ComponentValue::String("retained raw data".into()),
            )]),
        )
        .with_owner(PersistentComponentSchemaOwnerId::try_new("example.unknown-owner").unwrap()),
    );
    let scene = SceneDocument::from_ordered_roots([record.clone()]);
    let prefab = PrefabDocument::from_ordered_roots([record]);
    project.write(SCENE_PATH, scene.to_json_string().unwrap().as_bytes());
    project.write(PREFAB_PATH, prefab.to_ron_string().unwrap().as_bytes());
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &adapters, SceneFileLimits::default());
    let scene_request = basis
        .capture(
            Arc::clone(&index),
            Arc::from(SCENE_PATH),
            AuthoringDocumentKind::Scene,
        )
        .unwrap();
    let prefab_request = basis
        .capture(
            Arc::clone(&index),
            Arc::from(PREFAB_PATH),
            AuthoringDocumentKind::Prefab,
        )
        .unwrap();
    assert!(Arc::ptr_eq(&scene_request.index, &index));
    assert!(Arc::ptr_eq(
        &scene_request.bundle,
        &schema.bundle.shared_bundle()
    ));
    assert!(Arc::ptr_eq(&scene_request.adapters, &adapters));
    assert!(Arc::ptr_eq(&scene_request.bundle, &prefab_request.bundle));
    let expected_root = schema.root_identity();
    drop(schema);
    drop(index);
    drop(adapters);

    let (scene_admitted, prefab_admitted) = thread::spawn(move || {
        assert_eq!(scene_request.root.identity(), expected_root);
        (
            scene_request.prepare().unwrap(),
            prefab_request.prepare().unwrap(),
        )
    })
    .join()
    .unwrap();
    match scene_admitted {
        AdmittedAuthoringDocument::Scene { source, document } => {
            assert_eq!(source.relative_path(), SCENE_PATH);
            assert_eq!(
                source.digest(),
                ContentDigest::of_bytes(&fs::read(project.path(SCENE_PATH)).unwrap())
            );
            assert_eq!(document.document(), &scene);
            assert_eq!(document.readiness(), AuthoringDocumentReadiness::Degraded);
        }
        _ => panic!("the captured Scene request changed kind"),
    }
    match prefab_admitted {
        AdmittedAuthoringDocument::Prefab { source, document } => {
            assert_eq!(source.relative_path(), PREFAB_PATH);
            assert_eq!(
                source.digest(),
                ContentDigest::of_bytes(&fs::read(project.path(PREFAB_PATH)).unwrap())
            );
            assert_eq!(document.document(), &prefab);
            assert_eq!(document.readiness(), AuthoringDocumentReadiness::Degraded);
        }
        _ => panic!("the captured Prefab request changed kind"),
    }
}

#[test]
fn capture_checks_authority_without_opening_the_source() {
    let project = TestProject::new();
    let (schema, index) = project.admission();
    fs::remove_file(project.path(SCENE_PATH)).unwrap();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &adapters, SceneFileLimits::default());
    let request = basis
        .capture(
            Arc::clone(&index),
            Arc::from(SCENE_PATH),
            AuthoringDocumentKind::Scene,
        )
        .expect("capture must not open the indexed source");
    assert_eq!(
        request.prepare().err().unwrap().code(),
        "project.editor.document-source-open-failed"
    );
    for (path, kind, code) in [
        (
            "scenes/missing.scene.json",
            AuthoringDocumentKind::Scene,
            "project.editor.document-index-member-missing",
        ),
        (
            SCENE_PATH,
            AuthoringDocumentKind::Prefab,
            "project.editor.document-kind-mismatch",
        ),
    ] {
        assert_eq!(
            basis
                .capture(Arc::clone(&index), Arc::from(path), kind,)
                .err()
                .unwrap()
                .code(),
            code,
        );
        assert_eq!(basis.admit(&index, path, kind).err().unwrap().code(), code);
    }
}

#[test]
fn prepare_reads_later_bytes_and_preserves_borrowed_admission_errors() {
    let project = TestProject::new();
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    for (limits, bytes, expected) in [
        (
            SceneFileLimits::default(),
            b"{".as_slice(),
            "project.editor.document-source-format-invalid",
        ),
        (
            SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(8).unwrap()),
            b"larger than the captured byte limit".as_slice(),
            "project.editor.document-source-too-large",
        ),
    ] {
        let basis = AuthoringDocumentAdmissionBasis::new(&schema, &adapters, limits);
        let request = basis
            .capture(
                Arc::clone(&index),
                Arc::from(SCENE_PATH),
                AuthoringDocumentKind::Scene,
            )
            .unwrap();
        project.write(SCENE_PATH, bytes);
        let borrowed = basis
            .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
            .err()
            .unwrap();
        let owned = thread::spawn(move || request.prepare())
            .join()
            .unwrap()
            .err()
            .unwrap();
        assert_eq!(borrowed.code(), expected);
        assert_eq!(owned.code(), expected);
    }
}

#[test]
fn owned_preparation_rejects_a_replaced_indexed_file() {
    let project = TestProject::new();
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &adapters, SceneFileLimits::default());
    let request = basis
        .capture(
            Arc::clone(&index),
            Arc::from(SCENE_PATH),
            AuthoringDocumentKind::Scene,
        )
        .unwrap();
    fs::rename(
        project.path(SCENE_PATH),
        project.path("scenes/previous.scene.json"),
    )
    .unwrap();
    project.write(
        SCENE_PATH,
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    );
    assert_eq!(
        request.prepare().err().unwrap().code(),
        "project.editor.document-source-identity-changed"
    );
    assert_eq!(
        basis
            .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
            .err()
            .unwrap()
            .code(),
        "project.editor.document-source-identity-changed"
    );
}

#[test]
fn capture_rejects_an_index_from_another_schema_authority() {
    let project = TestProject::new();
    let (schema, index) = project.admission();
    let other = TestProject::new();
    let (other_schema, _) = other.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis =
        AuthoringDocumentAdmissionBasis::new(&other_schema, &adapters, SceneFileLimits::default());
    assert_eq!(
        basis
            .capture(
                Arc::clone(&index),
                Arc::from(SCENE_PATH),
                AuthoringDocumentKind::Scene,
            )
            .err()
            .unwrap()
            .code(),
        "project.editor.document-index-lineage-mismatch"
    );
    assert_eq!(
        basis
            .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
            .err()
            .unwrap()
            .code(),
        "project.editor.document-index-lineage-mismatch"
    );
    drop(schema);
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_owned_document_{}_{}",
            std::process::id(),
            sequence
        ));
        let project = Self { root };
        for directory in ["assets", "scenes", "prefabs", "scripts", "schema"] {
            fs::create_dir_all(project.path(directory)).unwrap();
        }
        project.write(
            "nara.toml",
            br#"schema_version = 1
[project]
source_universe = '02cc62a3-cb41-57b0-b1c8-a7b98f50bdf9'
native_document_identity = 'project_location'
name = "Owned Document"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
"#,
        );
        let bundle =
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
        project.write(
            "schema/authoring.bundle.json",
            bundle.to_json_string().unwrap().as_bytes(),
        );
        project.write(
            SCENE_PATH,
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        );
        project
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        fs::write(self.path(relative), bytes).unwrap();
    }

    fn admission(&self) -> (AuthoringSchemaReceipt, Arc<ProjectSourceIndexSnapshot>) {
        let root = DirectoryCapability::from_host_handle(
            host_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::Untrusted),
        )
        .unwrap();
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(root, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let pools = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
        let mut state = ProjectSourceIndexState::new(&schema);
        state.request_refresh().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_work() {
            state.advance(&pools);
            assert!(Instant::now() < deadline, "the source index did not finish");
            thread::sleep(Duration::from_millis(1));
        }
        let index = state.snapshot().expect("the source index must publish");
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        (schema, index)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
        OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    }
    #[cfg(not(windows))]
    {
        File::open(path).unwrap()
    }
}
