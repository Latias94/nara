use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::AssetSourcePath;
use nara_core::{ByteLimit, ItemLimit};
use nara_project::{ProjectManifest, ProjectPath};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SourceDocumentKind,
};

use super::*;
use crate::{
    project_host::authoring::{
        AuthoringProjectAdmissionLimits,
        source_upgrade_recovery::{
            RecoverySourceEncoding, SourceUpgradeRecoveryInput, SourceUpgradeRecoveryLimits,
            SourceUpgradeRecoveryMemberInput, SourceUpgradeRecoveryRootInput,
        },
        source_upgrade_storage::{
            RecoveryPendingPublication, RecoveryStorageHost, RecoveryStoragePublication,
        },
    },
    test_host_fs as host_fs,
};

const UNIVERSE: &str = "421112bd-693a-4e6b-85bc-318d567b2f21";
const COMPONENT: &str = "trial.enemy.EnemyStats";
const OWNER: &str = "trial.enemy";
const BUNDLE_PATH: &str = "schema/bundle.json";
const RECOVERY_PATH: &str = ".nara/recovery.json";
static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);

struct Member {
    path: &'static str,
    key: SourceDocumentKey,
    encoding: RecoverySourceEncoding,
    old: Vec<u8>,
    successor: Vec<u8>,
}

struct Fixture {
    directory: PathBuf,
    old_bundle: Vec<u8>,
    successor_bundle: Vec<u8>,
    members: Vec<Member>,
}

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "nara-recovered-publication-{}-{stamp}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&directory).unwrap();
        for parent in [".nara", "schema", "assets", "prefabs", "scenes", "z-extra"] {
            fs::create_dir(directory.join(parent)).unwrap();
        }
        fs::create_dir(directory.join("z-extra/scenes")).unwrap();
        let manifest = format!(
            "schema_version = 1\n[project]\nname = 'Recovered publication fixture'\n\
             source_universe = '{UNIVERSE}'\nnative_document_identity = 'project_location'\n\
             [authoring]\nstate = 'ready'\nschema_bundle = '{BUNDLE_PATH}'\n\
             [startup]\ndefault_scene = 'pending.scene.json'\n\
             [profiles.inactive.paths]\nscenes = 'z-extra/scenes'\n\
             [profiles.inactive.startup]\ndefault_scene = 'closed.scene.json'\n"
        )
        .into_bytes();
        let future_manifest = ProjectManifest::parse_toml_bytes(&manifest)
            .manifest
            .unwrap()
            .to_canonical_toml_bytes()
            .unwrap();
        let old_catalog = ComponentSchemaCatalog {
            components: vec![schema(false)],
            ..Default::default()
        };
        let mut successor_catalog = ComponentSchemaCatalog::successor_of(&old_catalog).unwrap();
        successor_catalog.components.push(schema(true));
        let old_bundle = bundle(old_catalog.clone(), None);
        let successor_bundle = bundle(successor_catalog, Some(old_catalog));
        let members = vec![
            member(
                "prefabs/already.prefab.ron",
                SourceDocumentKind::Prefab,
                Some(105),
            ),
            member(
                "scenes/pending.scene.json",
                SourceDocumentKind::Scene,
                Some(7),
            ),
            member(
                "scenes/unchanged.scene.json",
                SourceDocumentKind::Scene,
                None,
            ),
            member(
                "z-extra/scenes/closed.scene.json",
                SourceDocumentKind::Scene,
                Some(13),
            ),
        ];
        fs::write(directory.join("nara.toml"), &manifest).unwrap();
        fs::write(directory.join(BUNDLE_PATH), &old_bundle).unwrap();
        for member in &members {
            fs::write(directory.join(member.path), &member.old).unwrap();
        }
        let mut budget = budget();
        let inputs: Vec<_> = members
            .iter()
            .map(|member| SourceUpgradeRecoveryMemberInput {
                path: member.path,
                key: &member.key,
                encoding: member.encoding,
                old_disk: &member.old,
                reviewed_working: &member.old,
                successor: &member.successor,
            })
            .collect();
        let missing: Vec<_> = members
            .iter()
            .map(|member| format!("{}.meta", member.path))
            .collect();
        let missing: Vec<_> = missing.iter().map(String::as_str).collect();
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
                future_ready_manifest: &future_manifest,
                bundle_path: BUNDLE_PATH,
                old_bundle: &old_bundle,
                successor_bundle: &successor_bundle,
                component: &ComponentTypeId::new(COMPONENT),
                field: &ComponentFieldId::new("health"),
                successor_property: &ComponentPropertyPath::new(ComponentFieldId::new("health"))
                    .with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                        "milli",
                    ))]),
                members: &inputs,
                metadata: &[],
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
            _ => panic!("fixture recovery was not published"),
        };
        match stored
            .bind_pending(&mut budget)
            .unwrap()
            .publish(&mut budget)
        {
            RecoveryPendingPublication::Published(pending) => drop(pending),
            _ => panic!("fixture Pending was not published"),
        }
        // No live Schema receipt, Workspace, original inventory or publication owner survives.
        Self {
            directory,
            old_bundle,
            successor_bundle,
            members,
        }
    }

    fn recover(
        &self,
        limits: SceneFileLimits,
    ) -> (
        ObservedPendingRecoveryStorage,
        RecoveredSourceUpgradeInventory,
        AuthoringProjectAdmissionBudget,
    ) {
        let mut budget = budget();
        let pending = RecoveryStorageHost::load_pending(
            host_fs::writable_directory(&self.directory).unwrap(),
            SourceUpgradeRecoveryLimits::default(),
            &mut budget,
        )
        .unwrap();
        let recovered =
            RecoveredSourceUpgradeInventory::capture(&pending, limits, &mut budget).unwrap();
        (pending, recovered, budget)
    }

    fn replace(&self, path: &str, bytes: &[u8]) {
        let root = host_fs::writable_directory(&self.directory).unwrap();
        let file = root
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        let digest = file.digest(1_048_576).unwrap();
        let mut publisher =
            FilePublicationHost::bind_existing(&root, path, &file, digest, 1_048_576).unwrap();
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

    fn assert_sources(&self, successor: bool) {
        for member in &self.members {
            let expected = if successor {
                &member.successor
            } else {
                &member.old
            };
            assert_eq!(
                fs::read(self.directory.join(member.path)).unwrap(),
                expected.as_slice(),
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Remove only the fixed members of this uniquely created test directory.
        for path in [RECOVERY_PATH, BUNDLE_PATH, "nara.toml"] {
            let _ = fs::remove_file(self.directory.join(path));
        }
        for member in &self.members {
            let _ = fs::remove_file(self.directory.join(member.path));
        }
        for path in [
            ".nara",
            "schema",
            "assets",
            "prefabs",
            "scenes",
            "z-extra/scenes",
            "z-extra",
        ] {
            let _ = fs::remove_dir(self.directory.join(path));
        }
        let _ = fs::remove_dir(&self.directory);
    }
}

fn budget() -> AuthoringProjectAdmissionBudget {
    let mut limits = AuthoringProjectAdmissionLimits::default();
    limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    limits.peak_open_handles = ItemLimit::new(128).unwrap();
    AuthoringProjectAdmissionBudget::new(limits)
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

fn member(path: &'static str, kind: SourceDocumentKind, value: Option<i64>) -> Member {
    let encoding = if path.ends_with(".ron") {
        RecoverySourceEncoding::Ron
    } else {
        RecoverySourceEncoding::Json
    };
    let encode = |successor| {
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
            SourceDocumentKind::Prefab => PrefabDocument::new(scene.placements, scene.entities)
                .to_ron_string()
                .unwrap()
                .into_bytes(),
        }
    };
    Member {
        path,
        key: SourceDocumentKey::project_location(
            UNIVERSE.parse().unwrap(),
            AssetSourcePath::new(path).unwrap(),
            kind,
        ),
        encoding,
        old: encode(false),
        successor: encode(true),
    }
}

#[test]
fn recovered_mixed_sources_write_only_old_members_and_never_invent_skipped_publication() {
    for bundle_is_new in [false, true] {
        let fixture = Fixture::new();
        fixture.replace(fixture.members[0].path, &fixture.members[0].successor);
        if bundle_is_new {
            fixture.replace(BUNDLE_PATH, &fixture.successor_bundle);
        }
        let (pending, recovered, mut budget) = fixture.recover(SceneFileLimits::default());
        let already_new = recovered
            .inventory()
            .participants()
            .next()
            .unwrap()
            .file()
            .identity();
        let unchanged = recovered
            .inventory()
            .participants()
            .nth(2)
            .unwrap()
            .file()
            .identity();
        let bundle = recovered.bundle_file().identity();
        let handles = budget.usage().retained_open_handles();
        let mut publication =
            SourceUpgradePublication::prepare_recovered(&pending, &recovered, &mut budget).unwrap();
        assert_eq!(budget.usage().retained_open_handles(), handles);
        // The publisher, not the captured corpus, now keeps every original ancestor and leaf.
        drop(recovered);
        for index in 0..=fixture.members.len() {
            let skip =
                index == 0 || index == 2 || (index == fixture.members.len() && bundle_is_new);
            let step = publication.publish_next(&pending, &mut budget);
            if skip {
                assert!(
                    matches!(step, SourceUpgradePublicationStep::Observed { index: actual }
                        if actual == index)
                );
                assert!(publication.publication(index).is_none());
            } else {
                assert!(
                    matches!(step, SourceUpgradePublicationStep::Published { index: actual }
                        if actual == index)
                );
                assert!(publication.publication(index).is_some());
            }
        }
        publication
            .validate_complete(&pending, &mut budget)
            .unwrap();
        fixture.assert_sources(true);
        assert_eq!(
            fs::read(fixture.directory.join(BUNDLE_PATH)).unwrap(),
            fixture.successor_bundle
        );
        for (index, expected) in [(0, already_new), (2, unchanged)] {
            let observed = publication.observe_source(index, &mut budget).unwrap();
            assert_eq!(observed.file.identity(), expected);
            assert_eq!(observed.source_key(), &fixture.members[index].key);
            assert_eq!(
                observed.source.digest(),
                ContentDigest::of_bytes(&fixture.members[index].successor)
            );
            drop(observed);
            budget.release_open_handles(1);
        }
        let actual_bundle = pending
            .root()
            .open_file_exact(&RelativePath::new(BUNDLE_PATH).unwrap())
            .unwrap();
        assert_eq!(actual_bundle.identity() == bundle, bundle_is_new);
        assert!(matches!(
            publication.publish_next(&pending, &mut budget),
            SourceUpgradePublicationStep::Complete
        ));
    }
}

#[test]
fn recovered_all_new_set_uses_fresh_saved_observations_without_historical_writes() {
    let fixture = Fixture::new();
    for member in &fixture.members {
        if member.old != member.successor {
            fixture.replace(member.path, &member.successor);
        }
    }
    fixture.replace(BUNDLE_PATH, &fixture.successor_bundle);
    let (pending, recovered, mut budget) = fixture.recover(SceneFileLimits::default());
    let original = recovered
        .inventory()
        .participants()
        .next()
        .unwrap()
        .file()
        .identity();
    let mut publication =
        SourceUpgradePublication::prepare_recovered(&pending, &recovered, &mut budget).unwrap();
    while !publication.is_complete() {
        assert!(matches!(
            publication.publish_next(&pending, &mut budget),
            SourceUpgradePublicationStep::Observed { .. }
        ));
    }
    assert!((0..=fixture.members.len()).all(|index| publication.publication(index).is_none()));
    fixture.replace(fixture.members[0].path, &fixture.members[0].successor);
    publication
        .validate_complete(&pending, &mut budget)
        .unwrap();
    let actual = publication.observe_source(0, &mut budget).unwrap();
    assert_ne!(actual.file.identity(), original);
    assert_eq!(
        actual.source.digest(),
        ContentDigest::of_bytes(&fixture.members[0].successor)
    );
    assert!(publication.publication(0).is_none());
}

#[test]
fn recovered_skipped_member_still_checks_the_exact_captured_leaf_before_observing() {
    let fixture = Fixture::new();
    fixture.replace(fixture.members[0].path, &fixture.members[0].successor);
    let (pending, recovered, mut budget) = fixture.recover(SceneFileLimits::default());
    let mut publication =
        SourceUpgradePublication::prepare_recovered(&pending, &recovered, &mut budget).unwrap();
    // Equal successor bytes on another incarnation cannot satisfy the captured expected leaf.
    fixture.replace(fixture.members[0].path, &fixture.members[0].successor);
    assert!(matches!(
        publication.publish_next(&pending, &mut budget),
        SourceUpgradePublicationStep::Refused {
            index: 0,
            error: SourceUpgradePublicationError::SourceChanged,
        },
    ));
    assert_eq!(publication.completed(), 0);
    assert!(publication.publication(0).is_none());
    for member in &fixture.members[1..] {
        assert_eq!(
            fs::read(fixture.directory.join(member.path)).unwrap(),
            member.old
        );
    }
    assert_eq!(
        fs::read(fixture.directory.join(BUNDLE_PATH)).unwrap(),
        fixture.old_bundle
    );
}

#[test]
fn recovered_bundle_conflict_preserves_completed_source_evidence() {
    let fixture = Fixture::new();
    let (pending, recovered, mut budget) = fixture.recover(SceneFileLimits::default());
    let mut publication =
        SourceUpgradePublication::prepare_recovered(&pending, &recovered, &mut budget).unwrap();
    for _ in &fixture.members {
        assert!(matches!(
            publication.publish_next(&pending, &mut budget),
            SourceUpgradePublicationStep::Published { .. }
                | SourceUpgradePublicationStep::Observed { .. }
        ));
    }
    fixture.replace(BUNDLE_PATH, &fixture.old_bundle);
    assert!(matches!(
        publication.publish_next(&pending, &mut budget),
        SourceUpgradePublicationStep::Refused {
            index,
            error: SourceUpgradePublicationError::SourceChanged,
        } if index == fixture.members.len(),
    ));
    assert_eq!(publication.completed(), fixture.members.len());
    assert!(publication.publication(0).is_some());
    assert!(publication.publication(1).is_some());
    assert!(publication.publication(2).is_none());
    assert!(publication.publication(fixture.members.len()).is_none());
    fixture.assert_sources(true);
    assert_eq!(
        fs::read(fixture.directory.join(BUNDLE_PATH)).unwrap(),
        fixture.old_bundle
    );
}

#[test]
fn recovered_inventory_cannot_be_paired_with_another_pending_capability_root() {
    let fixture = Fixture::new();
    let other = Fixture::new();
    let (pending, recovered, mut budget) = fixture.recover(SceneFileLimits::default());
    let (other_pending, _, _) = other.recover(SceneFileLimits::default());
    assert_eq!(
        pending.recovery().digest(),
        other_pending.recovery().digest()
    );
    assert!(matches!(
        SourceUpgradePublication::prepare_recovered(&other_pending, &recovered, &mut budget),
        Err(SourceUpgradePublicationError::Inventory(
            SourceUpgradeInventoryError::AuthorityChanged
        ))
    ));
    fixture.assert_sources(false);
    other.assert_sources(false);
}

#[test]
fn recovered_publication_keeps_the_actual_source_file_limit() {
    let fixture = Fixture::new();
    let current_limit = fixture
        .members
        .iter()
        .map(|member| member.old.len())
        .max()
        .unwrap();
    assert!(fixture.members[0].successor.len() > current_limit);
    let limits =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(current_limit).unwrap());
    let (pending, recovered, mut budget) = fixture.recover(limits);
    let mut publication =
        SourceUpgradePublication::prepare_recovered(&pending, &recovered, &mut budget).unwrap();
    assert!(matches!(
        publication.publish_next(&pending, &mut budget),
        SourceUpgradePublicationStep::Failed {
            index: 0,
            stage: FilePublicationFailureStage::PayloadTooLarge,
        }
    ));
    assert_eq!(publication.completed(), 0);
    assert!(publication.publication(0).is_none());
    fixture.assert_sources(false);
    assert_eq!(
        fs::read(fixture.directory.join(BUNDLE_PATH)).unwrap(),
        fixture.old_bundle
    );
}
