use std::collections::BTreeMap;

use super::*;

fn checkpoint() -> Checkpoint {
    Checkpoint::new(CheckpointLocation {
        level: LevelKey::main(LevelId::STARTUP),
        anchor: SceneEntityId::new("checkpoint").unwrap(),
    })
}

fn content() -> Content {
    Content {
        player: SceneEntityId::new("player").unwrap(),
        checkpoint: SceneEntityId::new("checkpoint").unwrap(),
        checkpoint_position: Vec2::ZERO,
        pickups: BTreeMap::from([
            (SceneEntityId::new("first-pickup").unwrap(), Vec2::X),
            (SceneEntityId::new("second-pickup").unwrap(), Vec2::Y),
        ]),
    }
}

#[test]
fn equal_new_progress_and_active_levels_have_distinct_incarnations() {
    let progress = Progress::new(checkpoint());
    let progress_baseline = progress.capture_baseline();
    assert!(progress.matches_baseline(&progress_baseline));
    assert!(!Progress::new(checkpoint()).matches_baseline(&progress_baseline));

    let key = LevelKey::main(LevelId::STARTUP);
    let level = ActiveLevel::new(key, Some(content()));
    let level_baseline = level.capture_baseline();
    assert!(level.matches_baseline(&level_baseline));
    assert!(!ActiveLevel::new(key, Some(content())).matches_baseline(&level_baseline));

    let pending = ActiveLevel::new(key, None);
    assert!(!ActiveLevel::new(key, None).matches_baseline(&pending.capture_baseline()));
}

#[test]
fn progress_same_tick_anchor_aba_and_equal_writes_preserve_the_incarnation() {
    let mut world = World::new();
    world.insert_resource(Progress::new(checkpoint()));
    let tick = world.read_change_tick();
    let baseline = world.resource::<Progress>().capture_baseline();
    let anchor = checkpoint().location.anchor;
    world
        .resource_mut::<Progress>()
        .set_anchor(SceneEntityId::new("other-checkpoint").unwrap());
    world.resource_mut::<Progress>().set_anchor(anchor.clone());
    assert_eq!(world.resource::<Progress>().checkpoint(), &checkpoint());
    assert!(!world.resource::<Progress>().matches_baseline(&baseline));
    let after_aba = world.resource::<Progress>().capture_baseline();
    assert!(Arc::ptr_eq(&baseline.0, &after_aba.0));
    assert!(after_aba.1 > baseline.1);

    world.resource_mut::<Progress>().set_anchor(anchor);
    assert!(!world.resource::<Progress>().matches_baseline(&after_aba));
    assert_eq!(world.read_change_tick(), tick);

    let before_reinstall = world.resource::<Progress>().capture_baseline();
    world.remove_resource::<Progress>().unwrap();
    world.insert_resource(Progress::new(checkpoint()));
    assert!(
        !world
            .resource::<Progress>()
            .matches_baseline(&before_reinstall)
    );
}

#[test]
fn collection_and_marker_writes_invalidate_progress_while_duplicates_do_not() {
    let mut progress = Progress::new(checkpoint());
    let baseline = progress.capture_baseline();
    let key = progress.checkpoint().location.level;
    let pickup = CollectedPickup {
        level: key,
        entity: SceneEntityId::new("first-pickup").unwrap(),
    };
    assert!(progress.collect(pickup.clone()));
    assert_eq!(progress.checkpoint().score, 10);
    assert!(!progress.matches_baseline(&baseline));

    let collected = progress.capture_baseline();
    assert!(!progress.collect(pickup));
    assert!(progress.matches_baseline(&collected));

    let id = progress.add_marker(key, Vec2::new(3.0, 4.0)).unwrap();
    assert_eq!(progress.checkpoint().markers[0].id, id);
    assert_eq!(progress.checkpoint().markers[0].position, [3.0, 4.0]);
    assert!(progress.checkpoint().next_marker_id > id);
    assert!(!progress.matches_baseline(&collected));
    assert!(Arc::ptr_eq(&baseline.0, &progress.capture_baseline().0));
}

#[test]
fn failed_marker_identity_admission_preserves_the_progress_baseline() {
    let mut checkpoint = checkpoint();
    checkpoint.next_marker_id = MarkerId::new(u64::MAX - 1).unwrap();
    let key = checkpoint.location.level;
    let mut progress = Progress::new(checkpoint.clone());
    let baseline = progress.capture_baseline();
    assert!(progress.add_marker(key, Vec2::ZERO).is_err());
    assert_eq!(progress.checkpoint(), &checkpoint);
    assert!(progress.matches_baseline(&baseline));
}

#[test]
fn active_level_same_tick_content_aba_and_equal_writes_advance_only_generation() {
    let key = LevelKey::main(LevelId::STARTUP);
    let content = content();
    let mut world = World::new();
    world.insert_resource(ActiveLevel::new(key, Some(content.clone())));
    let tick = world.read_change_tick();
    let baseline = world.resource::<ActiveLevel>().capture_baseline();
    let mut changed = content.clone();
    *changed.pickups.values_mut().next().unwrap() = Vec2::new(20.0, 30.0);
    world.resource_mut::<ActiveLevel>().set_content(changed);
    world
        .resource_mut::<ActiveLevel>()
        .set_content(content.clone());

    let level = world.resource::<ActiveLevel>();
    assert_eq!(level.key(), key);
    assert_eq!(level.content().unwrap().pickups, content.pickups);
    assert!(!level.matches_baseline(&baseline));
    let after_aba = level.capture_baseline();
    assert!(Arc::ptr_eq(&baseline.0, &after_aba.0));

    world.resource_mut::<ActiveLevel>().set_content(content);
    assert!(!world.resource::<ActiveLevel>().matches_baseline(&after_aba));
    assert_eq!(world.read_change_tick(), tick);
}

#[test]
#[should_panic(expected = "game resource mutation generation exhausted")]
fn mutation_generation_cannot_wrap_to_a_retained_baseline() {
    let mut revision = ResourceRevision {
        incarnation: Arc::new(()),
        generation: u64::MAX,
    };
    revision.advance();
}
