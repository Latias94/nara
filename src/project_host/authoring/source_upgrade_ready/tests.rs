use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetSourcePath;
use nara_project::{ProjectAuthoringRecovery, ProjectPath};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{SceneDocument, SourceDocumentKey, SourceDocumentKind};

use super::*;
use crate::{
    project_host::authoring::{
        AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
        source_upgrade_recovery::{
            RecoverySourceEncoding, SourceUpgradeRecoveryInput, SourceUpgradeRecoveryLimits,
            SourceUpgradeRecoveryMemberInput, SourceUpgradeRecoveryRootInput,
        },
    },
    test_host_fs as host_fs,
};

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);
const BUNDLE: &str = "schema/bundle.json";
const RECOVERY: &str = "recovery.json";
const COMPONENT: &str = "trial.enemy.EnemyStats";

struct Fixture {
    directory: PathBuf,
    root: DirectoryCapability,
    incumbent: Arc<AuthoringSchemaReceipt>,
    previous_basis: AuthoringSchemaBasis,
    recovery: SourceUpgradeRecovery,
    initial_budget: AuthoringProjectAdmissionBudget,
}

impl Fixture {
    fn new() -> Self {
        Self::with_bundle_padding(0)
    }

    fn with_bundle_padding(padding: usize) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nara-upgrade-ready-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::create_dir(directory.join("schema")).unwrap();
        fs::create_dir(directory.join("scenes")).unwrap();
        let scene_bytes = SceneDocument::default()
            .to_json_string()
            .unwrap()
            .into_bytes();
        fs::write(directory.join("scenes/empty.scene.json"), &scene_bytes).unwrap();
        let scene_key = SourceDocumentKey::project_location(
            "421112bd-693a-4e6b-85bc-318d567b2f21".parse().unwrap(),
            AssetSourcePath::new("scenes/empty.scene.json").unwrap(),
            SourceDocumentKind::Scene,
        );
        let original_manifest = format!("# Original observed bytes.\nschema_version = 1\n[project]\nname = 'Ready publication test'\nsource_universe = '421112bd-693a-4e6b-85bc-318d567b2f21'\nnative_document_identity = 'project_location'\n[authoring]\nstate = 'ready'\nschema_bundle = '{BUNDLE}'\n").into_bytes();
        let future_manifest = ProjectManifest::parse_toml_bytes(&original_manifest)
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        let old_catalog = ComponentSchemaCatalog {
            components: vec![schema(false)],
            ..Default::default()
        };
        let mut next_catalog = ComponentSchemaCatalog::successor_of(&old_catalog).unwrap();
        next_catalog.components.push(schema(true));
        let old_bundle = bundle(old_catalog.clone(), None);
        let mut next_bundle = bundle(next_catalog, Some(old_catalog));
        next_bundle.resize(next_bundle.len() + padding, b' ');
        fs::write(directory.join(PROJECT_MANIFEST_PATH), &original_manifest).unwrap();
        fs::write(directory.join(BUNDLE), &old_bundle).unwrap();
        let root = host_fs::writable_directory(&directory).unwrap();
        let incumbent = Arc::new(
            AuthoringProjectAdmissionHost::default()
                .prepare(root.share(), None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish(),
        );
        let adapters = project_authoring_adapters(incumbent.bundle()).unwrap();
        let previous_basis = AuthoringSchemaBasis::admit(incumbent.bundle(), &adapters).unwrap();
        let mut initial_budget =
            AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
        let recovery = SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                selected_profile: None,
                old_ready_manifest: &original_manifest,
                future_ready_manifest: &future_manifest,
                bundle_path: BUNDLE,
                old_bundle: &old_bundle,
                successor_bundle: &next_bundle,
                component: &ComponentTypeId::new(COMPONENT),
                field: &ComponentFieldId::new("health"),
                successor_property: &ComponentPropertyPath::new(ComponentFieldId::new("health"))
                    .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                        "milli",
                    ))]),
                // One actual unchanged source keeps the package meaningful. Complete-set
                // transformation is separately qualified by the enclosing operation.
                members: &[SourceUpgradeRecoveryMemberInput {
                    path: "scenes/empty.scene.json",
                    key: &scene_key,
                    encoding: RecoverySourceEncoding::Json,
                    explicit_native: false,
                    old_disk: &scene_bytes,
                    reviewed_working: &scene_bytes,
                    successor: &scene_bytes,
                }],
                metadata: &[],
                roots: &[
                    SourceUpgradeRecoveryRootInput {
                        path: "assets",
                        missing_ancestor: Some(""),
                    },
                    SourceUpgradeRecoveryRootInput {
                        path: "prefabs",
                        missing_ancestor: Some(""),
                    },
                    SourceUpgradeRecoveryRootInput {
                        path: "scenes",
                        missing_ancestor: None,
                    },
                ],
                missing_sidecars: &["scenes/empty.scene.json.meta"],
            },
            SourceUpgradeRecoveryLimits::default(),
            &mut initial_budget,
        )
        .unwrap();
        Self {
            directory,
            root,
            incumbent,
            previous_basis,
            recovery,
            initial_budget,
        }
    }

    fn selected(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> PreparedSourceUpgradeSchema {
        PreparedSourceUpgradeSchema::prepare(
            &self.incumbent,
            &self.previous_basis,
            &self.recovery,
            budget,
        )
        .unwrap()
    }

    fn publish_file(&self, path: &str, bytes: &[u8]) {
        let file = self
            .root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let digest = file
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap();
        let mut publisher = FilePublicationHost::bind_existing(
            &self.root,
            path,
            &file,
            digest,
            nara_project::DEFAULT_MANIFEST_BYTE_LIMIT,
        )
        .unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (_, source) = accepted.into_parts();
                publisher.commit_published(source);
            }
            FilePublicationOutcome::Rejected(reason) => {
                panic!("fixture publication rejected: {reason:?}")
            }
            FilePublicationOutcome::Failed(stage) => {
                panic!("fixture publication failed: {stage:?}")
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("fixture publication uncertain: {evidence:?}")
            }
        }
    }

    fn pending_manifest(&self) -> ProjectManifest {
        let mut manifest =
            ProjectManifest::parse_toml_bytes(self.recovery.manifest().successor.bytes())
                .manifest
                .unwrap();
        manifest.authoring.state = ProjectAuthoringState::Pending(
            ProjectAuthoringRecovery::new(
                ProjectPath::new(RECOVERY).unwrap(),
                self.recovery.digest(),
            )
            .unwrap(),
        );
        manifest
    }

    fn enter_pending(&self) {
        fs::write(self.directory.join(RECOVERY), self.recovery.encoded_bytes()).unwrap();
        self.publish_file(
            PROJECT_MANIFEST_PATH,
            &self.pending_manifest().to_canonical_toml_bytes().unwrap(),
        );
        self.publish_file(BUNDLE, self.recovery.bundle().successor.bytes());
    }

    fn prepare_ready(
        &self,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> PreparedSourceUpgradeReady {
        let selected = self.selected(budget);
        self.enter_pending();
        self.prepare_observed(selected, budget).unwrap()
    }

    fn prepare_observed(
        &self,
        selected: PreparedSourceUpgradeSchema,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<PreparedSourceUpgradeReady, SourceUpgradeReadyError> {
        let capability = self
            .root
            .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let digest = capability
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap();
        PreparedSourceUpgradeReady::prepare(
            selected,
            &self.recovery,
            SourceUpgradePendingManifest {
                capability: &capability,
                digest,
            },
            budget,
        )
        .map_err(|failure| failure.error)
    }

    fn assert_pending(&self) {
        let actual = fs::read(self.directory.join(PROJECT_MANIFEST_PATH)).unwrap();
        assert!(matches!(
            ProjectManifest::parse_toml_bytes(&actual)
                .manifest
                .unwrap()
                .authoring
                .state,
            ProjectAuthoringState::Pending(_)
        ));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Remove only fixed leaves beneath this fixture's exclusively created directory.
        for parent in ["schema", "schema-original"] {
            for leaf in ["bundle.json", "previous.json"] {
                let _ = fs::remove_file(self.directory.join(parent).join(leaf));
            }
            let _ = fs::remove_dir(self.directory.join(parent));
        }
        for leaf in [
            PROJECT_MANIFEST_PATH,
            RECOVERY,
            "previous.toml",
            "previous-recovery.json",
        ] {
            let _ = fs::remove_file(self.directory.join(leaf));
        }
        let _ = fs::remove_file(self.directory.join("scenes/empty.scene.json"));
        let _ = fs::remove_dir(self.directory.join("scenes"));
        if let Err(error) = fs::remove_dir(&self.directory) {
            if std::thread::panicking() {
                eprintln!("Ready fixture cleanup refused: {error}");
            } else {
                panic!("Ready fixture cleanup refused: {error}");
            }
        }
    }
}

fn schema(next: bool) -> ComponentSchema {
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

fn bundle(current: ComponentSchemaCatalog, predecessor: Option<ComponentSchemaCatalog>) -> Vec<u8> {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap(),
        current,
        predecessor,
    )])
    .unwrap()
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn published(outcome: SourceUpgradeReadyOutcome) -> PublishedSourceUpgradeReady {
    match outcome {
        SourceUpgradeReadyOutcome::Published(value) => value,
        SourceUpgradeReadyOutcome::Rejected { reason, .. } => panic!("Ready rejected: {reason:?}"),
        SourceUpgradeReadyOutcome::Failed { stage, .. } => panic!("Ready failed: {stage:?}"),
        SourceUpgradeReadyOutcome::Uncertain { publication, .. } => {
            panic!("Ready uncertain: {publication:?}")
        }
    }
}

#[test]
fn one_nominal_basis_survives_pending_binding_and_confirmed_ready_publication() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let original_handles = budget.usage().retained_open_handles();
    let selected = fixture.selected(&mut budget);
    let basis = selected.basis().clone();
    assert_eq!(selected.upgrade().previous_basis(), &fixture.previous_basis);
    assert_ne!(basis, fixture.previous_basis);
    assert_eq!(selected.upgrade().successor_basis(), &basis);
    assert!(Arc::ptr_eq(
        selected.incumbent().unwrap(),
        &fixture.incumbent
    ));
    assert_eq!(selected.operation(), fixture.recovery.digest());
    assert_eq!(
        selected
            .adapters()
            .iter()
            .filter(|adapter| adapter
                .components()
                .any(|component| component.as_str() == COMPONENT))
            .count(),
        1
    );
    fixture.enter_pending();
    let prepared = fixture.prepare_observed(selected, &mut budget).unwrap();
    assert_eq!(prepared.basis(), &basis);
    assert!(prepared.future.manifest.capability.get().is_none());
    assert!(matches!(
        prepared
            .observed_pending_schema()
            .settings()
            .authoring_state,
        ProjectAuthoringState::Pending(_)
    ));
    assert!(matches!(
        prepared.settings().authoring_state,
        ProjectAuthoringState::Ready
    ));
    assert_eq!(
        prepared.observed_pending_schema().bundle(),
        prepared.selected.bundle()
    );
    let pending_identity = prepared.observed_pending_schema().manifest_identity();
    let bound = prepared.bind(&mut budget).unwrap();
    assert_eq!(bound.basis(), &basis);
    let accepted = published(bound.publish(&mut budget).unwrap());
    let actual = fixture
        .root
        .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
        .unwrap();
    assert_eq!(accepted.basis, basis);
    assert_eq!(accepted.schema.manifest_identity(), actual.identity());
    assert_ne!(accepted.schema.manifest_identity(), pending_identity);
    assert_eq!(
        accepted.schema.manifest_parent_identity(),
        Some(fixture.root.identity())
    );
    assert_eq!(
        accepted.schema.manifest_bytes(),
        fixture.recovery.manifest().successor.bytes()
    );
    assert_eq!(
        accepted.schema.manifest_digest(),
        actual
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap()
    );
    assert_eq!(
        accepted.schema.bundle_bytes(),
        fixture.recovery.bundle().successor.bytes()
    );
    assert!(matches!(
        accepted.schema.settings().authoring_state,
        ProjectAuthoringState::Ready
    ));
    assert_eq!(
        accepted.publication.receipt().previous_identity(),
        Some(pending_identity)
    );
    assert_eq!(
        accepted.publication.receipt().candidate_identity(),
        actual.identity()
    );
    assert_eq!(accepted.usage, budget.usage());
    assert_eq!(accepted.schema.limits(), fixture.incumbent.limits());
    assert_eq!(accepted.schema.usage().decode_work(), 0);
    assert_eq!(accepted.schema.usage().retained_open_handles(), 2);
    assert!(
        accepted.schema.usage().retained_candidate_bytes() > accepted.schema.bundle_bytes().len()
    );
    // Only the actual Ready manifest and successor bundle remain from this owner. No
    // observed-Pending receipt escaped this direct publication control.
    assert_eq!(budget.usage().retained_open_handles(), original_handles + 2);
    assert!(accepted.adapters.iter().any(|adapter| {
        adapter
            .components()
            .any(|component| component.as_str() == COMPONENT)
    }));
    // A fresh ordinary admission consumes the real completed disk generation, not the private
    // future slot or the retained Pending view.
    let reopened = AuthoringProjectAdmissionHost::default()
        .prepare(fixture.root.share(), None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    assert_eq!(
        reopened.manifest_identity(),
        accepted.schema.manifest_identity()
    );
    assert_eq!(reopened.bundle(), accepted.schema.bundle());
}

#[test]
fn ready_receipt_keeps_ordinary_capture_policy_outside_the_completed_upgrade_allowance() {
    for work_limit in [100_000_000, 200_000_000] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        budget.limits.decode_work = ItemLimit::new(work_limit).unwrap();
        let accepted = published(
            fixture
                .prepare_ready(&mut budget)
                .bind(&mut budget)
                .unwrap()
                .publish(&mut budget)
                .unwrap(),
        );
        assert_eq!(accepted.usage, budget.usage());
        assert!(accepted.usage.decode_work() > 0);
        assert_eq!(
            accepted.schema.limits().decode_work(),
            fixture.incumbent.limits().decode_work()
        );
        let next = accepted
            .schema
            .budget
            .for_source_operation(ItemLimit::new(1).unwrap(), ItemLimit::new(64).unwrap());
        assert_eq!(next.usage().decode_work(), 0);
        assert_eq!(
            next.usage().retained_candidate_bytes(),
            accepted.schema.usage().retained_candidate_bytes()
        );
        assert_eq!(next.usage().retained_open_handles(), 2);
        assert!(next.ensure_within_limits().is_ok());
    }
}

#[test]
fn default_budget_completes_ready_when_bundle_capacity_reservation_nearly_fills_the_allowance() {
    // Valid JSON whitespace increases the byte-derived maximum node capacity without adding
    // any Schema records. The old permanent capacity charge consumed the default allowance
    // before preparing Pending's manifest and receipt storage for this ordinary small catalog.
    let fixture = Fixture::with_bundle_padding(128 * 1024);
    let mut budget = fixture.initial_budget;
    assert_eq!(budget.limits, AuthoringProjectAdmissionLimits::default());
    let selected = fixture.selected(&mut budget);
    let basis = selected.basis().clone();
    fixture.enter_pending();
    let prepared = fixture.prepare_observed(selected, &mut budget).unwrap();
    let accepted = published(
        prepared
            .bind(&mut budget)
            .unwrap()
            .publish(&mut budget)
            .unwrap(),
    );
    assert_eq!(accepted.basis, basis);
    assert_eq!(
        accepted.schema.manifest_bytes(),
        fixture.recovery.manifest().successor.bytes()
    );
    assert_eq!(
        accepted.schema.bundle_bytes(),
        fixture.recovery.bundle().successor.bytes()
    );
    assert_eq!(accepted.usage, budget.usage());
    assert_eq!(accepted.schema.usage().decode_work(), 0);
    assert_eq!(
        accepted.schema.usage().bundle_encoded_bytes(),
        fixture.recovery.bundle().successor.bytes().len()
    );
    assert_eq!(
        accepted.schema.limits(),
        AuthoringProjectAdmissionLimits::default()
    );
    assert!(
        budget.usage().retained_candidate_bytes() < budget.limits.retained_candidate_bytes().get()
    );
    assert!(budget.usage().decoded_bytes() < budget.limits.aggregate_decoded_bytes().get());
}

#[test]
fn schema_preparation_charges_actual_models_independently_of_unused_text_node_capacity() {
    let fixture = Fixture::new();
    let compact = fixture.recovery.bundle().successor.bytes();
    let mut padded = compact.to_vec();
    padded.resize(padded.len() + 128 * 1024, b' ');
    let mut compact_budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let mut padded_budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let compact_models = prepare_schema_models(compact, &mut compact_budget).unwrap();
    let padded_models = prepare_schema_models(&padded, &mut padded_budget).unwrap();
    assert_eq!(compact_models.bundle, padded_models.bundle);
    assert_eq!(compact_models.adapters.len(), padded_models.adapters.len());
    assert_ne!(compact_models.basis, padded_models.basis);
    assert_eq!(
        compact_budget.usage().decoded_bytes(),
        padded_budget.usage().decoded_bytes()
    );
    assert_eq!(
        compact_budget.usage().retained_candidate_bytes(),
        padded_budget.usage().retained_candidate_bytes()
    );
    assert!(padded_budget.usage().decode_work() > compact_budget.usage().decode_work());
}

#[test]
fn failed_schema_preparation_refunds_temporary_storage_and_preserves_cumulative_work() {
    let mut malformed = b"{".to_vec();
    malformed.resize(128 * 1024, b' ');
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    let previous = budget.usage();
    assert!(matches!(
        prepare_schema_models(&malformed, &mut budget),
        Err(SchemaModelsError::Admission(
            AuthoringProjectAdmissionError::BundleInvalid(_)
        ))
    ));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        previous.retained_candidate_bytes()
    );
    assert_eq!(budget.usage().decoded_bytes(), previous.decoded_bytes());
    assert!(budget.usage().decode_work() > previous.decode_work());
}

#[test]
fn failed_metadata_upgrade_after_model_preparation_releases_storage_before_retry() {
    let fixture = Fixture::new();
    let empty =
        AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap();
    let unrelated = AuthoringSchemaBasis::admit(&empty, &[]).unwrap();
    let mut budget = fixture.initial_budget;
    let before = budget.usage();
    let Err(error) = PreparedSourceUpgradeSchema::prepare(
        &fixture.incumbent,
        &unrelated,
        &fixture.recovery,
        &mut budget,
    ) else {
        panic!("an unrelated old basis cannot authorize the upgrade");
    };
    assert!(matches!(error, SourceUpgradeReadyError::Upgrade(_)));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
    drop(error);

    // The failed attempt prepared the manifest, bundle and a new nominal basis, then dropped
    // them all. The same cumulative controller can admit the actual previous basis next.
    let previous_work = budget.usage().decode_work();
    let selected = fixture.selected(&mut budget);
    assert_eq!(selected.upgrade().previous_basis(), &fixture.previous_basis);
    assert!(Arc::ptr_eq(
        selected.incumbent().unwrap(),
        &fixture.incumbent
    ));
    assert!(budget.usage().retained_candidate_bytes() > before.retained_candidate_bytes());
    assert!(budget.usage().decode_work() > previous_work);
}

#[test]
fn refused_metadata_storage_admission_releases_the_unallocated_reservation() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let before = budget.usage();
    let maximum = budget.limits.retained_candidate_bytes;
    // Exact source checks fit; the following complete manifest reservation cannot fit.
    budget.limits.retained_candidate_bytes = ByteLimit::new(
        before
            .retained_candidate_bytes()
            .checked_add(SOURCE_SCRATCH)
            .unwrap(),
    )
    .unwrap();
    let Err(error) = PreparedSourceUpgradeSchema::prepare(
        &fixture.incumbent,
        &fixture.previous_basis,
        &fixture.recovery,
        &mut budget,
    ) else {
        panic!("the manifest owner must refuse the reduced retained allowance");
    };
    assert!(matches!(
        error,
        SourceUpgradeReadyError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        )
    ));
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
    budget.limits.retained_candidate_bytes = maximum;
    let previous_work = budget.usage().decode_work();
    let selected = fixture.selected(&mut budget);
    assert_eq!(selected.upgrade().previous_basis(), &fixture.previous_basis);
    assert!(budget.usage().decode_work() > previous_work);
}

#[test]
fn refused_ready_storage_returns_the_same_selected_owner_for_successful_retry() {
    for another_operation in [true, false] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let selected = fixture.selected(&mut budget);
        let basis = selected.basis().clone();
        let operation = selected.operation();
        fixture.enter_pending();
        let mut wrong = fixture.pending_manifest();
        if another_operation {
            wrong.authoring.state = ProjectAuthoringState::Pending(
                ProjectAuthoringRecovery::new(
                    ProjectPath::new(RECOVERY).unwrap(),
                    ProjectAuthoringRecoveryDigest::from_parts(1, [7; 32]),
                )
                .unwrap(),
            );
        } else {
            wrong.project.name.push_str(" edited");
        }
        fixture.publish_file(
            PROJECT_MANIFEST_PATH,
            &wrong.to_canonical_toml_bytes().unwrap(),
        );
        let capability = fixture
            .root
            .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let digest = capability
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap();
        let before = budget.usage();
        let Err(failure) = PreparedSourceUpgradeReady::prepare(
            selected,
            &fixture.recovery,
            SourceUpgradePendingManifest {
                capability: &capability,
                digest,
            },
            &mut budget,
        ) else {
            panic!("the competing Pending declaration must refuse preparation");
        };
        if another_operation {
            assert!(matches!(
                failure.error,
                SourceUpgradeReadyError::RecoveryMismatch
            ));
        } else {
            assert!(matches!(
                failure.error,
                SourceUpgradeReadyError::ConfigurationMismatch
            ));
        }
        assert_eq!(failure.selected.basis(), &basis);
        assert_eq!(failure.selected.operation(), operation);
        assert!(Arc::ptr_eq(
            failure.selected.incumbent().unwrap(),
            &fixture.incumbent
        ));
        assert_eq!(
            budget.usage().retained_candidate_bytes(),
            before.retained_candidate_bytes()
        );
        assert!(budget.usage().decode_work() > before.decode_work());
        fixture.assert_pending();

        fixture.publish_file(
            PROJECT_MANIFEST_PATH,
            &fixture
                .pending_manifest()
                .to_canonical_toml_bytes()
                .unwrap(),
        );
        let previous_work = budget.usage().decode_work();
        let prepared = fixture
            .prepare_observed(failure.selected, &mut budget)
            .unwrap();
        assert_eq!(prepared.basis(), &basis);
        assert!(prepared.future.manifest.capability.get().is_none());
        assert!(budget.usage().decode_work() > previous_work);
        let accepted = published(
            prepared
                .bind(&mut budget)
                .unwrap()
                .publish(&mut budget)
                .unwrap(),
        );
        assert_eq!(accepted.basis, basis);
        assert!(matches!(
            accepted.schema.settings().authoring_state,
            ProjectAuthoringState::Ready
        ));
    }
}

#[test]
fn bind_and_publication_admission_refusals_release_only_new_storage_for_retry() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let prepared = fixture.prepare_ready(&mut budget);
    let basis = prepared.basis().clone();
    let before = budget.usage();
    let maximum = budget.limits.retained_candidate_bytes;
    // All validation scratch fits. Refuse precisely the additional publisher Host storage.
    budget.limits.retained_candidate_bytes = ByteLimit::new(
        before
            .retained_candidate_bytes()
            .checked_add(SOURCE_SCRATCH)
            .unwrap()
            .checked_add(size_of::<FilePublicationHost>())
            .unwrap()
            - 1,
    )
    .unwrap();
    let Err(failure) = prepared.bind(&mut budget) else {
        panic!("the new publisher storage must be admitted before binding");
    };
    assert!(matches!(
        failure.error,
        SourceUpgradeReadyError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        )
    ));
    assert_eq!(failure.prepared.basis(), &basis);
    assert!(failure.prepared.future.manifest.capability.get().is_none());
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
    fixture.assert_pending();

    budget.limits.retained_candidate_bytes = maximum;
    let bound = failure.prepared.bind(&mut budget).unwrap();
    let before = budget.usage();
    let bytes = bound.prepared.selected.future_manifest_bytes.len();
    budget.limits.retained_candidate_bytes = ByteLimit::new(
        before
            .retained_candidate_bytes()
            .checked_add(SOURCE_SCRATCH)
            .unwrap()
            .checked_add(bytes)
            .unwrap()
            - 1,
    )
    .unwrap();
    let Err(refusal) = bound.publish(&mut budget) else {
        panic!("publication scratch must be admitted before writing Ready");
    };
    assert!(matches!(
        refusal.error,
        SourceUpgradeReadyError::Admission(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        )
    ));
    assert_eq!(refusal.ready.basis(), &basis);
    assert!(
        refusal
            .ready
            .prepared
            .future
            .manifest
            .capability
            .get()
            .is_none()
    );
    assert_eq!(
        budget.usage().retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert!(budget.usage().decode_work() > before.decode_work());
    fixture.assert_pending();

    budget.limits.retained_candidate_bytes = maximum;
    let accepted = published(refusal.ready.publish(&mut budget).unwrap());
    assert_eq!(accepted.basis, basis);
    assert!(matches!(
        accepted.schema.settings().authoring_state,
        ProjectAuthoringState::Ready
    ));
}

#[test]
fn ordinary_admission_rejects_pending_before_even_opening_the_missing_bundle() {
    let fixture = Fixture::new();
    fixture.enter_pending();
    fs::remove_file(fixture.directory.join(BUNDLE)).unwrap();
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(fixture.root.share(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn ready_preparation_keeps_the_selected_basis_when_the_confirmed_pending_leaf_changed() {
    for replace in [true, false] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let selected = fixture.selected(&mut budget);
        let basis = selected.basis().clone();
        fixture.enter_pending();
        let capability = fixture
            .root
            .open_file_exact(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let bytes = capability
            .read_to_end_bounded(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        if replace {
            fixture.publish_file(PROJECT_MANIFEST_PATH, &bytes);
        } else {
            // An in-place edit keeps the native leaf but invalidates its confirmed digest.
            let mut edited = bytes.clone();
            edited.extend_from_slice(b"\n# External edit\n");
            fs::write(fixture.directory.join(PROJECT_MANIFEST_PATH), edited).unwrap();
        }
        let Err(failure) = PreparedSourceUpgradeReady::prepare(
            selected,
            &fixture.recovery,
            SourceUpgradePendingManifest {
                capability: &capability,
                digest,
            },
            &mut budget,
        ) else {
            panic!(
                "a competing Pending edit cannot be accepted as the original operation's observation"
            );
        };
        assert!(matches!(
            failure.error,
            SourceUpgradeReadyError::SourceChanged(SourceUpgradeReadySource::Manifest)
        ));
        assert_eq!(failure.selected.basis(), &basis);
        assert_eq!(failure.selected.operation(), fixture.recovery.digest());
        fixture.assert_pending();
    }
}

#[test]
fn metadata_preparation_refuses_changed_original_manifest_or_bundle() {
    for (path, kind) in [
        (PROJECT_MANIFEST_PATH, SourceUpgradeReadySource::Manifest),
        (BUNDLE, SourceUpgradeReadySource::Bundle),
    ] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let previous = budget.usage().decode_work();
        fixture.publish_file(path, b"external change");
        assert!(
            matches!(PreparedSourceUpgradeSchema::prepare(&fixture.incumbent, &fixture.previous_basis, &fixture.recovery, &mut budget), Err(SourceUpgradeReadyError::SourceChanged(actual)) if actual == kind)
        );
        assert!(budget.usage().decode_work() > previous);
        assert_eq!(
            fs::read(fixture.directory.join(path)).unwrap(),
            b"external change"
        );
    }
}

#[test]
fn ready_preparation_rejects_another_pending_operation_or_changed_configuration() {
    for another_operation in [true, false] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let selected = fixture.selected(&mut budget);
        fixture.enter_pending();
        let mut pending = fixture.pending_manifest();
        if another_operation {
            pending.authoring.state = ProjectAuthoringState::Pending(
                ProjectAuthoringRecovery::new(
                    ProjectPath::new(RECOVERY).unwrap(),
                    ProjectAuthoringRecoveryDigest::from_parts(1, [7; 32]),
                )
                .unwrap(),
            );
        } else {
            pending.project.name.push_str(" edited");
        }
        fixture.publish_file(
            PROJECT_MANIFEST_PATH,
            &pending.to_canonical_toml_bytes().unwrap(),
        );
        let result = fixture.prepare_observed(selected, &mut budget);
        if another_operation {
            assert!(matches!(
                result,
                Err(SourceUpgradeReadyError::RecoveryMismatch)
            ));
        } else {
            assert!(matches!(
                result,
                Err(SourceUpgradeReadyError::ConfigurationMismatch)
            ));
        }
        fixture.assert_pending();
    }
}

#[test]
fn ready_preparation_requires_the_actual_published_successor_bundle() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let selected = fixture.selected(&mut budget);
    fixture.enter_pending();
    fixture.publish_file(BUNDLE, fixture.recovery.bundle().old_disk.bytes());
    assert!(matches!(
        fixture.prepare_observed(selected, &mut budget),
        Err(SourceUpgradeReadyError::SourceChanged(
            SourceUpgradeReadySource::Bundle
        ))
    ));
    fixture.assert_pending();
}

#[test]
fn ready_binding_checks_equal_leaf_replacement_and_native_parent_protection() {
    for parent in [true, false] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let prepared = fixture.prepare_ready(&mut budget);
        let basis = prepared.basis().clone();
        if parent {
            match fs::rename(
                fixture.directory.join("schema"),
                fixture.directory.join("schema-original"),
            ) {
                Ok(()) => {}
                Err(error)
                    if cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied =>
                {
                    // The retained native namespace lease prevents this external parent move.
                    // Verify its unchanged real binding instead of weakening that protection
                    // to manufacture a replacement this backend does not permit.
                    let bound = prepared.bind(&mut budget).unwrap();
                    assert_eq!(bound.basis(), &basis);
                    fixture.assert_pending();
                    continue;
                }
                Err(error) => panic!("unexpected parent replacement failure: {error}"),
            }
            fs::create_dir(fixture.directory.join("schema")).unwrap();
        } else {
            fs::rename(
                fixture.directory.join(BUNDLE),
                fixture.directory.join("schema/previous.json"),
            )
            .unwrap();
        }
        fs::write(
            fixture.directory.join(BUNDLE),
            fixture.recovery.bundle().successor.bytes(),
        )
        .unwrap();
        let Err(failure) = prepared.bind(&mut budget) else {
            panic!("replacement escaped the exact observed bundle binding");
        };
        assert!(matches!(
            failure.error,
            SourceUpgradeReadyError::SourceChanged(SourceUpgradeReadySource::Bundle)
        ));
        assert_eq!(failure.prepared.basis(), &basis);
        assert!(failure.prepared.future.manifest.capability.get().is_none());
        fixture.assert_pending();
    }
}

#[test]
fn final_publication_revalidates_manifest_bundle_and_recovery() {
    for (path, kind) in [
        (PROJECT_MANIFEST_PATH, SourceUpgradeReadySource::Manifest),
        (BUNDLE, SourceUpgradeReadySource::Bundle),
        (RECOVERY, SourceUpgradeReadySource::Recovery),
    ] {
        let fixture = Fixture::new();
        let mut budget = fixture.initial_budget;
        let bound = fixture
            .prepare_ready(&mut budget)
            .bind(&mut budget)
            .unwrap();
        let original = fs::read(fixture.directory.join(path)).unwrap();
        fixture.publish_file(path, &original);
        let Err(refusal) = bound.publish(&mut budget) else {
            panic!("an external equal-byte replacement escaped final binding");
        };
        assert!(
            matches!(refusal.error, SourceUpgradeReadyError::SourceChanged(actual) if actual == kind)
        );
        assert!(
            refusal
                .ready
                .prepared
                .future
                .manifest
                .capability
                .get()
                .is_none()
        );
        fixture.assert_pending();
    }
}

#[test]
fn failed_file_publication_retains_pending_cleanup_custody_and_the_same_basis_for_retry() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let mut bound = fixture
        .prepare_ready(&mut budget)
        .bind(&mut budget)
        .unwrap();
    let basis = bound.basis().clone();
    let control = bound.publisher.test_fail_after_write();
    let SourceUpgradeReadyOutcome::Failed { mut ready, stage } =
        bound.publish(&mut budget).unwrap()
    else {
        panic!("expected controlled file failure");
    };
    assert_eq!(stage, FilePublicationFailureStage::WriteTemporary);
    assert!(ready.cleanup_pending());
    assert!(ready.prepared.future.manifest.capability.get().is_none());
    assert_eq!(ready.basis(), &basis);
    fixture.assert_pending();
    control.release();
    assert!(ready.retry_cleanup());
    let accepted = published(ready.publish(&mut budget).unwrap());
    assert_eq!(accepted.basis, basis);
    assert_eq!(
        accepted.schema.manifest_bytes(),
        fixture.recovery.manifest().successor.bytes()
    );
}

#[test]
fn uncertain_file_owner_refuses_ready_and_keeps_the_unfilled_receipt_private() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let mut bound = fixture
        .prepare_ready(&mut budget)
        .bind(&mut budget)
        .unwrap();
    bound.publisher.mark_uncertain();
    let SourceUpgradeReadyOutcome::Rejected { ready, reason } = bound.publish(&mut budget).unwrap()
    else {
        panic!("an uncertain publisher must require reconciliation");
    };
    assert_eq!(reason, FilePublicationRejection::RequiresReconcile);
    assert!(ready.uncertain());
    assert!(ready.prepared.future.manifest.capability.get().is_none());
    fixture.assert_pending();
}

#[test]
fn exhausted_shared_work_refuses_before_ready_without_resetting_failed_work() {
    let fixture = Fixture::new();
    let mut budget = fixture.initial_budget;
    let bound = fixture
        .prepare_ready(&mut budget)
        .bind(&mut budget)
        .unwrap();
    let previous = budget.usage().decode_work();
    budget.limits.decode_work = ItemLimit::new(previous).unwrap();
    let Err(refusal) = bound.publish(&mut budget) else {
        panic!("Ready cannot bypass the enclosing work limit");
    };
    assert!(matches!(
        refusal.error,
        SourceUpgradeReadyError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        )
    ));
    assert!(budget.usage().decode_work() > previous);
    assert!(
        refusal
            .ready
            .prepared
            .future
            .manifest
            .capability
            .get()
            .is_none()
    );
    fixture.assert_pending();
}
