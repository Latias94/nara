//! Custom observer authoring through the version-bound Bevy ECS substrate.
//!
//! Bevy 0.20.0's Event and EntityEvent derives have no crate-path override. The local
//! `bevy_ecs` import follows the upstream workaround for resolving their generated paths.

use crate::ecs::{self, bevy_ecs};
use bevy_ecs::observer::{Observer, On};

#[derive(Default, ecs::Resource)]
struct ScoreLedger {
    rounds: Vec<u32>,
    players: Vec<(ecs::Entity, ecs::Entity, u32)>,
}

#[derive(bevy_ecs::event::Event)]
struct RoundFinished {
    points: u32,
}

#[derive(bevy_ecs::event::EntityEvent)]
struct PlayerScored {
    #[event_target]
    player: ecs::Entity,
    points: u32,
}

fn record_player_score(event: On<PlayerScored>, mut ledger: ecs::ResMut<ScoreLedger>) {
    ledger
        .players
        .push((event.observer(), event.player, event.points));
}

#[test]
fn custom_event_observers_record_payloads_and_stop_after_retirement() {
    let mut world = ecs::World::new();
    world.init_resource::<ScoreLedger>();
    let observer = world
        .add_observer(
            |event: On<RoundFinished>, mut ledger: ecs::ResMut<ScoreLedger>| {
                ledger.rounds.push(event.points);
            },
        )
        .id();
    world.flush();
    assert!(world.get::<Observer>(observer).is_some());

    world.trigger(RoundFinished { points: 7 });
    world.trigger(RoundFinished { points: 11 });
    assert_eq!(world.resource::<ScoreLedger>().rounds, [7, 11]);

    assert!(world.despawn(observer));
    world.flush();
    assert!(world.get_entity(observer).is_err());

    world.trigger(RoundFinished { points: 99 });
    assert_eq!(world.resource::<ScoreLedger>().rounds, [7, 11]);
}

#[test]
fn entity_events_isolate_targets_and_retire_observers() {
    let mut world = ecs::World::new();
    world.init_resource::<ScoreLedger>();
    let first_player = world.spawn_empty().id();
    let second_player = world.spawn_empty().id();
    let unobserved_player = world.spawn_empty().id();
    let first_observer = world
        .spawn(Observer::new(record_player_score).with_entity(first_player))
        .id();
    let second_observer = world
        .spawn(Observer::new(record_player_score).with_entity(second_player))
        .id();
    world.flush();
    assert!(world.get::<Observer>(first_observer).is_some());
    assert!(world.get::<Observer>(second_observer).is_some());

    world.trigger(PlayerScored {
        player: unobserved_player,
        points: 99,
    });
    assert!(world.resource::<ScoreLedger>().players.is_empty());

    world.trigger(PlayerScored {
        player: first_player,
        points: 3,
    });
    assert_eq!(
        world.resource::<ScoreLedger>().players,
        [(first_observer, first_player, 3)]
    );

    world.trigger(PlayerScored {
        player: second_player,
        points: 5,
    });
    assert_eq!(
        world.resource::<ScoreLedger>().players,
        [
            (first_observer, first_player, 3),
            (second_observer, second_player, 5)
        ]
    );

    assert!(world.despawn(first_observer));
    world.flush();
    assert!(world.get_entity(first_observer).is_err());
    assert!(world.get_entity(first_player).is_ok());

    world.trigger(PlayerScored {
        player: first_player,
        points: 99,
    });
    world.trigger(PlayerScored {
        player: second_player,
        points: 7,
    });
    assert_eq!(
        world.resource::<ScoreLedger>().players,
        [
            (first_observer, first_player, 3),
            (second_observer, second_player, 5),
            (second_observer, second_player, 7)
        ]
    );

    assert!(world.despawn(second_player));
    world.flush();
    assert!(world.get_entity(second_observer).is_err());

    world.trigger(PlayerScored {
        player: unobserved_player,
        points: 99,
    });
    assert_eq!(
        world.resource::<ScoreLedger>().players,
        [
            (first_observer, first_player, 3),
            (second_observer, second_player, 5),
            (second_observer, second_player, 7)
        ]
    );
}
