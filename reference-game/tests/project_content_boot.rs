#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::project_host::{ProjectContentBudgetKind, ProjectContentMemberKind};
use nara_reference_game::REFERENCE_GAME_SCHEMA_OWNER_ID;
use project_content_fixture::{load_project_content, texture_stable_id};

const CHILD_MARKER: &str = "NARA_REFERENCE_CONTENT_CHILD";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn committed_project_content_boots_from_random_process_environment() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        assert_committed_content();
        return;
    }

    let environment = TemporaryEnvironment::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("committed_project_content_boots_from_random_process_environment")
        .arg("--nocapture")
        .current_dir(environment.cwd())
        .env(CHILD_MARKER, "1")
        .env("HOME", environment.home())
        .env("USERPROFILE", environment.home())
        .env("XDG_CONFIG_HOME", environment.home().join("config"))
        .env("XDG_DATA_HOME", environment.home().join("data"))
        .env("XDG_CACHE_HOME", environment.home().join("cache"))
        .env_remove("NARA_REFERENCE_GAME_MANIFEST")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_committed_content() {
    let loaded = load_project_content();
    let snapshot = loaded.snapshot;
    assert_eq!(snapshot.lineage(), loaded.candidate.lineage());
    assert_eq!(snapshot.lineage(), loaded.plan.lineage());
    assert_eq!(
        snapshot.schema_fingerprint(),
        loaded.plan.schema_validation().composition_fingerprint(),
    );
    assert_eq!(
        loaded
            .plan
            .schema_validation()
            .owner_receipts()
            .find(|receipt| receipt.owner() == REFERENCE_GAME_SCHEMA_OWNER_ID)
            .map(|receipt| receipt.generation()),
        Some(5),
    );
    assert!(!snapshot.source_upgrade_required());
    assert_eq!(snapshot.revision().to_hex().len(), 64);
    assert_eq!(snapshot.prefabs().len(), 2);
    assert_eq!(
        snapshot
            .prefabs()
            .iter()
            .map(|prefab| prefab.path().as_str())
            .collect::<Vec<_>>(),
        ["enemy.prefab.json", "reference-ui.prefab.json"],
    );
    assert_eq!(snapshot.images().len(), 1);
    assert_eq!(
        snapshot.images()[0].path().as_str(),
        "textures/tiny-dungeon.png"
    );
    assert_eq!(
        snapshot.images()[0].image().source().stable_id(),
        texture_stable_id(),
    );
    assert_eq!(snapshot.images()[0].image().extent().width, 192);
    assert_eq!(snapshot.images()[0].image().extent().height, 176);
    assert_eq!(snapshot.images()[0].image().pixels().len(), 192 * 176 * 4);
    assert_eq!(
        &snapshot.images()[0].image().pixels()[..4],
        &[118, 59, 54, 255],
    );

    let before_clone = loaded.loader.budget_snapshot();
    let cloned = snapshot.clone();
    assert_eq!(loaded.loader.budget_snapshot(), before_clone);
    assert!(std::ptr::eq(snapshot.scene(), cloned.scene(),));
    assert!(std::ptr::eq(
        snapshot.expanded_scene(),
        cloned.expanded_scene(),
    ));
    assert!(
        snapshot
            .prefabs()
            .iter()
            .zip(cloned.prefabs())
            .all(|(source, clone)| std::ptr::eq(source.document(), clone.document()))
    );
    assert_eq!(
        snapshot.images()[0].image().pixels().as_ptr(),
        cloned.images()[0].image().pixels().as_ptr(),
    );
    assert_eq!(
        snapshot
            .startup_content_closure()
            .expect("the loader selected the committed startup Scene")
            .members()
            .iter()
            .map(|member| (member.kind(), member.path().as_str().to_owned()))
            .collect::<Vec<_>>(),
        [
            (
                ProjectContentMemberKind::AssetSource,
                "assets/textures/tiny-dungeon.png".to_owned()
            ),
            (
                ProjectContentMemberKind::AssetMetadata,
                "assets/textures/tiny-dungeon.png.meta".to_owned()
            ),
            (
                ProjectContentMemberKind::ProjectManifest,
                "nara.toml".to_owned()
            ),
            (
                ProjectContentMemberKind::Prefab,
                "prefabs/enemy.prefab.json".to_owned()
            ),
            (
                ProjectContentMemberKind::Prefab,
                "prefabs/reference-ui.prefab.json".to_owned()
            ),
            (
                ProjectContentMemberKind::Scene,
                "scenes/startup.scene.json".to_owned()
            ),
        ],
    );
    // The content loader charges six reads: Scene + two Prefabs + image source + image
    // metadata discovery + image metadata admission/digest revalidation. Manifest ingestion
    // predates this loader; schema catalogs and unrelated images are outside this closure.
    assert_eq!(before_clone.high_water(ProjectContentBudgetKind::Files), 6);
    assert_eq!(
        before_clone.high_water(ProjectContentBudgetKind::DependencyEdges),
        8,
    );
    // Prefabs load on synchronous compiler demand; only the discovered image is queued.
    assert_eq!(
        before_clone.high_water(ProjectContentBudgetKind::QueuedJobs),
        1,
    );
    assert_eq!(
        before_clone.high_water(ProjectContentBudgetKind::InFlightJobs),
        1,
    );

    drop(snapshot);
    assert_eq!(loaded.loader.budget_snapshot(), before_clone);
    drop(cloned);
    assert_eq!(loaded.loader.budget_snapshot().active_reservations(), 0);
}

struct TemporaryEnvironment {
    root: PathBuf,
    cwd: PathBuf,
    home: PathBuf,
}

impl TemporaryEnvironment {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_reference_content_environment_{}_{}",
            std::process::id(),
            sequence,
        ));
        let cwd = root.join("cwd");
        let home = root.join("home");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(home.join("config")).unwrap();
        fs::create_dir_all(home.join("data")).unwrap();
        fs::create_dir_all(home.join("cache")).unwrap();
        Self { root, cwd, home }
    }

    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn home(&self) -> &Path {
        &self.home
    }
}

impl Drop for TemporaryEnvironment {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
