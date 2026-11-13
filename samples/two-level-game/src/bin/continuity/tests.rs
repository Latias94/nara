use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use nara::{
    app::{
        PluginCategory, PluginDeclaration, PluginId, PluginShutdownObligationId,
        RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    prelude::{App, Plugin, PluginError, Resource},
};

use super::*;

fn game() -> Game {
    let mut game = Game {
        session: HeadlessSession::new(
            two_level_game::open_project_root().unwrap(),
            two_level_game::product_recipe(),
        ),
        sequence: 0,
        execution_failed: false,
    };
    game.initialize(Instant::now() + OPERATION_TIMEOUT).unwrap();
    game
}

#[test]
fn expired_step_after_admission_cannot_be_consumed_by_a_successor_request() {
    let mut game = game();
    let before = game.snapshot().unwrap();
    let command = GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).unwrap(),
        GameplayCommandIngressSource::external("two-level-game.continuity").unwrap(),
        GameplayCommandSourceSequence::new(1).unwrap(),
        command_draft(SemanticCommand::Marker).unwrap(),
    );
    game.session.submit(command).unwrap();
    game.sequence = 1;
    assert!(
        game.complete_step(Instant::now() - Duration::from_nanos(1))
            .is_err()
    );
    assert!(game.execution_failed);
    assert!(
        game.execute(
            Command::Step {
                commands: Vec::new()
            },
            Instant::now() + OPERATION_TIMEOUT
        )
        .is_err()
    );
    let after = game.snapshot().unwrap();
    assert_eq!(after.tick, before.tick);
    assert_eq!(after.markers, before.markers);
    assert_eq!(
        game.session
            .observe(|world| world
                .resource::<nara::gameplay::GameplayCommandQueue>()
                .stats()
                .pending_commands)
            .unwrap(),
        1
    );
    assert!(!game.close(Instant::now() + OPERATION_TIMEOUT).unwrap());
    assert!(game.session.is_closed());
}

#[derive(Default, Resource, Clone)]
struct Gate {
    ready: Arc<AtomicBool>,
    begins: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

struct GateClose(Gate);

impl RuntimeCloseParticipant for GateClose {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.0.ready.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

impl Drop for GateClose {
    fn drop(&mut self) {
        self.0.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct GatePlugin;

const OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("two-level-game.continuity.test-close");

impl Plugin for GatePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.continuity.test-close"),
            PluginCategory::Runtime,
        )
        .shutdown_obligations(&[OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let gate = Gate::default();
        app.insert_resource(gate.clone())?;
        app.register_plugin_runtime_close_participant(
            OBLIGATION,
            RuntimeCloseParticipantId::new("two-level-game.continuity.test-close"),
            GateClose(gate),
        )?;
        Ok(())
    }
}

#[test]
fn a_timed_out_close_keeps_the_original_owner_for_a_second_cleanup_budget() {
    let mut game = Game {
        session: HeadlessSession::new(
            two_level_game::open_project_root().unwrap(),
            two_level_game::product_recipe()
                .add_plugin::<GatePlugin>()
                .unwrap(),
        ),
        sequence: 0,
        execution_failed: false,
    };
    game.initialize(Instant::now() + OPERATION_TIMEOUT).unwrap();
    let gate = game
        .session
        .observe(|world| world.resource::<Gate>().clone())
        .unwrap();
    assert!(
        game.close(Instant::now() + Duration::from_millis(1))
            .is_err()
    );
    assert!(!game.session.is_closed());
    assert_eq!(gate.begins.load(Ordering::SeqCst), 1);
    assert_eq!(gate.drops.load(Ordering::SeqCst), 0);
    gate.ready.store(true, Ordering::SeqCst);
    assert!(!game.close(Instant::now() + OPERATION_TIMEOUT).unwrap());
    assert!(game.session.is_closed());
    assert_eq!(gate.begins.load(Ordering::SeqCst), 1);
    assert_eq!(gate.drops.load(Ordering::SeqCst), 1);
}
