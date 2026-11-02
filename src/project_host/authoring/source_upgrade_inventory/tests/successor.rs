use std::sync::{Arc, OnceLock};

use nara_project::{ProjectAuthoringRecovery, ProjectPath};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{SceneComponentRecord, SceneEntityId, SceneEntityRecord};

use super::*;
use crate::project_host::{
    authoring::{
        AdmittedAuthoringManifest,
        bundle::AdmittedAuthoringSchemaBundle,
        project_authoring_adapters,
        source_upgrade_ready::{
            PreparedSourceUpgradeReady, PreparedSourceUpgradeSchema, SourceUpgradePendingManifest,
        },
        source_upgrade_recovery::{
            SourceUpgradeRecoveryInput, SourceUpgradeRecoveryLimits,
            SourceUpgradeRecoveryMemberInput, SourceUpgradeRecoveryMetadataInput,
            SourceUpgradeRecoveryRootInput,
        },
    },
    file_publication::{FilePublicationHost, FilePublicationOutcome},
};

const COMPONENT: &str = "trial.enemy.EnemyStats";
const BUNDLE: &str = "schema/authoring.bundle.json";
const RECOVERY: &str = ".nara/upgrade.json";
const OPEN: &str = "scenes/open.scene.json";
const CLOSED: &str = "prefabs/closed.prefab.ron";
const OTHER_PROFILE: &str = "other/scenes/closed.scene.json";
const CATALOGUED: &str = "assets/catalogued.json";

mod disposal;
mod recovered;

struct SuccessorFixture {
    project: Fixture,
    incumbent: Arc<AuthoringSchemaReceipt>,
    previous: AuthoringSchemaBasis,
    original: SourceUpgradeInventory,
    recovery: SourceUpgradeRecovery,
}

impl SuccessorFixture {
    fn new() -> Self {
        Self::with_prefix_metadata(false)
    }

    fn with_prefix_metadata(include_prefix: bool) -> Self {
        Self::with_source_options(include_prefix, false)
    }

    fn with_source_options(include_prefix: bool, unchanged_catalogued: bool) -> Self {
        let project = Fixture::new(
            "project_location",
            "[profiles.other.paths]\nscenes = 'other/scenes'\n\
             [profiles.dormant.paths]\nscenes = 'missing/scenes'\n",
        );
        fs::create_dir(project.root.join(".nara")).unwrap();
        for (path, kind) in [
            (OPEN, SourceDocumentKind::Scene),
            (CLOSED, SourceDocumentKind::Prefab),
            (OTHER_PROFILE, SourceDocumentKind::Scene),
            (CATALOGUED, SourceDocumentKind::Scene),
        ] {
            if unchanged_catalogued && path == CATALOGUED {
                project.scene(path);
            } else {
                project.write(path, &source(path, kind, false));
            }
        }
        project.metadata(
            CATALOGUED,
            "catalogued.json",
            FIRST_ID,
            AssetSourceKind::Scene,
            0,
        );
        project.metadata(
            "assets/unused.png",
            "unused.png",
            SECOND_ID,
            AssetSourceKind::Image,
            0,
        );
        if include_prefix {
            project.metadata(
                "assets/unused.png-extra",
                "unused.png-extra",
                "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
                AssetSourceKind::Image,
                0,
            );
        }
        let old_catalog = ComponentSchemaCatalog {
            components: vec![component(false)],
            ..Default::default()
        };
        let mut next_catalog = ComponentSchemaCatalog::successor_of(&old_catalog).unwrap();
        next_catalog.components.push(component(true));
        let old = bundle(old_catalog.clone(), None);
        let successor_bundle = bundle(next_catalog, Some(old_catalog));
        project.write(BUNDLE, &old);
        // The same real writable capability session is retained from ordinary admission through
        // Pending and final-set proof. No temporary Ready receipt or native registry is used.
        let root = crate::test_host_fs::writable_directory(&project.root).unwrap();
        let incumbent = Arc::new(
            AuthoringProjectAdmissionHost::default()
                .prepare(root, None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish(),
        );
        let adapters = project_authoring_adapters(incumbent.bundle()).unwrap();
        let previous = AuthoringSchemaBasis::admit(incumbent.bundle(), &adapters).unwrap();
        let original = capture(&incumbent);
        let successors: Vec<_> = original
            .participants()
            .map(|participant| {
                if unchanged_catalogued && participant.file().path() == CATALOGUED {
                    participant.bytes().to_vec()
                } else {
                    source(participant.file().path(), participant.kind(), true)
                }
            })
            .collect();
        let members: Vec<_> = original
            .participants()
            .zip(&successors)
            .map(
                |(participant, successor)| SourceUpgradeRecoveryMemberInput {
                    path: participant.file().path(),
                    key: participant.source_key(),
                    encoding: match participant.encoding() {
                        SourceUpgradeEncoding::Json => RecoverySourceEncoding::Json,
                        SourceUpgradeEncoding::Ron => RecoverySourceEncoding::Ron,
                    },
                    old_disk: participant.bytes(),
                    reviewed_working: participant.bytes(),
                    successor,
                },
            )
            .collect();
        let metadata_bytes: Vec<_> = original
            .metadata()
            .map(|metadata| fs::read(project.root.join(metadata.file().path())).unwrap())
            .collect();
        let metadata: Vec<_> = original
            .metadata()
            .zip(&metadata_bytes)
            .map(|(metadata, bytes)| SourceUpgradeRecoveryMetadataInput {
                path: metadata.file().path(),
                observed_bytes: bytes,
            })
            .collect();
        let roots: Vec<_> = original
            .roots()
            .map(|root| SourceUpgradeRecoveryRootInput {
                path: root.path(),
                missing_ancestor: match root.observation() {
                    SourceUpgradeRootObservation::Available { .. } => None,
                    SourceUpgradeRootObservation::Missing { ancestor_path, .. } => {
                        Some(ancestor_path)
                    }
                },
            })
            .collect();
        let missing_sidecars: Vec<_> = original.missing_sidecars().collect();
        let future = ProjectManifest::parse_toml_bytes(incumbent.manifest_bytes())
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        let recovery = SourceUpgradeRecovery::prepare(
            SourceUpgradeRecoveryInput {
                old_ready_manifest: incumbent.manifest_bytes(),
                future_ready_manifest: &future,
                bundle_path: BUNDLE,
                old_bundle: &old,
                successor_bundle: &successor_bundle,
                component: &ComponentTypeId::new(COMPONENT),
                field: &ComponentFieldId::new("health"),
                successor_property: &ComponentPropertyPath::new(ComponentFieldId::new("health"))
                    .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                        "milli",
                    ))]),
                members: &members,
                metadata: &metadata,
                roots: &roots,
                missing_sidecars: &missing_sidecars,
            },
            SourceUpgradeRecoveryLimits::default(),
            &mut operation_budget(),
        )
        .unwrap();
        Self {
            project,
            incumbent,
            previous,
            original,
            recovery,
        }
    }

    fn ready(&self) -> PreparedSourceUpgradeReady {
        let mut budget = operation_budget();
        let selected = PreparedSourceUpgradeSchema::prepare(
            &self.incumbent,
            &self.previous,
            &self.recovery,
            &mut budget,
        )
        .unwrap();
        let basis = selected.basis().clone();
        self.project.write(RECOVERY, self.recovery.encoded_bytes());
        let mut pending =
            ProjectManifest::parse_toml_bytes(self.recovery.manifest().successor.bytes())
                .manifest
                .unwrap();
        pending.authoring.state = ProjectAuthoringState::Pending(
            ProjectAuthoringRecovery::new(
                ProjectPath::new(RECOVERY).unwrap(),
                self.recovery.digest(),
            )
            .unwrap(),
        );
        self.publish("nara.toml", &pending.to_canonical_toml_bytes().unwrap());
        for member in self.recovery.members() {
            self.publish(&member.path, member.successor.bytes());
        }
        self.publish(BUNDLE, self.recovery.bundle().successor.bytes());
        let capability = self
            .incumbent
            .project_root()
            .open_file_exact(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let digest = capability
            .digest(nara_project::DEFAULT_MANIFEST_BYTE_LIMIT)
            .unwrap();
        let ready = PreparedSourceUpgradeReady::prepare(
            selected,
            &self.recovery,
            SourceUpgradePendingManifest {
                capability: &capability,
                digest,
            },
            &mut budget,
        )
        .unwrap_or_else(|failure| panic!("Pending preparation refused: {:?}", failure.error));
        assert_eq!(ready.basis(), &basis);
        assert_ne!(ready.basis(), &self.previous);
        ready.validate(&mut budget).unwrap();
        ready
    }

    fn publish(&self, path: &str, bytes: &[u8]) {
        let root = self.incumbent.project_root();
        let file = root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let maximum = nara_project::DEFAULT_MANIFEST_BYTE_LIMIT;
        let digest = file.digest(maximum).unwrap();
        let mut publisher =
            FilePublicationHost::bind_existing(&root, path, &file, digest, maximum).unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (_, capability) = accepted.into_parts();
                publisher.commit_published(capability);
            }
            FilePublicationOutcome::Rejected(reason) => {
                panic!("fixture publication refused: {reason:?}")
            }
            FilePublicationOutcome::Failed(stage) => {
                panic!("fixture publication failed: {stage:?}")
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("fixture publication uncertain: {evidence:?}")
            }
        }
    }

    fn future_open(
        &self,
        schema: &AuthoringSchemaReceipt,
        basis: &AuthoringSchemaBasis,
    ) -> AuthoringDocumentSourceFact {
        let pools = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
        let mut state = ProjectSourceIndexState::new(schema);
        state.request_refresh().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while state.has_pending_work() {
            state.advance(&pools);
            assert!(
                Instant::now() < deadline,
                "future source index did not finish"
            );
            thread::sleep(Duration::from_millis(1));
        }
        // Admit an actual future file under the already selected nominal basis, without
        // installing it in the incumbent Workspace or granting an ordinary project Open.
        let document =
            AuthoringDocumentAdmissionBasis::new(schema, basis, SceneFileLimits::default())
                .admit(
                    &state.snapshot().unwrap(),
                    OPEN,
                    AuthoringDocumentKind::Scene,
                )
                .unwrap();
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        document.source().clone()
    }

    /// Adversarial observations are still real Pending bytes, files and parsed settings. The
    /// production success tests exclusively use Ready's observed_pending_schema; this helper
    /// lets the inventory's independent configuration/operation/bundle guards face a conflict.
    fn conflicting_pending_observation(&self) -> Arc<AuthoringSchemaReceipt> {
        let root = self.incumbent.project_root();
        let manifest_bytes: Arc<[u8]> =
            Arc::from(fs::read(self.project.root.join("nara.toml")).unwrap());
        let parsed = ProjectManifest::parse_toml_bytes(&manifest_bytes)
            .manifest
            .unwrap();
        assert!(matches!(
            parsed.authoring.state,
            ProjectAuthoringState::Pending(_)
        ));
        let bundle_bytes: Arc<[u8]> = Arc::from(fs::read(self.project.root.join(BUNDLE)).unwrap());
        let bundle = Arc::new(AuthoringSchemaBundle::from_json_bytes(&bundle_bytes).unwrap());
        Arc::new(AuthoringSchemaReceipt {
            attempt: self.incumbent.attempt_generation(),
            project_root: root.share(),
            manifest: Arc::new(AdmittedAuthoringManifest {
                capability: OnceLock::from(
                    root.open_file_exact(&RelativePath::new("nara.toml").unwrap())
                        .unwrap(),
                ),
                digest: ContentDigest::of_bytes(&manifest_bytes),
                bytes: manifest_bytes,
            }),
            settings: parsed.resolve_profile(None).unwrap(),
            bundle: AdmittedAuthoringSchemaBundle::from_upgrade_observation(
                ProjectPath::new(BUNDLE).unwrap(),
                root.open_file_exact(&RelativePath::new(BUNDLE).unwrap())
                    .unwrap(),
                Arc::clone(&bundle_bytes),
                ContentDigest::of_bytes(&bundle_bytes),
                bundle,
            ),
            budget: operation_budget(),
        })
    }
}

fn operation_budget() -> AuthoringProjectAdmissionBudget {
    AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
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

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap()
}

fn bundle(current: ComponentSchemaCatalog, predecessor: Option<ComponentSchemaCatalog>) -> Vec<u8> {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        current,
        predecessor,
    )])
    .unwrap()
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn source(path: &str, kind: SourceDocumentKind, next: bool) -> Vec<u8> {
    let health = if next {
        ComponentValue::map([("milli", ComponentValue::I64(105_000))])
    } else {
        ComponentValue::I64(105)
    };
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("enemy").unwrap(),
    )
    .with_component(
        ComponentTypeId::new(COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion(if next { 2 } else { 1 }),
            ComponentValue::map([("wire_health", health), ("armor", ComponentValue::I64(7))]),
        )
        .with_owner(owner()),
    )]);
    match (kind, path.ends_with(".ron")) {
        (SourceDocumentKind::Scene, false) => document.to_json_string().unwrap().into_bytes(),
        (SourceDocumentKind::Scene, true) => document.to_ron_string().unwrap().into_bytes(),
        (SourceDocumentKind::Prefab, false) => {
            PrefabDocument::new(document.roots, document.entities)
                .to_json_string()
                .unwrap()
                .into_bytes()
        }
        (SourceDocumentKind::Prefab, true) => {
            PrefabDocument::new(document.roots, document.entities)
                .to_ron_string()
                .unwrap()
                .into_bytes()
        }
    }
}

fn verified(
    fixture: &SuccessorFixture,
    schema: &Arc<AuthoringSchemaReceipt>,
    future_open: &[SourceUpgradeOpenSource<'_>],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<VerifiedSourceUpgradeInventory, SourceUpgradeInventoryError> {
    VerifiedSourceUpgradeInventory::capture(
        schema,
        &fixture.recovery,
        future_open,
        SceneFileLimits::default(),
        budget,
    )
}

#[test]
fn actual_pending_successor_proof_preserves_receipt_lineage_and_ordinary_capture_still_refuses() {
    let fixture = SuccessorFixture::new();
    fixture
        .original
        .validate_original_recovery(
            &fixture.incumbent,
            &fixture.recovery,
            &mut operation_budget(),
        )
        .unwrap();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    assert!(matches!(
        schema.settings().authoring_state,
        ProjectAuthoringState::Pending(_)
    ));
    assert!(matches!(
        try_capture(schema),
        Err(SourceUpgradeInventoryError::SourceAuthoringPending)
    ));
    let open = fixture.future_open(schema, ready.basis());
    let future = [SourceUpgradeOpenSource::Bound(&open)];
    let mut budget = operation_budget();
    let proof = verified(&fixture, schema, &future, &mut budget).unwrap();
    assert!(Arc::ptr_eq(&proof.schema, schema));
    assert_eq!(proof.operation, fixture.recovery.digest());
    assert_eq!(proof.participants().len(), 4);
    assert_eq!(proof.inventory().metadata().len(), 2);
    let handles = budget.usage().retained_open_handles();
    assert_eq!(handles, proof.inventory().retained_handle_count());
    for participant in proof.participants() {
        let file = proof
            .inventory()
            .shared_file(participant.file().path())
            .unwrap();
        assert_eq!(file.identity(), participant.file().identity());
        assert_eq!(file.parent_identity(), participant.file().parent_identity());
        assert_eq!(
            proof
                .inventory()
                .shared_parent(participant.file().path())
                .unwrap()
                .identity(),
            participant.file().parent_identity().unwrap()
        );
    }
    proof
        .validate(schema, &fixture.recovery, &future, &mut budget)
        .unwrap();
    assert_eq!(budget.usage().retained_open_handles(), handles);
    assert_eq!(
        ready
            .basis()
            .schema(&ComponentTypeId::new(COMPONENT))
            .unwrap()
            .version(),
        ComponentSchemaVersion(2)
    );
    ready.validate(&mut budget).unwrap();
    // Ordinary admission cannot acquire a Ready receipt while private proofs exist.
    assert!(matches!(
        AuthoringProjectAdmissionHost::default().prepare(schema.project_root(), None),
        Err(AuthoringProjectAdmissionError::SourceAuthoringPending)
    ));
}

#[test]
fn successor_proof_refuses_original_bytes_in_any_unopened_or_other_profile_source() {
    for path in [CLOSED, OTHER_PROFILE, CATALOGUED] {
        let fixture = SuccessorFixture::new();
        let ready = fixture.ready();
        let old = fixture
            .recovery
            .members()
            .iter()
            .find(|member| member.path == path)
            .unwrap()
            .old_disk
            .bytes();
        fixture.publish(path, old);
        let mut budget = operation_budget();
        let retained = budget.usage().retained_candidate_bytes();
        assert!(matches!(
            verified(&fixture, ready.observed_pending_schema(), &[], &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory)
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert!(budget.usage().decode_work() > 0);
    }
}

#[test]
fn confirmation_refuses_unopened_edits_late_other_profile_members_and_removals() {
    for change in 0..3 {
        let fixture = SuccessorFixture::new();
        let ready = fixture.ready();
        let schema = ready.observed_pending_schema();
        let mut budget = operation_budget();
        let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
        match change {
            0 => fixture.project.write(
                CLOSED,
                PrefabDocument::default()
                    .to_ron_string()
                    .unwrap()
                    .as_bytes(),
            ),
            1 => fixture.project.scene("other/scenes/late.scene.json"),
            _ => fs::remove_file(fixture.project.root.join(OTHER_PROFILE)).unwrap(),
        }
        let retained = budget.usage().retained_candidate_bytes();
        let handles = budget.usage().retained_open_handles();
        let work = budget.usage().decode_work();
        let error = proof
            .validate(schema, &fixture.recovery, &[], &mut budget)
            .unwrap_err();
        if change == 2 {
            // Recorded members are read even when enumeration no longer discovers the leaf.
            assert!(
                matches!(&error, SourceUpgradeInventoryError::Host(error) if is_missing(error)),
                "{change}: {error:?}"
            );
        } else {
            assert!(
                matches!(error, SourceUpgradeInventoryError::RecoveryInventory),
                "{change}: {error:?}"
            );
        }
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
}

#[test]
fn confirmation_refuses_same_successor_bytes_at_a_different_leaf_identity() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
    let retained_leaf = proof.inventory().shared_file(CLOSED).unwrap();
    let old_identity = proof
        .participants()
        .find(|member| member.file().path() == CLOSED)
        .unwrap()
        .file()
        .identity();
    let bytes = fixture
        .recovery
        .members()
        .iter()
        .find(|member| member.path == CLOSED)
        .unwrap()
        .successor
        .bytes();
    fixture.publish(CLOSED, bytes);
    let file = schema
        .project_root()
        .open_file_exact(&RelativePath::new(CLOSED).unwrap())
        .unwrap();
    assert_ne!(file.identity(), old_identity);
    assert_eq!(
        retained_leaf
            .read_to_end_bounded(bytes.len() as u64)
            .unwrap(),
        bytes
    );
    assert!(!retained_leaf.same_object(&file).unwrap());
    let handles = budget.usage().retained_open_handles();
    assert!(matches!(
        proof.validate(schema, &fixture.recovery, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    assert_eq!(budget.usage().retained_open_handles(), handles);
}

#[test]
fn future_open_observations_are_exact_and_cannot_reuse_old_workspace_facts() {
    let fixture = SuccessorFixture::new();
    let old = fixture.future_open(&fixture.incumbent, &fixture.previous);
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    assert!(matches!(
        verified(
            &fixture,
            schema,
            &[SourceUpgradeOpenSource::Bound(&old)],
            &mut operation_budget()
        ),
        Err(SourceUpgradeInventoryError::OpenSourceChanged)
    ));
    assert!(matches!(
        verified(
            &fixture,
            schema,
            &[SourceUpgradeOpenSource::Unbound],
            &mut operation_budget()
        ),
        Err(SourceUpgradeInventoryError::UnboundOpenSource)
    ));
    let future = fixture.future_open(schema, ready.basis());
    let mut budget = operation_budget();
    let proof = verified(
        &fixture,
        schema,
        &[SourceUpgradeOpenSource::Bound(&future)],
        &mut budget,
    )
    .unwrap();
    assert!(matches!(
        proof.validate(schema, &fixture.recovery, &[], &mut budget),
        Err(SourceUpgradeInventoryError::MembershipChanged)
    ));
    let bytes = fixture
        .recovery
        .members()
        .iter()
        .find(|member| member.path == OPEN)
        .unwrap()
        .successor
        .bytes();
    fixture.publish(OPEN, bytes);
    assert!(matches!(
        proof.validate(
            schema,
            &fixture.recovery,
            &[SourceUpgradeOpenSource::Bound(&future)],
            &mut budget
        ),
        Err(SourceUpgradeInventoryError::OpenSourceChanged)
    ));
}

#[test]
fn metadata_membership_digest_and_positive_missing_root_observations_bind_successor() {
    for change in 0..5 {
        let fixture = SuccessorFixture::new();
        let ready = fixture.ready();
        let schema = ready.observed_pending_schema();
        let mut budget = operation_budget();
        let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
        match change {
            0 => fixture.project.metadata(
                "assets/unused.png",
                "unused.png",
                SECOND_ID,
                AssetSourceKind::Image,
                1,
            ),
            1 => fixture.project.metadata(
                "assets/late.png",
                "late.png",
                "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
                AssetSourceKind::Image,
                0,
            ),
            2 => fs::remove_file(fixture.project.root.join("assets/unused.png.meta")).unwrap(),
            3 => fs::create_dir_all(fixture.project.root.join("missing/scenes")).unwrap(),
            _ => fs::create_dir(fixture.project.root.join("missing")).unwrap(),
        }
        let retained = budget.usage().retained_candidate_bytes();
        assert!(matches!(
            proof.validate(schema, &fixture.recovery, &[], &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory)
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    }
}

#[test]
fn a_late_sidecar_is_not_an_unchanged_missing_observation() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
    fixture.project.metadata(
        OPEN,
        "open.scene.json",
        "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f",
        AssetSourceKind::Scene,
        0,
    );
    assert!(matches!(
        proof.validate(schema, &fixture.recovery, &[], &mut budget),
        Err(SourceUpgradeInventoryError::RecoveryInventory)
    ));
}

#[test]
fn repeated_successful_and_refused_confirmation_releases_only_temporary_retention() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    // This test exercises storage reuse across several complete scans. Give cumulative work
    // an explicit test allowance so an earlier spent-work refusal cannot stand in for the
    // changed-membership result asserted below. Production defaults remain unchanged.
    budget.limits.decode_work = nara_core::ItemLimit::new(200_000_000).unwrap();
    let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
    let retained = budget.usage().retained_candidate_bytes();
    let handles = budget.usage().retained_open_handles();
    for _ in 0..2 {
        let work = budget.usage().decode_work();
        proof
            .validate(schema, &fixture.recovery, &[], &mut budget)
            .unwrap();
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
    fixture.project.scene("other/scenes/late.scene.json");
    for _ in 0..2 {
        let work = budget.usage().decode_work();
        assert!(matches!(
            proof.validate(schema, &fixture.recovery, &[], &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory
                | SourceUpgradeInventoryError::MembershipChanged)
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), retained);
        assert_eq!(budget.usage().retained_open_handles(), handles);
        assert!(budget.usage().decode_work() > work);
    }
}

#[test]
fn confirmation_requires_the_same_pending_receipt_instead_of_equal_re_admitted_schema_bytes() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    let proof = verified(&fixture, schema, &[], &mut budget).unwrap();
    let other = fixture.conflicting_pending_observation();
    assert_eq!(other.manifest_digest(), schema.manifest_digest());
    assert_eq!(other.bundle_digest(), schema.bundle_digest());
    assert!(matches!(
        proof.validate(&other, &fixture.recovery, &[], &mut budget),
        Err(SourceUpgradeInventoryError::AuthorityChanged)
    ));
}

#[test]
fn wrong_pending_operation_full_profile_configuration_universe_and_successor_bundle_refuse() {
    for change in 0..5 {
        let fixture = SuccessorFixture::new();
        let ready = fixture.ready();
        let mut manifest =
            ProjectManifest::parse_toml_bytes(ready.observed_pending_schema().manifest_bytes())
                .manifest
                .unwrap();
        match change {
            0 => {
                manifest.authoring.state = ProjectAuthoringState::Pending(
                    ProjectAuthoringRecovery::new(
                        ProjectPath::new(RECOVERY).unwrap(),
                        ProjectAuthoringRecoveryDigest::from_parts(1, [7; 32]),
                    )
                    .unwrap(),
                )
            }
            1 => {
                manifest.profiles.get_mut("other").unwrap().paths.scenes =
                    Some("different/scenes".to_owned())
            }
            2 => {
                manifest.project.source_universe =
                    "dd84209a-c14c-49e4-8b13-b3738daf4f53".parse().unwrap()
            }
            3 => fixture.publish(BUNDLE, fixture.recovery.bundle().old_disk.bytes()),
            _ => manifest.authoring.schema_bundle = Some("schema/other.bundle.json".to_owned()),
        }
        if change != 3 {
            fixture.publish("nara.toml", &manifest.to_canonical_toml_bytes().unwrap());
        }
        let observed = fixture.conflicting_pending_observation();
        let mut budget = operation_budget();
        let before = budget.usage().retained_candidate_bytes();
        assert!(matches!(
            verified(&fixture, &observed, &[], &mut budget),
            Err(SourceUpgradeInventoryError::RecoveryInventory)
        ));
        assert_eq!(budget.usage().retained_candidate_bytes(), before);
        assert!(budget.usage().decode_work() > 0);
    }
}

#[test]
fn comparison_work_is_admitted_before_original_recovery_body_comparisons() {
    let fixture = SuccessorFixture::new();
    let mut budget = operation_budget();
    budget.limits.decode_work = nara_core::ItemLimit::new(1).unwrap();
    assert!(matches!(
        fixture.original.validate_original_recovery(
            &fixture.incumbent,
            &fixture.recovery,
            &mut budget
        ),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
}

#[test]
fn complete_metadata_comparison_uses_exact_sidecar_membership_when_source_order_differs() {
    let fixture = SuccessorFixture::with_prefix_metadata(true);
    let inventory_paths: Vec<_> = fixture
        .original
        .metadata()
        .map(|metadata| metadata.file().path())
        .collect();
    let recovery_paths: Vec<_> = fixture
        .recovery
        .metadata()
        .iter()
        .map(|metadata| metadata.path.as_str())
        .collect();
    assert_ne!(inventory_paths, recovery_paths);
    fixture
        .original
        .validate_original_recovery(
            &fixture.incumbent,
            &fixture.recovery,
            &mut operation_budget(),
        )
        .unwrap();
    let ready = fixture.ready();
    let mut budget = operation_budget();
    let proof = verified(&fixture, ready.observed_pending_schema(), &[], &mut budget).unwrap();
    proof
        .validate(
            ready.observed_pending_schema(),
            &fixture.recovery,
            &[],
            &mut budget,
        )
        .unwrap();
}

#[test]
fn failed_successor_capture_refunds_storage_and_preserves_cumulative_work() {
    let fixture = SuccessorFixture::new();
    let ready = fixture.ready();
    let schema = ready.observed_pending_schema();
    let mut budget = operation_budget();
    let retained = budget.usage().retained_candidate_bytes();
    budget.limits.decode_work = nara_core::ItemLimit::new(1).unwrap();
    assert!(matches!(
        verified(&fixture, schema, &[], &mut budget),
        Err(SourceUpgradeInventoryError::Admission(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { .. }
        ))
    ));
    assert_eq!(budget.usage().retained_candidate_bytes(), retained);
    assert!(budget.usage().decode_work() > 1);
}
