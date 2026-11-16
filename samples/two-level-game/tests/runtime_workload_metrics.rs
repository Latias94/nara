//! Arithmetic, identity custody, and public snapshot classification for the native workload.

#![cfg(all(feature = "desktop", feature = "cost-measurement"))]

#[path = "../src/bin/runtime_workload/observe.rs"]
#[allow(dead_code)]
mod observe;
#[path = "../src/bin/runtime_cost/process_cost.rs"]
#[allow(dead_code)]
mod process_cost;

use std::{collections::BTreeSet, time::Duration};

use nara::{identity::SceneInstanceId, prelude::Vec2};
use observe::{FrameClass, MAX_SAMPLES, PresentationIdentity};
use process_cost::ProcessCost;
use two_level_game::{
    GamePhase, GameSnapshot,
    levels::{LevelId, LevelKey},
    save::MarkerId,
};

#[test]
fn nearest_rank_uses_observed_values_for_small_even_and_unsorted_samples() {
    let report = observe::distribution(&[100, 1, 4, 2]).unwrap();
    assert_eq!(report["count"], 4);
    assert_eq!(report["min"], 1);
    assert_eq!(report["p50"], 2);
    assert_eq!(report["p95"], 100);
    assert_eq!(report["p99"], 100);
    assert_eq!(report["max"], 100);
    let report = observe::distribution(&(1..=100).rev().collect::<Vec<_>>()).unwrap();
    assert_eq!(report["p50"], 50);
    assert_eq!(report["p95"], 95);
    assert_eq!(report["p99"], 99);
}

#[test]
fn distribution_rejects_missing_and_excess_samples_and_preserves_integral_extremes() {
    assert!(observe::distribution(&[]).is_err());
    assert!(observe::distribution(&vec![0; MAX_SAMPLES + 1]).is_err());
    let report = observe::distribution(&[u64::MAX]).unwrap();
    assert_eq!(report["p50"], u64::MAX);
    assert_eq!(report["p99"], u64::MAX);
    assert_eq!(observe::distribution(&[0]).unwrap()["min"], 0);
    assert_eq!(
        observe::duration_ns(Duration::from_nanos(u64::MAX)).unwrap(),
        u64::MAX
    );
    assert!(observe::duration_ns(Duration::MAX).is_err());
}

fn identity(app_frame: u64) -> PresentationIdentity {
    PresentationIdentity {
        runtime_generation: 1,
        app_frame,
        render_frame: app_frame + 10,
        window: 1,
        surface_generation: 1,
        backend_instance: 1,
        device_epoch: 1,
    }
}

#[test]
fn duplicate_presentations_consume_no_allowance_and_all_original_owner_ids_matter() {
    let mut identities = BTreeSet::new();
    let original = identity(1);
    assert!(observe::retain_presentation(&mut identities, original).unwrap());
    assert!(!observe::retain_presentation(&mut identities, original).unwrap());
    let variants = [
        PresentationIdentity {
            runtime_generation: 2,
            ..original
        },
        PresentationIdentity {
            app_frame: 2,
            ..original
        },
        PresentationIdentity {
            render_frame: 12,
            ..original
        },
        PresentationIdentity {
            window: 2,
            ..original
        },
        PresentationIdentity {
            surface_generation: 2,
            ..original
        },
        PresentationIdentity {
            backend_instance: 2,
            ..original
        },
        PresentationIdentity {
            device_epoch: 2,
            ..original
        },
    ];
    for variant in variants {
        assert!(observe::retain_presentation(&mut identities, variant).unwrap());
    }
    assert_eq!(identities.len(), 8);
    for app_frame in 100..100 + (MAX_SAMPLES - identities.len()) as u64 {
        assert!(observe::retain_presentation(&mut identities, identity(app_frame)).unwrap());
    }
    assert_eq!(identities.len(), MAX_SAMPLES);
    assert!(!observe::retain_presentation(&mut identities, original).unwrap());
    assert!(observe::retain_presentation(&mut identities, identity(10_000)).is_err());
    assert!(
        observe::retain_presentation(
            &mut BTreeSet::new(),
            PresentationIdentity {
                device_epoch: 0,
                ..original
            }
        )
        .is_err()
    );
}

fn running_snapshot() -> GameSnapshot {
    GameSnapshot {
        initialized: true,
        level: Some(LevelKey::main(LevelId::Meadow)),
        current_instance: Some(SceneInstanceId::new(1).unwrap()),
        player: Vec2::new(-120.0, 0.0),
        scene_members: 6,
        ..GameSnapshot::default()
    }
}

#[test]
fn movement_score_and_markers_are_gameplay_while_observed_scene_changes_are_transitions() {
    let before = running_snapshot();
    let mut after = before.clone();
    after.score = 10;
    after.collected = 1;
    after.markers.push(MarkerId::new(1).unwrap());
    assert_eq!(
        observe::classify(Some(&before), &after).unwrap(),
        FrameClass::StableGameplay
    );
    after.player.x += 2.8;
    assert_eq!(
        observe::classify(Some(&before), &after).unwrap(),
        FrameClass::MovingGameplay
    );
    after.current_instance = Some(SceneInstanceId::new(2).unwrap());
    assert_eq!(
        observe::classify(Some(&before), &after).unwrap(),
        FrameClass::SceneTransition
    );
    after.current_instance = before.current_instance;
    after.phase = GamePhase::Preparing;
    assert_eq!(
        observe::classify(Some(&before), &after).unwrap(),
        FrameClass::SceneTransition
    );
    let returned = running_snapshot();
    assert_eq!(
        observe::classify(Some(&after), &returned).unwrap(),
        FrameClass::SceneTransition
    );
    after = before.clone();
    after.level = Some(LevelKey::new(LevelId::Cavern, 1).unwrap());
    assert_eq!(
        observe::classify(Some(&before), &after).unwrap(),
        FrameClass::SceneTransition
    );
    assert_eq!(
        observe::classify(Some(&before), &before).unwrap(),
        FrameClass::StableGameplay
    );
}

#[test]
fn startup_and_first_initialized_snapshot_do_not_fabricate_observed_movement() {
    let startup = GameSnapshot::default();
    let first = running_snapshot();
    assert_eq!(
        observe::classify(None, &startup).unwrap(),
        FrameClass::Startup
    );
    assert_eq!(
        observe::classify(Some(&startup), &first).unwrap(),
        FrameClass::InitializedBaseline
    );
    assert!(!observe::travel_completed(&first).unwrap());
}

#[test]
fn malformed_snapshot_positions_identities_and_gameplay_errors_are_rejected() {
    let mut snapshot = running_snapshot();
    snapshot.player.x = f32::NAN;
    assert!(observe::classify(None, &snapshot).is_err());
    snapshot.player.x = f32::INFINITY;
    assert!(observe::travel_completed(&snapshot).is_err());
    snapshot.player = Vec2::ZERO;
    snapshot.current_instance = None;
    assert!(observe::classify(None, &snapshot).is_err());
    snapshot = running_snapshot();
    snapshot.error = Some("scripted gameplay rejected a command".into());
    assert!(
        observe::travel_completed(&snapshot)
            .unwrap_err()
            .contains("rejected a command")
    );
}

#[test]
fn verified_flag_alone_cannot_accept_wrong_travel_counters_or_progress() {
    let mut snapshot = running_snapshot();
    snapshot.verified = true;
    assert!(observe::travel_completed(&snapshot).is_err());
    snapshot.score = 30;
    snapshot.collected = 3;
    snapshot.visible_pickups = 1;
    snapshot.markers = vec![MarkerId::new(1).unwrap()];
    snapshot.travel_count = 4;
    snapshot.restarts = 1;
    assert!(observe::travel_completed(&snapshot).unwrap());
    snapshot.player.x = -119.0;
    assert!(observe::travel_completed(&snapshot).is_err());
    snapshot.player.x = -120.0;
    snapshot.travel_count = 5;
    assert!(observe::travel_completed(&snapshot).is_err());
    snapshot.travel_count = 4;
    snapshot.held[1] = true;
    assert!(observe::travel_completed(&snapshot).is_err());
    snapshot.held = [false; 4];
    snapshot.markers.push(MarkerId::new(2).unwrap());
    assert!(observe::travel_completed(&snapshot).is_err());
}

#[test]
fn process_cpu_deltas_are_checked_and_memory_remains_point_readings() {
    let before = ProcessCost {
        user_ns: 100,
        kernel_ns: 30,
        working_set_bytes: 500,
        private_bytes: 700,
    };
    let after = ProcessCost {
        user_ns: 140,
        kernel_ns: 37,
        working_set_bytes: 450,
        private_bytes: 710,
    };
    let report = observe::process_deltas(before, after).unwrap();
    assert_eq!(report["user_ns"], 40);
    assert_eq!(report["kernel_ns"], 7);
    assert_eq!(report["total_cpu_ns"], 47);
    assert_eq!(report["initial_working_set_bytes"], 500);
    assert_eq!(report["final_working_set_bytes"], 450);
    assert!(observe::process_deltas(after, before).is_err());
    let zero = ProcessCost {
        user_ns: 0,
        kernel_ns: 0,
        working_set_bytes: 0,
        private_bytes: 0,
    };
    assert!(
        observe::process_deltas(
            zero,
            ProcessCost {
                user_ns: u64::MAX,
                kernel_ns: 1,
                ..zero
            }
        )
        .is_err()
    );
}
