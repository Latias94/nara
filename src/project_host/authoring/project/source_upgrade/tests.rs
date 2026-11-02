use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_core::ItemLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoredSchemaUpgradeLimits, AuthoringFieldEdit, SceneAuthoringHistoryUsage,
    SceneComponentRecord, SourceDocumentKey,
};
use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspaceCommand};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost,
    source_upgrade_recovery::{
        SourceUpgradeRecoveryInput, SourceUpgradeRecoveryLimits, SourceUpgradeRecoveryMemberInput,
        SourceUpgradeRecoveryRootInput,
    },
};

#[path = "proposal/tests.rs"]
mod complete_proposal;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const UNIVERSE: &str = "c7d1299e-58e0-4f44-a208-d359081ff2b0";
const COMPONENT: &str = "trial.enemy.EnemyStats";
const PATHS: [&str; 4] = [
    "scenes/active.scene.json",
    "scenes/inactive.scene.json",
    "prefabs/enemy.prefab.json",
    "other/scenes/closed.scene.json",
];

struct Fixture {
    root: PathBuf,
    schema: Arc<AuthoringSchemaReceipt>,
    previous: AuthoringSchemaBasis,
    successor_bundle: Vec<u8>,
    workspace: EditorWorkspace,
    open: Vec<(EditorDocumentId, AuthoringDocumentSourceFact)>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_upgrade_source_proof_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["assets", "scenes", "prefabs", "other/scenes"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let manifest = format!(
            "schema_version = 1\n[project]\nname = \"Source proof\"\nsource_universe = \"{UNIVERSE}\"\nnative_document_identity = \"project_location\"\n[authoring]\nstate = \"ready\"\nschema_bundle = \"schema.json\"\n[startup]\ndefault_scene = \"active.scene.json\"\n[profiles.extra.paths]\nscenes = \"other/scenes\"\n[profiles.extra.startup]\ndefault_scene = \"closed.scene.json\"\n"
        );
        fs::write(root.join("nara.toml"), manifest).unwrap();
        let catalog = ComponentSchemaCatalog {
            components: vec![component(false)],
            ..Default::default()
        };
        let old = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
            catalog.clone(),
            None,
        )])
        .unwrap();
        let mut next = ComponentSchemaCatalog::successor_of(&catalog).unwrap();
        next.components.push(component(true));
        let next = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
            next,
            Some(catalog),
        )])
        .unwrap();
        fs::write(root.join("schema.json"), old.to_json_string().unwrap()).unwrap();
        for (index, path) in PATHS.iter().enumerate() {
            fs::write(root.join(path), encoded(index, false, false)).unwrap();
        }
        let capability = DirectoryCapability::from_host_handle(
            host_directory(&root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
        )
        .unwrap();
        let schema = Arc::new(
            AuthoringProjectAdmissionHost::default()
                .prepare(capability, None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish(),
        );
        let adapters = super::super::super::project_authoring_adapters(schema.bundle()).unwrap();
        let previous = AuthoringSchemaBasis::admit(schema.bundle(), &adapters).unwrap();
        let mut workspace = EditorWorkspace::new();
        let mut open = Vec::new();
        for (index, path) in PATHS[..3].iter().enumerate() {
            let kind = if index == 2 {
                SourceDocumentKind::Prefab
            } else {
                SourceDocumentKind::Scene
            };
            let file = schema
                .project_root
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            let native = NativeDocumentSourcePolicy::new(schema.settings())
                .admit(&schema.project_root, path, kind, file.identity())
                .unwrap();
            let fact = AuthoringDocumentSourceFact::from_captured_source(
                &AuthoringSourceFact {
                    kind: if index == 2 {
                        AuthoringSourceKind::Prefab
                    } else {
                        AuthoringSourceKind::Scene
                    },
                    project_path: ProjectPath::new(*path).unwrap(),
                    identity: file.identity(),
                    parent_identity: file.parent_identity(),
                    digest: ContentDigest::of_bytes(&encoded(index, false, false)),
                    encoded_bytes: encoded(index, false, false).len(),
                    dependency_depth: 0,
                },
                &native,
            )
            .unwrap();
            let source = document::document_source(&ProjectPath::new(*path).unwrap()).unwrap();
            let binding = EditorDocumentSourceBinding::new(*path);
            let report = if index == 2 {
                let published =
                    PrefabDocumentCandidate::decode_json_bytes(&encoded(index, false, false))
                        .unwrap()
                        .admit_authoring_with_basis(source, &previous)
                        .unwrap()
                        .publish();
                workspace
                    .open_authoring_prefab_document(*path, binding, published)
                    .unwrap()
            } else {
                let published =
                    SceneDocumentCandidate::decode_json_bytes(&encoded(index, false, false))
                        .unwrap()
                        .admit_authoring_with_basis(source, &previous)
                        .unwrap()
                        .publish();
                workspace
                    .open_authoring_scene_document(*path, binding, published)
                    .unwrap()
            };
            let id = report.opened_document.unwrap();
            let edited = workspace.apply_authoring_command(
                EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                    document: id,
                    base_revision: workspace.document_revision(id).unwrap(),
                    edit: AuthoringFieldEdit::single(
                        entity(),
                        ComponentTypeId::new(COMPONENT),
                        ComponentSchemaVersion::ONE,
                        ComponentFieldId::new("health"),
                        ComponentValue::I64(109 + index as i64),
                    )
                    .unwrap(),
                },
            );
            assert!(edited.applied, "{:?}", edited.diagnostics);
            open.push((id, fact));
        }
        assert!(workspace.activate_document(open[0].0).applied);
        Self {
            root,
            schema,
            previous,
            successor_bundle: next.to_json_string().unwrap().into_bytes(),
            workspace,
            open,
        }
    }

    fn open_sources(&self) -> Vec<SourceUpgradeOpenDocument<'_>> {
        self.open
            .iter()
            .map(|(document, source)| SourceUpgradeOpenDocument {
                document: *document,
                source: Some(source),
            })
            .collect()
    }

    fn recovery(&self, omit_closed: bool, stale_working: bool) -> SourceUpgradeRecovery {
        self.recovery_with_successors(omit_closed, stale_working, |_| {})
    }

    fn recovery_with_successors(
        &self,
        omit_closed: bool,
        stale_working: bool,
        amend: impl FnOnce(&mut [Vec<u8>]),
    ) -> SourceUpgradeRecovery {
        let end = if omit_closed { 3 } else { 4 };
        let old: Vec<_> = (0..end).map(|index| encoded(index, false, false)).collect();
        let working: Vec<_> = (0..end)
            .map(|index| encoded(index, !stale_working, false))
            .collect();
        let mut next: Vec<_> = (0..end)
            .map(|index| encoded(index, !stale_working, true))
            .collect();
        amend(&mut next);
        let keys: Vec<_> = PATHS[..end]
            .iter()
            .enumerate()
            .map(|(index, path)| {
                SourceDocumentKey::project_location(
                    UNIVERSE.parse().unwrap(),
                    AssetSourcePath::new(*path).unwrap(),
                    if index == 2 {
                        SourceDocumentKind::Prefab
                    } else {
                        SourceDocumentKind::Scene
                    },
                )
            })
            .collect();
        let members: Vec<_> = PATHS[..end]
            .iter()
            .enumerate()
            .map(|(index, path)| SourceUpgradeRecoveryMemberInput {
                path,
                key: &keys[index],
                encoding: RecoverySourceEncoding::Json,
                old_disk: &old[index],
                reviewed_working: &working[index],
                successor: &next[index],
            })
            .collect();
        let missing: Vec<_> = PATHS[..end]
            .iter()
            .map(|path| format!("{path}.meta"))
            .collect();
        let missing_refs: Vec<_> = missing.iter().map(String::as_str).collect();
        let roots: Vec<_> = ["assets", "scenes", "prefabs", "other/scenes"]
            .iter()
            .map(|path| SourceUpgradeRecoveryRootInput {
                path,
                missing_ancestor: None,
            })
            .collect();
        let ready = nara_project::ProjectManifest::parse_toml_bytes(self.schema.manifest_bytes())
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                old_ready_manifest: self.schema.manifest_bytes(),
                future_ready_manifest: &ready,
                bundle_path: "schema.json",
                old_bundle: self.schema.bundle_bytes(),
                successor_bundle: &self.successor_bundle,
                component: &ComponentTypeId::new(COMPONENT),
                field: &ComponentFieldId::new("health"),
                successor_property: &selector(),
                members: &members,
                metadata: &[],
                roots: &roots,
                missing_sidecars: &missing_refs,
            },
            SourceUpgradeRecoveryLimits::default(),
            &mut budget(),
        )
        .unwrap()
    }

    fn capture_inventory(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> SourceUpgradeInventory {
        let inputs = inventory_inputs(&self.open_sources(), budget).unwrap();
        let bytes =
            checked_product(inputs.len(), size_of::<SourceUpgradeOpenSource<'_>>()).unwrap();
        let result = SourceUpgradeInventory::capture(
            &self.schema,
            &inputs,
            SceneFileLimits::default(),
            budget,
        );
        drop(inputs);
        budget.reconcile_retained_reservation(bytes, 0).unwrap();
        result.unwrap()
    }

    fn capture_workspace(
        &self,
        work: &mut AuthoredSchemaUpgradeWork,
    ) -> EditorWorkspaceAuthoredSchemaUpgradeInput {
        self.workspace
            .capture_authored_schema_upgrade_input(&self.previous, work)
            .unwrap()
    }

    fn prepare(
        &self,
        recovery: SourceUpgradeRecovery,
    ) -> Result<PreparedAuthoredSourceUpgrade, SourceUpgradePreparationError> {
        let mut budget = budget();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let input = self.capture_workspace(&mut work);
        let inventory = self.capture_inventory(&mut budget);
        let selected = PreparedSourceUpgradeSchema::prepare(
            &self.schema,
            &self.previous,
            &recovery,
            &mut budget,
        )
        .unwrap();
        PreparedAuthoredSourceUpgrade::prepare(
            &self.schema,
            &selected,
            &recovery.into_shared(&mut budget).unwrap(),
            inventory,
            input,
            &self.open_sources(),
            &mut budget,
            &mut work,
        )
        .map_err(|failure| failure.error)
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
                .starts_with("nara_upgrade_source_proof_")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap()
}
fn entity() -> nara_scene::SceneEntityId {
    nara_scene::SceneEntityId::new("enemy").unwrap()
}
fn selector() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
    ])
}
fn budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(Default::default())
}

fn component(next: bool) -> ComponentSchema {
    let health = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["wire_health"]),
            health,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("armor"),
            "Armor",
            ComponentFieldPath::from_fields(["armor"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn encoded(index: usize, working: bool, next: bool) -> Vec<u8> {
    let health = if working && index < 3 { 109 } else { 105 } + index as i64;
    let health = if next {
        ComponentValue::map([(
            "milli",
            ComponentValue::I64(health.checked_mul(1000).unwrap()),
        )])
    } else {
        ComponentValue::I64(health)
    };
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(entity()).with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion(if next { 2 } else { 1 }),
                ComponentValue::map([("wire_health", health), ("armor", ComponentValue::I64(7))]),
            )
            .with_owner(owner()),
        ),
        SceneEntityRecord::new(nara_scene::SceneEntityId::new("unknown").unwrap()).with_component(
            ComponentTypeId::new("absent.Data"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("retain opaque source data".into()),
            ),
        ),
    ]);
    if index == 2 {
        PrefabDocument::new(document.placements, document.entities)
            .to_json_string()
            .unwrap()
            .into_bytes()
    } else {
        document.to_json_string().unwrap().into_bytes()
    }
}

fn history(workspace: &EditorWorkspace, id: EditorDocumentId) -> SceneAuthoringHistoryUsage {
    workspace
        .scene(id)
        .map(|slot| slot.session().history_usage())
        .or_else(|| {
            workspace
                .prefab(id)
                .map(|slot| slot.session().history_usage())
        })
        .unwrap()
}

#[test]
fn complete_review_prepares_dirty_inactive_and_unopened_sources_without_installing_or_saving() {
    let fixture = Fixture::new();
    let original: Vec<_> = fixture
        .open
        .iter()
        .map(|(id, _)| {
            (
                *id,
                fixture.workspace.document_revision(*id).unwrap(),
                history(&fixture.workspace, *id),
            )
        })
        .collect();
    let prepared = fixture.prepare(fixture.recovery(false, false)).unwrap();
    assert_eq!(prepared.recovery().members().len(), 4);
    assert_eq!(prepared.workspace().documents().len(), 3);
    assert_eq!(prepared.unopened().len(), 1);
    assert_ne!(prepared.workspace().schema_basis(), &fixture.previous);
    assert_eq!(
        prepared
            .workspace()
            .schema_basis()
            .schema(&ComponentTypeId::new(COMPONENT))
            .unwrap()
            .version(),
        ComponentSchemaVersion(2)
    );
    for (id, revision, usage) in original {
        assert_eq!(fixture.workspace.document_revision(id), Some(revision));
        assert_eq!(history(&fixture.workspace, id), usage);
        assert_eq!(fixture.workspace.document_is_dirty(id), Some(true));
    }
    assert_eq!(fixture.workspace.active_document(), Some(fixture.open[0].0));
    for (index, path) in PATHS.iter().enumerate() {
        assert_eq!(
            fs::read(fixture.root.join(path)).unwrap(),
            encoded(index, false, false)
        );
    }
    prepared
        .validate(
            &fixture.schema,
            &fixture.workspace,
            &fixture.open_sources(),
            &mut budget(),
            &mut AuthoredSchemaUpgradeWork::default(),
        )
        .unwrap();
}

#[test]
fn preparation_consumes_the_reviewed_inventory_without_readmitting_changed_disk_inputs() {
    let fixture = Fixture::new();
    let mut budget = budget();
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = fixture.capture_workspace(&mut work);
    let inventory = fixture.capture_inventory(&mut budget);
    let original_file = inventory.shared_file(PATHS[3]).unwrap();
    let original_bytes = inventory
        .participants()
        .find(|source| source.file().path() == PATHS[3])
        .unwrap()
        .bytes()
        .as_ptr();
    let recovery = fixture
        .recovery(false, false)
        .into_shared(&mut budget)
        .unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    fs::write(
        fixture.root.join(PATHS[3]),
        b"an external edit after review capture",
    )
    .unwrap();
    let prepared = PreparedAuthoredSourceUpgrade::prepare(
        &fixture.schema,
        &selected,
        &recovery,
        inventory,
        input,
        &fixture.open_sources(),
        &mut budget,
        &mut work,
    )
    .expect("preparation must use the immutable reviewed corpus, not a second disk capture");
    assert!(Arc::ptr_eq(
        &prepared.inventory().shared_file(PATHS[3]).unwrap(),
        &original_file,
    ));
    assert_eq!(
        prepared
            .inventory()
            .participants()
            .find(|source| source.file().path() == PATHS[3])
            .unwrap()
            .bytes()
            .as_ptr(),
        original_bytes,
    );
    assert!(matches!(
        prepared.validate(
            &fixture.schema,
            &fixture.workspace,
            &fixture.open_sources(),
            &mut budget,
            &mut work,
        ),
        Err(SourceUpgradePreparationError::Inventory(_))
    ));
    assert_eq!(
        fs::read(fixture.root.join(PATHS[3])).unwrap(),
        b"an external edit after review capture",
    );
}

#[test]
fn owned_complete_source_preparation_runs_on_a_worker_and_binds_only_the_original_live_workspace() {
    for edit_after_capture in [false, true] {
        let mut fixture = Fixture::new();
        let mut budget = budget();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let input = fixture.capture_workspace(&mut work);
        let inventory = fixture.capture_inventory(&mut budget);
        let recovery = fixture
            .recovery(false, false)
            .into_shared(&mut budget)
            .unwrap();
        let selected = PreparedSourceUpgradeSchema::prepare(
            &fixture.schema,
            &fixture.previous,
            &recovery,
            &mut budget,
        )
        .unwrap();
        let schema = Arc::clone(&fixture.schema);
        let sources = fixture.open.clone();
        let (release, started) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            started.recv().unwrap();
            let open: Vec<_> = sources
                .iter()
                .map(|(document, source)| SourceUpgradeOpenDocument {
                    document: *document,
                    source: Some(source),
                })
                .collect();
            let prepared = PreparedAuthoredSourceUpgrade::prepare(
                &schema,
                &selected,
                &recovery,
                inventory,
                input,
                &open,
                &mut budget,
                &mut work,
            )
            .unwrap();
            (prepared, budget, work)
        });
        if edit_after_capture {
            let document = fixture.open[1].0;
            assert!(
                fixture
                    .workspace
                    .apply_authoring_command(EditorWorkspaceCommand::Undo {
                        document,
                        base_revision: fixture.workspace.document_revision(document).unwrap(),
                    })
                    .applied
            );
        }
        release.send(()).unwrap();
        let (prepared, mut budget, mut work) = worker.join().unwrap();
        let result = prepared.validate(
            &fixture.schema,
            &fixture.workspace,
            &fixture.open_sources(),
            &mut budget,
            &mut work,
        );
        if edit_after_capture {
            assert!(matches!(
                result,
                Err(SourceUpgradePreparationError::Workspace(_))
            ));
        } else {
            result.expect("the original unchanged Workspace accepts its worker preparation");
        }
        for (index, path) in PATHS.iter().enumerate() {
            assert_eq!(
                fs::read(fixture.root.join(path)).unwrap(),
                encoded(index, false, false)
            );
        }
    }
}

#[test]
fn recovery_subset_cannot_omit_an_unopened_source_in_another_profile() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.prepare(fixture.recovery(true, false)),
        Err(SourceUpgradePreparationError::Inventory(
            SourceUpgradeInventoryError::RecoveryInventory
        ))
    ));
}

#[test]
fn stale_reviewed_dirty_working_copy_refuses_without_clearing_history() {
    let fixture = Fixture::new();
    let before = history(&fixture.workspace, fixture.open[1].0);
    assert!(matches!(
        fixture.prepare(fixture.recovery(false, true)),
        Err(SourceUpgradePreparationError::WorkingCopy)
    ));
    assert_eq!(history(&fixture.workspace, fixture.open[1].0), before);
}

#[test]
fn refused_complete_preparation_returns_the_original_inventory_without_refunding_its_custody() {
    for (subset, stale) in [(true, false), (false, true)] {
        let fixture = Fixture::new();
        let mut budget = budget();
        budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
        let mut inventory = fixture.capture_inventory(&mut budget);
        let original_file = inventory.shared_file(PATHS[0]).unwrap();
        let original_bytes = inventory.participants().next().unwrap().bytes().as_ptr();
        let recovery = fixture
            .recovery(subset, stale)
            .into_shared(&mut budget)
            .unwrap();
        let selected = PreparedSourceUpgradeSchema::prepare(
            &fixture.schema,
            &fixture.previous,
            &recovery,
            &mut budget,
        )
        .unwrap();
        let retained = budget.usage().retained_candidate_bytes();
        let handles = budget.usage().retained_open_handles();
        let recovery_owners = Arc::strong_count(&recovery);
        let recovery_body = recovery.encoded_bytes().as_ptr();
        let mut previous_work = budget.usage().decode_work();
        let mut source_work = AuthoredSchemaUpgradeWork::default();
        let mut input = fixture.capture_workspace(&mut source_work);
        let mut previous_source_work = source_work.usage().work_bytes;
        for _ in 0..2 {
            let result = PreparedAuthoredSourceUpgrade::prepare(
                &fixture.schema,
                &selected,
                &recovery,
                inventory,
                input,
                &fixture.open_sources(),
                &mut budget,
                &mut source_work,
            );
            let failure = result
                .err()
                .expect("the incomplete or stale proposal must refuse");
            assert!(matches!(
                failure.error,
                SourceUpgradePreparationError::Inventory(
                    SourceUpgradeInventoryError::RecoveryInventory
                ) | SourceUpgradePreparationError::WorkingCopy
            ));
            inventory = failure.inventory;
            input = failure.input;
            assert!(Arc::ptr_eq(
                &inventory.shared_file(PATHS[0]).unwrap(),
                &original_file
            ));
            assert_eq!(
                inventory.participants().next().unwrap().bytes().as_ptr(),
                original_bytes
            );
            assert_eq!(budget.usage().retained_candidate_bytes(), retained);
            assert_eq!(budget.usage().retained_open_handles(), handles);
            assert_eq!(Arc::strong_count(&recovery), recovery_owners);
            assert_eq!(recovery.encoded_bytes().as_ptr(), recovery_body);
            assert!(budget.usage().decode_work() > previous_work);
            previous_work = budget.usage().decode_work();
            if stale {
                assert!(source_work.usage().work_bytes > previous_source_work);
            } else {
                assert_eq!(source_work.usage().work_bytes, previous_source_work);
            }
            previous_source_work = source_work.usage().work_bytes;
        }
        for (index, path) in PATHS.iter().enumerate() {
            assert_eq!(
                fs::read(fixture.root.join(path)).unwrap(),
                encoded(index, false, false)
            );
        }
    }
}

#[test]
fn a_corrected_proposal_reuses_returned_inventory_and_the_exact_single_success_source_capacity() {
    let fixture = Fixture::new();
    let mut sample_budget = budget();
    let inventory = fixture.capture_inventory(&mut sample_budget);
    let recovery = fixture
        .recovery(false, false)
        .into_shared(&mut sample_budget)
        .unwrap();
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut sample_budget,
    )
    .unwrap();
    let mut sample_work = AuthoredSchemaUpgradeWork::default();
    let sample_input = fixture.capture_workspace(&mut sample_work);
    let prepared = PreparedAuthoredSourceUpgrade::prepare(
        &fixture.schema,
        &selected,
        &recovery,
        inventory,
        sample_input,
        &fixture.open_sources(),
        &mut sample_budget,
        &mut sample_work,
    )
    .unwrap();
    let single_success = sample_work.usage();
    assert!(single_success.peak_retained_bytes > 0);
    drop((prepared, selected, recovery, sample_work, sample_budget));

    for exact in [false, true] {
        let mut budget = budget();
        let inventory = fixture.capture_inventory(&mut budget);
        let original = inventory.shared_file(PATHS[0]).unwrap();
        let stale = fixture
            .recovery_with_successors(false, false, |next| {
                let mut document = SceneDocumentCandidate::decode_json_bytes(&next[1])
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let record = document.entities[0]
                    .components
                    .get_mut(&ComponentTypeId::new(COMPONENT))
                    .unwrap();
                record.value = ComponentValue::map([
                    (
                        "wire_health",
                        ComponentValue::map([("milli", ComponentValue::I64(110000))]),
                    ),
                    ("armor", ComponentValue::I64(8)),
                ]);
                next[1] = document.to_json_string().unwrap().into_bytes();
            })
            .into_shared(&mut budget)
            .unwrap();
        let stale_schema = PreparedSourceUpgradeSchema::prepare(
            &fixture.schema,
            &fixture.previous,
            &stale,
            &mut budget,
        )
        .unwrap();
        let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
            nara_core::ByteLimit::new(512 * 1024 * 1024).unwrap(),
            nara_core::ByteLimit::new(single_success.peak_retained_bytes - usize::from(!exact))
                .unwrap(),
        ));
        let input = fixture.capture_workspace(&mut work);
        let failure = PreparedAuthoredSourceUpgrade::prepare(
            &fixture.schema,
            &stale_schema,
            &stale,
            inventory,
            input,
            &fixture.open_sources(),
            &mut budget,
            &mut work,
        )
        .err()
        .expect("the out-of-scope armor change must refuse before publication");
        assert!(matches!(
            failure.error,
            SourceUpgradePreparationError::Workspace(_)
        ));
        let after_failure = work.usage();
        assert!(after_failure.work_bytes > 0);
        assert!(Arc::ptr_eq(
            &failure.inventory.shared_file(PATHS[0]).unwrap(),
            &original,
        ));
        let corrected = fixture
            .recovery(false, false)
            .into_shared(&mut budget)
            .unwrap();
        let selected = PreparedSourceUpgradeSchema::prepare(
            &fixture.schema,
            &fixture.previous,
            &corrected,
            &mut budget,
        )
        .unwrap();
        let result = PreparedAuthoredSourceUpgrade::prepare(
            &fixture.schema,
            &selected,
            &corrected,
            failure.inventory,
            failure.input,
            &fixture.open_sources(),
            &mut budget,
            &mut work,
        );
        assert!(work.usage().work_bytes > after_failure.work_bytes);
        if exact {
            let prepared = result.expect("a corrected proposal fits the single-success capacity");
            assert!(Arc::ptr_eq(
                &prepared.inventory().shared_file(PATHS[0]).unwrap(),
                &original,
            ));
            assert_eq!(
                work.usage().peak_retained_bytes,
                single_success.peak_retained_bytes
            );
            assert!(work.usage().work_bytes > single_success.work_bytes);
        } else {
            let failure = result
                .err()
                .expect("one byte below the measured peak must refuse");
            let diagnostics = match failure.error {
                SourceUpgradePreparationError::Source(report) => report.diagnostics,
                SourceUpgradePreparationError::Workspace(report) => report.diagnostics,
                other => panic!("one-short capacity refused for an unrelated reason: {other:?}"),
            };
            assert!(diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.authored-schema-upgrade-limit-exceeded"
            }));
            assert!(Arc::ptr_eq(
                &failure.inventory.shared_file(PATHS[0]).unwrap(),
                &original,
            ));
        }
    }
}

#[test]
fn omitted_duplicate_and_unbound_open_participants_refuse() {
    let fixture = Fixture::new();
    let all = fixture.open_sources();
    for input in [
        all[..2].to_vec(),
        vec![all[0], all[0], all[2]],
        vec![
            all[0],
            SourceUpgradeOpenDocument {
                document: all[1].document,
                source: None,
            },
            all[2],
        ],
    ] {
        let recovery = fixture.recovery(false, false);
        let mut budget = budget();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let frozen = fixture.capture_workspace(&mut work);
        let inventory = fixture.capture_inventory(&mut budget);
        let selected = PreparedSourceUpgradeSchema::prepare(
            &fixture.schema,
            &fixture.previous,
            &recovery,
            &mut budget,
        )
        .unwrap();
        let result = PreparedAuthoredSourceUpgrade::prepare(
            &fixture.schema,
            &selected,
            &recovery.into_shared(&mut budget).unwrap(),
            inventory,
            frozen,
            &input,
            &mut budget,
            &mut work,
        );
        assert!(matches!(
            result,
            Err(SourceUpgradePreparationFailure {
                error: SourceUpgradePreparationError::OpenSet,
                ..
            })
        ));
    }
}

#[test]
fn confirmation_reenumerates_members_and_checks_the_exact_open_revision() {
    let mut fixture = Fixture::new();
    let prepared = fixture.prepare(fixture.recovery(false, false)).unwrap();
    fs::write(
        fixture.root.join("other/scenes/late.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    assert!(matches!(
        prepared.validate(
            &fixture.schema,
            &fixture.workspace,
            &fixture.open_sources(),
            &mut budget(),
            &mut AuthoredSchemaUpgradeWork::default()
        ),
        Err(SourceUpgradePreparationError::Inventory(_))
    ));
    let id = fixture.open[1].0;
    assert!(
        fixture
            .workspace
            .apply_authoring_command(EditorWorkspaceCommand::Undo {
                document: id,
                base_revision: fixture.workspace.document_revision(id).unwrap()
            })
            .applied
    );
    assert!(
        prepared
            .workspace
            .validate(
                &fixture.workspace,
                &mut AuthoredSchemaUpgradeWork::default()
            )
            .is_err()
    );
}

#[test]
fn repeated_confirmation_releases_temporary_sets_and_keeps_cumulative_work() {
    let fixture = Fixture::new();
    let recovery = fixture.recovery(false, false);
    let mut budget = budget();
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let inventory = fixture.capture_inventory(&mut budget);
    let selected = PreparedSourceUpgradeSchema::prepare(
        &fixture.schema,
        &fixture.previous,
        &recovery,
        &mut budget,
    )
    .unwrap();
    let mut work = AuthoredSchemaUpgradeWork::default();
    let input = fixture.capture_workspace(&mut work);
    let prepared = PreparedAuthoredSourceUpgrade::prepare(
        &fixture.schema,
        &selected,
        &recovery.into_shared(&mut budget).unwrap(),
        inventory,
        input,
        &fixture.open_sources(),
        &mut budget,
        &mut work,
    )
    .unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let mut previous_work = budget.usage().decode_work();
    for _ in 0..3 {
        prepared
            .validate(
                &fixture.schema,
                &fixture.workspace,
                &fixture.open_sources(),
                &mut budget,
                &mut work,
            )
            .unwrap();
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert!(budget.usage().decode_work() > previous_work);
        previous_work = budget.usage().decode_work();
    }
    let mut missing = fixture.open_sources();
    missing[1].source = None;
    assert!(
        prepared
            .validate(
                &fixture.schema,
                &fixture.workspace,
                &missing,
                &mut budget,
                &mut work
            )
            .is_err()
    );
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert!(budget.usage().decode_work() > previous_work);
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

mod adoption;
mod publication;
