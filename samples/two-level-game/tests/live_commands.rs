#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the original public game and physical close owner"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use support::{Game, MARKER, action, movement};
use two_level_game::GamePhase;

#[test]
fn out_of_range_press_and_release_preserve_state_and_allow_healthy_continuation() {
    let mut game = Game::source();
    let initial = game.snapshot();
    game.submit(movement(4, true));
    assert_eq!(game.session.step().unwrap(), None);
    let rejected = game.snapshot();
    assert_eq!(rejected.tick, initial.tick + 1);
    assert_eq!(rejected.phase, GamePhase::Running);
    assert_eq!(rejected.current_instance, initial.current_instance);
    assert_eq!(rejected.player, initial.player);
    assert_eq!(rejected.held, [false; 4]);
    assert_eq!(rejected.score, initial.score);
    assert_eq!(rejected.collected, initial.collected);
    assert_eq!(
        rejected.error.as_deref(),
        Some("movement direction must be in 0..4")
    );
    assert_eq!(game.queue().acknowledged_through_tick, rejected.tick);
    assert_eq!(game.queue().pending_commands, 0);

    let recovered = game.command(action(MARKER));
    assert!(recovered.error.is_none());
    assert_eq!(recovered.markers.len(), 1);
    assert_eq!(recovered.player, initial.player);
    let moving = game.command(movement(1, true));
    assert_eq!(moving.held, [false, true, false, false]);
    assert!(moving.player.x > initial.player.x);
    assert_eq!(moving.player.y, initial.player.y);

    game.submit(movement(4, false));
    assert_eq!(game.session.step().unwrap(), None);
    let rejected_release = game.snapshot();
    assert_eq!(rejected_release.current_instance, initial.current_instance);
    assert_eq!(rejected_release.held, moving.held);
    assert!(rejected_release.player.x > moving.player.x);
    assert_eq!(
        rejected_release.error.as_deref(),
        Some("movement direction must be in 0..4")
    );
    assert_eq!(
        game.queue().acknowledged_through_tick,
        rejected_release.tick
    );
    assert_eq!(game.queue().pending_commands, 0);

    game.submit(movement(1, false));
    let released = game.command(action(MARKER));
    assert_eq!(released.held, [false; 4]);
    assert_eq!(released.player, rejected_release.player);
    assert_eq!(released.markers.len(), 2);
    assert!(released.error.is_none());
    let settled = game.step();
    assert_eq!(settled.player, released.player);
    assert_eq!(settled.held, [false; 4]);
    game.capture();
    game.close();
}
