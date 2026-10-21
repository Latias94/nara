use nara::gameplay::{
    GameplayCommandDraft, GameplayCommandPayload, GameplayCommandTypeId, GameplayCommandValue,
};

pub(super) const MOVE: &str = "two-level-game.move";
pub(super) const TRAVEL: &str = "two-level-game.travel";
pub(super) const INSTANCE: &str = "two-level-game.instance";
pub(super) const MARKER: &str = "two-level-game.marker";
pub(super) const SAVE: &str = "two-level-game.save";
pub(super) const RESTORE: &str = "two-level-game.restore";
pub(super) const RETRY: &str = "two-level-game.retry";

pub(super) fn action(id: &str) -> GameplayCommandDraft {
    GameplayCommandDraft::new(GameplayCommandTypeId::new(id).expect("static game command"))
}

pub(super) fn movement(direction: usize, pressed: bool) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(direction as i64))
        .unwrap();
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .unwrap();
    action(MOVE).with_payload(payload)
}
