use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId, StartupStage},
    ecs::schedule::IntoScheduleConfigs,
    prelude::{App, CoreStage, Plugin, PluginError, Res, ResMut, Resource},
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome},
};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, Scenario,
    levels::{LevelId, LevelKey},
};

#[path = "support/child.rs"]
mod child;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "two-level-consumer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn project(&self) -> PathBuf {
        let destination = self.0.join("project");
        for file in [
            "nara.toml",
            "scenes/meadow.scene.json",
            "scenes/cavern.scene.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "schema/authoring.bundle.json",
            "prefabs/README.md",
        ] {
            let path = destination.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join(file), path).unwrap();
        }
        destination
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn public_headless_process_saves_in_b_then_a_fresh_process_restores_two_instances() {
    let directory = Directory::new();
    let save = directory.0.join("checkpoint.json");
    for flag in ["--verify-save", "--verify-restore"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_headless"));
        command.arg(flag).arg(&save).current_dir(&directory.0);
        let output = child::run(command, &directory.0, flag);
        assert!(
            output.status.success(),
            "{flag}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("verified=true"));
    }
    let saved = two_level_game::save::decode_checkpoint(&fs::read(save).unwrap())
        .unwrap()
        .checkpoint;
    assert_eq!(saved.location.level, LevelKey::main(LevelId::Cavern));
    assert_eq!(saved.score, 30);
    assert_eq!(saved.collected.len(), 3);
    assert_eq!(
        saved
            .markers
            .iter()
            .map(|marker| marker.id.get())
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(
        saved
            .collected
            .iter()
            .any(|pickup| pickup.level == LevelKey::new(LevelId::Cavern, 1).unwrap())
    );
}

#[test]
fn ordinary_travel_returns_to_a_and_retry_replaces_current_b_without_losing_progress() {
    let directory = Directory::new();
    let mut command = Command::new(env!("CARGO_BIN_EXE_headless"));
    command.arg("--verify-travel");
    let output = child::run(command, &directory.0, "--verify-travel");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("score=30 collected=3"));
}

static CORRUPT: Mutex<Option<PathBuf>> = Mutex::new(None);
const MUTATOR_ID: PluginId = PluginId::new("two-level-game.test-source-change");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(MUTATOR_ID, PluginCategory::Runtime)
    .requires_plugins(&[nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID]);
#[derive(Default)]
struct CorruptAfterStartup;
impl Plugin for CorruptAfterStartup {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            StartupStage::Runtime,
            corrupt_destination.after(nara::advanced_prelude::StartupSceneActivationSet),
        )?;
        Ok(())
    }
}
fn corrupt_destination() {
    fs::write(
        CORRUPT.lock().unwrap().as_ref().unwrap(),
        b"{ malformed target",
    )
    .unwrap();
}

fn run_until_error(root: &Path, options: RunOptions, corrupt: bool) -> GameSnapshot {
    let recipe = two_level_game::configured_recipe(options);
    let recipe = if corrupt {
        recipe.add_plugin::<CorruptAfterStartup>().unwrap()
    } else {
        recipe
    };
    let intent =
        HeadlessRunIntent::frames(NonZeroU32::new(200_000).unwrap(), Duration::from_millis(20))
            .with_recipe(recipe)
            .stop_when(|snapshot: &GameSnapshot| snapshot.error.is_some());
    let mut run = HeadlessRun::new(
        two_level_game::open_explicit_project_root(root).unwrap(),
        intent,
        Vec::new(),
    );
    finish(&mut run)
}

fn finish(run: &mut HeadlessRun<GameSnapshot>) -> GameSnapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => return snapshot.clone(),
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            _ => panic!(
                "runtime did not retain a usable game: {:?}",
                report.diagnostics()
            ),
        }
    }
}

static CURRENT_B: Mutex<Option<PathBuf>> = Mutex::new(None);
static BEFORE_RETRY: Mutex<Option<nara::identity::SceneInstanceId>> = Mutex::new(None);
#[derive(Resource)]
struct DestinationChange {
    path: PathBuf,
    done: bool,
}
#[derive(Default)]
struct CorruptCommittedB;
impl Plugin for CorruptCommittedB {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-committed-source-change"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PluginId::new("two-level-game.exploration")]);
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(DestinationChange {
            path: CURRENT_B.lock().unwrap().as_ref().unwrap().clone(),
            done: false,
        })?
        .add_systems(
            CoreStage::FixedUpdate,
            change_committed_b.after(nara::gameplay::GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}
fn change_committed_b(snapshot: Res<GameSnapshot>, mut change: ResMut<DestinationChange>) {
    if !change.done
        && snapshot.level == Some(LevelKey::main(LevelId::Cavern))
        && snapshot.travel_count == 3
        && snapshot.restarts == 0
        && snapshot.phase == GamePhase::Running
    {
        *BEFORE_RETRY.lock().unwrap() = snapshot.current_instance;
        fs::write(&change.path, b"{ externally broken committed B source").unwrap();
        change.done = true;
    }
}

#[test]
fn retry_of_committed_b_uses_retained_source_even_when_the_file_has_become_invalid() {
    let directory = Directory::new();
    let project = directory.project();
    let path = project.join("scenes/cavern.scene.json");
    *CURRENT_B.lock().unwrap() = Some(path.clone());
    let recipe = two_level_game::configured_recipe(RunOptions {
        scenario: Scenario::Travel,
        save_path: None,
    })
    .add_plugin::<CorruptCommittedB>()
    .unwrap();
    let intent =
        HeadlessRunIntent::frames(NonZeroU32::new(200_000).unwrap(), Duration::from_millis(20))
            .with_recipe(recipe)
            .stop_when(|snapshot: &GameSnapshot| {
                snapshot.restarts == 1 || snapshot.error.is_some()
            });
    let mut run = HeadlessRun::new(
        two_level_game::open_explicit_project_root(&project).unwrap(),
        intent,
        Vec::new(),
    );
    let snapshot = finish(&mut run);
    assert_eq!(
        fs::read(path).unwrap(),
        b"{ externally broken committed B source"
    );
    assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_eq!(snapshot.restarts, 1);
    let old_instance = BEFORE_RETRY
        .lock()
        .unwrap()
        .expect("the source change observed committed B");
    assert!(snapshot.current_instance.is_some());
    assert_ne!(snapshot.current_instance, Some(old_instance));
    assert_eq!(snapshot.scene_members, 6);
    assert_eq!(snapshot.travel_count, 3);
    assert_eq!(snapshot.score, 30);
    assert_eq!(snapshot.collected, 3);
    assert_eq!(snapshot.visible_pickups, 1);
    assert_eq!(
        snapshot
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(snapshot.phase, GamePhase::Running);
    assert!(snapshot.error.is_none());
}

#[test]
fn target_changed_after_startup_is_rejected_while_current_a_remains_playable() {
    let directory = Directory::new();
    let project = directory.project();
    *CORRUPT.lock().unwrap() = Some(project.join("scenes/cavern.scene.json"));
    let snapshot = run_until_error(
        &project,
        RunOptions {
            scenario: Scenario::Travel,
            save_path: None,
        },
        true,
    );
    assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(snapshot.travel_count, 0);
    assert_eq!(snapshot.score, 10);
    assert_eq!(snapshot.visible_pickups, 1);
    assert_eq!(
        snapshot
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(snapshot.scene_members, 6);
    assert_eq!(snapshot.phase, GamePhase::Running);
    assert!(
        snapshot
            .error
            .as_ref()
            .is_some_and(|error| error.contains("scene-format-invalid"))
    );
}

#[test]
fn corrupt_save_keeps_current_a_and_its_progress_unchanged() {
    let directory = Directory::new();
    let save = directory.0.join("checkpoint.json");
    fs::write(&save, b"{ not a checkpoint").unwrap();
    let snapshot = run_until_error(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        RunOptions {
            scenario: Scenario::Restore,
            save_path: Some(save),
        },
        false,
    );
    assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(snapshot.score, 0);
    assert_eq!(snapshot.travel_count, 0);
    assert_eq!(snapshot.visible_pickups, 2);
    assert_eq!(snapshot.scene_members, 6);
    assert_eq!(snapshot.phase, GamePhase::Running);
    assert!(snapshot.error.is_some());
}

#[test]
fn valid_wire_with_missing_checkpoint_or_other_level_pickup_cannot_publish_game_state() {
    use two_level_game::save::{Checkpoint, CheckpointLocation, CollectedPickup, io::SaveSlot};
    for missing_anchor in [true, false] {
        let directory = Directory::new();
        let save = directory.0.join("checkpoint.json");
        let mut checkpoint = Checkpoint::new(CheckpointLocation {
            level: LevelKey::main(LevelId::Cavern),
            anchor: nara::identity::SceneEntityId::new(if missing_anchor {
                "missing-checkpoint"
            } else {
                "checkpoint"
            })
            .unwrap(),
        });
        if !missing_anchor {
            checkpoint.collected.push(CollectedPickup {
                level: LevelKey::main(LevelId::Meadow),
                entity: nara::identity::SceneEntityId::new("missing-pickup").unwrap(),
            });
            checkpoint.score = 10;
        }
        SaveSlot::open(&save).unwrap().save(&checkpoint).unwrap();
        let snapshot = run_until_error(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            RunOptions {
                scenario: Scenario::Restore,
                save_path: Some(save),
            },
            false,
        );
        assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
        assert_eq!(snapshot.score, 0);
        assert_eq!(snapshot.collected, 0);
        assert_eq!(snapshot.travel_count, 0);
        assert_eq!(snapshot.visible_pickups, 2);
        assert_eq!(snapshot.scene_members, 6);
        assert_eq!(snapshot.phase, GamePhase::Running);
        assert!(
            snapshot
                .error
                .as_ref()
                .is_some_and(|error| error.contains(if missing_anchor {
                    "checkpoint anchor"
                } else {
                    "saved pickup"
                }))
        );
    }
}

#[test]
fn valid_wire_score_inconsistent_with_pickups_is_rejected_by_the_game_owner() {
    use two_level_game::save::{Checkpoint, CheckpointLocation, io::SaveSlot};
    let directory = Directory::new();
    let path = directory.0.join("checkpoint.json");
    let mut checkpoint = Checkpoint::new(CheckpointLocation {
        level: LevelKey::main(LevelId::Cavern),
        anchor: nara::identity::SceneEntityId::new("checkpoint").unwrap(),
    });
    checkpoint.score = 10;
    SaveSlot::open(&path).unwrap().save(&checkpoint).unwrap();
    let snapshot = run_until_error(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        RunOptions {
            scenario: Scenario::Restore,
            save_path: Some(path),
        },
        false,
    );
    assert_eq!(snapshot.score, 0);
    assert_eq!(snapshot.collected, 0);
    assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(snapshot.travel_count, 0);
    assert!(
        snapshot
            .error
            .as_ref()
            .is_some_and(|error| error.contains("score must equal"))
    );
}
