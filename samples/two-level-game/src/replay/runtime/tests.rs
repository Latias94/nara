use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::Arc,
};

use crate::command::CommandSubmitError;

use super::*;

#[test]
fn playback_refuses_competing_ingress_before_the_original_game_consumes_it() {
    super::super::with_game(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        Mode::Replay,
        |game| {
            let before = game.capture()?;
            for source in [
                CommandSource::external("unexpected.external").unwrap(),
                CommandSource::test("unexpected.test").unwrap(),
                CommandSource::replay("unexpected.replay").unwrap(),
                CommandSource::LocalAction,
            ] {
                let result = ScheduledCommand::new(
                    CommandTick::new(1).unwrap(),
                    source,
                    CommandSequence::new(1).unwrap(),
                    SemanticCommand::Marker.intent(),
                )
                .submit_headless(&mut game.session);
                assert!(matches!(result, Err(CommandSubmitError::Rejected(_))));
            }
            game.session.frame(Duration::ZERO).map_err(runtime_error)?;
            assert_eq!(
                crate::resume::encode_resume(&game.capture()?).unwrap(),
                crate::resume::encode_resume(&before).unwrap()
            );
            let (tick, consumed, after) = game.step(&[SemanticCommand::Marker])?;
            assert_eq!(tick, 1);
            assert_eq!(consumed.len(), 1);
            assert_eq!(
                consumed[0].source(),
                &CommandSource::replay(REPLAY_SOURCE).unwrap()
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
