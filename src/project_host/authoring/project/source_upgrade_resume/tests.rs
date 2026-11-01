use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, AssetSourcePath, StableAssetId,
};
use nara_core::ItemLimit;
use nara_fs::{ContentDigest, DirectoryCapability, FileCapability, RelativePath};
use nara_project::{ProjectAuthoringState, ProjectManifest, ProjectPath};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SceneFileEncoding, SourceDocumentKey, SourceDocumentKind,
};

use super::{
    AuthoredSchemaUpgradeRecovery, AuthoredSchemaUpgradeRecoveryError,
    AuthoredSchemaUpgradeRecoveryErrorKind, AuthoredSchemaUpgradeRecoveryMemberState,
    AuthoredSchemaUpgradeRecoveryPhase, AuthoredSchemaUpgradeRecoveryReceipt,
};
use crate::{
    project_host::{
        authoring::{
            AuthoredSchemaUpgradeHistoryPolicy, AuthoredSchemaUpgradeHostLimits,
            AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
            AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
            AuthoringProjectReadiness, PreparedAuthoringProject,
            document::AuthoringDocumentSourceFact,
            native_source::NativeDocumentSourcePolicy,
            project_authoring_adapters,
            source_upgrade_inventory::{
                SourceUpgradeInventory, SourceUpgradeInventoryError, SourceUpgradeOpenSource,
            },
            source_upgrade_recovery::{
                RecoverySourceEncoding, SourceUpgradeRecovery, SourceUpgradeRecoveryInput,
                SourceUpgradeRecoveryLimits, SourceUpgradeRecoveryMemberInput,
                SourceUpgradeRecoveryMetadataInput, SourceUpgradeRecoveryRootInput,
            },
            source_upgrade_storage::{
                RecoveryPendingPublication, RecoveryStorageHost, RecoveryStoragePublication,
            },
        },
        file_publication::{FilePublicationHost, FilePublicationOutcome},
    },
    project_tooling::host_fs,
};

const UNIVERSE: &str = "42b4f318-eef4-49f0-a8a4-a3790ce8fbd1";
const COMPONENT: &str = "trial.enemy.EnemyStats";
const OWNER: &str = "trial.enemy";
const BUNDLE_PATH: &str = "schema/bundle.json";
const RECOVERY_PATH: &str = ".nara/recovery.json";
const EXTRA_PATH: &str = "scenes/added.scene.json";
const SCRIPT_PATH: &str = "scripts/nested/open.scene.json";
const SCRIPT_METADATA_PATH: &str = "scripts/nested/open.scene.json.meta";
const UNCHANGED_METADATA_PATH: &str = "scenes/unchanged.scene.json.meta";
const SCRIPT_ID: &str = "1d3d0cf3-6a95-4457-97bd-7f2e41f3b72f";
static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);

struct Member {
    path: &'static str,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    old: Vec<u8>,
    working: Vec<u8>,
    successor: Vec<u8>,
}

struct FrozenFile {
    path: &'static str,
    file: FileCapability,
    bytes: Vec<u8>,
}

struct Fixture {
    directory: PathBuf,
    root: DirectoryCapability,
    old_bundle: Vec<u8>,
    successor_bundle: Vec<u8>,
    ready_manifest: Vec<u8>,
    members: Vec<Member>,
}

impl Fixture {
    fn new() -> Self {
        Self::with_scripts_source(false)
    }

    fn with_scripts_source(scripts_source: bool) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nara-headless-upgrade-recovery-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        for parent in [".nara", "schema", "assets", "prefabs", "scenes", "z-extra"] {
            fs::create_dir(directory.join(parent)).unwrap();
        }
        fs::create_dir(directory.join("z-extra/scenes")).unwrap();
        let manifest = format!(
            "schema_version = 1\n[project]\nname = 'Headless recovery fixture'\n\
             source_universe = '{UNIVERSE}'\nnative_document_identity = 'project_location'\n\
             [authoring]\nstate = 'ready'\nschema_bundle = '{BUNDLE_PATH}'\n\
             [startup]\ndefault_scene = 'pending.scene.json'\n\
             [profiles.inactive.paths]\nscenes = 'z-extra/scenes'\n\
             [profiles.inactive.startup]\ndefault_scene = 'closed.scene.json'\n"
        )
        .into_bytes();
        let ready_manifest = ProjectManifest::parse_toml_bytes(&manifest)
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
        let successor_bundle = bundle(next_catalog, Some(old_catalog));
        let mut members = vec![
            member(
                "prefabs/enemy.prefab.ron",
                SourceDocumentKind::Prefab,
                Some(105),
                Some(105),
            ),
            member(
                "scenes/pending.scene.json",
                SourceDocumentKind::Scene,
                Some(7),
                Some(9),
            ),
            member(
                "scenes/unchanged.scene.json",
                SourceDocumentKind::Scene,
                None,
                None,
            ),
            member(
                "z-extra/scenes/closed.scene.json",
                SourceDocumentKind::Scene,
                Some(13),
                Some(13),
            ),
        ];
        if scripts_source {
            fs::create_dir_all(directory.join("scripts/nested")).unwrap();
            members.push(member(
                SCRIPT_PATH,
                SourceDocumentKind::Scene,
                Some(17),
                Some(19),
            ));
            members.sort_unstable_by_key(|member| member.path);
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    StableAssetId::parse_str(SCRIPT_ID).unwrap(),
                    AssetPath::new("nested/open.scene.json").unwrap(),
                    AssetSourceKind::Scene,
                ),
                0_u32,
            );
            fs::write(
                directory.join(SCRIPT_METADATA_PATH),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(directory.join("nara.toml"), &manifest).unwrap();
        fs::write(directory.join(BUNDLE_PATH), &old_bundle).unwrap();
        for member in &members {
            fs::write(directory.join(member.path), &member.old).unwrap();
        }
        {
            let mut limits = AuthoringProjectAdmissionLimits::default();
            limits.decode_work = complete_limits().host_work();
            limits.peak_open_handles = complete_limits().open_handles();
            let mut budget = AuthoringProjectAdmissionBudget::new(limits);
            let original = scripts_source.then(|| {
                let schema = AuthoringProjectAdmissionHost::default()
                    .prepare(host_fs::writable_directory(&directory).unwrap(), None)
                    .unwrap()
                    .admit_bundle()
                    .unwrap()
                    .publish();
                let root = schema.project_root();
                let file = root
                    .open_file_exact(&RelativePath::new(SCRIPT_PATH).unwrap())
                    .unwrap();
                let native = NativeDocumentSourcePolicy::new(schema.settings())
                    .admit(
                        &root,
                        SCRIPT_PATH,
                        SourceDocumentKind::Scene,
                        file.identity(),
                    )
                    .unwrap();
                let source = AuthoringDocumentSourceFact::from_native_observation(
                    SCRIPT_PATH,
                    file.identity(),
                    file.parent_identity(),
                    file.digest(1_048_576).unwrap(),
                    &native,
                )
                .unwrap();
                let inventory = SourceUpgradeInventory::capture(
                    &schema,
                    &[SourceUpgradeOpenSource::Bound(&source)],
                    nara_scene::SceneFileLimits::default(),
                    &mut budget,
                )
                .unwrap();
                assert_eq!(inventory.participants().len(), members.len());
                assert_eq!(
                    inventory
                        .roots()
                        .map(|root| root.path())
                        .collect::<Vec<_>>(),
                    ["assets", "prefabs", "scenes", "z-extra/scenes"]
                );
                let script = inventory
                    .participants()
                    .find(|participant| participant.file().path() == SCRIPT_PATH)
                    .unwrap();
                assert_eq!(script.source_key(), source.source_key());
                assert_eq!(script.kind(), SourceDocumentKind::Scene);
                assert_eq!(
                    script.source_key(),
                    &members
                        .iter()
                        .find(|member| member.path == SCRIPT_PATH)
                        .unwrap()
                        .key
                );
                inventory
            });
            let inputs: Vec<_> = members
                .iter()
                .map(|member| SourceUpgradeRecoveryMemberInput {
                    path: member.path,
                    key: &member.key,
                    encoding: member.encoding,
                    old_disk: &member.old,
                    reviewed_working: &member.working,
                    successor: &member.successor,
                })
                .collect();
            let missing: Vec<_> = members
                .iter()
                .filter(|member| !scripts_source || member.path != SCRIPT_PATH)
                .map(|member| format!("{}.meta", member.path))
                .collect();
            let missing: Vec<_> = missing.iter().map(String::as_str).collect();
            let metadata: Vec<_> = original
                .iter()
                .flat_map(|inventory| inventory.metadata())
                .map(|metadata| SourceUpgradeRecoveryMetadataInput {
                    path: metadata.file().path(),
                    observed_bytes: metadata.bytes(),
                })
                .collect();
            let roots: Vec<_> = ["assets", "prefabs", "scenes", "z-extra/scenes"]
                .into_iter()
                .map(|path| SourceUpgradeRecoveryRootInput {
                    path,
                    missing_ancestor: None,
                })
                .collect();
            let recovery = SourceUpgradeRecovery::prepare(
                SourceUpgradeRecoveryInput {
                    old_ready_manifest: &manifest,
                    future_ready_manifest: &ready_manifest,
                    bundle_path: BUNDLE_PATH,
                    old_bundle: &old_bundle,
                    successor_bundle: &successor_bundle,
                    component: &ComponentTypeId::new(COMPONENT),
                    field: &ComponentFieldId::new("health"),
                    successor_property: &successor_property(),
                    members: &inputs,
                    metadata: &metadata,
                    roots: &roots,
                    missing_sidecars: &missing,
                },
                SourceUpgradeRecoveryLimits::default(),
                &mut budget,
            )
            .unwrap()
            .into_shared(&mut budget)
            .unwrap();
            let prepared = RecoveryStorageHost::capture(
                host_fs::writable_directory(&directory).unwrap(),
                &mut budget,
            )
            .unwrap()
            .prepare(
                recovery,
                ProjectPath::new(RECOVERY_PATH).unwrap(),
                &mut budget,
            )
            .unwrap();
            let stored = match prepared.publish(&mut budget) {
                RecoveryStoragePublication::Published(stored) => stored,
                _ => panic!("fixture recovery package was not published"),
            };
            match stored
                .bind_pending(&mut budget)
                .unwrap()
                .publish(&mut budget)
            {
                RecoveryPendingPublication::Published(pending) => drop(pending),
                _ => panic!("fixture Pending manifest was not published"),
            }
        }
        // Reopen only the directory after all original storage/publication owners have dropped.
        // The inspector receives no old Schema receipt, Session, inventory or saved token.
        let root = host_fs::writable_directory(&directory).unwrap();
        Self {
            directory,
            root,
            old_bundle,
            successor_bundle,
            ready_manifest,
            members,
        }
    }

    fn inspect(
        &self,
        limits: AuthoredSchemaUpgradeHostLimits,
    ) -> Result<AuthoredSchemaUpgradeRecovery, AuthoredSchemaUpgradeRecoveryError> {
        AuthoredSchemaUpgradeRecovery::inspect(self.root.share(), None, limits)
    }

    fn file(&self, path: &str) -> FileCapability {
        self.root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap()
    }

    fn freeze(&self) -> Vec<FrozenFile> {
        [
            "nara.toml",
            BUNDLE_PATH,
            RECOVERY_PATH,
            EXTRA_PATH,
            SCRIPT_METADATA_PATH,
            UNCHANGED_METADATA_PATH,
        ]
        .into_iter()
        .chain(self.members.iter().map(|member| member.path))
        .filter(|path| self.directory.join(path).exists())
        .map(|path| FrozenFile {
            path,
            file: self.file(path),
            bytes: fs::read(self.directory.join(path)).unwrap(),
        })
        .collect()
    }

    fn assert_unchanged(&self, frozen: &[FrozenFile]) {
        for before in frozen {
            assert_eq!(
                self.file(before.path).identity(),
                before.file.identity(),
                "{}",
                before.path
            );
            assert_eq!(
                fs::read(self.directory.join(before.path)).unwrap(),
                before.bytes,
                "{}",
                before.path
            );
        }
    }

    fn replace(&self, path: &str, bytes: &[u8]) {
        let file = self.file(path);
        let mut publisher = FilePublicationHost::bind_existing(
            &self.root,
            path,
            &file,
            file.digest(1_048_576).unwrap(),
            1_048_576,
        )
        .unwrap();
        match publisher.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (_, source) = accepted.into_parts();
                publisher.commit_published(source);
            }
            FilePublicationOutcome::Rejected(reason) => {
                panic!("fixture replacement refused: {reason:?}")
            }
            FilePublicationOutcome::Failed(stage) => {
                panic!("fixture replacement failed: {stage:?}")
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("fixture replacement uncertain: {evidence:?}")
            }
        }
    }

    fn assert_pending(&self) {
        let manifest =
            ProjectManifest::parse_toml_bytes(&fs::read(self.directory.join("nara.toml")).unwrap())
                .manifest
                .unwrap();
        assert!(matches!(
            manifest.authoring.state,
            ProjectAuthoringState::Pending(_)
        ));
        assert!(matches!(
            AuthoringProjectAdmissionHost::default().prepare(self.root.share(), None),
            Err(AuthoringProjectAdmissionError::SourceAuthoringPending),
        ));
    }

    fn complete(
        &self,
        mut recovery: AuthoredSchemaUpgradeRecovery,
    ) -> AuthoredSchemaUpgradeRecoveryReceipt {
        let pending = self.file("nara.toml");
        let operation = recovery.view().operation;
        publish_sources(&mut recovery);
        prepare_ready(&mut recovery);
        self.assert_pending();
        let complete = recovery.advance().unwrap();
        assert_eq!(complete.phase, AuthoredSchemaUpgradeRecoveryPhase::Complete);
        assert_eq!(complete.completed_sources, self.members.len());
        assert!(complete.bundle_complete);
        assert!(complete.refusal.is_none());
        assert!(!complete.cleanup_pending);
        assert!(!complete.uncertain);
        assert_eq!(recovery.advance().unwrap(), complete);
        let receipt = recovery.into_receipt().unwrap();
        assert_eq!(receipt.operation(), operation);
        assert_eq!(receipt.source_count(), self.members.len());
        assert_eq!(receipt.usage(), complete.usage);
        assert_eq!(
            receipt.ready_publication().previous_identity(),
            Some(pending.identity())
        );
        assert_eq!(
            receipt.ready_publication().published_identity(),
            Some(self.file("nara.toml").identity())
        );
        assert_eq!(
            receipt.ready_publication().published_content_digest(),
            Some(ContentDigest::of_bytes(&self.ready_manifest))
        );
        assert_eq!(
            fs::read(self.directory.join("nara.toml")).unwrap(),
            self.ready_manifest
        );
        assert_eq!(
            fs::read(self.directory.join(BUNDLE_PATH)).unwrap(),
            self.successor_bundle
        );
        for member in &self.members {
            assert_eq!(
                fs::read(self.directory.join(member.path)).unwrap(),
                member.successor,
                "{}",
                member.path
            );
        }
        receipt
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Remove only this fixture's fixed leaves and directories; never recurse through a path.
        for path in [
            RECOVERY_PATH,
            BUNDLE_PATH,
            "nara.toml",
            EXTRA_PATH,
            SCRIPT_METADATA_PATH,
            UNCHANGED_METADATA_PATH,
        ]
        .into_iter()
        .chain(self.members.iter().map(|member| member.path))
        {
            let _ = fs::remove_file(self.directory.join(path));
        }
        for path in [
            ".nara",
            "schema",
            "assets",
            "prefabs",
            "scenes",
            "scripts/nested",
            "scripts",
            "z-extra/scenes",
            "z-extra",
        ] {
            let _ = fs::remove_dir(self.directory.join(path));
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

fn complete_limits() -> AuthoredSchemaUpgradeHostLimits {
    AuthoredSchemaUpgradeHostLimits::default().with_open_handles(ItemLimit::new(128).unwrap())
}

fn successor_property() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
    ])
}

fn schema(successor: bool) -> ComponentSchema {
    let health = if successor {
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
        ComponentSchemaVersion(if successor { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("health"),
        "Health",
        ComponentFieldPath::from_fields(["wire_health"]),
        health,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn bundle(current: ComponentSchemaCatalog, previous: Option<ComponentSchemaCatalog>) -> Vec<u8> {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap(),
        current,
        previous,
    )])
    .unwrap()
    .to_json_string()
    .unwrap()
    .into_bytes()
}

fn encoded(kind: SourceDocumentKind, value: Option<i64>, successor: bool) -> Vec<u8> {
    let scene = match value {
        None => SceneDocument::default(),
        Some(value) => {
            let mut entity = SceneEntityRecord::new(SceneEntityId::new("enemy").unwrap());
            let health = if successor {
                ComponentValue::map([("milli", ComponentValue::I64(value * 1000))])
            } else {
                ComponentValue::I64(value)
            };
            entity.components.insert(
                ComponentTypeId::new(COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(if successor { 2 } else { 1 }),
                    ComponentValue::map([("wire_health", health)]),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()),
            );
            SceneDocument::from_ordered_roots([entity])
        }
    };
    match kind {
        SourceDocumentKind::Scene => scene.to_json_string().unwrap().into_bytes(),
        SourceDocumentKind::Prefab => PrefabDocument::new(scene.roots, scene.entities)
            .to_ron_string()
            .unwrap()
            .into_bytes(),
    }
}

fn member(
    path: &'static str,
    kind: SourceDocumentKind,
    old: Option<i64>,
    working: Option<i64>,
) -> Member {
    Member {
        path,
        key: SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(path).unwrap(),
            kind,
        ),
        encoding: if kind == SourceDocumentKind::Prefab {
            RecoverySourceEncoding::Ron
        } else {
            RecoverySourceEncoding::Json
        },
        old: encoded(kind, old, false),
        working: encoded(kind, working, false),
        successor: encoded(kind, working, true),
    }
}

fn publish_sources(recovery: &mut AuthoredSchemaUpgradeRecovery) {
    let confirmed = recovery.confirm().unwrap();
    assert_eq!(
        confirmed.phase,
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources
    );
    assert_eq!(confirmed.completed_sources, 0);
    assert!(!confirmed.bundle_complete);
    assert!(recovery.review().is_none());
    for completed in 1..=confirmed.source_count {
        let view = recovery.advance().unwrap();
        assert_eq!(
            view.phase,
            AuthoredSchemaUpgradeRecoveryPhase::PublishingSources
        );
        assert_eq!(view.source_count, confirmed.source_count);
        assert_eq!(view.completed_sources, completed);
        assert!(!view.bundle_complete);
    }
    let bundle = recovery.advance().unwrap();
    assert_eq!(
        bundle.phase,
        AuthoredSchemaUpgradeRecoveryPhase::PreparingReady
    );
    assert_eq!(bundle.completed_sources, confirmed.source_count);
    assert!(bundle.bundle_complete);
}

fn prepare_ready(recovery: &mut AuthoredSchemaUpgradeRecovery) {
    for _ in 0..8 {
        let view = recovery.view();
        if view.phase == AuthoredSchemaUpgradeRecoveryPhase::PublishingReady {
            return;
        }
        assert_eq!(
            view.phase,
            AuthoredSchemaUpgradeRecoveryPhase::PreparingReady
        );
        assert_eq!(view.completed_sources, view.source_count);
        assert!(view.bundle_complete);
        recovery.advance().unwrap();
    }
    panic!("finite Ready preparation did not reach its publication phase");
}

fn cause<'a, T: Error + 'static>(mut error: &'a (dyn Error + 'static)) -> Option<&'a T> {
    loop {
        if let Some(cause) = error.downcast_ref::<T>() {
            return Some(cause);
        }
        error = error.source()?;
    }
}

#[test]
fn all_old_pending_reviews_dirty_working_and_finishes_with_actual_ready_and_ordinary_admission() {
    let fixture = Fixture::new();
    let frozen = fixture.freeze();
    fixture.assert_pending();
    let recovery = fixture.inspect(complete_limits()).unwrap();
    let view = recovery.view();
    assert_eq!(view.phase, AuthoredSchemaUpgradeRecoveryPhase::Review);
    assert_eq!(view.source_count, 4);
    assert_eq!(view.completed_sources, 0);
    assert!(!view.bundle_complete);
    assert_eq!(view.limits, complete_limits());
    let review = recovery.review().unwrap();
    assert_eq!(
        review.schema_bundle().to_json_string().unwrap().as_bytes(),
        fixture.old_bundle
    );
    assert_eq!(
        review
            .successor_schema_bundle()
            .to_json_string()
            .unwrap()
            .as_bytes(),
        fixture.successor_bundle
    );
    assert_eq!(review.component(), &ComponentTypeId::new(COMPONENT));
    assert_eq!(review.field(), &ComponentFieldId::new("health"));
    assert_eq!(review.successor_property(), &successor_property());
    assert_eq!(
        review.history_policy(),
        AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
    );
    let adapters = project_authoring_adapters(review.schema_bundle()).unwrap();
    let data_adapter = adapters
        .iter()
        .find(|adapter| {
            adapter
                .components()
                .any(|component| component.as_str() == COMPONENT)
        })
        .unwrap();
    assert!(data_adapter.contribution().is_none());
    assert_eq!(
        review.bundle_state(),
        AuthoredSchemaUpgradeRecoveryMemberState::Original
    );
    assert_eq!(review.sources().len(), fixture.members.len());
    for (actual, expected) in review.sources().zip(&fixture.members) {
        assert_eq!(actual.key, &expected.key);
        assert_eq!(actual.project_path, expected.path);
        assert_eq!(actual.original_disk, expected.old);
        assert_eq!(actual.reviewed_working, expected.working);
        assert_eq!(actual.successor, expected.successor);
        assert_eq!(
            actual.encoding,
            if expected.key.kind() == SourceDocumentKind::Prefab {
                SceneFileEncoding::Ron
            } else {
                SceneFileEncoding::Json
            }
        );
        assert_eq!(
            actual.observed,
            if expected.old == expected.successor {
                AuthoredSchemaUpgradeRecoveryMemberState::Unchanged
            } else {
                AuthoredSchemaUpgradeRecoveryMemberState::Original
            }
        );
    }
    assert_ne!(fixture.members[1].old, fixture.members[1].working);
    fixture.assert_unchanged(&frozen);
    fixture.complete(recovery);
    for (profile, source) in [
        (None, &fixture.members[1]),
        (Some("inactive"), &fixture.members[3]),
    ] {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(fixture.root.share(), profile)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let project = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish()
            .snapshot();
        assert_eq!(project.readiness(), AuthoringProjectReadiness::Complete);
        assert_eq!(
            project.startup_source().project_path().as_str(),
            source.path
        );
        assert_eq!(
            project.startup_source().digest(),
            ContentDigest::of_bytes(&source.successor)
        );
        assert_eq!(
            project
                .startup_scene()
                .document()
                .to_json_string()
                .unwrap()
                .as_bytes(),
            source.successor
        );
    }
}

#[test]
fn recorded_open_scripts_source_recovers_to_ready_and_is_freshly_admitted() {
    let fixture = Fixture::with_scripts_source(true);
    let script = fixture
        .members
        .iter()
        .find(|member| member.path == SCRIPT_PATH)
        .unwrap();
    let frozen = fixture.freeze();
    let recovery = fixture.inspect(complete_limits()).unwrap();
    assert_eq!(recovery.view().source_count, 5);
    let review = recovery.review().unwrap();
    let reviewed = review
        .sources()
        .find(|source| source.project_path == SCRIPT_PATH)
        .unwrap();
    assert_eq!(reviewed.key, &script.key);
    assert_eq!(reviewed.key.kind(), SourceDocumentKind::Scene);
    assert_eq!(reviewed.encoding, SceneFileEncoding::Json);
    assert_eq!(reviewed.original_disk, script.old);
    assert_eq!(reviewed.reviewed_working, script.working);
    assert_eq!(reviewed.successor, script.successor);
    assert_ne!(reviewed.original_disk, reviewed.reviewed_working);
    assert_eq!(
        reviewed.observed,
        AuthoredSchemaUpgradeRecoveryMemberState::Original
    );
    fixture.assert_unchanged(&frozen);

    // The public owner has no open set. It must carry this authenticated member through every
    // successor scan, actual source/bundle writes and its final confirmed Ready replacement.
    let receipt = fixture.complete(recovery);
    assert_eq!(receipt.source_count(), 5);
    let metadata = frozen
        .iter()
        .find(|file| file.path == SCRIPT_METADATA_PATH)
        .unwrap();
    assert_eq!(
        fixture.file(SCRIPT_METADATA_PATH).identity(),
        metadata.file.identity()
    );
    assert_eq!(
        fs::read(fixture.directory.join(SCRIPT_METADATA_PATH)).unwrap(),
        metadata.bytes
    );

    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(
            host_fs::writable_directory(&fixture.directory).unwrap(),
            None,
        )
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let root = schema.project_root();
    let file = root
        .open_file_exact(&RelativePath::new(SCRIPT_PATH).unwrap())
        .unwrap();
    assert_eq!(
        file.digest(1_048_576).unwrap(),
        ContentDigest::of_bytes(&script.successor)
    );
    let native = NativeDocumentSourcePolicy::new(schema.settings())
        .admit(
            &root,
            SCRIPT_PATH,
            SourceDocumentKind::Scene,
            file.identity(),
        )
        .unwrap();
    assert_eq!(native.key(), &script.key);
    assert_eq!(native.key().kind(), SourceDocumentKind::Scene);
    // Project-location keys do not acquire metadata identity. Check the actual retained sidecar.
    assert!(native.metadata_source().is_none());
    let admitted_metadata = root
        .open_file_exact(&RelativePath::new(SCRIPT_METADATA_PATH).unwrap())
        .unwrap();
    assert!(admitted_metadata.same_object(&metadata.file).unwrap());
    let metadata_parent = RelativePath::new("scripts/nested").unwrap();
    let original_parent = fixture.root.open_directory_exact(&metadata_parent).unwrap();
    let admitted_parent = root.open_directory_exact(&metadata_parent).unwrap();
    assert!(admitted_parent.same_object(&original_parent).unwrap());
    assert_eq!(
        metadata.file.parent_identity(),
        Some(original_parent.identity())
    );
    assert_eq!(
        admitted_metadata.parent_identity(),
        Some(admitted_parent.identity())
    );
    assert_eq!(
        admitted_metadata.digest(1_048_576).unwrap(),
        ContentDigest::of_bytes(&metadata.bytes)
    );
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    let project = PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .unwrap()
        .publish()
        .snapshot();
    assert_eq!(project.readiness(), AuthoringProjectReadiness::Complete);
    assert_eq!(
        project.startup_source().digest(),
        ContentDigest::of_bytes(&fixture.members[1].successor)
    );
}

#[test]
fn scripts_successor_final_validation_refuses_source_metadata_membership_and_negative_fact_changes()
{
    for change in ["source", "metadata", "membership", "missing-sidecar"] {
        let fixture = Fixture::with_scripts_source(true);
        let mut recovery = fixture.inspect(complete_limits()).unwrap();
        publish_sources(&mut recovery);
        prepare_ready(&mut recovery);
        let prepared = recovery.view();
        assert_eq!(prepared.completed_sources, 5);
        match change {
            "source" => fs::write(
                fixture.directory.join(SCRIPT_PATH),
                encoded(SourceDocumentKind::Scene, Some(23), true),
            )
            .unwrap(),
            "metadata" => {
                let before =
                    fs::read_to_string(fixture.directory.join(SCRIPT_METADATA_PATH)).unwrap();
                let changed = before.replace(SCRIPT_ID, "1d3d0cf3-6a95-4457-97bd-7f2e41f3b730");
                assert_ne!(before, changed);
                fs::write(fixture.directory.join(SCRIPT_METADATA_PATH), changed).unwrap();
            }
            "membership" => fs::write(
                fixture.directory.join(EXTRA_PATH),
                encoded(SourceDocumentKind::Scene, None, false),
            )
            .unwrap(),
            "missing-sidecar" => fs::write(
                fixture.directory.join(UNCHANGED_METADATA_PATH),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str("1d3d0cf3-6a95-4457-97bd-7f2e41f3b730").unwrap(),
                        AssetPath::new("unchanged.scene.json").unwrap(),
                        AssetSourceKind::Scene,
                    ),
                    0_u32,
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap(),
            _ => unreachable!(),
        }
        let frozen = fixture.freeze();
        let error = recovery.advance().unwrap_err();
        assert_eq!(
            error.kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::Project,
            "{change}"
        );
        assert!(
            matches!(
                &error.detail,
                super::RecoveryFailure::Project(super::SourceUpgradeProjectError::Inventory(
                    SourceUpgradeInventoryError::RecoveryInventory
                ))
            ),
            "{change}: {error}"
        );
        let refused = recovery.view();
        assert_eq!(
            refused.phase,
            AuthoredSchemaUpgradeRecoveryPhase::PublishingReady
        );
        assert_eq!(refused.operation, prepared.operation);
        assert_eq!(refused.completed_sources, 5);
        assert!(refused.bundle_complete);
        assert_eq!(
            recovery.retry().unwrap_err().kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::Project,
            "{change}"
        );
        fixture.assert_unchanged(&frozen);
        fixture.assert_pending();
        assert!(recovery.into_receipt().is_err());
    }
}

#[test]
fn mixed_sources_and_new_bundle_preserve_already_new_and_unchanged_leaves() {
    let fixture = Fixture::new();
    fixture.replace(fixture.members[0].path, &fixture.members[0].successor);
    fixture.replace(BUNDLE_PATH, &fixture.successor_bundle);
    let skipped: Vec<_> = fixture
        .freeze()
        .into_iter()
        .filter(|file| {
            [
                fixture.members[0].path,
                fixture.members[2].path,
                BUNDLE_PATH,
                RECOVERY_PATH,
            ]
            .contains(&file.path)
        })
        .collect();
    let recovery = fixture.inspect(complete_limits()).unwrap();
    let review = recovery.review().unwrap();
    assert_eq!(
        review.bundle_state(),
        AuthoredSchemaUpgradeRecoveryMemberState::Successor
    );
    assert_eq!(
        review
            .sources()
            .map(|source| source.observed)
            .collect::<Vec<_>>(),
        [
            AuthoredSchemaUpgradeRecoveryMemberState::Successor,
            AuthoredSchemaUpgradeRecoveryMemberState::Original,
            AuthoredSchemaUpgradeRecoveryMemberState::Unchanged,
            AuthoredSchemaUpgradeRecoveryMemberState::Original,
        ]
    );
    fixture.complete(recovery);
    fixture.assert_unchanged(&skipped);
}

#[test]
fn all_new_pending_only_publishes_ready_and_keeps_every_source_bundle_and_package_leaf() {
    let fixture = Fixture::new();
    for member in &fixture.members {
        if member.old != member.successor {
            fixture.replace(member.path, &member.successor);
        }
    }
    fixture.replace(BUNDLE_PATH, &fixture.successor_bundle);
    let frozen: Vec<_> = fixture
        .freeze()
        .into_iter()
        .filter(|file| file.path != "nara.toml")
        .collect();
    let recovery = fixture.inspect(complete_limits()).unwrap();
    assert!(recovery.review().unwrap().sources().all(|source| matches!(
        source.observed,
        AuthoredSchemaUpgradeRecoveryMemberState::Successor
            | AuthoredSchemaUpgradeRecoveryMemberState::Unchanged,
    )));
    fixture.assert_pending();
    fixture.complete(recovery);
    fixture.assert_unchanged(&frozen);
}

#[test]
fn confirmation_refuses_equal_byte_replacement_third_content_and_membership_change_without_writes()
{
    enum Conflict {
        EqualBytes,
        ThirdContent,
        AddedMember,
    }
    for conflict in [
        Conflict::EqualBytes,
        Conflict::ThirdContent,
        Conflict::AddedMember,
    ] {
        let fixture = Fixture::new();
        let mut recovery = fixture.inspect(complete_limits()).unwrap();
        let before = recovery.view();
        match conflict {
            Conflict::EqualBytes => {
                fixture.replace(fixture.members[1].path, &fixture.members[1].old)
            }
            Conflict::ThirdContent => fixture.replace(
                fixture.members[1].path,
                &encoded(SourceDocumentKind::Scene, Some(8), false),
            ),
            Conflict::AddedMember => fs::write(
                fixture.directory.join(EXTRA_PATH),
                encoded(SourceDocumentKind::Scene, None, false),
            )
            .unwrap(),
        }
        let frozen = fixture.freeze();
        let error = recovery.confirm().unwrap_err();
        assert_eq!(
            error.kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::Inventory
        );
        assert!(matches!(
            cause::<SourceUpgradeInventoryError>(error),
            Some(
                SourceUpgradeInventoryError::MembershipChanged
                    | SourceUpgradeInventoryError::RecoveryInventory
            )
        ));
        let refused = recovery.view();
        assert_eq!(refused.operation, before.operation);
        assert_eq!(refused.phase, AuthoredSchemaUpgradeRecoveryPhase::Review);
        assert_eq!(refused.completed_sources, 0);
        assert!(!refused.bundle_complete);
        assert_eq!(refused.limits, before.limits);
        assert!(refused.usage.host_work > before.usage.host_work);
        assert_eq!(
            recovery.confirm().unwrap_err().kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::Inventory
        );
        assert_eq!(recovery.view(), refused);
        fixture.assert_unchanged(&frozen);
        fixture.assert_pending();
    }
}

#[test]
fn ready_tail_refuses_external_leaf_replacement_and_retains_the_same_pending_owner_for_retry() {
    let fixture = Fixture::new();
    let mut recovery = fixture.inspect(complete_limits()).unwrap();
    publish_sources(&mut recovery);
    prepare_ready(&mut recovery);
    let prepared = recovery.view();
    // Even an equal-byte replacement of a closed, alternate-profile leaf invalidates Ready.
    fixture.replace(fixture.members[3].path, &fixture.members[3].successor);
    let frozen = fixture.freeze();
    assert_eq!(
        recovery.advance().unwrap_err().kind(),
        AuthoredSchemaUpgradeRecoveryErrorKind::Project
    );
    let refused = recovery.view();
    assert_eq!(
        refused.phase,
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady
    );
    assert_eq!(refused.operation, prepared.operation);
    assert_eq!(refused.completed_sources, prepared.source_count);
    assert!(refused.bundle_complete);
    assert!(!refused.uncertain);
    assert!(!refused.cleanup_pending);
    assert_eq!(refused.limits, prepared.limits);
    assert!(refused.usage.host_work > prepared.usage.host_work);
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();
    assert_eq!(
        recovery.retry().unwrap_err().kind(),
        AuthoredSchemaUpgradeRecoveryErrorKind::Project
    );
    let retried = recovery.view();
    assert_eq!(retried.operation, prepared.operation);
    assert_eq!(
        retried.phase,
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady
    );
    assert_eq!(retried.limits, prepared.limits);
    assert!(retried.usage.host_work > refused.usage.host_work);
    let recovery = recovery.into_receipt().unwrap_err();
    assert_eq!(recovery.view(), retried);
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();
}

#[test]
fn finite_work_and_native_custody_preserve_selected_limits_and_specific_refusals() {
    let fixture = Fixture::new();
    let frozen = fixture.freeze();
    let defaults = AuthoredSchemaUpgradeHostLimits::default();
    assert_eq!(defaults.host_work().get(), 200_000_000);
    assert_eq!(defaults.open_handles().get(), 64);
    let work = complete_limits().with_host_work(ItemLimit::new(1).unwrap());
    let error = fixture.inspect(work).unwrap_err();
    assert!(matches!(
        cause::<AuthoringProjectAdmissionError>(&error),
        Some(AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: 1 })
    ));
    fixture.assert_unchanged(&frozen);
    let custody = complete_limits().with_open_handles(ItemLimit::new(1).unwrap());
    let error = fixture.inspect(custody).unwrap_err();
    assert!(matches!(
        cause::<AuthoringProjectAdmissionError>(&error),
        Some(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: 1 })
    ));
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();

    // Preserve the ordinary 64-handle policy through the complete operation. If this corpus's
    // final overlap cannot fit, the owner must retain Pending and report that exact ceiling.
    let inspected = fixture.inspect(defaults);
    let mut bounded = match inspected {
        Ok(recovery) => recovery,
        Err(error) => {
            assert!(matches!(
                cause::<AuthoringProjectAdmissionError>(&error),
                Some(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: 64 })
            ));
            fixture.assert_unchanged(&frozen);
            return;
        }
    };
    let operation = bounded.view().operation;
    let mut refused = bounded.confirm().is_err();
    for _ in 0..20 {
        if refused || bounded.view().phase == AuthoredSchemaUpgradeRecoveryPhase::Complete {
            break;
        }
        refused = bounded.advance().is_err();
    }
    assert_eq!(bounded.view().limits, defaults);
    assert_eq!(bounded.view().operation, operation);
    assert!(bounded.view().usage.peak_open_handles <= 64);
    if refused {
        assert!(matches!(
            cause::<AuthoringProjectAdmissionError>(bounded.failure().unwrap()),
            Some(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: 64 })
        ));
        let before_retry = bounded.view();
        assert!(bounded.retry().is_err());
        assert_eq!(bounded.view().limits, defaults);
        assert!(bounded.view().usage.host_work >= before_retry.usage.host_work);
        fixture.assert_pending();
    } else {
        assert_eq!(
            bounded.view().phase,
            AuthoredSchemaUpgradeRecoveryPhase::Complete
        );
        assert_eq!(
            bounded.into_receipt().unwrap().source_count(),
            fixture.members.len()
        );
    }
}

#[test]
fn confirmation_work_exhaustion_and_retry_share_the_original_finite_allowance() {
    let fixture = Fixture::new();
    let frozen = fixture.freeze();
    // Measure this fixture's actual read-only inspection rather than guessing a parser cost.
    let control = fixture.inspect(complete_limits()).unwrap();
    let inspection_work = control.view().usage.host_work;
    drop(control);
    let limits = complete_limits().with_host_work(ItemLimit::new(inspection_work).unwrap());
    let mut recovery = fixture.inspect(limits).unwrap();
    let inspected = recovery.view();
    assert_eq!(inspected.usage.host_work, inspection_work);
    let error = recovery.confirm().unwrap_err();
    assert!(
        matches!(cause::<AuthoringProjectAdmissionError>(error), Some(
        AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum }
    ) if *maximum == inspection_work)
    );
    let refused = recovery.view();
    assert!(refused.usage.host_work > inspection_work);
    let error = recovery.retry().unwrap_err();
    assert!(
        matches!(cause::<AuthoringProjectAdmissionError>(error), Some(
        AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum }
    ) if *maximum == inspection_work)
    );
    let retried = recovery.view();
    assert_eq!(retried.phase, AuthoredSchemaUpgradeRecoveryPhase::Review);
    assert_eq!(retried.operation, inspected.operation);
    assert_eq!(retried.limits, limits);
    assert!(retried.usage.host_work >= refused.usage.host_work);
    assert_eq!(retried.completed_sources, 0);
    assert!(!retried.bundle_complete);
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();
}

#[test]
fn advance_before_confirmation_returns_phase_error_without_consuming_review_or_writing() {
    let fixture = Fixture::new();
    let frozen = fixture.freeze();
    let mut recovery = fixture.inspect(complete_limits()).unwrap();
    let inspected = recovery.view();
    assert_eq!(
        recovery.advance().unwrap_err().kind(),
        AuthoredSchemaUpgradeRecoveryErrorKind::Phase
    );
    assert_eq!(recovery.view(), inspected);
    assert!(recovery.failure().is_none());
    assert_eq!(
        recovery.review().unwrap().sources().len(),
        fixture.members.len()
    );
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();
    // A mistimed action does not poison the same owner's subsequent explicit confirmation.
    fixture.complete(recovery);
}

// These controls exercise successful native cleanup after release. Unix currently refuses
// identity-bound temporary unlink, so it cannot supply that success control.
#[cfg(windows)]
fn recovery_at_publication(
    fixture: &Fixture,
    phase: AuthoredSchemaUpgradeRecoveryPhase,
    limits: AuthoredSchemaUpgradeHostLimits,
) -> AuthoredSchemaUpgradeRecovery {
    let mut recovery = fixture.inspect(limits).unwrap();
    match phase {
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources => {
            recovery.confirm().unwrap();
        }
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady => {
            publish_sources(&mut recovery);
            prepare_ready(&mut recovery);
        }
        _ => panic!("cleanup control requires a publication phase"),
    }
    assert_eq!(recovery.view().phase, phase);
    recovery
}

#[cfg(windows)]
fn fail_publication_with_cleanup(
    fixture: &Fixture,
    phase: AuthoredSchemaUpgradeRecoveryPhase,
    limits: AuthoredSchemaUpgradeHostLimits,
) -> (
    AuthoredSchemaUpgradeRecovery,
    crate::project_host::file_publication::CleanupTestControl,
) {
    let mut recovery = recovery_at_publication(fixture, phase, limits);
    let control = match recovery.stage.as_mut().unwrap() {
        super::Stage::Sources => recovery.publication.test_fail_next_write(),
        super::Stage::Ready(project) => project.test_fail_next_write(),
        _ => unreachable!("the control reached its requested publisher"),
    };
    let before = recovery.view();
    let error = recovery.advance().unwrap_err();
    assert!(matches!(
        &error.detail,
        super::RecoveryFailure::Failed(
            crate::project_host::file_publication::FilePublicationFailureStage::WriteTemporary,
        )
    ));
    let failed = recovery.view();
    assert_eq!(failed.phase, before.phase);
    assert_eq!(failed.completed_sources, before.completed_sources);
    assert_eq!(failed.bundle_complete, before.bundle_complete);
    assert!(failed.cleanup_pending);
    assert!(!failed.uncertain);
    fixture.assert_pending();
    (recovery, control)
}

#[cfg(windows)]
fn cleanup_for_fixture_teardown(recovery: &mut AuthoredSchemaUpgradeRecovery) -> bool {
    match recovery.stage.as_mut().unwrap() {
        super::Stage::Sources => recovery.publication.retry_cleanup(),
        super::Stage::Ready(project) => project.retry_cleanup(),
        _ => unreachable!("failed publication still owns its cleanup"),
    }
}

#[cfg(windows)]
fn assert_sustained_cleanup_refusal(phase: AuthoredSchemaUpgradeRecoveryPhase) {
    let fixture = Fixture::new();
    let (mut recovery, control) = fail_publication_with_cleanup(&fixture, phase, complete_limits());
    let frozen = fixture.freeze();
    let failed = recovery.view();
    let source_work = recovery.work.usage();
    for _ in 0..3 {
        let before = recovery.budget;
        assert_eq!(
            recovery.retry().unwrap_err().kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::PublicationFailed,
        );
        let retried = recovery.view();
        assert_eq!(retried.phase, failed.phase);
        assert_eq!(retried.operation, failed.operation);
        assert_eq!(retried.completed_sources, failed.completed_sources);
        assert_eq!(retried.bundle_complete, failed.bundle_complete);
        assert_eq!(retried.limits, failed.limits);
        assert!(retried.cleanup_pending);
        assert!(!retried.uncertain);
        assert_eq!(
            recovery.budget.decode_work - before.decode_work,
            super::CLEANUP_WORK
        );
        assert_eq!(
            recovery.budget.retained_candidate_bytes,
            before.retained_candidate_bytes
        );
        assert_eq!(
            recovery.budget.retained_open_handles,
            before.retained_open_handles
        );
        assert_eq!(recovery.budget.peak_open_handles, before.peak_open_handles);
        assert_eq!(
            recovery.budget.source_encoded_bytes,
            before.source_encoded_bytes
        );
        assert_eq!(recovery.budget.decoded_bytes, before.decoded_bytes);
        assert_eq!(recovery.budget.path_bytes, before.path_bytes);
        assert_eq!(recovery.budget.unique_files, before.unique_files);
        assert_eq!(recovery.work.usage(), source_work);
    }
    fixture.assert_unchanged(&frozen);
    fixture.assert_pending();

    control.release();
    let retried = recovery.retry().unwrap();
    assert!(!retried.cleanup_pending);
    assert!(!retried.uncertain);
    match phase {
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources => {
            assert_eq!(retried.phase, phase);
            assert_eq!(retried.completed_sources, failed.completed_sources + 1);
            assert!(recovery.publication.publication(0).is_some());
            fixture.assert_pending();
        }
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady => {
            assert_eq!(retried.phase, AuthoredSchemaUpgradeRecoveryPhase::Complete);
            assert_eq!(
                recovery.into_receipt().unwrap().source_count(),
                fixture.members.len()
            );
            assert_eq!(
                fs::read(fixture.directory.join("nara.toml")).unwrap(),
                fixture.ready_manifest
            );
        }
        _ => unreachable!(),
    }
}

#[cfg(windows)]
#[test]
fn source_cleanup_refusals_charge_work_and_return_scratch_without_releasing_custody() {
    assert_sustained_cleanup_refusal(AuthoredSchemaUpgradeRecoveryPhase::PublishingSources);
}

#[cfg(windows)]
#[test]
fn ready_cleanup_refusals_charge_work_and_return_scratch_without_releasing_custody() {
    assert_sustained_cleanup_refusal(AuthoredSchemaUpgradeRecoveryPhase::PublishingReady);
}

#[cfg(windows)]
#[test]
fn measured_work_limit_refuses_before_released_source_and_ready_cleanup() {
    for phase in [
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources,
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady,
    ] {
        let control_fixture = Fixture::new();
        let (mut measured, measured_control) =
            fail_publication_with_cleanup(&control_fixture, phase, complete_limits());
        let publication_work = measured.budget.decode_work;
        assert!(measured.retry().is_err());
        let maximum = measured.budget.decode_work;
        assert!(maximum > publication_work);
        measured_control.release();
        assert!(cleanup_for_fixture_teardown(&mut measured));
        drop(measured);
        drop(control_fixture);

        // Select the measured ceiling once, before inspection. It permits one blocked cleanup
        // attempt but leaves no work for another; no running operation's limits are changed.
        let fixture = Fixture::new();
        let limits = complete_limits().with_host_work(ItemLimit::new(maximum).unwrap());
        let (mut recovery, control) = fail_publication_with_cleanup(&fixture, phase, limits);
        assert_eq!(recovery.budget.decode_work, publication_work);
        assert_eq!(
            recovery.retry().unwrap_err().kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::PublicationFailed,
        );
        assert_eq!(recovery.budget.decode_work, maximum);
        let before = recovery.budget;
        let failed = recovery.view();
        let frozen = fixture.freeze();

        control.release();
        let error = recovery.retry().unwrap_err();
        assert_eq!(
            error.kind(),
            AuthoredSchemaUpgradeRecoveryErrorKind::Admission
        );
        assert!(matches!(
            cause::<AuthoringProjectAdmissionError>(error),
            Some(AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: actual })
                if *actual == maximum
        ));
        let refused = recovery.view();
        assert_eq!(refused.phase, failed.phase);
        assert_eq!(refused.operation, failed.operation);
        assert_eq!(refused.completed_sources, failed.completed_sources);
        assert_eq!(refused.bundle_complete, failed.bundle_complete);
        assert_eq!(refused.limits, limits);
        assert!(refused.cleanup_pending);
        assert!(!refused.uncertain);
        assert_eq!(
            recovery.budget.retained_candidate_bytes,
            before.retained_candidate_bytes
        );
        assert_eq!(
            recovery.budget.retained_open_handles,
            before.retained_open_handles
        );
        assert_eq!(recovery.budget.peak_open_handles, before.peak_open_handles);
        assert_eq!(
            recovery.budget.source_encoded_bytes,
            before.source_encoded_bytes
        );
        assert_eq!(recovery.budget.path_bytes, before.path_bytes);
        assert_eq!(recovery.budget.unique_files, before.unique_files);
        fixture.assert_unchanged(&frozen);
        fixture.assert_pending();

        // Direct cleanup is only fixture teardown and proves the released native operation
        // could succeed. Public retry had to refuse before consuming this same cleanup owner.
        assert!(cleanup_for_fixture_teardown(&mut recovery));
        assert!(!recovery.view().cleanup_pending);
        fixture.assert_unchanged(&frozen);
    }
}

#[cfg(windows)]
#[test]
fn retry_without_cleanup_charges_only_the_normal_source_or_ready_advance() {
    for phase in [
        AuthoredSchemaUpgradeRecoveryPhase::PublishingSources,
        AuthoredSchemaUpgradeRecoveryPhase::PublishingReady,
    ] {
        let control_fixture = Fixture::new();
        let mut measured = recovery_at_publication(&control_fixture, phase, complete_limits());
        let before = measured.budget.decode_work;
        measured.advance().unwrap();
        let advance_work = measured.budget.decode_work - before;
        drop(measured);
        drop(control_fixture);

        let fixture = Fixture::new();
        let mut recovery = recovery_at_publication(&fixture, phase, complete_limits());
        let member = match phase {
            AuthoredSchemaUpgradeRecoveryPhase::PublishingSources => &fixture.members[0],
            AuthoredSchemaUpgradeRecoveryPhase::PublishingReady => &fixture.members[3],
            _ => unreachable!(),
        };
        // A temporary in-place edit causes a refusal without creating publication cleanup.
        // Restore the original observed leaf's bytes, preserving its continuously held object.
        fs::write(fixture.directory.join(member.path), b"external conflict").unwrap();
        assert!(recovery.advance().is_err());
        assert!(!recovery.view().cleanup_pending);
        assert!(!recovery.view().uncertain);
        let restored = match phase {
            AuthoredSchemaUpgradeRecoveryPhase::PublishingSources => &member.old,
            AuthoredSchemaUpgradeRecoveryPhase::PublishingReady => &member.successor,
            _ => unreachable!(),
        };
        fs::write(fixture.directory.join(member.path), restored).unwrap();
        let before = recovery.budget.decode_work;
        recovery.retry().unwrap();
        assert_eq!(recovery.budget.decode_work - before, advance_work);
        assert!(!recovery.view().cleanup_pending);
        assert!(!recovery.view().uncertain);
    }
}
