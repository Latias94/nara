use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use nara_core::ItemLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_scene::SceneEntityId;
use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspaceCommand};

use super::*;
use crate::project_host::{
    authoring::AuthoringProjectAdmissionHost, file_publication::FilePublicationHost,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const UNIVERSE: &str = "214862a4-063d-4210-9101-360bf1be61b5";
const PATHS: [&str; 4] = [
    "scenes/active.scene.json",
    "scenes/inactive.scene.json",
    "prefabs/enemy.prefab.ron",
    "other/scenes/closed.scene.json",
];

struct Fixture {
    root: PathBuf,
    project: AuthoringProjectSessionSnapshot,
    workspace: EditorWorkspace,
    open: Vec<(
        EditorDocumentId,
        AuthoringDocumentSourceFact,
        FilePublicationHost,
    )>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_upgrade_baseline_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["assets", "scenes", "prefabs", "other/scenes"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            format!(
                "schema_version = 1\n[project]\nname = \"Baseline capture\"\nsource_universe = \"{UNIVERSE}\"\nnative_document_identity = \"project_location\"\n[authoring]\nstate = \"ready\"\nschema_bundle = \"schema.json\"\n[startup]\ndefault_scene = \"active.scene.json\"\n[profiles.extra.paths]\nscenes = \"other/scenes\"\n[profiles.extra.startup]\ndefault_scene = \"closed.scene.json\"\n"
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
        for path in PATHS {
            let entity = SceneEntityRecord::new(SceneEntityId::new("disk").unwrap());
            let encoded = if path.ends_with(".ron") {
                PrefabDocument::from_ordered_roots([entity])
                    .to_ron_string()
                    .unwrap()
            } else {
                SceneDocument::from_ordered_roots([entity])
                    .to_json_string()
                    .unwrap()
            };
            fs::write(root.join(path), encoded).unwrap();
        }
        let project = admit_project(&root);
        let basis = project.startup_scene.schema_basis();
        let mut workspace = EditorWorkspace::new();
        let mut open = Vec::new();
        for path in &PATHS[..3] {
            let root = project.project_root();
            let file = root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            let kind = if path.ends_with(".ron") {
                SourceDocumentKind::Prefab
            } else {
                SourceDocumentKind::Scene
            };
            let native = NativeDocumentSourcePolicy::new(project.settings())
                .admit(&root, path, kind, file.identity())
                .unwrap();
            let bytes = file.read_to_end_bounded(65_536).unwrap();
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
                FilePublicationHost::bind_existing(&root, path, &file, digest, 65_536).unwrap();
            let locator = document::document_source(&ProjectPath::new(*path).unwrap()).unwrap();
            let binding = EditorDocumentSourceBinding::new(*path);
            let report = if kind == SourceDocumentKind::Scene {
                workspace
                    .open_authoring_scene_document(
                        *path,
                        binding,
                        SceneDocumentCandidate::decode_json_bytes(&bytes)
                            .unwrap()
                            .admit_authoring_with_basis(locator, &basis)
                            .unwrap()
                            .publish(),
                    )
                    .unwrap()
            } else {
                workspace
                    .open_authoring_prefab_document(
                        *path,
                        binding,
                        PrefabDocumentCandidate::decode_ron_bytes(&bytes)
                            .unwrap()
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
                SceneEntityRecord::new(SceneEntityId::new("working").unwrap()),
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
            open.push((document, source, host));
        }
        assert!(workspace.activate_document(open[0].0).applied);
        Self {
            root,
            project,
            workspace,
            open,
        }
    }

    fn capture_parts(
        &self,
    ) -> (
        EditorWorkspaceAuthoredSchemaUpgradeInput,
        Vec<SourceUpgradeOpenInput>,
        AuthoringSourceCaptureInputs,
        AuthoredSchemaUpgradeWork,
    ) {
        let mut work = AuthoredSchemaUpgradeWork::default();
        let input = self
            .workspace
            .capture_authored_schema_upgrade_input(
                &self.project.startup_scene.schema_basis(),
                &mut work,
            )
            .unwrap();
        let worker_bytes = self
            .open
            .iter()
            .try_fold(
                size_of::<SourceUpgradeCaptureRequest>(),
                |bytes, (_, source, _)| {
                    checked_sum([
                        bytes,
                        SourceUpgradeOpenInput::capture_retained_bytes(Some(source))?,
                    ])
                },
            )
            .unwrap();
        let host_bytes = self
            .open
            .iter()
            .try_fold(0, |bytes, (_, source, _)| {
                checked_sum([
                    bytes,
                    size_of::<EditorDocumentId>(),
                    source.checked_retained_bytes().unwrap(),
                ])
            })
            .unwrap();
        let mut inputs = self
            .project
            .admit_source_capture_inputs(host_bytes, worker_bytes)
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
                SourceUpgradeOpenInput {
                    document: *document,
                    source: Some(source.clone()),
                    observation: Some(observation),
                }
            })
            .collect();
        (input, open, inputs, work)
    }

    fn request(&self) -> SourceUpgradeCaptureRequest {
        let (input, open, inputs, work) = self.capture_parts();
        self.project
            .capture_authored_upgrade_request(input, open, inputs, work)
            .unwrap()
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
                .starts_with("nara_upgrade_baseline_")
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
            .with_components(ItemLimit::new(64).unwrap()),
    )
    .admit_startup_closure()
    .unwrap()
    .publish()
    .into_session_snapshot()
    .unwrap()
}

#[test]
fn finite_worker_keeps_frozen_working_sources_and_the_complete_all_profile_inventory() {
    let mut fixture = Fixture::new();
    let disk = fixture.disk_state();
    let inactive = fixture.open[1].0;
    let original = fixture
        .workspace
        .scene(inactive)
        .unwrap()
        .session()
        .shared_document();
    let (input, mut open, inputs, work) = fixture.capture_parts();
    let inherited = inputs.budget.usage();
    let work_usage = work.usage();
    // Host order is irrelevant; every original ID must still occur exactly once.
    open.reverse();
    let request = fixture
        .project
        .capture_authored_upgrade_request(input, open, inputs, work)
        .unwrap();
    let (release, ready) = mpsc::channel();
    let worker = thread::spawn(move || {
        ready.recv().unwrap();
        request.capture()
    });
    let revision = fixture.workspace.document_revision(inactive).unwrap();
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::Undo {
                document: inactive,
                base_revision: revision,
            })
            .applied
    );
    release.send(()).unwrap();
    let baseline = worker.join().unwrap().unwrap();
    assert!(baseline.matches(&fixture.project));
    assert_eq!(baseline.source_count(), 4);
    assert_eq!(baseline.open_count(), 3);
    assert_eq!(baseline.work_usage(), work_usage);
    assert!(std::ptr::eq(
        baseline.bundle(),
        fixture.project.authoring_bundle()
    ));
    assert_eq!(
        baseline.inventory().scene_limits(),
        fixture.project.scene_limits
    );
    assert_eq!(baseline.scene_limits, fixture.project.scene_limits);
    assert_eq!(
        baseline.usage().retained_open_handles(),
        inherited.retained_open_handles() + baseline.inventory().retained_handle_count()
    );
    assert!(baseline.usage().decode_work() > inherited.decode_work());
    assert!(baseline.usage().retained_candidate_bytes() > inherited.retained_candidate_bytes());

    let mut paths = Vec::new();
    assert_eq!(baseline.sources().len(), baseline.source_count());
    for source in baseline.sources() {
        paths.push(source.path);
        assert_eq!(
            source.disk,
            fs::read(fixture.root.join(source.path)).unwrap()
        );
        assert_eq!(
            source.key,
            &SourceDocumentKey::project_location(
                UNIVERSE.parse().unwrap(),
                nara_asset::AssetSourcePath::new(source.path).unwrap(),
                if source.path.ends_with(".ron") {
                    SourceDocumentKind::Prefab
                } else {
                    SourceDocumentKind::Scene
                },
            )
        );
        if source.path == PATHS[3] {
            assert!(source.document.is_none() && source.scene.is_none() && source.prefab.is_none());
        } else if source.path.ends_with(".ron") {
            assert_eq!(source.encoding, SceneFileEncoding::Ron);
            assert!(source.document.is_some() && source.scene.is_none());
            assert_eq!(source.prefab.unwrap().entities.len(), 2);
            assert_eq!(
                PrefabDocumentCandidate::decode_ron_bytes(source.disk)
                    .unwrap()
                    .into_composition_source()
                    .unwrap()
                    .entities
                    .len(),
                1
            );
        } else {
            assert_eq!(source.encoding, SceneFileEncoding::Json);
            assert!(source.document.is_some() && source.prefab.is_none());
            assert_eq!(source.scene.unwrap().entities.len(), 2);
            if source.document == Some(inactive) {
                assert!(std::ptr::eq(source.scene.unwrap(), original.as_ref()));
                assert_eq!(baseline.input().document_revision(inactive), Some(revision));
                assert_ne!(
                    baseline.input().document_revision(inactive),
                    fixture.workspace.document_revision(inactive)
                );
            }
        }
    }
    let mut expected = PATHS.to_vec();
    expected.sort_unstable();
    assert_eq!(paths, expected);
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn external_leaf_replacement_refuses_even_if_copied_source_facts_are_updated() {
    for update_copied_fact in [false, true] {
        let fixture = Fixture::new();
        let mut request = fixture.request();
        let path = fixture.root.join(PATHS[1]);
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, fixture.root.join("retired.original")).unwrap();
        fs::write(&path, bytes).unwrap();
        let current = fixture
            .project
            .project_root()
            .open_file_exact(&RelativePath::new(PATHS[1]).unwrap())
            .unwrap();
        let selected = request
            .open
            .iter_mut()
            .find(|selected| {
                selected
                    .source
                    .as_ref()
                    .is_some_and(|source| source.relative_path() == PATHS[1])
            })
            .unwrap();
        assert!(
            !selected
                .observation
                .as_ref()
                .unwrap()
                .matches(&current)
                .unwrap()
        );
        if update_copied_fact {
            // Exercise the worker's live-proof check after the inventory's old-fact guard.
            // An updated copied tuple cannot relabel the actual original Host capability.
            selected.source = Some(
                selected
                    .source
                    .as_ref()
                    .unwrap()
                    .with_persistence_observation(
                        current.identity(),
                        current.parent_identity(),
                        ContentDigest::of_bytes(&fs::read(&path).unwrap()),
                    )
                    .unwrap(),
            );
        }
        let disk = fixture.disk_state();
        assert!(matches!(
            request.capture(),
            Err(SourceUpgradePreparationError::Inventory(
                SourceUpgradeInventoryError::OpenSourceChanged
            ))
        ));
        assert_eq!(fixture.disk_state(), disk);
    }
}

#[test]
fn omitted_unbound_unobservable_and_repeated_open_slots_refuse_explicitly() {
    let fixture = Fixture::new();
    let disk = fixture.disk_state();
    for case in 0..4 {
        let (input, mut open, inputs, work) = fixture.capture_parts();
        match case {
            0 => {
                open.pop();
            }
            1 => open[1].source = None,
            2 => open[1].observation = None,
            3 => open[1].document = open[0].document,
            _ => unreachable!(),
        }
        assert!(matches!(
            fixture
                .project
                .capture_authored_upgrade_request(input, open, inputs, work),
            Err(SourceUpgradePreparationError::OpenSet)
        ));
    }
    assert_eq!(fixture.disk_state(), disk);
}

#[test]
fn actual_schema_authority_and_nominal_basis_cannot_be_replaced_by_equal_bytes() {
    let fixture = Fixture::new();
    let baseline = fixture.request().capture().unwrap();
    let mut other = admit_project(&fixture.root);
    let other_startup = std::mem::replace(
        &mut other.startup_scene,
        Arc::clone(&fixture.project.startup_scene),
    );
    assert!(!baseline.matches(&other));
    other.schema = Arc::clone(&fixture.project.schema);
    assert!(baseline.matches(&other));
    other.startup_scene = other_startup;
    assert!(!baseline.matches(&other));
    let (input, open, inputs, work) = fixture.capture_parts();
    assert!(matches!(
        other.capture_authored_upgrade_request(input, open, inputs, work),
        Err(SourceUpgradePreparationError::OpenSet)
    ));
}

#[test]
fn a_new_upgrade_counts_work_from_acceptance_while_retaining_the_original_input_facts() {
    let fixture = Fixture::new();
    let ordinary = fixture.project.admit_source_capture_inputs(0, 0).unwrap();
    assert!(ordinary.budget.usage().decode_work() > 0);
    let upgrade = fixture
        .project
        .admit_authored_upgrade_inputs(
            0,
            0,
            ItemLimit::new(1).unwrap(),
            ItemLimit::new(64).unwrap(),
        )
        .unwrap();
    assert_eq!(upgrade.budget.usage().decode_work(), 0);
    assert_eq!(
        upgrade.budget.usage().aggregate_encoded_bytes(),
        ordinary.budget.usage().aggregate_encoded_bytes()
    );
    assert_eq!(
        upgrade.budget.usage().retained_candidate_bytes(),
        ordinary.budget.usage().retained_candidate_bytes()
    );
    assert_eq!(
        upgrade.budget.usage().retained_open_handles(),
        ordinary.budget.usage().retained_open_handles()
    );
    assert!(upgrade.budget.ensure_within_limits().is_ok());
}

#[test]
fn request_metadata_continues_the_preadmitted_budget_instead_of_obtaining_a_new_allowance() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let admitted = request.budget.usage().retained_candidate_bytes();
    drop(request);
    let (input, open, mut inputs, work) = fixture.capture_parts();
    inputs.budget.limits = inputs
        .budget
        .limits
        .with_retained_candidate_bytes(ByteLimit::new(admitted - 1).unwrap());
    assert!(matches!(
        fixture
            .project
            .capture_authored_upgrade_request(input, open, inputs, work),
        Err(SourceUpgradePreparationError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
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
