use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::Arc,
};

use nara::{
    gameplay::{GameplayCommandSource, GameplayCommandSubmission},
    project_host::HeadlessSessionError,
};

use super::*;

#[test]
fn playback_refuses_competing_ingress_before_the_original_game_consumes_it() {
    super::super::with_game(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        Mode::Replay,
        |game| {
            let before = game.capture()?;
            for source in [
                GameplayCommandIngressSource::external("unexpected.external").unwrap(),
                GameplayCommandIngressSource::test("unexpected.test").unwrap(),
                GameplayCommandIngressSource::ai("unexpected.ai").unwrap(),
                GameplayCommandIngressSource::replay("unexpected.replay").unwrap(),
            ] {
                let result = game.session.submit(GameplayCommandSubmission::new(
                    GameplayCommandTick::new(1).unwrap(),
                    source,
                    GameplayCommandSourceSequence::new(1).unwrap(),
                    SemanticCommand::Marker.draft(),
                ));
                assert!(matches!(
                    result,
                    Err(HeadlessSessionError::CommandRejected(_))
                ));
            }
            assert_eq!(
                crate::resume::encode_resume(&game.capture()?).unwrap(),
                crate::resume::encode_resume(&before).unwrap()
            );
            let (tick, consumed, after) = game.step(&[SemanticCommand::Marker])?;
            assert_eq!(tick, 1);
            assert_eq!(consumed.len(), 1);
            assert_eq!(
                consumed[0].source(),
                &GameplayCommandSource::replay(REPLAY_SOURCE).unwrap()
            );
            assert_eq!(after.progress.markers.len(), 1);
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn a_panicking_consumer_retires_the_original_world_before_resuming_its_payload() {
    let original_slot = RefCell::new(None);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _: Result<(), _> = super::super::with_game(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            Mode::Record,
            |game| {
                original_slot.replace(Some(Arc::downgrade(&game.slot.0)));
                panic!("original recording operation panic");
            },
        );
    }));
    let payload = result.unwrap_err();
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"original recording operation panic")
    );
    assert!(original_slot.borrow().as_ref().unwrap().upgrade().is_none());
}
