use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetPath, StableAssetId};
use nara_core::ItemLimit;
use nara_project::ProjectManifest;
use nara_scene::{
    PrefabEntityTarget, SceneEntityId, SceneEntityPlacement, ScenePlacement, ScenePlacementParent,
};
use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand};

use super::*;
use crate::{
    project_host::{
        authoring::{AuthoringProjectAdmissionHost, document},
        file_publication::{FilePublicationHost, FilePublicationOutcome},
    },
    test_host_fs as host_fs,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) const RECOVERY: &str = "recovery/new-package.json";
pub(super) const P: &str = "prefabs/actor.prefab.json";
pub(super) const Q: &str = "prefabs/new.prefab.json";
pub(super) const SIDECAR: &str = "prefabs/new.prefab.json.meta";
const SCENE: &str = "scenes/start.scene.json";
const UNIVERSE: &str = "73a52edc-e01a-429d-8acf-bbb3b2732478";

pub(super) struct Fixture {
    pub(super) directory: PathBuf,
    pub(super) project: AuthoringProjectSessionSnapshot,
    pub(super) workspace: EditorWorkspace,
    pub(super) document: EditorDocumentId,
    source: AuthoringDocumentSourceFact,
    publisher: FilePublicationHost,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "nara-prefab-recovery-storage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        for parent in ["prefabs", "scenes", "recovery"] {
            fs::create_dir_all(directory.join(parent)).unwrap();
        }
        fs::write(
            directory.join("nara.toml"),
            format!(
                "schema_version = 1\n[project]\nname = 'Prefab recovery storage'\n\
                 source_universe = '{UNIVERSE}'\nnative_document_identity = 'project_location'\n\
                 [authoring]\nstate = 'ready'\nschema_bundle = 'schema.json'\n\
                 [startup]\ndefault_scene = 'start.scene.json'\n"
            ),
        )
        .unwrap();
        fs::write(
            directory.join("schema.json"),
            AuthoringSchemaBundle::new([])
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let id = |name| SceneEntityId::new(name).unwrap();
        let prefab = PrefabDocument::new(
            [
                SceneEntityPlacement::root(id("branch")),
                SceneEntityPlacement::new(
                    PrefabEntityTarget::source(id("leaf")),
                    ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                        id("branch"),
                    ))),
                ),
                SceneEntityPlacement::root(id("outside")),
            ],
            ["branch", "leaf", "outside"].map(|name| SceneEntityRecord::new(id(name))),
        );
        fs::write(directory.join(P), prefab.to_json_string().unwrap()).unwrap();
        fs::write(
            directory.join(format!("{P}.meta")),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str("238c736d-8bbc-452a-86e1-c7df33267878").unwrap(),
                    AssetPath::new("actor.prefab.json").unwrap(),
                    AssetSourceKind::Prefab,
                ),
                0_u32,
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let mut actor = SceneEntityRecord::new(id("actor"));
        actor.prefab = Some(PrefabInstance::forest(
            AssetRef::path("actor.prefab.json").unwrap(),
        ));
        fs::write(
            directory.join(SCENE),
            SceneDocument::from_ordered_roots([actor])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();

        let root = host_fs::writable_directory(&directory).unwrap();
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(root, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters =
            crate::project_host::authoring::project_authoring_adapters(schema.bundle()).unwrap();
        let project = PreparedAuthoringProject::with_scene_limits(
            schema,
            adapters,
            SceneFileLimits::default()
                .with_encoded_bytes(ByteLimit::new(65_536).unwrap())
                .with_entities(ItemLimit::new(64).unwrap())
                .with_components(ItemLimit::new(64).unwrap()),
        )
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap();
        let root = project.project_root();
        let file = root
            .open_file_exact(&RelativePath::new(P).unwrap())
            .unwrap();
        let native = NativeDocumentSourcePolicy::new(project.settings())
            .admit(&root, P, SourceDocumentKind::Prefab, file.identity())
            .unwrap();
        let bytes = file.read_to_end_bounded(65_536).unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        let source = AuthoringDocumentSourceFact::from_native_observation(
            P,
            file.identity(),
            file.parent_identity(),
            digest,
            &native,
        )
        .unwrap();
        let publisher =
            FilePublicationHost::bind_existing(&root, P, &file, digest, 65_536).unwrap();
        let locator = document::document_source(&ProjectPath::new(P).unwrap()).unwrap();
        let mut workspace = EditorWorkspace::new();
        let opened = workspace
            .open_authoring_prefab_document(
                P,
                EditorDocumentSourceBinding::new(P),
                PrefabDocumentCandidate::decode_json_bytes_with_limits(
                    &bytes,
                    project.scene_limits,
                )
                .unwrap()
                .admit_authoring_with_basis(locator, &project.startup_scene.schema_basis())
                .unwrap()
                .publish(),
            )
            .unwrap();
        let document = opened.opened_document.unwrap();
        let session = workspace.prefab(document).unwrap().session();
        let patch = crate::scene_test_support::add_root_entity_patch(
            &session.document().placements,
            SceneEntityRecord::new(id("working")),
        );
        let base_revision = session.revision();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::ApplyPrefabPatch {
                    document,
                    base_revision,
                    patch,
                })
                .applied
        );
        Self {
            directory,
            project,
            workspace,
            document,
            source,
            publisher,
        }
    }

    pub(super) fn recovery(&self) -> PreparedProjectPrefabExtractionRecovery {
        let mut work = PrefabExtractionWork::new(Default::default()).unwrap();
        let input = self
            .workspace
            .capture_prefab_extraction_input(
                self.document,
                &SceneEntityId::new("branch").unwrap(),
                &self.project.startup_scene.schema_basis(),
                &mut work,
            )
            .unwrap();
        let metadata = AssetMetaIdentity::new(
            StableAssetId::parse_str("5fc6d667-2036-4a41-b189-bc642731b0ca").unwrap(),
            AssetPath::new("new.prefab.json").unwrap(),
            AssetSourceKind::Prefab,
        );
        let worker_bytes = checked_sum([
            PrefabExtractionCaptureRequest::metadata_retained_bytes(&self.project, &metadata)
                .unwrap(),
            PrefabExtractionOpenInput::retained_bytes(Some(&self.source)).unwrap(),
        ])
        .unwrap();
        let mut inputs = self
            .project
            .admit_source_capture_inputs(0, worker_bytes)
            .unwrap();
        let observation = inputs
            .capture_source_observation(self.publisher.source_observation_cost().unwrap(), || {
                self.publisher
                    .capture_source_observation((
                        self.source.identity(),
                        self.source.parent_identity(),
                        self.source.digest(),
                    ))
                    .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
            })
            .unwrap();
        self.project
            .capture_prefab_extraction_request(
                input,
                vec![PrefabExtractionOpenInput {
                    document: self.document,
                    source: Some(self.source.clone()),
                    observation: Some(observation),
                }],
                inputs,
                work,
                PrefabExtractionTarget { metadata },
            )
            .capture()
            .unwrap()
            .prepare()
            .unwrap()
            .prepare_review(vec![])
            .unwrap()
            .prepare_outputs()
            .unwrap()
            .prepare_recovery(Default::default())
            .unwrap()
    }

    pub(super) fn storage(&self) -> ProjectPrefabExtractionStorage {
        self.recovery()
            .prepare_storage(ProjectPath::new(RECOVERY).unwrap())
            .unwrap()
    }

    pub(super) fn pending(&self) -> ProjectPrefabExtractionStorage {
        until(
            self.storage(),
            ProjectPrefabExtractionStoragePhase::Confirmed,
        )
    }

    pub(super) fn fresh_root(&self) -> DirectoryCapability {
        host_fs::writable_directory(&self.directory).unwrap()
    }

    pub(super) fn manifest(&self) -> ProjectManifest {
        ProjectManifest::parse_toml_bytes(&fs::read(self.directory.join("nara.toml")).unwrap())
            .manifest
            .unwrap()
    }

    pub(super) fn replace(&self, path: &str, bytes: &[u8]) {
        let root = self.fresh_root();
        let file = root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let digest = file.digest(64 * 1024 * 1024).unwrap();
        let mut publisher =
            FilePublicationHost::bind_existing(&root, path, &file, digest, 64 * 1024 * 1024)
                .unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(_) => {}
            _ => panic!("fixture replacement must finish with actual publication evidence"),
        }
    }

    pub(super) fn originals(&self) -> Vec<(PathBuf, Vec<u8>)> {
        ["schema.json", P, "prefabs/actor.prefab.json.meta", SCENE]
            .into_iter()
            .map(|path| {
                (
                    PathBuf::from(path),
                    fs::read(self.directory.join(path)).unwrap(),
                )
            })
            .collect()
    }

    pub(super) fn assert_destinations_missing(&self) {
        assert!(!self.directory.join(Q).exists());
        assert!(!self.directory.join(SIDECAR).exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let resolved = self.directory.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-prefab-recovery-storage-")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

pub(super) fn until(
    mut storage: ProjectPrefabExtractionStorage,
    phase: ProjectPrefabExtractionStoragePhase,
) -> ProjectPrefabExtractionStorage {
    while storage.phase() != phase {
        assert_ne!(
            storage.phase(),
            ProjectPrefabExtractionStoragePhase::Confirmed
        );
        storage = storage.advance().unwrap();
    }
    storage
}
