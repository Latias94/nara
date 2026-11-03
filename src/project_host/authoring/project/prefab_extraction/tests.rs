use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{AssetMeta, AssetProductRef, ImportedProductId, StableAssetId};
use nara_core::{ByteLimit, ItemLimit};
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_scene::{
    PrefabEntityTarget, SceneCompilationLimits, SceneDocumentCandidate, SceneEntityId,
    SceneEntityPlacement, ScenePlacement, ScenePlacementParent,
};
use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand};

use super::*;
use crate::project_host::{
    authoring::{AuthoringProjectAdmissionHost, document},
    file_publication::FilePublicationHost,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const UNIVERSE: &str = "73a52edc-e01a-429d-8acf-bbb3b2732478";
const P_ID: &str = "238c736d-8bbc-452a-86e1-c7df33267878";
const R_ID: &str = "488970ab-043b-44ad-8c36-a674cd391636";
const Q_ID: &str = "5fc6d667-2036-4a41-b189-bc642731b0ca";
const ALIAS: &str = "actor.prefab.json";
const P: &str = "desktop/prefabs/actor.prefab.json";
const R: &str = "mobile/prefabs/actor.prefab.json";
const SHARED: &str = "scenes/shared.scene.json";
const INACTIVE: &str = "scenes/inactive.scene.json";
const GUARD: &str = "desktop/prefabs/guard.prefab.ron";
const CLOSED: &str = "scenes/closed.scene.json";
const Q: &str = "desktop/prefabs/new.prefab.json";
const Q_SIDECAR: &str = "desktop/prefabs/new.prefab.json.meta";

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn metadata(local: &str, stable: &str) -> AssetMetaIdentity {
    AssetMetaIdentity::new(
        StableAssetId::parse_str(stable).unwrap(),
        AssetPath::new(local).unwrap(),
        AssetSourceKind::Prefab,
    )
}

fn actor_prefab(leaf: &str) -> PrefabDocument {
    let child = |entity: &str, parent: &str| {
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(id(entity)),
            ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                id(parent),
            ))),
        )
    };
    PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("trunk")),
            child("branch", "trunk"),
            child(leaf, "branch"),
            SceneEntityPlacement::root(id("outside")),
        ],
        ["trunk", "branch", leaf, "outside"]
            .into_iter()
            .map(|name| SceneEntityRecord::new(id(name))),
    )
}

struct Fixture {
    root: PathBuf,
    project: AuthoringProjectSessionSnapshot,
    workspace: EditorWorkspace,
    authority: nara_tooling::EditorPersistenceAuthority,
    open: Vec<(
        EditorDocumentId,
        AuthoringDocumentSourceFact,
        FilePublicationHost,
    )>,
    target: AssetMetaIdentity,
}

impl Fixture {
    fn new() -> Self {
        Self::with_mobile_assets("mobile/assets")
    }

    fn with_mobile_assets(mobile_assets: &str) -> Self {
        Self::with_profiles(mobile_assets, "")
    }

    fn with_profiles(mobile_assets: &str, extra_profiles: &str) -> Self {
        Self::with_guard_route(mobile_assets, extra_profiles, false)
    }

    fn with_guard_route(
        mobile_assets: &str,
        extra_profiles: &str,
        obsolete_disk_route: bool,
    ) -> Self {
        Self::with_guard_target(
            mobile_assets,
            extra_profiles,
            obsolete_disk_route.then_some("desktop-leaf"),
        )
    }

    fn with_guard_target(
        mobile_assets: &str,
        extra_profiles: &str,
        guard_target: Option<&str>,
    ) -> Self {
        Self::with_native_configuration(
            mobile_assets,
            extra_profiles,
            guard_target,
            "project_location",
        )
    }

    fn with_native_identity(identity: &str) -> Self {
        Self::with_native_configuration("mobile/assets", "", None, identity)
    }

    fn with_native_configuration(
        mobile_assets: &str,
        extra_profiles: &str,
        guard_target: Option<&str>,
        identity: &str,
    ) -> Self {
        Self::with_binding_layout(mobile_assets, extra_profiles, guard_target, identity, false)
    }

    fn with_scoped_bindings() -> Self {
        Self::with_binding_layout("mobile/assets", "", None, "project_location", true)
    }

    fn with_binding_layout(
        mobile_assets: &str,
        extra_profiles: &str,
        guard_target: Option<&str>,
        identity: &str,
        scoped_bindings: bool,
    ) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_project_extraction_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in [
            "assets",
            "assets/shared",
            "scenes",
            "desktop/prefabs",
            "desktop/prefabs/isolated",
            "mobile/assets",
            "mobile/prefabs",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            format!(
                r#"schema_version = 1
[project]
name = "Project extraction capture"
source_universe = "{UNIVERSE}"
native_document_identity = "{identity}"
[authoring]
state = "ready"
schema_bundle = "schema.json"
[paths]
prefabs = "desktop/prefabs"
[startup]
default_scene = "shared.scene.json"
[profiles.mobile.paths]
assets = "{mobile_assets}"
prefabs = "mobile/prefabs"
{extra_profiles}
"#,
            ),
        )
        .unwrap();
        fs::write(
            root.join("schema.json"),
            AuthoringSchemaBundle::new([])
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        for (path, leaf, stable) in [(P, "desktop-leaf", P_ID), (R, "mobile-leaf", R_ID)] {
            fs::write(
                root.join(path),
                actor_prefab(leaf).to_json_string().unwrap(),
            )
            .unwrap();
            fs::write(
                root.join(format!("{path}.meta")),
                AssetMeta::new(metadata(ALIAS, stable), 0_u32)
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
        }
        let mut actor = SceneEntityRecord::new(id("actor"));
        actor.prefab = Some(PrefabInstance::forest(AssetRef::path(ALIAS).unwrap()));
        for path in [SHARED, CLOSED] {
            fs::write(
                root.join(path),
                SceneDocument::from_ordered_roots([actor.clone()])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
        }
        fs::write(
            root.join(INACTIVE),
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("disk"))])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut guard = SceneEntityRecord::new(id("disk"));
        if let Some(target) = guard_target {
            let p = SourceDocumentKey::project_location(
                UNIVERSE.parse().unwrap(),
                AssetSourcePath::new(P).unwrap(),
                SourceDocumentKind::Prefab,
            );
            let mut overlay = nara_scene::PrefabOverlay::new(p);
            overlay
                .suppressions
                .push(PrefabEntityTarget::source(id(target)));
            let mut instance = PrefabInstance::forest(AssetRef::path(ALIAS).unwrap());
            instance.overrides = Some(overlay);
            guard.prefab = Some(instance);
        }
        fs::write(
            root.join(GUARD),
            PrefabDocument::from_ordered_roots([guard])
                .to_ron_string()
                .unwrap(),
        )
        .unwrap();

        if identity == "stable_metadata" {
            for (path, local, kind, stable) in [
                (
                    SHARED,
                    "shared.scene.json",
                    AssetSourceKind::Scene,
                    "a3fa46d3-119c-4212-b9cb-ff9f5f513f52",
                ),
                (
                    CLOSED,
                    "closed.scene.json",
                    AssetSourceKind::Scene,
                    "a4f0040e-0af5-4a8c-9eb9-6365ed990e71",
                ),
                (
                    INACTIVE,
                    "inactive.scene.json",
                    AssetSourceKind::Scene,
                    "6572ecb6-4aab-43b8-a537-3df46f746c49",
                ),
                (
                    GUARD,
                    "guard.prefab.ron",
                    AssetSourceKind::Prefab,
                    "9b0d0fd4-d4c8-4ea3-8af7-b5157f688d80",
                ),
            ] {
                let declaration = AssetMetaIdentity::new(
                    StableAssetId::parse_str(stable).unwrap(),
                    AssetPath::new(local).unwrap(),
                    kind,
                );
                fs::write(
                    root.join(format!("{path}.meta")),
                    AssetMeta::new(declaration, 0_u32).to_json_string().unwrap(),
                )
                .unwrap();
            }
        }

        let project = admit_project(&root);
        let basis = project.startup_scene.schema_basis();
        let byte_limit = u64::try_from(project.scene_limits.encoded_bytes().get()).unwrap();
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let mut open = Vec::new();
        for path in [P, SHARED, INACTIVE, GUARD] {
            let root = project.project_root();
            let file = root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            let kind = if path.ends_with(".scene.json") {
                SourceDocumentKind::Scene
            } else {
                SourceDocumentKind::Prefab
            };
            let native = NativeDocumentSourcePolicy::new(project.settings())
                .admit(&root, path, kind, file.identity())
                .unwrap();
            let bytes = file.read_to_end_bounded(byte_limit).unwrap();
            let digest = ContentDigest::of_bytes(&bytes);
            let source = AuthoringDocumentSourceFact::from_native_observation(
                path,
                file.identity(),
                file.parent_identity(),
                digest,
                &native,
            )
            .unwrap();
            let host =
                FilePublicationHost::bind_existing(&root, path, &file, digest, byte_limit).unwrap();
            let locator = document::document_source(&ProjectPath::new(path).unwrap()).unwrap();
            let binding = EditorDocumentSourceBinding::new(if scoped_bindings {
                path.rsplit('/').next().unwrap()
            } else {
                path
            });
            let report = if kind == SourceDocumentKind::Scene {
                workspace
                    .open_authoring_scene_document(
                        path,
                        binding,
                        SceneDocumentCandidate::decode_json_bytes_with_limits(
                            &bytes,
                            project.scene_limits,
                        )
                        .unwrap()
                        .admit_authoring_with_basis(locator, &basis)
                        .unwrap()
                        .publish(),
                    )
                    .unwrap()
            } else {
                let candidate = if path.ends_with(".ron") {
                    PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                        &bytes,
                        project.scene_limits,
                    )
                } else {
                    PrefabDocumentCandidate::decode_json_bytes_with_limits(
                        &bytes,
                        project.scene_limits,
                    )
                }
                .unwrap();
                workspace
                    .open_authoring_prefab_document(
                        path,
                        binding,
                        candidate
                            .admit_authoring_with_basis(locator, &basis)
                            .unwrap()
                            .publish(),
                    )
                    .unwrap()
            };
            let document = report.opened_document.unwrap();
            let placements = if kind == SourceDocumentKind::Scene {
                &workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .document()
                    .placements
            } else {
                &workspace
                    .prefab(document)
                    .unwrap()
                    .session()
                    .document()
                    .placements
            };
            let patch = crate::scene_test_support::add_root_entity_patch(
                placements,
                SceneEntityRecord::new(id("working")),
            );
            let base_revision = workspace.document_revision(document).unwrap();
            let report = workspace.apply_authoring_command(if kind == SourceDocumentKind::Scene {
                EditorWorkspaceCommand::ApplyScenePatch {
                    document,
                    base_revision,
                    patch,
                }
            } else {
                EditorWorkspaceCommand::ApplyPrefabPatch {
                    document,
                    base_revision,
                    patch,
                }
            });
            assert!(report.applied, "{:?}", report.diagnostics);
            if path == GUARD && guard_target.is_some() {
                let session = workspace.prefab(document).unwrap().session();
                let expected = session
                    .document()
                    .entities
                    .iter()
                    .find(|record| record.id == id("disk"))
                    .unwrap()
                    .prefab
                    .clone();
                let patch = nara_scene::ScenePatchDocument::new([
                    nara_scene::ScenePatchOperation::SetPrefabInstance {
                        entity: id("disk"),
                        expected,
                        value: Some(PrefabInstance::forest(AssetRef::path(ALIAS).unwrap())),
                    },
                ]);
                let base_revision = workspace.document_revision(document).unwrap();
                assert!(
                    workspace
                        .apply_authoring_command(EditorWorkspaceCommand::ApplyPrefabPatch {
                            document,
                            base_revision,
                            patch
                        })
                        .applied
                );
            }
            assert_eq!(workspace.document_is_dirty(document), Some(true));
            open.push((document, source, host));
        }
        assert!(workspace.activate_document(open[0].0).applied);
        Self {
            root,
            project,
            workspace,
            authority,
            open,
            target: metadata("new.prefab.json", Q_ID),
        }
    }

    fn document(&self, path: &str) -> EditorDocumentId {
        self.open
            .iter()
            .find(|(_, source, _)| source.relative_path() == path)
            .unwrap()
            .0
    }

    fn request(&self) -> PrefabExtractionCaptureRequest {
        self.request_with_limits(SceneCompilationLimits::default())
    }

    fn request_with_limits(
        &self,
        limits: SceneCompilationLimits,
    ) -> PrefabExtractionCaptureRequest {
        self.request_with_host_handles(limits, ItemLimit::new(128).unwrap())
    }

    fn request_with_host_handles(
        &self,
        limits: SceneCompilationLimits,
        handles: ItemLimit,
    ) -> PrefabExtractionCaptureRequest {
        self.request_with_host_limits(limits, handles, ItemLimit::new(200_000_000).unwrap())
    }

    fn request_with_host_limits(
        &self,
        limits: SceneCompilationLimits,
        handles: ItemLimit,
        host_work: ItemLimit,
    ) -> PrefabExtractionCaptureRequest {
        let mut work = PrefabExtractionWork::new(limits).unwrap();
        let input = self
            .workspace
            .capture_prefab_extraction_input(
                self.document(P),
                &id("branch"),
                &self.project.startup_scene.schema_basis(),
                &mut work,
            )
            .unwrap();
        let worker_bytes = self
            .open
            .iter()
            .try_fold(
                PrefabExtractionCaptureRequest::metadata_retained_bytes(
                    &self.project,
                    &self.target,
                )
                .unwrap(),
                |bytes, (_, source, _)| {
                    checked_sum([
                        bytes,
                        PrefabExtractionOpenInput::retained_bytes(Some(source))?,
                    ])
                },
            )
            .unwrap();
        let mut inputs = self
            .project
            .admit_source_operation_inputs(0, worker_bytes, host_work, handles)
            .unwrap();
        let open = self
            .open
            .iter()
            .map(|(document, source, host)| {
                let observation = inputs
                    .capture_source_observation(host.source_observation_cost().unwrap(), || {
                        host.capture_source_observation((
                            source.identity(),
                            source.parent_identity(),
                            source.digest(),
                        ))
                        .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
                    })
                    .unwrap();
                PrefabExtractionOpenInput {
                    document: *document,
                    source: Some(source.clone()),
                    observation: Some(observation),
                }
            })
            .collect();
        self.project.capture_prefab_extraction_request(
            input,
            open,
            inputs,
            work,
            PrefabExtractionTarget {
                metadata: self.target.clone(),
            },
        )
    }

    fn disk_state(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut directories = vec![self.root.clone()];
        let mut files = Vec::new();
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    directories.push(path);
                } else {
                    files.push((
                        path.strip_prefix(&self.root).unwrap().to_owned(),
                        fs::read(path).unwrap(),
                    ));
                }
            }
        }
        files.sort_unstable();
        files
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_project_extraction_")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

fn admit_project(root: &Path) -> AuthoringProjectSessionSnapshot {
    let root = DirectoryCapability::from_host_handle(
        host_directory(root).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters =
        crate::project_host::authoring::project_authoring_adapters(schema.bundle()).unwrap();
    PreparedAuthoringProject::with_scene_limits(
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
    .unwrap()
}

fn source_index(sources: &[CapturedExtractionSource], path: &str) -> usize {
    let kind = if path.ends_with(".scene.json") {
        SourceDocumentKind::Scene
    } else {
        SourceDocumentKind::Prefab
    };
    let key = SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(path).unwrap(),
        kind,
    );
    sources
        .iter()
        .position(|source| source.key() == &key)
        .unwrap()
}

#[test]
fn actual_capture_separates_disk_from_frozen_working_sources_and_keeps_unopened_members() {
    let mut fixture = Fixture::new();
    let disk = fixture.disk_state();
    let inactive = fixture.document(INACTIVE);
    let guard = fixture.document(GUARD);
    let scene = fixture
        .workspace
        .scene(inactive)
        .unwrap()
        .session()
        .shared_document();
    let prefab = fixture
        .workspace
        .prefab(guard)
        .unwrap()
        .session()
        .shared_document();
    let mut request = fixture.request();
    let inherited = request.inputs.budget.usage();
    let initial_work = request.work.usage();
    request.open.reverse();
    for document in [inactive, guard] {
        let base_revision = fixture.workspace.document_revision(document).unwrap();
        assert!(
            fixture
                .workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision,
                })
                .applied
        );
    }
    let mut baseline = request.capture().unwrap();
    baseline.revalidate_project(&fixture.project).unwrap();
    assert!(Arc::ptr_eq(
        &baseline.context.schema,
        &fixture.project.schema
    ));
    assert_eq!(baseline.context.inventory.participants().len(), 6);
    assert_eq!(baseline.context.open.len(), 4);
    assert_eq!(baseline.bindings.len(), 4);
    assert_eq!(baseline.input.selected_document(), fixture.document(P));
    assert_eq!(baseline.input.root(), &id("branch"));
    assert_eq!(
        baseline.context.inventory.scene_limits(),
        fixture.project.scene_limits,
    );
    assert!(baseline.work.usage().work_bytes > initial_work.work_bytes);
    assert!(baseline.context.budget.usage().decode_work() > inherited.decode_work());
    assert_eq!(
        baseline.context.budget.usage().retained_open_handles(),
        inherited.retained_open_handles() + baseline.context.inventory.retained_handle_count(),
    );
    for member in baseline.context.inventory.participants() {
        let index = source_index(baseline.request.sources(), member.file().path());
        assert_eq!(
            member.bytes(),
            fs::read(fixture.root.join(member.file().path())).unwrap(),
        );
        let original = &baseline.context.disk[index];
        let working = &baseline.request.sources()[index];
        assert_eq!(original.key(), working.key());
        match (original, working) {
            (
                CapturedExtractionSource::Scene { document: disk, .. },
                CapturedExtractionSource::Scene {
                    document: captured, ..
                },
            ) => {
                assert!(
                    !disk
                        .entities
                        .iter()
                        .any(|entity| entity.id == id("working"))
                );
                if member.file().path() == CLOSED {
                    assert!(Arc::ptr_eq(disk, captured));
                } else {
                    assert!(!Arc::ptr_eq(disk, captured));
                    assert!(
                        captured
                            .entities
                            .iter()
                            .any(|entity| entity.id == id("working"))
                    );
                }
                if member.file().path() == INACTIVE {
                    assert!(Arc::ptr_eq(captured, &scene));
                    assert!(!Arc::ptr_eq(
                        captured,
                        &fixture
                            .workspace
                            .scene(inactive)
                            .unwrap()
                            .session()
                            .shared_document(),
                    ));
                }
            }
            (
                CapturedExtractionSource::Prefab { source: disk },
                CapturedExtractionSource::Prefab { source: captured },
            ) => {
                assert!(
                    !disk
                        .document()
                        .entities
                        .iter()
                        .any(|entity| entity.id == id("working"))
                );
                if member.file().path() == R {
                    assert!(std::ptr::eq(disk.document(), captured.document()));
                } else {
                    assert!(!std::ptr::eq(disk.document(), captured.document()));
                    assert!(
                        captured
                            .document()
                            .entities
                            .iter()
                            .any(|entity| entity.id == id("working"))
                    );
                }
                if member.file().path() == GUARD {
                    assert!(std::ptr::eq(captured.document(), prefab.as_ref()));
                    assert!(!std::ptr::eq(
                        captured.document(),
                        fixture
                            .workspace
                            .prefab(guard)
                            .unwrap()
                            .session()
                            .document(),
                    ));
                }
            }
            _ => panic!("capture changed the actual source kind"),
        }
    }
    for path in [R, CLOSED] {
        let index = source_index(baseline.request.sources(), path);
        assert!(
            !baseline
                .bindings
                .iter()
                .any(|binding| binding.source == index)
        );
    }
    assert!(
        baseline
            .input
            .validate(&fixture.workspace, &mut baseline.work)
            .is_err()
    );
    assert!(!fixture.root.join(Q).exists());
    assert!(!fixture.root.join(Q_SIDECAR).exists());
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn actual_profile_catalogues_and_preparation_bind_the_original_workspace_without_opening_q() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let tabs = fixture.workspace.authoring_model().documents;
    let selected = fixture
        .workspace
        .prefab(fixture.document(P))
        .unwrap()
        .session()
        .shared_document();
    let mut baseline = fixture.request().capture().unwrap();
    baseline.revalidate_project(&fixture.project).unwrap();
    assert_eq!(
        baseline
            .request
            .environments()
            .iter()
            .map(|environment| environment.name.as_str())
            .collect::<Vec<_>>(),
        ["default", "profile:mobile"],
    );
    assert_eq!(baseline.request.selected_environment(), 0);
    let sources = baseline.request.sources();
    let p = source_index(sources, P);
    let r = source_index(sources, R);
    let shared = source_index(sources, SHARED);
    let CapturedExtractionSource::Prefab { source } = &sources[p] else {
        panic!("the selected source must remain a Prefab");
    };
    assert!(std::ptr::eq(source.document(), selected.as_ref()));
    let q = &baseline.context.destination.key;
    for (index, (path, visible, hidden)) in
        [(P, P_ID, R_ID), (R, R_ID, P_ID)].into_iter().enumerate()
    {
        let environment = &baseline.request.environments()[index];
        let policy = &baseline.context.inventory.configurations()[index];
        let binding = policy
            .catalogue_bindings()
            .iter()
            .find(|binding| binding.directory().as_str() == path.rsplit_once('/').unwrap().0)
            .unwrap();
        let expected = sources[if index == 0 { p } else { r }].key();
        assert!(environment.sources.contains(&shared));
        assert!(
            environment
                .sources
                .contains(&if index == 0 { p } else { r })
        );
        assert!(
            !environment
                .sources
                .contains(&if index == 0 { r } else { p })
        );
        for reference in [
            AssetRef::path(ALIAS).unwrap(),
            AssetRef::StableId(visible.parse().unwrap()),
            AssetRef::Product(
                AssetProductRef::new(visible.parse().unwrap(), ImportedProductId::PRIMARY).unwrap(),
            ),
        ] {
            for bindings in [&environment.before, &environment.after] {
                assert_eq!(
                    bindings
                        .iter()
                        .find(|entry| entry.reference == reference)
                        .map(|entry| &entry.source),
                    Some(expected),
                );
            }
            for assets in [
                environment.assets_before.as_ref().unwrap(),
                environment.assets_after.as_ref().unwrap(),
            ] {
                let resolver = assets.database.resolver(binding.scope());
                assert_eq!(
                    resolver.resolve_ref(&reference).unwrap().path().as_str(),
                    path
                );
                assert!(
                    resolver
                        .resolve_ref(&AssetRef::StableId(hidden.parse().unwrap()))
                        .is_err()
                );
            }
        }
        let before = environment.assets_before.as_ref().unwrap();
        let after = environment.assets_after.as_ref().unwrap();
        for reference in [
            AssetRef::path("new.prefab.json").unwrap(),
            AssetRef::StableId(fixture.target.stable_id()),
            AssetRef::Product(
                AssetProductRef::new(fixture.target.stable_id(), ImportedProductId::PRIMARY)
                    .unwrap(),
            ),
        ] {
            assert!(
                !environment
                    .before
                    .iter()
                    .any(|entry| entry.reference == reference)
            );
            assert!(
                before
                    .database
                    .resolver(binding.scope())
                    .resolve_ref(&reference)
                    .is_err()
            );
            let association = environment
                .after
                .iter()
                .find(|entry| entry.reference == reference);
            let resolved = after
                .database
                .resolver(binding.scope())
                .resolve_ref(&reference);
            if index == 0 {
                assert_eq!(association.map(|entry| &entry.source), Some(q));
                assert_eq!(resolved.unwrap().path().as_str(), Q);
            } else {
                assert!(association.is_none());
                assert!(resolved.is_err());
            }
        }
    }
    let captured_work = baseline.work.usage();
    let mut prepared = baseline.prepare().unwrap();
    prepared.revalidate_project(&fixture.project).unwrap();
    prepared.workspace.validate(&fixture.workspace).unwrap();
    let corpus = prepared.workspace.corpus();
    let CapturedExtractionSource::Prefab { source } = &corpus.original_sources()[p] else {
        panic!("preparation must retain the original selected Prefab");
    };
    assert!(std::ptr::eq(source.document(), selected.as_ref()));
    assert_eq!(corpus.source_changed(p), Some(true));
    assert_eq!(corpus.source_changed(r), Some(false));
    for path in [INACTIVE, GUARD] {
        assert_eq!(
            corpus.source_changed(source_index(corpus.original_sources(), path)),
            Some(false),
        );
        assert_eq!(
            fixture.workspace.document_is_dirty(fixture.document(path)),
            Some(true),
        );
    }
    assert_eq!(corpus.extracted().key(), &prepared.context.destination.key);
    let extracted = &corpus.extracted().document().entities;
    assert!(extracted.iter().any(|entity| entity.id == id("branch")));
    assert!(
        extracted
            .iter()
            .any(|entity| entity.id == id("desktop-leaf"))
    );
    assert!(
        !extracted
            .iter()
            .any(|entity| entity.id == id("mobile-leaf"))
    );
    assert!(prepared.workspace.usage().work_bytes > captured_work.work_bytes);
    assert_eq!(prepared.workspace.bindings().len(), tabs.len());
    assert_eq!(fixture.workspace.authoring_model().documents, tabs);
    assert!(!fixture.root.join(Q).exists());
    assert!(!fixture.root.join(Q_SIDECAR).exists());
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn omitted_unbound_unobserved_duplicate_and_misbound_open_inputs_return_the_original_request() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    for case in 0..5 {
        let mut request = fixture.request();
        match case {
            0 => {
                request.open.pop();
            }
            1 => request.open[1].source = None,
            2 => request.open[1].observation = None,
            3 => request.open[1].document = request.open[0].document,
            4 => request.open[1].source = request.open[0].source.clone(),
            _ => unreachable!(),
        }
        let allocation = request.open.as_ptr();
        let work = request.work.usage();
        let inherited = request.inputs.budget.usage();
        let source = request
            .input
            .prefab_input(fixture.document(P))
            .unwrap()
            .shared_document();
        let mut refused = request.capture().unwrap_err();
        assert!(
            matches!(refused.error, PrefabExtractionProjectError::OpenSet),
            "{:?}",
            refused.error,
        );
        assert_eq!(refused.request.open.as_ptr(), allocation);
        assert_eq!(refused.request.work.usage(), work);
        assert_eq!(refused.request.target.metadata, fixture.target);
        assert!(Arc::ptr_eq(
            &refused.request.schema,
            &fixture.project.schema
        ));
        assert!(Arc::ptr_eq(
            &refused
                .request
                .input
                .prefab_input(fixture.document(P))
                .unwrap()
                .shared_document(),
            &source,
        ));
        assert_eq!(
            refused
                .request
                .inputs
                .budget
                .usage()
                .retained_candidate_bytes(),
            inherited.retained_candidate_bytes(),
        );
        assert_eq!(
            refused
                .request
                .inputs
                .budget
                .usage()
                .retained_open_handles(),
            inherited.retained_open_handles(),
        );
        refused
            .request
            .input
            .validate(&fixture.workspace, &mut refused.request.work)
            .unwrap();
    }
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn occupied_q_sidecar_or_a_mobile_only_identity_refuses_without_replacing_the_request_or_work() {
    for case in 0..3 {
        let mut fixture = Fixture::new();
        if case == 2 {
            // R is not in the default catalogue; the global physical inventory still owns it.
            fixture.target = metadata("new.prefab.json", R_ID);
        }
        let request = fixture.request();
        let allocation = request.open.as_ptr();
        let work = request.work.usage();
        let inherited = request.inputs.budget.usage();
        match case {
            0 => {
                fs::write(
                    fixture.root.join(Q),
                    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("incumbent"))])
                        .to_json_string()
                        .unwrap(),
                )
                .unwrap();
            }
            1 => {
                fs::write(
                    fixture.root.join(Q_SIDECAR),
                    AssetMeta::new(fixture.target.clone(), 0_u32)
                        .to_json_string()
                        .unwrap(),
                )
                .unwrap();
            }
            2 => {}
            _ => unreachable!(),
        }
        let disk = fixture.disk_state();
        let mut refused = request.capture().unwrap_err();
        match &refused.error {
            PrefabExtractionProjectError::Occupied if case < 2 => {}
            PrefabExtractionProjectError::Destination if case == 2 => {}
            // A valid orphan Prefab sidecar is already a catalogue claim. The complete
            // inventory can refuse its absent source before destination observation.
            PrefabExtractionProjectError::Inventory(SourceInventoryError::Host(FsError::Io {
                source,
                ..
            })) if case == 1 && source.kind() == io::ErrorKind::NotFound => {}
            other => panic!("destination control {case} returned an unrelated refusal: {other:?}"),
        }
        assert_eq!(refused.request.open.as_ptr(), allocation);
        assert_eq!(refused.request.work.usage(), work);
        assert_eq!(refused.request.target.metadata, fixture.target);
        assert_eq!(
            refused
                .request
                .inputs
                .budget
                .usage()
                .retained_candidate_bytes(),
            inherited.retained_candidate_bytes(),
        );
        assert_eq!(
            refused
                .request
                .inputs
                .budget
                .usage()
                .retained_open_handles(),
            inherited.retained_open_handles(),
        );
        assert!(refused.request.inputs.budget.usage().decode_work() > inherited.decode_work());
        refused
            .request
            .input
            .validate(&fixture.workspace, &mut refused.request.work)
            .unwrap();
        if case == 2 {
            assert_eq!(
                refused.request.target.metadata.stable_id(),
                StableAssetId::parse_str(R_ID).unwrap(),
            );
            assert!(!fixture.root.join(Q).exists());
            assert!(!fixture.root.join(Q_SIDECAR).exists());
        }
        assert_eq!(fixture.disk_state(), disk);
    }
}

#[test]
fn late_inventory_source_ancestor_destination_and_authority_changes_refuse_revalidation() {
    for case in 0..6 {
        let mut fixture = Fixture::new();
        if case == 2 {
            fixture.target = metadata("isolated/new.prefab.json", Q_ID);
        }
        let baseline = fixture.request().capture().unwrap();
        let mut prepared = baseline.prepare().unwrap();
        prepared.revalidate_project(&fixture.project).unwrap();
        let tabs = fixture.workspace.authoring_model().documents;
        match case {
            0 => {
                fs::write(
                    fixture.root.join("scenes/late.scene.json"),
                    SceneDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
            }
            1 => {
                // Keep the original handle alive while replacing its name with equal bytes.
                let bytes = fs::read(fixture.root.join(CLOSED)).unwrap();
                fs::rename(
                    fixture.root.join(CLOSED),
                    fixture.root.join("retired.scene"),
                )
                .unwrap();
                fs::write(fixture.root.join(CLOSED), bytes).unwrap();
            }
            2 => {
                fs::rename(
                    fixture.root.join("desktop/prefabs/isolated"),
                    fixture.root.join("desktop/prefabs/retired-isolated"),
                )
                .unwrap();
                fs::create_dir(fixture.root.join("desktop/prefabs/isolated")).unwrap();
            }
            3 => {
                fs::write(
                    fixture.root.join(Q),
                    PrefabDocument::default().to_json_string().unwrap(),
                )
                .unwrap();
            }
            4 => {
                fs::write(
                    fixture.root.join(Q_SIDECAR),
                    AssetMeta::new(fixture.target.clone(), 0_u32)
                        .to_json_string()
                        .unwrap(),
                )
                .unwrap();
            }
            5 => {}
            _ => unreachable!(),
        }
        let disk = fixture.disk_state();
        let error = if case == 5 {
            let other = admit_project(&fixture.root);
            prepared.revalidate_project(&other).unwrap_err()
        } else {
            prepared.revalidate_project(&fixture.project).unwrap_err()
        };
        match case {
            0 | 1 => assert!(
                matches!(
                    error,
                    PrefabExtractionProjectError::Inventory(
                        SourceInventoryError::MembershipChanged
                    )
                ),
                "{error:?}",
            ),
            2 => assert!(
                matches!(error, PrefabExtractionProjectError::Destination),
                "{error:?}",
            ),
            3 | 4 => assert!(
                matches!(error, PrefabExtractionProjectError::Occupied),
                "{error:?}",
            ),
            5 => assert!(
                matches!(error, PrefabExtractionProjectError::OpenSet),
                "{error:?}",
            ),
            _ => unreachable!(),
        }
        prepared.workspace.validate(&fixture.workspace).unwrap();
        assert_eq!(fixture.workspace.authoring_model().documents, tabs);
        assert_eq!(fixture.disk_state(), disk);
    }
}

#[test]
fn original_host_and_source_allowances_bound_capture_without_resetting_spent_work() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let mut request = fixture.request();
    request.inputs.worker_retained_bytes -= 1;
    assert!(matches!(
        request.capture().unwrap_err().error,
        PrefabExtractionProjectError::OpenSet,
    ));

    let mut request = fixture.request();
    let inherited = request.inputs.budget.usage();
    request.inputs.budget.limits = request.inputs.budget.limits.with_retained_candidate_bytes(
        ByteLimit::new(inherited.retained_candidate_bytes()).unwrap(),
    );
    let refused = request.capture().unwrap_err();
    assert!(
        matches!(
            refused.error,
            PrefabExtractionProjectError::Admission(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
            )
        ),
        "{:?}",
        refused.error,
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_candidate_bytes(),
        inherited.retained_candidate_bytes(),
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_open_handles(),
        inherited.retained_open_handles(),
    );
    assert!(refused.request.inputs.budget.usage().decode_work() > inherited.decode_work());

    let original = fixture.request();
    let frozen_work = original.work.usage();
    drop(original);
    let request = fixture.request_with_limits(
        SceneCompilationLimits::default()
            .with_working_bytes(ByteLimit::new(frozen_work.work_bytes + 1).unwrap()),
    );
    assert_eq!(request.work.usage(), frozen_work);
    let inherited = request.inputs.budget.usage();
    let refused = request.capture().unwrap_err();
    assert!(
        matches!(refused.error, PrefabExtractionProjectError::SourceWork(_)),
        "{:?}",
        refused.error,
    );
    assert_eq!(refused.request.work.usage(), frozen_work);
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_candidate_bytes(),
        inherited.retained_candidate_bytes(),
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_open_handles(),
        inherited.retained_open_handles(),
    );
    let spent = refused.request.inputs.budget.usage().decode_work();
    let retried = refused.request.capture().unwrap_err();
    assert!(
        matches!(retried.error, PrefabExtractionProjectError::SourceWork(_)),
        "{:?}",
        retried.error,
    );
    assert_eq!(retried.request.work.usage(), frozen_work);
    assert!(retried.request.inputs.budget.usage().decode_work() > spent);
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn proposed_sidecar_inside_another_profiles_scan_must_be_valid_there() {
    let mut fixture = Fixture::with_mobile_assets("desktop/prefabs/isolated");
    fixture.target = metadata("isolated/new.prefab.json", Q_ID);
    let disk = fixture.disk_state();
    let request = fixture.request();
    let inherited = request.inputs.budget.usage();
    let mut refused = request.capture().unwrap_err();
    assert!(
        matches!(
            refused.error,
            PrefabExtractionProjectError::Project(
                AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. }
            )
        ),
        "{:?}",
        refused.error
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_candidate_bytes(),
        inherited.retained_candidate_bytes()
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_open_handles(),
        inherited.retained_open_handles()
    );
    assert_eq!(refused.request.target.metadata, fixture.target);
    refused
        .request
        .input
        .validate(&fixture.workspace, &mut refused.request.work)
        .unwrap();
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn empty_configuration_still_checks_existing_sidecars_in_its_actual_scan_roots() {
    let fixture = Fixture::with_profiles(
        "mobile/assets",
        r#"
[profiles.dormant.paths]
assets = "assets/shared"
scenes = "dormant/scenes"
prefabs = "dormant/prefabs"
[profiles.dormant.startup]
clear_default_scene = true
"#,
    );
    fs::write(
        fixture.root.join("assets/shared/image.bin"),
        b"not demanded",
    )
    .unwrap();
    fs::write(
        fixture.root.join("assets/shared/image.bin.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                "513bc07a-49a7-46dd-a41e-c183298160b3".parse().unwrap(),
                AssetPath::new("shared/image.bin").unwrap(),
                AssetSourceKind::Image,
            ),
            0_u32,
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let disk = fixture.disk_state();
    let request = fixture.request();
    let inherited = request.inputs.budget.usage();
    let mut refused = request.capture().unwrap_err();
    assert!(
        matches!(
            refused.error,
            PrefabExtractionProjectError::Project(
                AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { .. }
            )
        ),
        "{:?}",
        refused.error
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_candidate_bytes(),
        inherited.retained_candidate_bytes()
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_open_handles(),
        inherited.retained_open_handles()
    );
    refused
        .request
        .input
        .validate(&fixture.workspace, &mut refused.request.work)
        .unwrap();
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn proposed_identity_cannot_collide_with_an_asset_visible_only_in_another_profile() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("mobile/assets/image.bin"),
        b"not demanded",
    )
    .unwrap();
    fs::write(
        fixture.root.join("mobile/assets/image.bin.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                fixture.target.stable_id(),
                AssetPath::new("image.bin").unwrap(),
                AssetSourceKind::Image,
            ),
            0_u32,
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let disk = fixture.disk_state();
    let request = fixture.request();
    let inherited = request.inputs.budget.usage();
    let refused = request.capture().unwrap_err();
    assert!(
        matches!(refused.error, PrefabExtractionProjectError::Destination),
        "{:?}",
        refused.error
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_candidate_bytes(),
        inherited.retained_candidate_bytes()
    );
    assert_eq!(
        refused
            .request
            .inputs
            .budget
            .usage()
            .retained_open_handles(),
        inherited.retained_open_handles()
    );
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn reviewed_generation_uses_fixed_saved_outputs_and_original_disk_guards() {
    use nara_tooling::EditorPrefabExtractionDisposition;
    let fixture = Fixture::new();
    let disk_bytes = fixture.disk_state();
    let baseline = fixture.request().capture().unwrap();
    let inactive = fixture.document(INACTIVE);
    let guard = fixture.document(GUARD);
    let disk_guard = match &baseline.context.disk[source_index(&baseline.context.disk, GUARD)] {
        CapturedExtractionSource::Prefab { source } => source.document() as *const PrefabDocument,
        _ => unreachable!(),
    };
    let working = fixture
        .workspace
        .scene(inactive)
        .unwrap()
        .session()
        .shared_document();
    let prepared = baseline.prepare().unwrap();
    let usage = prepared.workspace.usage();
    let mut review = prepared.prepare_review(vec![inactive]).unwrap();
    review.require_saved_generation().unwrap();
    review.revalidate_project(&fixture.project).unwrap();
    assert_eq!(
        review.persistence.disposition(fixture.document(P)),
        Some(EditorPrefabExtractionDisposition::ReplaceSource)
    );
    assert_eq!(
        review.persistence.disposition(inactive),
        Some(EditorPrefabExtractionDisposition::SaveCurrent)
    );
    assert_eq!(
        review.persistence.disposition(guard),
        Some(EditorPrefabExtractionDisposition::Guard)
    );
    let saved = review.saved_generation();
    let index = source_index(saved.sources(), INACTIVE);
    let CapturedExtractionSource::Scene { document, .. } = &saved.sources()[index] else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(document, &working));
    assert!(Arc::ptr_eq(
        document,
        review.persistence.saved_scene(inactive).unwrap()
    ));
    assert!(saved.published()[index]);
    let guard_index = source_index(saved.sources(), GUARD);
    let CapturedExtractionSource::Prefab { source } = &saved.sources()[guard_index] else {
        unreachable!()
    };
    assert_eq!(source.document() as *const _, disk_guard);
    assert!(!saved.published()[guard_index]);
    assert!(review.persistence.saved_prefab(guard).is_none());
    assert!(review.persistence.usage().work_bytes > usage.work_bytes);
    assert_eq!(fixture.workspace.document_is_dirty(inactive), Some(true));
    assert_eq!(fixture.workspace.document_is_dirty(guard), Some(true));
    assert_eq!(fixture.disk_state(), disk_bytes);
}

#[test]
fn affected_guard_which_only_names_the_retained_cut_root_can_keep_original_disk_bytes() {
    let fixture = Fixture::with_guard_target("mobile/assets", "", Some("branch"));
    let disk = fixture.disk_state();
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let mut review = prepared.prepare_review(vec![]).unwrap();
    review.require_saved_generation().unwrap();
    let guard = source_index(review.saved.sources(), GUARD);
    assert!(!review.saved.published()[guard]);
    assert_eq!(
        review.persistence.disposition(fixture.document(GUARD)),
        Some(nara_tooling::EditorPrefabExtractionDisposition::Guard)
    );
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn saved_guard_relocation_refusal_and_explicit_save_current_retry_keep_actual_preparations() {
    let fixture = Fixture::with_guard_route("mobile/assets", "", true);
    let original_disk = fixture.disk_state();
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let guard = fixture.document(GUARD);
    let original = prepared.workspace.corpus().sources().as_ptr();
    let p = source_index(prepared.workspace.corpus().sources(), P);
    let p_document = match &prepared.workspace.corpus().sources()[p] {
        CapturedExtractionSource::Prefab { source } => source.document() as *const PrefabDocument,
        _ => unreachable!(),
    };
    let q_document = prepared.workspace.corpus().extracted().document() as *const PrefabDocument;
    let refused = prepared.prepare_review(vec![]).unwrap_err();
    let spent = match &refused {
        review::ProjectPrefabExtractionReviewFailure::Saved {
            persistence, error, ..
        } => {
            let EditorWorkspaceExtractionSavedGenerationFailure::Scene(error) = error else {
                panic!("the actual captured disk Guard must report its route conflict");
            };
            assert!(error.report().diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.prefab-extraction-saved-guard-relocation"
            }));
            assert_eq!(
                persistence.sources().prepared().corpus().sources().as_ptr(),
                original
            );
            persistence.usage()
        }
        _ => panic!("{refused:?}"),
    };
    let mut review = refused.retry(vec![guard]).unwrap();
    review.require_saved_generation().unwrap();
    assert_eq!(
        review
            .persistence
            .sources()
            .prepared()
            .corpus()
            .sources()
            .as_ptr(),
        original
    );
    assert_eq!(review.saved.extracted().document() as *const _, q_document);
    let CapturedExtractionSource::Prefab { source } = &review.saved.sources()[p] else {
        unreachable!()
    };
    assert_eq!(source.document() as *const _, p_document);
    assert!(review.persistence.usage().work_bytes > spent.work_bytes);
    assert!(review.saved.published()[source_index(review.saved.sources(), GUARD)]);
    assert_eq!(
        review.persistence.disposition(guard),
        Some(nara_tooling::EditorPrefabExtractionDisposition::SaveCurrent)
    );
    assert_eq!(fixture.disk_state(), original_disk);
    assert_eq!(fixture.workspace.document_is_dirty(guard), Some(true));
}

#[test]
fn review_can_change_unpublished_save_selection_without_recapturing_sources() {
    let fixture = Fixture::new();
    let original_disk = fixture.disk_state();
    let inactive = fixture.document(INACTIVE);
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let source_table = prepared.workspace.corpus().sources().as_ptr();
    let q = prepared.workspace.corpus().extracted().document() as *const PrefabDocument;
    let mut review = prepared.prepare_review(vec![inactive]).unwrap();
    review.require_saved_generation().unwrap();
    let index = source_index(review.saved.sources(), INACTIVE);
    assert!(review.saved.published()[index]);
    let spent = review.persistence.usage();

    let mut revised = review.revise(vec![]).unwrap();
    revised.require_saved_generation().unwrap();
    revised.revalidate_project(&fixture.project).unwrap();
    assert_eq!(
        revised
            .persistence
            .sources()
            .prepared()
            .corpus()
            .sources()
            .as_ptr(),
        source_table,
    );
    assert_eq!(revised.saved.extracted().document() as *const _, q);
    assert!(!revised.saved.published()[index]);
    assert_eq!(
        revised.persistence.disposition(inactive),
        Some(nara_tooling::EditorPrefabExtractionDisposition::Guard),
    );
    let CapturedExtractionSource::Scene {
        document: saved, ..
    } = &revised.saved.sources()[index]
    else {
        unreachable!()
    };
    let CapturedExtractionSource::Scene { document: disk, .. } = &revised.context.disk[index]
    else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(saved, disk));
    assert!(revised.persistence.usage().work_nodes > spent.work_nodes);
    assert!(revised.persistence.usage().work_bytes > spent.work_bytes);
    assert_eq!(fixture.disk_state(), original_disk);
    assert_eq!(fixture.workspace.document_is_dirty(inactive), Some(true));
}

#[test]
fn review_refuses_revision_until_its_actual_outstanding_checkpoint_returns() {
    let fixture = Fixture::new();
    let original_disk = fixture.disk_state();
    let inactive = fixture.document(INACTIVE);
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let mut review = prepared.prepare_review(vec![inactive]).unwrap();
    let (checkpoint, _) = review.persistence.take_checkpoint(inactive).unwrap();
    let source_table = review
        .persistence
        .sources()
        .prepared()
        .corpus()
        .sources()
        .as_ptr();
    let q = review.saved.extracted().document() as *const PrefabDocument;

    let refused = review.revise(vec![]).unwrap_err();
    let mut retained = refused.into_review().unwrap();
    retained.require_saved_generation().unwrap();
    assert_eq!(
        retained
            .persistence
            .sources()
            .prepared()
            .corpus()
            .sources()
            .as_ptr(),
        source_table
    );
    retained.persistence.return_checkpoint(checkpoint).unwrap();
    let mut revised = retained.revise(vec![]).unwrap();
    revised.require_saved_generation().unwrap();
    assert_eq!(revised.saved.extracted().document() as *const _, q);
    assert_eq!(
        revised
            .persistence
            .sources()
            .prepared()
            .corpus()
            .sources()
            .as_ptr(),
        source_table
    );
    assert_eq!(fixture.disk_state(), original_disk);
    assert_eq!(fixture.workspace.document_is_dirty(inactive), Some(true));
}

#[test]
fn refused_revision_preserves_the_complete_review_after_actual_file_publication() {
    use crate::project_host::file_publication::{FilePublicationBinding, FilePublicationOutcome};
    use nara_tooling::{EditorPersistenceCommit, EditorPrefabExtractionDisposition};

    let mut fixture = Fixture::new();
    let inactive = fixture.document(INACTIVE);
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let mut review = prepared.prepare_review(vec![inactive]).unwrap();
    let source_table = review
        .persistence
        .sources()
        .prepared()
        .corpus()
        .sources()
        .as_ptr();
    let saved_sources = review.saved.sources().as_ptr();
    let publication_mask = review.saved.published().as_ptr();
    let q = review.saved.extracted().document() as *const PrefabDocument;
    let encoded = review
        .persistence
        .encode_checkpoint(inactive, SceneFileEncoding::Json)
        .unwrap();
    let expected_bytes = encoded.bytes().to_vec();
    let (_, _, publisher) = fixture
        .open
        .iter_mut()
        .find(|(document, _, _)| *document == inactive)
        .unwrap();
    let FilePublicationBinding::Existing { identity, .. } = publisher.binding() else {
        panic!("the captured Scene must still have its actual existing binding");
    };
    let FilePublicationOutcome::Published(accepted) = publisher.publish(encoded.bytes()) else {
        panic!("the real fixed Scene output must publish successfully");
    };
    let (publication, _source) = accepted.into_parts();
    let (receipt, _directory_sync) = publication.into_parts();
    let commit = EditorPersistenceCommit::from_publication(encoded, identity, receipt).unwrap();
    review.persistence.accept_commit(commit).unwrap();
    let spent = review.persistence.usage();
    assert_eq!(
        fs::read(fixture.root.join(INACTIVE)).unwrap(),
        expected_bytes
    );

    let refused = review.revise(vec![]).unwrap_err();
    let refused = refused.retry(vec![inactive]).unwrap_err();
    let mut retained = refused.into_review().unwrap();
    assert_eq!(retained.persistence.usage(), spent);
    assert_eq!(retained.saved.sources().as_ptr(), saved_sources);
    assert_eq!(retained.saved.published().as_ptr(), publication_mask);
    assert_eq!(retained.saved.extracted().document() as *const _, q);
    assert_eq!(
        retained
            .persistence
            .sources()
            .prepared()
            .corpus()
            .sources()
            .as_ptr(),
        source_table
    );
    retained.require_saved_generation().unwrap();
    assert_eq!(
        retained.persistence.disposition(inactive),
        Some(EditorPrefabExtractionDisposition::SaveCurrent)
    );
    assert!(retained.persistence.take_checkpoint(inactive).is_err());
    assert!(
        retained
            .persistence
            .encode_checkpoint(inactive, SceneFileEncoding::Json)
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.root.join(INACTIVE)).unwrap(),
        expected_bytes
    );
    // Leaf evidence is retained without prematurely acknowledging this partial Project operation.
    assert_eq!(fixture.workspace.document_is_dirty(inactive), Some(true));
}

#[test]
fn corrected_review_selection_reuses_prepared_sources_and_spent_work() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    let prepared = fixture.request().capture().unwrap().prepare().unwrap();
    let failed = prepared
        .prepare_review(vec![fixture.document(P)])
        .unwrap_err();
    let before = match &failed {
        review::ProjectPrefabExtractionReviewFailure::Persistence { failure, .. } => {
            failure.report()
        }
        _ => panic!("{failed:?}"),
    };
    assert!(before.diagnostics.has_errors());
    let mut fixed = failed.retry(vec![fixture.document(INACTIVE)]).unwrap();
    fixed.require_saved_generation().unwrap();
    fixed.revalidate_project(&fixture.project).unwrap();
    assert_eq!(fixture.disk_state(), disk);
    for (_, source, _) in &fixture.open {
        assert_eq!(
            fixture
                .workspace
                .document_is_dirty(fixture.document(source.relative_path())),
            Some(true)
        );
    }
}

#[cfg(windows)]
fn host_directory(path: &Path) -> io::Result<fs::File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
}

#[cfg(target_os = "linux")]
fn host_directory(path: &Path) -> io::Result<fs::File> {
    fs::File::open(path)
}

#[path = "outputs/tests.rs"]
mod output_tests;

#[path = "recovered_inventory/tests.rs"]
mod recovered_inventory_tests;

#[path = "recovered_generation/tests.rs"]
mod recovered_generation_tests;

#[path = "publication/tests.rs"]
mod publication_tests;

#[path = "session_publication/tests.rs"]
mod session_publication_tests;

#[path = "resume/tests.rs"]
mod resume_tests;
#[path = "session_review/tests.rs"]
mod session_review_tests;

#[test]
fn session_review_recovery_encoding_keeps_exact_work_refusal_distinct_from_invalid_sources() {
    let prepare = |fixture: &Fixture, maximum| {
        let request = fixture.request_with_host_limits(
            SceneCompilationLimits::default(),
            ItemLimit::new(128).unwrap(),
            ItemLimit::new(maximum).unwrap(),
        );
        PrefabExtractionReviewRequest::new(request, vec![fixture.document(INACTIVE)])
            .unwrap()
            .prepare()
    };
    let probe = Fixture::new();
    let review = prepare(&probe, 200_000_000).unwrap();
    let required = review.host_usage().decode_work();
    for maximum in [required, required - 1] {
        let fixture = Fixture::new();
        let disk = fixture.disk_state();
        match prepare(&fixture, maximum) {
            Ok(review) => {
                assert_eq!(maximum, required);
                assert_eq!(review.host_usage().decode_work(), required);
            }
            Err(error) => {
                assert_eq!(maximum, required - 1);
                assert!(error.error().host_work_limit_exceeded(), "{error:?}");
                assert!(!error.error().open_handles_limit_exceeded());
                assert!(
                    error.to_string().ends_with("recovery data preparation"),
                    "{error:?}"
                );
            }
        }
        assert_eq!(fixture.disk_state(), disk);
    }
}
