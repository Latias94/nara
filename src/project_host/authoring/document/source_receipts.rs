use super::*;

use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
use nara_scene::PrefabInstance;

use crate::project_host::{
    authoring::project::{AuthoringProjectSessionSnapshot, PreparedAuthoringProject},
    file_publication::{FilePublicationHost, FilePublicationOutcome},
};

const SCENE_STABLE_ID: &str = "bf136559-bc6e-41b9-9171-e46e176576a8";
const PREFAB_STABLE_ID: &str = "74773fbe-6783-4324-9300-50a23804bcbc";

#[test]
fn captured_scene_and_prefab_receipts_retain_admitted_observations_without_reads() {
    for stable in [false, true] {
        let (project, snapshot) = captured_project(stable);
        let prefab = snapshot.prefab_sources().next().unwrap();
        fs::remove_file(project.path(SCENE_PATH)).unwrap();
        fs::remove_file(project.path(PREFAB_PATH)).unwrap();
        if stable {
            fs::remove_file(project.path(&format!("{SCENE_PATH}.meta"))).unwrap();
            fs::remove_file(project.path(&format!("{PREFAB_PATH}.meta"))).unwrap();
        }
        for (source, native) in [
            (snapshot.startup_source(), snapshot.startup_native_source()),
            (prefab.source(), prefab.native_source()),
        ] {
            let fact = AuthoringDocumentSourceFact::from_captured_source(source, native).unwrap();
            assert_eq!(fact.relative_path(), source.project_path().as_str());
            assert_eq!(fact.identity(), source.identity());
            assert_eq!(fact.parent_identity(), source.parent_identity());
            assert_eq!(fact.digest(), source.digest());
            assert_eq!(fact.encoded_bytes(), source.encoded_bytes());
            assert_eq!(fact.native_source(), native);
        }
    }
}

#[test]
fn captured_receipts_reject_non_documents_wrong_kinds_and_foreign_file_identities() {
    let (project, snapshot) = captured_project(true);
    let source = snapshot.startup_source();
    let native = snapshot.startup_native_source();
    let prefab = snapshot.prefab_sources().next().unwrap();
    assert!(matches!(
        AuthoringDocumentSourceFact::from_captured_source(
            native.metadata_source().unwrap(),
            native,
        ),
        Err(AuthoringDocumentAdmissionError::DocumentKindMismatch),
    ));
    assert!(matches!(
        AuthoringDocumentSourceFact::from_captured_source(source, prefab.native_source()),
        Err(AuthoringDocumentAdmissionError::DocumentKindMismatch),
    ));
    let root = snapshot.project_root();
    let foreign_path = "scenes/foreign.scene.json";
    project.write(
        foreign_path,
        SceneDocument::default()
            .to_json_string()
            .unwrap()
            .as_bytes(),
    );
    write_metadata(
        &project,
        foreign_path,
        "foreign.scene.json",
        AssetSourceKind::Scene,
        "8ba6bc24-1ab9-47be-997e-01c278999913",
    );
    let foreign_file = root
        .open_file_exact(&RelativePath::new(foreign_path).unwrap())
        .unwrap();
    let foreign_native = NativeDocumentSourcePolicy::new(snapshot.settings())
        .admit(
            &root,
            foreign_path,
            SourceDocumentKind::Scene,
            foreign_file.identity(),
        )
        .unwrap();
    assert!(matches!(
        AuthoringDocumentSourceFact::from_captured_source(source, &foreign_native),
        Err(AuthoringDocumentAdmissionError::SourceIdentityChanged),
    ));
}

#[test]
fn captured_receipts_reject_an_admitted_alias_of_the_same_physical_file() {
    for stable in [false, true] {
        let (project, snapshot) = captured_project(stable);
        let alias = "scenes/alias.scene.json";
        fs::hard_link(project.path(SCENE_PATH), project.path(alias)).unwrap();
        if stable {
            write_metadata(
                &project,
                alias,
                "alias.scene.json",
                AssetSourceKind::Scene,
                SCENE_STABLE_ID,
            );
        }
        let native = NativeDocumentSourcePolicy::new(snapshot.settings())
            .admit(
                &snapshot.project_root(),
                alias,
                SourceDocumentKind::Scene,
                snapshot.startup_source().identity(),
            )
            .unwrap();
        assert!(matches!(
            AuthoringDocumentSourceFact::from_captured_source(snapshot.startup_source(), &native),
            Err(AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch),
        ));
        let fact = AuthoringDocumentSourceFact::from_captured_source(
            snapshot.startup_source(),
            snapshot.startup_native_source(),
        )
        .unwrap();
        assert!(matches!(
            fact.with_native_source(native),
            Err(AuthoringDocumentAdmissionError::NativeSourceMetadataMismatch),
        ));
    }
}

#[test]
fn accepted_publication_advances_only_file_observation_and_preserves_old_receipts() {
    let (project, snapshot) = captured_project(true);
    let original = AuthoringDocumentSourceFact::from_captured_source(
        snapshot.startup_source(),
        snapshot.startup_native_source(),
    )
    .unwrap();
    let mut publication = FilePublicationHost::bind_existing(
        &snapshot.project_root(),
        original.relative_path(),
        original.identity(),
        original.digest(),
        65_536,
    )
    .unwrap();
    let bytes = SceneDocument::default().to_json_string().unwrap();
    let FilePublicationOutcome::Published(accepted) = publication.publish(bytes.as_bytes()) else {
        panic!("the current source must accept a replacement");
    };
    let (_, binding) = accepted.into_parts();
    publication.commit_published(binding);
    let (identity, parent, digest) = publication.accepted_source_observation().unwrap();
    fs::remove_file(project.path(SCENE_PATH)).unwrap();
    let updated = original
        .with_persistence_observation(identity, parent, digest)
        .unwrap();
    assert_ne!(updated.identity(), original.identity());
    assert_eq!(updated.identity(), identity);
    assert_eq!(updated.parent_identity(), parent);
    assert_eq!(updated.digest(), ContentDigest::of_bytes(bytes.as_bytes()));
    assert_eq!(updated.encoded_bytes(), bytes.len());
    assert_eq!(updated.source_key(), original.source_key());
    assert_eq!(updated.relative_path(), original.relative_path());
    assert_eq!(updated.native_source().source_identity(), identity);
    assert_eq!(
        updated.native_source().metadata_source(),
        original.native_source().metadata_source(),
    );
    assert_eq!(original.identity(), snapshot.startup_source().identity());
    assert_eq!(original.digest(), snapshot.startup_source().digest());
    assert_eq!(
        original.encoded_bytes(),
        snapshot.startup_source().encoded_bytes(),
    );
}

#[test]
fn native_refresh_requires_current_file_and_same_canonical_identity() {
    let (project, snapshot) = captured_project(true);
    let root = snapshot.project_root();
    let original = AuthoringDocumentSourceFact::from_captured_source(
        snapshot.startup_source(),
        snapshot.startup_native_source(),
    )
    .unwrap();
    let mut publication = FilePublicationHost::bind_existing(
        &root,
        original.relative_path(),
        original.identity(),
        original.digest(),
        65_536,
    )
    .unwrap();
    let bytes = SceneDocument::default().to_json_string().unwrap();
    let FilePublicationOutcome::Published(accepted) = publication.publish(bytes.as_bytes()) else {
        panic!("the current source must accept a replacement");
    };
    let (_, binding) = accepted.into_parts();
    publication.commit_published(binding);
    let (identity, parent, digest) = publication.accepted_source_observation().unwrap();
    let current = original
        .with_persistence_observation(identity, parent, digest)
        .unwrap();
    let metadata_path = project.path(&format!("{SCENE_PATH}.meta"));
    let metadata = fs::read_to_string(&metadata_path).unwrap();
    fs::write(&metadata_path, format!("{metadata}\n")).unwrap();
    let policy = NativeDocumentSourcePolicy::new(snapshot.settings());
    let native = policy
        .admit(&root, SCENE_PATH, SourceDocumentKind::Scene, identity)
        .unwrap();
    assert!(matches!(
        original.with_native_source(native.clone()),
        Err(AuthoringDocumentAdmissionError::SourceIdentityChanged),
    ));
    let refreshed = current.with_native_source(native.clone()).unwrap();
    assert_eq!(refreshed.native_source(), &native);
    assert_eq!(refreshed.identity(), current.identity());
    assert_eq!(refreshed.parent_identity(), current.parent_identity());
    assert_eq!(refreshed.digest(), current.digest());
    assert_eq!(refreshed.encoded_bytes(), current.encoded_bytes());
    assert_ne!(
        refreshed.native_source().metadata_source(),
        current.native_source().metadata_source(),
    );
    let prefab_native = snapshot
        .prefab_sources()
        .next()
        .unwrap()
        .native_source()
        .clone();
    assert!(matches!(
        current.with_native_source(prefab_native),
        Err(AuthoringDocumentAdmissionError::DocumentKindMismatch),
    ));
    write_metadata(
        &project,
        SCENE_PATH,
        "external.scene.json",
        AssetSourceKind::Scene,
        "2c9c468a-d640-4a43-9594-02313a5888be",
    );
    let changed_key = policy
        .admit(&root, SCENE_PATH, SourceDocumentKind::Scene, identity)
        .unwrap();
    assert!(matches!(
        current.with_native_source(changed_key),
        Err(AuthoringDocumentAdmissionError::NativeSourceIdentityConflict),
    ));
}

#[cfg(target_pointer_width = "32")]
#[test]
fn persistence_observation_rejects_a_length_that_cannot_fit_the_host() {
    let (_project, snapshot) = captured_project(false);
    let source = snapshot.startup_source();
    let fact =
        AuthoringDocumentSourceFact::from_captured_source(source, snapshot.startup_native_source())
            .unwrap();
    let length = u64::from(u32::MAX) + 1;
    assert!(matches!(
        fact.with_persistence_observation(
            source.identity(),
            source.parent_identity(),
            ContentDigest::from_parts(length, [0; 32]),
        ),
        Err(AuthoringDocumentAdmissionError::SourceTooLarge { maximum })
            if maximum == u64::from(u32::MAX),
    ));
}

fn captured_project(stable: bool) -> (TestProject, AuthoringProjectSessionSnapshot) {
    let project = TestProject::new();
    let mut manifest = fs::read_to_string(project.path("nara.toml")).unwrap();
    if stable {
        manifest = manifest.replace("project_location", "stable_metadata");
    }
    manifest.push_str("\n[startup]\ndefault_scene = 'external.scene.json'\n");
    project.write("nara.toml", manifest.as_bytes());
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("external.prefab.ron").unwrap(),
        overrides: None,
    });
    project.write(
        SCENE_PATH,
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap()
            .as_bytes(),
    );
    project.write(
        PREFAB_PATH,
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("leaf").unwrap(),
        )])
        .to_ron_string()
        .unwrap()
        .as_bytes(),
    );
    if stable {
        for (path, declaration, kind, id) in [
            (
                SCENE_PATH,
                "external.scene.json",
                AssetSourceKind::Scene,
                SCENE_STABLE_ID,
            ),
            (
                PREFAB_PATH,
                "external.prefab.ron",
                AssetSourceKind::Prefab,
                PREFAB_STABLE_ID,
            ),
        ] {
            write_metadata(&project, path, declaration, kind, id);
        }
    }
    let root = DirectoryCapability::from_host_handle(
        host_directory(&project.root),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let snapshot = PreparedAuthoringProject::new(schema, Box::new([]))
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_parts()
        .unwrap()
        .snapshot;
    (project, snapshot)
}

fn write_metadata(
    project: &TestProject,
    path: &str,
    declaration: &str,
    kind: AssetSourceKind,
    stable_id: &str,
) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(declaration).unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    project.write(&format!("{path}.meta"), metadata.as_bytes());
}
