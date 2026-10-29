use super::*;
use crate::snapshot::{EnemySnapshot, WaveSnapshot, WaveSnapshotContent};
use nara::ecs::World;

fn assert_independent<R: SceneProductResource>(original: &R, other: &R) {
    let baseline = original.capture_baseline();
    assert!(original.matches_baseline(&baseline));
    assert!(!other.matches_baseline(&baseline));
}

#[test]
fn equal_new_wave_resources_have_distinct_incarnations() {
    assert_independent(&MovementIntent::default(), &MovementIntent::default());
    assert_independent(&WaveRunGeneration::default(), &WaveRunGeneration::default());
    assert_independent(&WaveRetryStatus::default(), &WaveRetryStatus::default());
    assert_independent(&WaveState::default(), &WaveState::default());
    assert_independent(&WaveSnapshot::default(), &WaveSnapshot::default());
}

#[test]
fn cloned_wave_resources_preserve_values_but_start_independent_incarnations() {
    let mut movement = MovementIntent::default();
    movement.apply(MovementDirection::Right, true);
    let movement_clone = movement.clone();
    assert_eq!(movement, movement_clone);
    assert_independent(&movement, &movement_clone);

    let mut generation = WaveRunGeneration::default();
    generation.advance_for_reset(4).unwrap();
    let generation_clone = generation.clone();
    assert_eq!(generation, generation_clone);
    assert_independent(&generation, &generation_clone);

    let mut retry = WaveRetryStatus::default();
    retry.mark_pending(2, Some(7));
    let retry_clone = retry.clone();
    assert_eq!(retry, retry_clone);
    assert_independent(&retry, &retry_clone);

    let snapshot = WaveSnapshot::new(WaveSnapshotContent {
        tick: 7,
        score: 100,
        ..Default::default()
    });
    let snapshot_clone = snapshot.clone();
    assert_eq!(snapshot, snapshot_clone);
    assert_independent(&snapshot, &snapshot_clone);
}

#[test]
fn movement_same_tick_aba_and_equal_writes_only_advance_the_generation() {
    let mut world = World::new();
    world.insert_resource(MovementIntent::default());
    let tick = world.read_change_tick();
    let baseline = world.resource::<MovementIntent>().capture_baseline();

    world
        .resource_mut::<MovementIntent>()
        .apply(MovementDirection::Left, true);
    world
        .resource_mut::<MovementIntent>()
        .apply(MovementDirection::Left, false);

    let movement = world.resource::<MovementIntent>();
    assert_eq!(movement.velocity(), Vec2::ZERO);
    assert!(!movement.matches_baseline(&baseline));
    let after_aba = movement.capture_baseline();
    assert!(Arc::ptr_eq(&baseline.0, &after_aba.0));
    assert!(after_aba.1 > baseline.1);

    world
        .resource_mut::<MovementIntent>()
        .apply(MovementDirection::Stop, true);
    assert!(
        !world
            .resource::<MovementIntent>()
            .matches_baseline(&after_aba)
    );
    assert_eq!(world.read_change_tick(), tick);

    let before_reinstall = world.resource::<MovementIntent>().capture_baseline();
    world.remove_resource::<MovementIntent>().unwrap();
    world.insert_resource(MovementIntent::default());
    assert!(
        !world
            .resource::<MovementIntent>()
            .matches_baseline(&before_reinstall)
    );
}

#[test]
fn retry_baseline_rejects_restored_pending_and_repeated_equal_status_writes() {
    let mut retry = WaveRetryStatus::default();
    retry.mark_pending(1, Some(4));
    let pending = retry.clone();
    let baseline = retry.capture_baseline();
    retry.reject(WaveRetryRejection::ReplacementRejected);
    retry.mark_pending(1, Some(4));
    assert_eq!(retry, pending);
    assert!(!retry.matches_baseline(&baseline));
    assert!(Arc::ptr_eq(&baseline.0, &retry.capture_baseline().0));

    retry.reject_duplicate();
    let duplicate = retry.capture_baseline();
    assert!(!retry.mark_pending(1, Some(4)));
    assert!(!retry.matches_baseline(&duplicate));

    retry.mark_applied(2, Some(4), None);
    let applied = retry.capture_baseline();
    retry.mark_applied(2, Some(4), None);
    assert!(!retry.matches_baseline(&applied));
}

#[test]
fn run_reset_changes_baseline_without_reallocating_and_failed_reset_preserves_it() {
    let mut generation = WaveRunGeneration::default();
    let baseline = generation.capture_baseline();
    assert!(generation.advance_for_reset(0).is_none());
    assert!(generation.matches_baseline(&baseline));

    generation.advance_for_reset(9).unwrap();
    assert_eq!(generation.get(), 2);
    assert_eq!(generation.run_tick(9), Some(1));
    assert!(!generation.matches_baseline(&baseline));
    assert!(Arc::ptr_eq(&baseline.0, &generation.capture_baseline().0));
}

#[test]
fn wave_state_baseline_covers_progress_identity_and_outcome_aba() {
    let mut state = WaveState::default();
    let baseline = state.capture_baseline();
    state.record_defeats(100, 1);
    state.record_defeats(0, 0);
    assert_eq!(state.score(), 0);
    assert_eq!(state.defeated_enemies(), 0);
    assert!(!state.matches_baseline(&baseline));

    state.set_planned_enemies(2);
    let population = state.capture_baseline();
    state.set_planned_enemies(3);
    state.set_planned_enemies(2);
    assert!(!state.matches_baseline(&population));

    let projectile = state.capture_baseline();
    state.set_next_projectile_id(2);
    state.set_next_projectile_id(1);
    assert!(!state.matches_baseline(&projectile));

    let outcome = state.capture_baseline();
    state.finish(WaveOutcome::Defeated);
    state.finish(WaveOutcome::Running);
    assert!(!state.matches_baseline(&outcome));
    assert!(Arc::ptr_eq(&baseline.0, &state.capture_baseline().0));
}

#[test]
fn tick_gate_cycles_invalidate_baselines_and_read_only_results_preserve_them() {
    let mut state = WaveState::default();
    let baseline = state.capture_baseline();
    state.admit_tick();
    state.reject_tick();
    state.begin_tick();
    assert!(state.tick_is_pending());
    assert!(!state.matches_baseline(&baseline));

    state.admit_tick();
    let admitted = state.capture_baseline();
    state.admit_tick();
    assert_eq!(state.reject_on_error(Ok::<_, ()>(())), Ok(()));
    assert!(state.matches_baseline(&admitted));
    assert_eq!(state.reject_on_error(Err::<(), _>(())), Err(()));
    assert!(!state.matches_baseline(&admitted));

    let rejected = state.capture_baseline();
    state.begin_projection_only_tick();
    assert!(state.can_publish_snapshot());
    assert!(!state.matches_baseline(&rejected));
    assert!(Arc::ptr_eq(&baseline.0, &state.capture_baseline().0));
}

#[test]
fn snapshot_same_tick_publication_rejects_content_aba_and_equal_publication() {
    let content = WaveSnapshotContent {
        tick: 7,
        enemies: vec![EnemySnapshot {
            id: "enemy".into(),
            position: Vec2::ZERO,
            hit_points: 10,
            spawn_tick: 1,
            active: true,
        }],
        ..Default::default()
    };
    let mut world = World::new();
    world.insert_resource(WaveSnapshot::new(content.clone()));
    let tick = world.read_change_tick();
    let baseline = world.resource::<WaveSnapshot>().capture_baseline();
    let mut changed = content.clone();
    changed.enemies[0].hit_points = 5;
    world.resource_mut::<WaveSnapshot>().publish(changed);
    world
        .resource_mut::<WaveSnapshot>()
        .publish(content.clone());

    let snapshot = world.resource::<WaveSnapshot>();
    assert_eq!(&**snapshot, &content);
    assert!(!snapshot.matches_baseline(&baseline));
    let after_aba = snapshot.capture_baseline();
    assert!(Arc::ptr_eq(&baseline.0, &after_aba.0));
    world.resource_mut::<WaveSnapshot>().publish(content);
    assert!(
        !world
            .resource::<WaveSnapshot>()
            .matches_baseline(&after_aba)
    );
    assert_eq!(world.read_change_tick(), tick);
}

#[test]
#[should_panic(expected = "wave resource mutation generation exhausted")]
fn mutation_generation_cannot_wrap_to_a_retained_baseline() {
    let mut revision = WaveResourceRevision {
        incarnation: Arc::new(()),
        generation: u64::MAX,
    };
    revision.advance();
}
