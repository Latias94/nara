use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use collect_gameplay::{Direction, GameSnapshot, demonstration_commands, movement};
use nara::{
    gameplay::{
        GameplayCommandIngressSource, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick,
    },
    project_host::{HeadlessRun, HeadlessRunOutcome},
};

fn run(ticks: u32, commands: Vec<GameplayCommandSubmission>) -> GameSnapshot {
    let mut run = HeadlessRun::<GameSnapshot>::from_recipe(
        nara_game::open_project_root().expect("open sample source"),
        nara_game::product_recipe(),
        NonZeroU32::new(ticks).unwrap(),
        commands,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => return snapshot.clone(),
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            _ => panic!("game run failed: {:?}", report.diagnostics()),
        }
    }
}

#[test]
fn movement_collects_all_three_authored_targets_and_changes_the_background() {
    let initial = run(1, Vec::new());
    assert_eq!(initial.player, nara::core::Vec2::new(-120.0, 0.0));
    assert_eq!(initial.visible_targets, 3);
    assert_eq!(initial.collected, 0);
    assert!(!initial.won);

    let completed = run(62, demonstration_commands(false));
    assert!(completed.player.x > initial.player.x);
    assert_eq!(completed.collected, 3);
    assert_eq!(completed.target_count, 3);
    assert_eq!(completed.visible_targets, 0);
    assert!(completed.won);
    assert!(completed.victory_color);
}

#[test]
fn reset_restores_positions_targets_and_color_without_removing_authored_entities() {
    let initial = run(1, Vec::new());
    let reset = run(66, demonstration_commands(true));
    assert_eq!(reset.player, initial.player);
    assert_eq!(reset.target_count, initial.target_count);
    assert_eq!(reset.visible_targets, initial.visible_targets);
    assert_eq!(reset.collected, 0);
    assert_eq!(reset.resets, 1);
    assert!(!reset.won);
    assert!(!reset.victory_color);
}

#[test]
fn releasing_a_direction_stops_movement() {
    let commands = [(1, true), (6, false)]
        .into_iter()
        .enumerate()
        .map(|(index, (tick, pressed))| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick).unwrap(),
                GameplayCommandIngressSource::test("sample.release").unwrap(),
                GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                movement(Direction::Right, pressed),
            )
        })
        .collect();
    let stopped = run(10, commands);
    assert!((stopped.player.x - (-100.0)).abs() < 0.001);
    assert_eq!(stopped.collected, 0);
}
