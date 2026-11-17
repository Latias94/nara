use nara::gameplay::{
    GameplayCommandDraft, GameplayCommandPayload, GameplayCommandTypeId, GameplayCommandValue,
};

pub(crate) const MOVE: &str = "two-level-game.move";
pub(crate) const TRAVEL: &str = "two-level-game.travel";
pub(crate) const INSTANCE: &str = "two-level-game.instance";
pub(crate) const MARKER: &str = "two-level-game.marker";
pub(crate) const SAVE: &str = "two-level-game.save";
pub(crate) const RESTORE: &str = "two-level-game.restore";
pub(crate) const RETRY: &str = "two-level-game.retry";
pub(crate) const PLAYER_LAUNCH: &str = "two-level-game.player-launch";

pub(crate) fn action(id: &str) -> GameplayCommandDraft {
    GameplayCommandDraft::new(GameplayCommandTypeId::new(id).expect("static game command"))
}

pub(crate) fn movement(direction: usize, pressed: bool) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(direction as i64))
        .unwrap();
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .unwrap();
    action(MOVE).with_payload(payload)
}
