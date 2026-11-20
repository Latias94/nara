#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "shared game driver also serves other resume tests"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::time::{Duration, Instant};

use nara::{
    advanced_prelude::CurrentScene,
    app::RuntimeState,
    identity::{EntityLookup, SceneEntityId},
};
use support::{DEADLINE, Game, MARKER, TRAVEL, action};
use two_level_game::{
    GamePhase, GameResumeError, GameSnapshot,
    levels::{LevelId, LevelKey},
    request_resume,
    resume::PlayerState,
};

fn assert_retained(game: &Game, before: &GameSnapshot, player: PlayerState) {
    assert_eq!(game.session.state(), Some(RuntimeState::Paused));
    let snapshot = game.snapshot();
    assert_eq!(snapshot.current_instance, before.current_instance);
    assert_eq!(snapshot.scene_members, before.scene_members);
    assert_eq!(snapshot.level, before.level);
    assert_eq!(snapshot.player, before.player);
    assert_eq!(snapshot.score, before.score);
    assert_eq!(snapshot.collected, before.collected);
    assert_eq!(snapshot.visible_pickups, before.visible_pickups);
    assert_eq!(snapshot.markers, before.markers);
    assert_eq!(snapshot.travel_count, before.travel_count);
    assert_eq!(snapshot.restarts, before.restarts);
    assert_eq!(snapshot.saves, before.saves);
    assert_eq!(snapshot.restores, before.restores);
    game.session
        .observe(|world| {
            let current =
                CurrentScene::get(world).expect("the retained Scene must remain available");
            assert_eq!(
                Some(current.receipt().instance_id()),
                before.current_instance
            );
            let id = SceneEntityId::new("explorer").unwrap();
            let entity = match current.receipt().resolve(world, &id) {
                EntityLookup::Resolved(entity) => entity,
                other => panic!("the retained Scene must resolve its explorer: {other:?}"),
            };
            let transform = world
                .get::<nara::prelude::Transform2d>(entity)
                .expect("the retained explorer must keep its pose");
            assert_eq!(transform.translation.to_array(), player.translation);
            assert_eq!(transform.rotation, player.rotation);
            assert_eq!(transform.scale.to_array(), player.scale);
        })
        .unwrap();
}

#[test]
fn cavern_resume_overlap_refuses_before_replacing_the_fresh_meadow_game() {
    let mut original = Game::source();
    let arrived = original.command(action(TRAVEL));
    assert_eq!(arrived.level, Some(LevelKey::main(LevelId::Cavern)));
    original.command(action(MARKER));
    let mut state = original.capture();
    assert_eq!(state.progress.markers.len(), 1);
    let retained = original.snapshot();
    let original_player = state.player;
    state.player.translation = [20.0, -12.0];
    state
        .validate()
        .expect("target wall overlap must remain valid resume wire data");

    let mut candidate = Game::source();
    let fresh = candidate.snapshot();
    assert_eq!(fresh.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(fresh.tick, 0);
    let candidate_player = candidate.capture().player;
    let deadline = Instant::now() + DEADLINE;
    let reason = match request_resume(&mut candidate.session, &state) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        Err(error) => panic!("expected a target placement refusal, got {error:?}"),
        Ok(()) => loop {
            assert_retained(&original, &retained, original_player);
            assert_retained(&candidate, &fresh, candidate_player);
            let snapshot = candidate.snapshot();
            if let Some(error) = &snapshot.error {
                break error.clone();
            }
            assert!(
                snapshot.restoring,
                "the overlapping Cavern pose was accepted: {snapshot:?}"
            );
            assert!(
                Instant::now() < deadline,
                "target placement refusal timed out: {snapshot:?}"
            );
            candidate.maintenance();
            std::thread::sleep(Duration::from_millis(1));
        },
    };

    assert!(reason.contains("overlaps a static wall"), "{reason}");
    assert_retained(&original, &retained, original_player);
    assert_retained(&candidate, &fresh, candidate_player);
    let rejected = candidate.snapshot();
    assert!(!rejected.restoring, "{rejected:?}");
    assert_eq!(rejected.phase, GamePhase::Running);
    candidate.close();
    original.close();
}
