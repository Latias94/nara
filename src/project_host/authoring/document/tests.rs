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
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PersistentComponentSchemaOwnerId,
};
use nara_scene::__private::AuthoringAdapterReceipt;
use nara_scene::{
    AuthoringDocumentReadiness, PrefabDocument, SceneComponentRecord, SceneDocument,
    SceneEntityRecord,
};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, source_index::ProjectSourceIndexState,
};

#[path = "source_receipts.rs"]
mod source_receipts;

const SCENE_PATH: &str = "scenes/external.scene.json";
const PREFAB_PATH: &str = "prefabs/external.prefab.ron";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn prepared_documents_keep_the_actual_read_leaf_until_host_admission_or_rejection() {
    use crate::project_host::persistence::{PrefabPersistenceHost, ScenePersistenceHost};
    use nara_tooling::{EditorDocumentSourceBinding, EditorPersistenceFailureStage};

    let project = TestProject::new();
    project.write(
        PREFAB_PATH,
        PrefabDocument::default()
            .to_ron_string()
            .unwrap()
            .as_bytes(),
    );
    let (schema, index) = project.admission();
    let schema_basis = AuthoringSchemaBasis::admit(schema.bundle(), &[]).unwrap();
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &schema_basis, SceneFileLimits::default());
    for (path, kind) in [
        (SCENE_PATH, AuthoringDocumentKind::Scene),
        (PREFAB_PATH, AuthoringDocumentKind::Prefab),
    ] {
        for reject in [false, true] {
            assert_source_open(&project.path(path), false);
            let request = basis
                .capture(Arc::clone(&index), Arc::from(path), kind)
                .unwrap();
            let admitted = thread::spawn(move || request.prepare().unwrap())
                .join()
                .unwrap();
            assert_source_open(&project.path(path), true);
            let expected = admitted.source().clone();
            let digest = if reject {
                ContentDigest::of_bytes(b"a different observation")
            } else {
                expected.digest()
            };
            match admitted {
                AdmittedAuthoringDocument::Scene { source, file, .. } => {
                    let bound = ScenePersistenceHost::bind_authoring_source(
                        &schema.project_root(),
                        source.relative_path(),
                        file,
                        digest,
                    );
                    if reject {
                        assert!(matches!(
                            bound,
                            Err(EditorPersistenceFailureStage::Validate)
                        ));
                    } else {
                        let (host, _) = bound.unwrap();
                        assert_eq!(
                            host.accepted_source_observation(),
                            Some((expected.identity(), expected.parent_identity(), digest)),
                        );
                        assert_source_open(&project.path(path), true);
                        drop(host);
                    }
                }
                AdmittedAuthoringDocument::Prefab { source, file, .. } => {
                    let bound = PrefabPersistenceHost::bind_authoring_source(
                        &schema.project_root(),
                        EditorDocumentSourceBinding::new(source.relative_path()),
                        file,
                        digest,
                    );
                    if reject {
                        assert!(matches!(
                            bound,
                            Err(EditorPersistenceFailureStage::Validate)
                        ));
                    } else {
                        let (host, _) = bound.unwrap();
                        assert_eq!(
                            host.accepted_source_observation(),
                            Some((expected.identity(), expected.parent_identity(), digest)),
                        );
                        assert_source_open(&project.path(path), true);
                        drop(host);
                    }
                }
            }
            assert_source_open(&project.path(path), false);
            // Historical facts can outlive custody without retaining a native object.
            assert_eq!(expected.relative_path(), path);
        }
    }
}

#[test]
fn binding_rejects_a_replaced_name_while_the_prepared_read_object_is_still_alive() {
    use crate::project_host::persistence::{PrefabPersistenceHost, ScenePersistenceHost};
    use nara_tooling::{EditorDocumentSourceBinding, EditorPersistenceFailureStage};

    let project = TestProject::new();
    project.write(
        PREFAB_PATH,
        PrefabDocument::default()
            .to_ron_string()
            .unwrap()
            .as_bytes(),
    );
    let (schema, index) = project.admission();
    let schema_basis = AuthoringSchemaBasis::admit(schema.bundle(), &[]).unwrap();
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &schema_basis, SceneFileLimits::default());
    for (path, kind) in [
        (SCENE_PATH, AuthoringDocumentKind::Scene),
        (PREFAB_PATH, AuthoringDocumentKind::Prefab),
    ] {
        let admitted = basis
            .capture(Arc::clone(&index), Arc::from(path), kind)
            .unwrap()
            .prepare()
            .unwrap();
        let original = project.path(path);
        let retained = original.with_extension("retained");
        let bytes = fs::read(&original).unwrap();
        fs::rename(&original, &retained).unwrap();
        project.write(path, &bytes);
        assert_source_open(&retained, true);
        assert_source_open(&original, false);
        let rejected = match admitted {
            AdmittedAuthoringDocument::Scene { source, file, .. } => {
                ScenePersistenceHost::bind_authoring_source(
                    &schema.project_root(),
                    source.relative_path(),
                    file,
                    source.digest(),
                )
                .map(|_| ())
            }
            AdmittedAuthoringDocument::Prefab { source, file, .. } => {
                PrefabPersistenceHost::bind_authoring_source(
                    &schema.project_root(),
                    EditorDocumentSourceBinding::new(source.relative_path()),
                    file,
                    source.digest(),
                )
                .map(|_| ())
            }
        };
        assert_eq!(rejected, Err(EditorPersistenceFailureStage::Validate));
        assert_source_open(&retained, false);
        assert_source_open(&original, false);
    }
}

#[test]
fn discarding_prepared_documents_closes_their_read_leaves() {
    let project = TestProject::new();
    project.write(
        PREFAB_PATH,
        PrefabDocument::default()
            .to_ron_string()
            .unwrap()
            .as_bytes(),
    );
    let (schema, index) = project.admission();
    let schema_basis = AuthoringSchemaBasis::admit(schema.bundle(), &[]).unwrap();
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &schema_basis, SceneFileLimits::default());
    for (path, kind) in [
        (SCENE_PATH, AuthoringDocumentKind::Scene),
        (PREFAB_PATH, AuthoringDocumentKind::Prefab),
    ] {
        let admitted = basis
            .capture(Arc::clone(&index), Arc::from(path), kind)
            .unwrap()
            .prepare()
            .unwrap();
        assert_source_open(&project.path(path), true);
        drop(admitted);
        assert_source_open(&project.path(path), false);
        project.write(path, b"{");
        assert!(
            basis
                .capture(Arc::clone(&index), Arc::from(path), kind)
                .unwrap()
                .prepare()
                .is_err()
        );
        assert_source_open(&project.path(path), false);
    }
}

fn assert_source_open(path: &Path, expected: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let exclusive = fs::OpenOptions::new().read(true).share_mode(0).open(path);
        if expected {
            assert_eq!(
                exclusive
                    .err()
                    .expect("the read leaf must remain open")
                    .raw_os_error(),
                Some(32),
                "the live read leaf must cause a sharing violation",
            );
        } else {
            drop(exclusive.expect("the read leaf must be physically closed"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        let path = fs::canonicalize(path).unwrap();
        let open = fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_link(entry.path()).ok())
            .any(|target| target == path);
        assert_eq!(open, expected, "read-leaf descriptor lifetime differs");
    }
}

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
    let schema_basis = AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap();
    let basis =
        AuthoringDocumentAdmissionBasis::new(&schema, &schema_basis, SceneFileLimits::default());
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
    assert_eq!(scene_request.basis, schema_basis);
    assert_eq!(scene_request.basis, prefab_request.basis);
    let expected_root = schema.root_identity();
    drop(schema);
    drop(index);
    drop(adapters);
    drop(schema_basis);

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
        AdmittedAuthoringDocument::Scene {
            source, document, ..
        } => {
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
        AdmittedAuthoringDocument::Prefab {
            source, document, ..
        } => {
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
    let basis = AuthoringDocumentAdmissionBasis::new(
        &schema,
        &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
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
        let basis = AuthoringDocumentAdmissionBasis::new(
            &schema,
            &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
            limits,
        );
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
    let basis = AuthoringDocumentAdmissionBasis::new(
        &schema,
        &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
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
    let basis = AuthoringDocumentAdmissionBasis::new(
        &other_schema,
        &AuthoringSchemaBasis::admit(other_schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
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

#[test]
fn location_admission_keeps_each_opened_document_in_its_full_project_location() {
    let project = TestProject::new();
    project.write(
        "scenes/second.scene.json",
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    );
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis = AuthoringDocumentAdmissionBasis::new(
        &schema,
        &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
    let first = basis
        .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
        .unwrap();
    let second = basis
        .admit(
            &index,
            "scenes/second.scene.json",
            AuthoringDocumentKind::Scene,
        )
        .unwrap();
    assert_ne!(first.source().source_key(), second.source().source_key());
    for admitted in [&first, &second] {
        assert!(matches!(
            admitted.source().source_key().origin(),
            nara_scene::SourceDocumentOrigin::ProjectLocation { universe, path }
                if *universe == schema.settings().project.source_universe
                    && path.as_str() == admitted.source().relative_path()
        ));
    }
}

#[test]
fn stable_admission_requires_exact_metadata_and_revalidates_its_prepared_observation() {
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    let project = TestProject::new();
    let manifest = fs::read_to_string(project.path("nara.toml"))
        .unwrap()
        .replace("project_location", "stable_metadata");
    project.write("nara.toml", manifest.as_bytes());
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis = AuthoringDocumentAdmissionBasis::new(
        &schema,
        &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
    assert!(
        basis
            .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
            .is_err()
    );
    assert!(!project.path(&format!("{SCENE_PATH}.meta")).exists());
    let stable_id = StableAssetId::parse_str("98ded649-6558-4d57-a287-d74c05b20d9e").unwrap();
    let metadata = |path: &str, kind| {
        AssetMeta::new(
            AssetMetaIdentity::new(stable_id, AssetPath::new(path).unwrap(), kind),
            (),
        )
        .to_json_string()
        .unwrap()
    };
    for (path, kind) in [
        ("other.scene.json", AssetSourceKind::Scene),
        ("external.scene.json", AssetSourceKind::Prefab),
    ] {
        project.write(
            &format!("{SCENE_PATH}.meta"),
            metadata(path, kind).as_bytes(),
        );
        assert_eq!(
            basis
                .admit(&index, SCENE_PATH, AuthoringDocumentKind::Scene)
                .err()
                .unwrap()
                .code(),
            "project.editor.native-source-metadata-mismatch",
        );
    }
    let valid = metadata("external.scene.json", AssetSourceKind::Scene);
    project.write(&format!("{SCENE_PATH}.meta"), valid.as_bytes());
    let request = basis
        .capture(
            Arc::clone(&index),
            Arc::from(SCENE_PATH),
            AuthoringDocumentKind::Scene,
        )
        .unwrap();
    let admitted = request.prepare().unwrap();
    assert_eq!(
        admitted.source().source_key(),
        &SourceDocumentKey::stable_asset(stable_id, SourceDocumentKind::Scene),
    );
    admitted
        .source()
        .revalidate_native_source(&schema.project_root())
        .unwrap();
    project.write(
        &format!("{SCENE_PATH}.meta"),
        format!("{valid}\n").as_bytes(),
    );
    assert_eq!(
        admitted
            .source()
            .revalidate_native_source(&schema.project_root())
            .unwrap_err()
            .code(),
        "project.editor.document-source-identity-changed",
    );
    assert_eq!(
        fs::read(project.path(SCENE_PATH)).unwrap(),
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes()
    );
}

#[test]
fn stable_documents_use_their_containing_configured_root_instead_of_the_document_kind() {
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    let project = TestProject::new();
    project.write(
        "nara.toml",
        fs::read_to_string(project.path("nara.toml"))
            .unwrap()
            .replace("project_location", "stable_metadata")
            .as_bytes(),
    );
    fs::create_dir_all(project.path("assets/cutscenes")).unwrap();
    for path in [
        "assets/cutscenes/intro.scene.json",
        "prefabs/intro.scene.json",
    ] {
        project.write(
            path,
            SceneDocument::default()
                .to_json_string()
                .unwrap()
                .as_bytes(),
        );
    }
    let (schema, index) = project.admission();
    let adapters: Arc<[AuthoringAdapterReceipt]> = Arc::from([]);
    let basis = AuthoringDocumentAdmissionBasis::new(
        &schema,
        &AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap(),
        SceneFileLimits::default(),
    );
    let id = StableAssetId::parse_str("98ded649-6558-4d57-a287-d74c05b20d9e").unwrap();
    for (path, declaration) in [
        (
            "assets/cutscenes/intro.scene.json",
            "cutscenes/intro.scene.json",
        ),
        ("prefabs/intro.scene.json", "intro.scene.json"),
    ] {
        let metadata = |declared: &str| {
            AssetMeta::new(
                AssetMetaIdentity::new(
                    id,
                    AssetPath::new(declared).unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap()
        };
        project.write(&format!("{path}.meta"), metadata(declaration).as_bytes());
        let admitted = basis
            .admit(&index, path, AuthoringDocumentKind::Scene)
            .unwrap();
        assert_eq!(
            admitted.source().source_key(),
            &SourceDocumentKey::stable_asset(id, SourceDocumentKind::Scene)
        );
        project.write(
            &format!("{path}.meta"),
            metadata("other.scene.json").as_bytes(),
        );
        assert!(matches!(
            basis.admit(&index, path, AuthoringDocumentKind::Scene),
            Err(AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch),
        ));
    }
}

#[test]
fn native_metadata_read_preserves_aggregate_source_and_io_failure_classes() {
    use crate::project_host::authoring::{
        AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
        AuthoringProjectAdmissionLimits, AuthoringProjectSourceAdmissionError, AuthoringSourceKind,
    };
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    let project = TestProject::new();
    let (schema, _) = project.admission();
    let mut settings = schema.settings().clone();
    settings.project.native_document_identity =
        nara_project::NativeDocumentIdentity::StableMetadata;
    let policy = NativeDocumentSourcePolicy::new(&settings);
    let root = schema.project_root();
    let identity = root
        .open_file(&RelativePath::new(SCENE_PATH).unwrap())
        .unwrap()
        .identity();
    let metadata_path = format!("{SCENE_PATH}.meta");
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("98ded649-6558-4d57-a287-d74c05b20d9e").unwrap(),
            AssetPath::new("external.scene.json").unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    assert!(metadata.len() > 128);
    project.write(&metadata_path, metadata.as_bytes());
    let mut budget = AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::new(
        ByteLimit::new(256).unwrap(),
    ));
    budget.charge_source_bytes(128, 0, 0, 0).unwrap();
    assert!(matches!(
        policy.admit_budgeted(
            &root,
            SCENE_PATH,
            SourceDocumentKind::Scene,
            identity,
            0,
            3,
            &mut budget
        ),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { maximum: 256 },
        )),
    ));
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let admitted = policy
        .admit_budgeted(
            &root,
            SCENE_PATH,
            SourceDocumentKind::Scene,
            identity,
            0,
            3,
            &mut budget,
        )
        .unwrap();
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        admitted.retained_bytes().unwrap()
    );
    assert_eq!(budget.usage().aggregate_encoded_bytes(), metadata.len());
    project.write(&metadata_path, &vec![b' '; 65_537]);
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    assert!(matches!(
        policy.admit_budgeted(
            &root,
            SCENE_PATH,
            SourceDocumentKind::Scene,
            identity,
            0,
            3,
            &mut budget
        ),
        Err(AuthoringProjectSourceAdmissionError::SourceTooLarge {
            kind: AuthoringSourceKind::AssetMetadata,
            maximum: 65_536,
            ..
        }),
    ));
    assert!(matches!(
        policy.admit(&root, SCENE_PATH, SourceDocumentKind::Scene, identity),
        Err(AuthoringDocumentAdmissionError::NativeSourceMetadataTooLarge { maximum: 65_536 }),
    ));
    fs::remove_file(project.path(&metadata_path)).unwrap();
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    assert!(matches!(
        policy.admit_budgeted(
            &root,
            SCENE_PATH,
            SourceDocumentKind::Scene,
            identity,
            0,
            3,
            &mut budget
        ),
        Err(AuthoringProjectSourceAdmissionError::NativeSource(
            AuthoringDocumentAdmissionError::OpenSource(_)
        )),
    ));
}

#[test]
fn native_source_retention_is_admitted_before_opening_and_reconciled_after_success() {
    use crate::project_host::authoring::{
        AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
        AuthoringProjectAdmissionLimits, AuthoringProjectSourceAdmissionError,
    };
    let project = TestProject::new();
    let (schema, _) = project.admission();
    let root = schema.project_root();
    let identity = root
        .open_file(&RelativePath::new(SCENE_PATH).unwrap())
        .unwrap()
        .identity();
    for stable in [false, true] {
        let mut settings = schema.settings().clone();
        if stable {
            settings.project.native_document_identity =
                nara_project::NativeDocumentIdentity::StableMetadata;
        }
        let policy = NativeDocumentSourcePolicy::new(&settings);
        let mut limits = AuthoringProjectAdmissionLimits::default();
        limits.retained_candidate_bytes = ByteLimit::new(1).unwrap();
        let mut budget = AuthoringProjectAdmissionBudget::new(limits);
        // The missing source is a read canary: reaching the open would produce OpenSource.
        assert!(matches!(
            policy.admit_budgeted(
                &root,
                "scenes/missing.scene.json",
                SourceDocumentKind::Scene,
                identity,
                0,
                3,
                &mut budget
            ),
            Err(AuthoringProjectSourceAdmissionError::Budget(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum: 1 },
            )),
        ));
    }
    let policy = NativeDocumentSourcePolicy::new(schema.settings());
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let source = policy
        .admit_budgeted(
            &root,
            SCENE_PATH,
            SourceDocumentKind::Scene,
            identity,
            0,
            3,
            &mut budget,
        )
        .unwrap();
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        source.retained_bytes().unwrap()
    );
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
