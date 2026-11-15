use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    ecs::schedule::IntoScheduleConfigs,
    gameplay::{
        GameplayCommandDraft, GameplayCommandSet, GameplayCommandTarget, GameplayCommandTargetId,
    },
    prelude::{App, CoreStage, Plugin, PluginError, Resource},
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome},
};

use super::*;
use crate::{
    gameplay::{GameSnapshot, Progress, commands::SAVE},
    levels::{LevelId, LevelKey},
    save::decode_checkpoint,
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "two-level-player-launch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Frame {
    snapshot: GameSnapshot,
    interactive: bool,
    empty_slot: bool,
}

#[derive(Clone, Default, Resource)]
struct Observed {
    original_storage: Option<Storage>,
    frames: Vec<Frame>,
    saved_via_original: Option<Result<(), String>>,
}

#[derive(Default)]
struct LaunchProbe<const SAVE_VIA_ORIGINAL: bool>;

impl<const SAVE_VIA_ORIGINAL: bool> Plugin for LaunchProbe<SAVE_VIA_ORIGINAL> {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-player-launch"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[crate::gameplay::PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let storage = app.world_mut()?.resource::<Storage>().clone();
        app.insert_resource(Observed {
            original_storage: Some(storage),
            ..Observed::default()
        })?
        .add_systems(
            CoreStage::FixedUpdate,
            observe::<SAVE_VIA_ORIGINAL>.after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

fn observe<const SAVE_VIA_ORIGINAL: bool>(world: &mut World) {
    let frame = Frame {
        snapshot: world.resource::<GameSnapshot>().clone(),
        interactive: world.resource::<Verification>().is_interactive(),
        empty_slot: world.resource::<Storage>().permits_resume(),
    };
    if SAVE_VIA_ORIGINAL && frame.snapshot.tick == 1 {
        let result = world
            .resource::<Observed>()
            .original_storage
            .as_ref()
            .unwrap()
            .save(world.resource::<Progress>().checkpoint());
        world.resource_mut::<Observed>().saved_via_original = Some(result);
    }
    world.resource_mut::<Observed>().frames.push(frame);
}

fn run<const SAVE_VIA_ORIGINAL: bool>(
    options: RunOptions,
    commands: Vec<GameplayCommandSubmission>,
    ticks: u32,
) -> Observed {
    let recipe = crate::gameplay::configured_recipe(options)
        .add_plugin::<LaunchProbe<SAVE_VIA_ORIGINAL>>()
        .unwrap();
    let mut run = HeadlessRun::<Observed>::new(
        crate::open_project_root().unwrap(),
        HeadlessRunIntent::frames(NonZeroU32::new(ticks).unwrap(), Duration::from_millis(20))
            .with_recipe(recipe),
        commands,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(observed) => return observed.clone(),
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            _ => panic!("Player launch Host failed: {:?}", report.diagnostics()),
        }
    }
}

fn travel_launch() -> GameplayCommandSubmission {
    player_launch(RunOptions {
        scenario: Scenario::Travel,
        save_path: None,
    })
    .unwrap()
    .unwrap()
}

fn submit(
    draft: GameplayCommandDraft,
    tick: u64,
    source: &str,
    sequence: u64,
) -> GameplayCommandSubmission {
    GameplayCommandSubmission::new(
        GameplayCommandTick::new(tick).unwrap(),
        GameplayCommandIngressSource::external(source).unwrap(),
        GameplayCommandSourceSequence::new(sequence).unwrap(),
        draft,
    )
}

fn assert_refused(frame: &Frame) {
    assert!(frame.snapshot.error.is_some(), "{:?}", frame.snapshot);
    assert!(frame.interactive);
    assert!(frame.empty_slot);
    assert_eq!(frame.snapshot.phase, GamePhase::Running);
    assert_eq!(frame.snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(frame.snapshot.travel_count, 0);
    assert_eq!(frame.snapshot.saves, 0);
    assert_eq!(frame.snapshot.restores, 0);
}

#[test]
fn launch_encoding_is_finite_and_does_not_open_the_save_path() {
    assert!(player_launch(RunOptions::default()).unwrap().is_none());
    let directory = Directory::new();
    let path = directory.0.join("checkpoint.json");
    for scenario in [
        Scenario::Interactive,
        Scenario::Travel,
        Scenario::Save,
        Scenario::Restore,
    ] {
        let command = player_launch(RunOptions {
            scenario,
            save_path: Some(path.clone()),
        })
        .unwrap()
        .unwrap();
        assert_eq!(command.tick().get(), 1);
        assert_eq!(command.source().id().as_str(), SOURCE);
        assert_eq!(command.source_sequence().get(), 1);
        assert_eq!(command.command().command_type().as_str(), PLAYER_LAUNCH);
        assert!(command.command().target().is_none());
        assert_eq!(command.command().payload().len(), 3);
        let decoded = decode(command.command().payload()).unwrap();
        assert_eq!(decoded.scenario, scenario);
        assert_eq!(decoded.save_path.as_ref(), Some(&path));
        assert!(!path.exists());
    }
    for scenario in [Scenario::Save, Scenario::Restore] {
        assert!(
            player_launch(RunOptions {
                scenario,
                save_path: None,
            })
            .is_err()
        );
    }
    for path in [
        PathBuf::from("checkpoint.json"),
        directory.0.join("x".repeat(MAX_SAVE_PATH_BYTES)),
    ] {
        assert!(
            player_launch(RunOptions {
                scenario: Scenario::Save,
                save_path: Some(path),
            })
            .is_err()
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn non_utf8_save_paths_are_refused_without_lossy_conversion() {
    #[cfg(windows)]
    let invalid = {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        OsString::from_wide(&[0xd800])
    };
    #[cfg(unix)]
    let invalid = {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        OsString::from_vec(vec![0xff])
    };
    let error = player_launch(RunOptions {
        scenario: Scenario::Save,
        save_path: Some(std::env::temp_dir().join(invalid)),
    })
    .unwrap_err();
    assert!(error.contains("UTF-8"), "{error}");
}

#[test]
fn first_tick_launch_installs_the_original_script_before_the_next_driver_call() {
    let observed = run::<false>(RunOptions::default(), vec![travel_launch()], 2);
    assert_eq!(observed.frames.len(), 2);
    let first = &observed.frames[0];
    assert_eq!(first.snapshot.tick, 1);
    assert_eq!(first.snapshot.logical_tick, 1);
    assert!(first.snapshot.initialized);
    assert!(!first.interactive);
    assert!(first.empty_slot);
    assert!(first.snapshot.error.is_none());
    assert_eq!(first.snapshot.held, [false; 4]);
    let second = &observed.frames[1];
    assert!(second.snapshot.held[1]);
    assert!(second.snapshot.player.x > first.snapshot.player.x);
    assert!(second.snapshot.error.is_none());
}

#[test]
fn launch_precedes_earlier_sorted_ordinary_save_and_reuses_the_original_storage() {
    let directory = Directory::new();
    let path = directory.0.join("checkpoint.json");
    let launch = player_launch(RunOptions {
        scenario: Scenario::Interactive,
        save_path: Some(path.clone()),
    })
    .unwrap()
    .unwrap();
    let save = submit(action(SAVE), 1, "two-level-game.aaa-save", 1);
    let observed = run::<true>(RunOptions::default(), vec![save, launch], 1);
    let frame = &observed.frames[0];
    assert!(frame.snapshot.error.is_none(), "{:?}", frame.snapshot);
    assert!(frame.interactive);
    assert!(!frame.empty_slot);
    assert_eq!(frame.snapshot.saves, 1);
    assert_eq!(observed.saved_via_original, Some(Ok(())));
    let checkpoint = decode_checkpoint(&fs::read(path).unwrap())
        .unwrap()
        .checkpoint;
    assert_eq!(checkpoint.location.level, LevelKey::main(LevelId::Meadow));
    assert_eq!(checkpoint.score, 0);
}

#[test]
fn malformed_launch_payloads_are_refused_before_any_slot_or_script_effects() {
    let base = travel_launch().command().payload().clone();
    let mut payloads = Vec::new();
    for (key, value) in [
        ("unexpected", GameplayCommandValue::Bool(true)),
        ("version", GameplayCommandValue::I64(2)),
        ("version", GameplayCommandValue::Bool(true)),
        ("scenario", GameplayCommandValue::String("unknown".into())),
        ("scenario", GameplayCommandValue::String("save".into())),
        (
            "scenario",
            GameplayCommandValue::String("interactive".into()),
        ),
        ("scenario", GameplayCommandValue::Bool(true)),
        ("save_path", GameplayCommandValue::I64(1)),
        (
            "save_path",
            GameplayCommandValue::String("relative.json".into()),
        ),
        (
            "save_path",
            GameplayCommandValue::String("x".repeat(MAX_SAVE_PATH_BYTES + 1)),
        ),
        ("save_path", GameplayCommandValue::String("\0".into())),
    ] {
        let mut payload = base.clone();
        payload.insert(key, value).unwrap();
        payloads.push(payload);
    }
    payloads.push(GameplayCommandPayload::new());
    for payload in payloads {
        let launch = submit(action(PLAYER_LAUNCH).with_payload(payload), 1, SOURCE, 1);
        let observed = run::<false>(RunOptions::default(), vec![launch], 1);
        assert_refused(&observed.frames[0]);
    }
}

#[test]
fn duplicate_launches_in_one_batch_are_refused_before_the_first_is_applied() {
    let first = travel_launch();
    let second = submit(first.command().clone(), 1, SOURCE, 2);
    let observed = run::<false>(RunOptions::default(), vec![first, second], 1);
    assert_refused(&observed.frames[0]);
    assert!(
        observed.frames[0]
            .snapshot
            .error
            .as_ref()
            .unwrap()
            .contains("exactly once")
    );
}

#[test]
fn late_launch_and_wrong_provenance_or_target_are_refused() {
    let draft = travel_launch().command().clone();
    let targeted = draft.clone().with_target(GameplayCommandTarget::Named(
        GameplayCommandTargetId::new("unexpected-target").unwrap(),
    ));
    for command in [
        submit(draft.clone(), 2, SOURCE, 1),
        submit(draft.clone(), 1, "two-level-game.other-source", 1),
        submit(draft, 1, SOURCE, 2),
        submit(targeted, 1, SOURCE, 1),
    ] {
        let tick = command.tick().get();
        let observed = run::<false>(RunOptions::default(), vec![command], tick as u32);
        assert_refused(observed.frames.last().unwrap());
        assert_eq!(observed.frames.last().unwrap().snapshot.logical_tick, tick);
    }
}

#[test]
fn rejected_late_launch_preserves_same_batch_release_and_ordinary_save() {
    let directory = Directory::new();
    let path = directory.0.join("checkpoint.json");
    let observed = run::<false>(
        RunOptions {
            scenario: Scenario::Interactive,
            save_path: Some(path.clone()),
        },
        vec![
            submit(
                super::super::commands::movement(1, true),
                1,
                "two-level-game.test-input",
                1,
            ),
            submit(
                super::super::commands::movement(1, false),
                2,
                "two-level-game.test-input",
                2,
            ),
            submit(action(SAVE), 2, "two-level-game.aaa-save", 1),
            submit(travel_launch().command().clone(), 2, SOURCE, 1),
        ],
        3,
    );
    let [pressed, released, continued] = observed.frames.as_slice() else {
        panic!("expected three ordinary managed frames");
    };
    assert!(pressed.snapshot.held[1]);
    assert!(!released.snapshot.held[1], "{:?}", released.snapshot);
    assert_eq!(released.snapshot.player, pressed.snapshot.player);
    assert_eq!(continued.snapshot.player, released.snapshot.player);
    assert_eq!(released.snapshot.saves, 1);
    assert!(
        released
            .snapshot
            .error
            .as_ref()
            .unwrap()
            .contains("first-tick")
    );
    assert!(released.interactive && !released.empty_slot);
    let saved = decode_checkpoint(&fs::read(path).unwrap())
        .unwrap()
        .checkpoint;
    assert_eq!(saved.location.level, LevelKey::main(LevelId::Meadow));
    assert_eq!(saved.score, 0);
}

#[test]
fn configured_slot_and_its_original_owner_survive_an_incompatible_launch() {
    let directory = Directory::new();
    let original = directory.0.join("original.json");
    let replacement = directory.0.join("replacement.json");
    let launch = player_launch(RunOptions {
        scenario: Scenario::Save,
        save_path: Some(replacement.clone()),
    })
    .unwrap()
    .unwrap();
    let observed = run::<true>(
        RunOptions {
            scenario: Scenario::Interactive,
            save_path: Some(original.clone()),
        },
        vec![launch],
        1,
    );
    let frame = &observed.frames[0];
    assert!(frame.snapshot.error.is_some());
    assert!(frame.interactive);
    assert!(!frame.empty_slot);
    assert_eq!(observed.saved_via_original, Some(Ok(())));
    let checkpoint = decode_checkpoint(&fs::read(original).unwrap())
        .unwrap()
        .checkpoint;
    assert_eq!(checkpoint.location.level, LevelKey::main(LevelId::Meadow));
    assert!(!replacement.exists());
}

#[test]
fn configured_script_and_exact_state_resume_cannot_be_reinterpreted_as_player_launch() {
    let observed = run::<false>(
        RunOptions {
            scenario: Scenario::Travel,
            save_path: None,
        },
        vec![travel_launch()],
        1,
    );
    assert!(observed.frames[0].snapshot.error.is_some());
    assert!(!observed.frames[0].interactive);
    assert!(observed.frames[0].empty_slot);
    let resume = submit(
        action(crate::gameplay::resume::RESTORE_STATE),
        1,
        "two-level-game.resume",
        1,
    );
    let observed = run::<false>(RunOptions::default(), vec![travel_launch(), resume], 1);
    assert_refused(&observed.frames[0]);
}
