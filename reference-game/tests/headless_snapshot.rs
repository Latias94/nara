#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{num::NonZeroU32, time::Duration};

use nara::{
    prelude::Vec2,
    project_host::{HeadlessRun, HeadlessRunOutcome},
};
use nara_reference_game::{
    EnemySnapshot, ProjectileSnapshot, WaveOutcome, bundled_wave_commands, wave_headless_intent,
};
use project_content_fixture::{load_project_content, project_root_capability};

#[test]
fn authoritative_snapshot_uses_sorted_instance_local_identities() {
    let maximum = NonZeroU32::new(3).unwrap();
    let intent = wave_headless_intent(maximum).stop_when(|snapshot| snapshot.tick == 3);
    let mut run = HeadlessRun::new(project_root_capability(), intent, bundled_wave_commands());
    let snapshot = loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => break snapshot.clone(),
            HeadlessRunOutcome::CleanupIncomplete => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            HeadlessRunOutcome::Failed => panic!("reference wave failed: {report:#?}"),
        }
    };

    assert_eq!(snapshot.tick, 3);
    assert_eq!(snapshot.outcome, WaveOutcome::Running);
    assert_eq!(snapshot.score, 0);
    assert_eq!(snapshot.planned_enemies, 3);
    assert_eq!(snapshot.defeated_enemies, 0);
    assert_eq!(snapshot.player.id, "player");
    assert_eq!(snapshot.player.position, Vec2::new(-3.0, 0.0));
    assert_eq!(snapshot.player.hit_points, 20);
    let loaded = load_project_content();
    let enemy_id = |root_anchor: &str| {
        loaded
            .snapshot
            .expanded_startup_scene_provenance()
            .iter()
            .find_map(|(id, source)| {
                (source.source_entity_id().as_str() == "enemy"
                    && source.projection_path().is_some_and(|path| {
                        path.root_anchor().as_str() == root_anchor && path.source_chain().len() == 1
                    }))
                .then(|| id.as_str().to_owned())
            })
            .expect("the expected enemy projection must exist")
    };
    let mut expected_enemies = vec![
        EnemySnapshot {
            id: enemy_id("enemy-anchor-2"),
            position: Vec2::new(9.0, 0.0),
            hit_points: 10,
            spawn_tick: 5,
            active: false,
        },
        EnemySnapshot {
            id: enemy_id("enemy-anchor-3"),
            position: Vec2::new(13.0, 0.0),
            hit_points: 10,
            spawn_tick: 9,
            active: false,
        },
        EnemySnapshot {
            id: enemy_id("enemy-anchor"),
            position: Vec2::new(3.5, 0.0),
            hit_points: 10,
            spawn_tick: 1,
            active: true,
        },
    ];
    expected_enemies.sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(snapshot.enemies, expected_enemies);
    assert_eq!(
        snapshot.projectiles,
        vec![ProjectileSnapshot {
            id: 1,
            position: Vec2::new(-3.0, 0.0),
            velocity: Vec2::new(2.0, 0.0),
            ttl_ticks: 64,
        }]
    );
}
