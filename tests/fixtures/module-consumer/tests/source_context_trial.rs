//! Executable owner-design experiment. This does not qualify the production project/UI adapters.

#[path = "support/source_context_trial.rs"]
mod trial;

use std::{fs, path::PathBuf};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_identity::EntityLookup;
use nara_scene::{SceneDocument, SceneEntityId, SceneEntityRecord};
use trial::{Atom, BoundScene, Error, FileSources, Occurrence, Product, TrialHost, Universe};

const UNIVERSE: &str = "8dfe3c41-46ae-4ddb-a194-e7629d8aec28";
const OTHER_UNIVERSE: &str = "e75c7791-1371-4f26-a982-6c594b39099f";
const ASSET: &str = "2e78c2d1-82e6-40ee-96f2-8dcd28a6e3e1";
const SOURCE_PATH: &str = "scenes/second.scene.json";

fn universe() -> Universe {
    Universe::parse(UNIVERSE).unwrap()
}

fn local() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}

fn document() -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(local())])
}

struct Project {
    temporary: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new(policy: &str) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("original");
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::write(root.join(SOURCE_PATH), document().to_json_string().unwrap()).unwrap();
        let result = Self { temporary, root };
        result.config(UNIVERSE, policy);
        result
    }

    fn config(&self, universe: &str, policy: &str) {
        fs::write(self.root.join("nara.toml"), format!(
            "[project]\nsource_universe = \"{universe}\"\n[content]\nnative_document_identity = \"{policy}\"\n"
        )).unwrap();
    }

    fn metadata(&self, path: &str, kind: AssetSourceKind) {
        let path = AssetPath::new(path).unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(StableAssetId::parse_str(ASSET).unwrap(), path.clone(), kind),
            (),
        );
        fs::write(
            self.root.join(path.meta_path()),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    fn move_root(&mut self) {
        let destination = self.temporary.path().join("relocated");
        fs::rename(&self.root, &destination).unwrap();
        self.root = destination;
    }
}

#[test]
fn direct_and_location_sources_share_one_intent_replace_end_workflow() {
    let project = Project::new("project_location");
    let original_config = fs::read(project.root.join("nara.toml")).unwrap();
    let original_scene = fs::read(project.root.join(SOURCE_PATH)).unwrap();
    let request = AssetRef::path(SOURCE_PATH).unwrap();
    let mut host = TrialHost::new();
    let direct = BoundScene::named(universe(), "hud", document()).unwrap();
    let mut loader = FileSources::open(&project.root).unwrap();
    let file = loader.load(&request, host.registry()).unwrap();
    let left = Occurrence::new(universe(), "left").unwrap();
    let right = Occurrence::new(universe(), "right").unwrap();

    let first = host
        .commit(host.intent(&direct, &left, None).unwrap())
        .unwrap();
    let second = host
        .commit(host.intent(&file, &right, None).unwrap())
        .unwrap();
    assert_ne!(first.key(local()).unwrap(), second.key(local()).unwrap());
    assert_ne!(
        first.key(local()).unwrap().encode_exact(),
        second.key(local()).unwrap().encode_exact()
    );
    assert!(matches!(
        first.receipt.resolve(host.world(), &local()),
        EntityLookup::Resolved(_)
    ));
    assert!(matches!(
        second.receipt.resolve(host.world(), &local()),
        EntityLookup::Resolved(_)
    ));
    assert!(matches!(
        host.intent(&file, &left, None),
        Err(Error::Occupied)
    ));

    let stale_intent = host.intent(&file, &left, Some(&first)).unwrap();
    let rebuilt = host
        .commit(host.intent(&direct, &left, Some(&first)).unwrap())
        .unwrap();
    assert_eq!(first.key(local()).unwrap(), rebuilt.key(local()).unwrap());
    assert_eq!(
        first.key(local()).unwrap().encode_exact(),
        rebuilt.key(local()).unwrap().encode_exact()
    );
    assert_ne!(first.receipt.instance_id(), rebuilt.receipt.instance_id());
    assert!(matches!(
        first.receipt.resolve(host.world(), &local()),
        EntityLookup::Tombstoned(_)
    ));
    assert!(matches!(host.commit(stale_intent), Err(Error::Stale)));
    assert!(matches!(host.end(&first), Err(Error::Stale)));

    let switched = host
        .commit(host.intent(&file, &left, Some(&rebuilt)).unwrap())
        .unwrap();
    assert_ne!(
        rebuilt.key(local()).unwrap(),
        switched.key(local()).unwrap()
    );
    assert_ne!(switched.key(local()).unwrap(), second.key(local()).unwrap());
    assert_eq!(host.end(&switched).unwrap(), 1);
    assert!(matches!(
        second.receipt.resolve(host.world(), &local()),
        EntityLookup::Resolved(_)
    ));
    assert_eq!(host.end(&second).unwrap(), 1);
    assert_eq!(
        fs::read(project.root.join("nara.toml")).unwrap(),
        original_config
    );
    assert_eq!(
        fs::read(project.root.join(SOURCE_PATH)).unwrap(),
        original_scene
    );
}

#[test]
fn empty_occurrences_are_exclusive_and_scene_rejection_keeps_the_predecessor() {
    let mut host = TrialHost::new();
    let empty = BoundScene::named(universe(), "empty", SceneDocument::default()).unwrap();
    let slot = Occurrence::new(universe(), "empty-slot").unwrap();
    let foreign_intent = host.intent(&empty, &slot, None).unwrap();
    assert!(matches!(
        TrialHost::new().commit(foreign_intent),
        Err(Error::WrongWorld)
    ));
    let active = host
        .commit(host.intent(&empty, &slot, None).unwrap())
        .unwrap();
    assert!(active.receipt.is_empty());
    assert!(matches!(
        host.intent(&empty, &slot, None),
        Err(Error::Occupied)
    ));
    let replacement = host
        .commit(host.intent(&empty, &slot, Some(&active)).unwrap())
        .unwrap();
    assert_ne!(
        active.receipt.instance_id(),
        replacement.receipt.instance_id()
    );
    assert!(matches!(host.end(&active), Err(Error::Stale)));

    let invalid =
        BoundScene::named(universe(), "invalid", SceneDocument::new([local()], [])).unwrap();
    let error = host
        .commit(host.intent(&invalid, &slot, Some(&replacement)).unwrap())
        .unwrap_err();
    let Error::Scene(diagnostics) = error else {
        panic!("expected Scene admission failure")
    };
    assert!(diagnostics.has_errors());
    assert!(matches!(
        host.intent(&empty, &slot, None),
        Err(Error::Occupied)
    ));
    assert_eq!(host.end(&replacement).unwrap(), 0);
    let successor = host
        .commit(host.intent(&empty, &slot, None).unwrap())
        .unwrap();
    assert!(matches!(host.end(&replacement), Err(Error::Stale)));
    assert_eq!(host.end(&successor).unwrap(), 0);
}

#[test]
fn file_continuity_uses_the_authored_universe_and_not_the_project_directory() {
    let mut project = Project::new("project_location");
    let host = TrialHost::new();
    let request = AssetRef::path(SOURCE_PATH).unwrap();
    let original = FileSources::open(&project.root)
        .unwrap()
        .load(&request, host.registry())
        .unwrap();
    fs::create_dir_all(project.root.join(".nara/import-cache")).unwrap();
    fs::write(
        project.root.join(".nara/import-cache/disposable"),
        b"cache is not identity",
    )
    .unwrap();
    fs::remove_file(project.root.join(".nara/import-cache/disposable")).unwrap();
    project.move_root();
    let reopened = FileSources::open(&project.root)
        .unwrap()
        .load(&request, host.registry())
        .unwrap();
    assert_eq!(original.key(), reopened.key());
    fs::rename(
        project.root.join(SOURCE_PATH),
        project.root.join("scenes/renamed.scene.json"),
    )
    .unwrap();
    let renamed = FileSources::open(&project.root)
        .unwrap()
        .load(
            &AssetRef::path("scenes/renamed.scene.json").unwrap(),
            host.registry(),
        )
        .unwrap();
    assert_ne!(original.key(), renamed.key());
    project.config(OTHER_UNIVERSE, "project_location");
    let fork = FileSources::open(&project.root)
        .unwrap()
        .load(
            &AssetRef::path("scenes/renamed.scene.json").unwrap(),
            host.registry(),
        )
        .unwrap();
    assert_ne!(renamed.key(), fork.key());
}

#[test]
fn stable_aliases_preserve_product_and_refuse_metadata_loss_or_policy_changes() {
    let project = Project::new("stable_metadata");
    project.metadata(SOURCE_PATH, AssetSourceKind::Scene);
    let host = TrialHost::new();
    let path = AssetRef::path(SOURCE_PATH).unwrap();
    let stable = AssetRef::stable_id(ASSET).unwrap();
    let mut loader = FileSources::open(&project.root).unwrap();
    let by_path = loader.load(&path, host.registry()).unwrap();
    let by_id = loader.load(&stable, host.registry()).unwrap();
    assert_eq!(by_path.key(), by_id.key());
    assert_eq!(by_path.key().encode_exact(), by_id.key().encode_exact());
    let mut product = by_id.key().clone();
    product.product = Product::Named(Atom::new("extra-document").unwrap());
    assert_ne!(by_id.key(), &product);
    assert_ne!(by_id.key().encode_exact(), product.encode_exact());

    let other_project = Project::new("stable_metadata");
    other_project.config(OTHER_UNIVERSE, "stable_metadata");
    other_project.metadata(SOURCE_PATH, AssetSourceKind::Scene);
    let imported_elsewhere = FileSources::open(&other_project.root)
        .unwrap()
        .load(&stable, host.registry())
        .unwrap();
    assert_eq!(by_id.key(), imported_elsewhere.key());
    project.metadata(SOURCE_PATH, AssetSourceKind::Prefab);
    assert!(matches!(
        loader.load(&path, host.registry()),
        Err(Error::Metadata)
    ));
    project.metadata(SOURCE_PATH, AssetSourceKind::Scene);
    fs::remove_file(project.root.join(format!("{SOURCE_PATH}.meta"))).unwrap();
    assert!(matches!(
        loader.load(&path, host.registry()),
        Err(Error::Metadata)
    ));
    assert!(matches!(
        FileSources::open(&project.root)
            .unwrap()
            .load(&stable, host.registry()),
        Err(Error::Metadata)
    ));

    project.metadata(SOURCE_PATH, AssetSourceKind::Scene);
    project.config(UNIVERSE, "project_location");
    assert!(matches!(
        FileSources::open(&project.root)
            .unwrap()
            .load(&path, host.registry()),
        Err(Error::PolicyConflict)
    ));
    project.config("00000000-0000-0000-0000-000000000000", "project_location");
    assert!(matches!(
        FileSources::open(&project.root),
        Err(Error::Context)
    ));
}

#[test]
fn stable_path_admission_outside_the_scanned_root_keeps_owner_proven_aliases() {
    let project = Project::new("stable_metadata");
    let host = TrialHost::new();
    project.metadata(SOURCE_PATH, AssetSourceKind::Scene);
    let original = FileSources::open(&project.root)
        .unwrap()
        .load(&AssetRef::stable_id(ASSET).unwrap(), host.registry())
        .unwrap();
    fs::create_dir(project.root.join("browser-documents")).unwrap();
    let moved = "browser-documents/moved.scene.json";
    fs::rename(project.root.join(SOURCE_PATH), project.root.join(moved)).unwrap();
    fs::remove_file(project.root.join(format!("{SOURCE_PATH}.meta"))).unwrap();
    project.metadata(moved, AssetSourceKind::Scene);

    let mut reopened = FileSources::open(&project.root).unwrap();
    let by_path = reopened
        .load(&AssetRef::path(moved).unwrap(), host.registry())
        .unwrap();
    let by_id = reopened
        .load(&AssetRef::stable_id(ASSET).unwrap(), host.registry())
        .unwrap();
    assert_eq!(original.key(), by_path.key());
    assert_eq!(by_path.key(), by_id.key());
}

#[test]
fn ui_payload_budget_failure_does_not_claim_to_undo_scene_publication() {
    let mut host = TrialHost::new();
    let source = BoundScene::named(universe(), &"x".repeat(230), document()).unwrap();
    let slot = Occurrence::new(universe(), "slot").unwrap();
    let active = host
        .commit(host.intent(&source, &slot, None).unwrap())
        .unwrap();
    let Error::KeyLimit { actual } = active.key(local()).unwrap_err() else {
        panic!("key must exceed the trial envelope")
    };
    assert_eq!(actual, 1 + 1 + 16 + 4 + 230 + 1 + 16 + 4 + 4 + 4 + 4);
    assert!(matches!(
        active.receipt.resolve(host.world(), &local()),
        EntityLookup::Resolved(_)
    ));
    assert_eq!(host.end(&active).unwrap(), 1);
}

#[test]
#[ignore = "explicit owner-trial timing; not a release benchmark"]
fn measure_trial_intent_and_replace_costs() {
    use std::time::Instant;

    let mut host = TrialHost::new();
    let source = BoundScene::named(universe(), "hud", document()).unwrap();
    let slot = Occurrence::new(universe(), "slot").unwrap();
    let mut active = host
        .commit(host.intent(&source, &slot, None).unwrap())
        .unwrap();
    let mut intent_ns = Vec::new();
    let mut commit_ns = Vec::new();
    for index in 0..80 {
        let started = Instant::now();
        let intent = host.intent(&source, &slot, Some(&active)).unwrap();
        let elapsed_intent = started.elapsed().as_nanos();
        let started = Instant::now();
        active = host.commit(intent).unwrap();
        if index >= 16 {
            intent_ns.push(elapsed_intent);
            commit_ns.push(started.elapsed().as_nanos());
        }
    }
    intent_ns.sort_unstable();
    commit_ns.sort_unstable();
    println!(
        "trial debug samples=64; payload_bytes={}; intent_ns p50={} p95={}; commit_ns p50={} p95={}",
        active.key(local()).unwrap().encode_exact().len(),
        intent_ns[31],
        intent_ns[60],
        commit_ns[31],
        commit_ns[60]
    );
    assert!(matches!(
        active.receipt.resolve(host.world(), &local()),
        EntityLookup::Resolved(_)
    ));
    assert_eq!(host.end(&active).unwrap(), 1);
}
