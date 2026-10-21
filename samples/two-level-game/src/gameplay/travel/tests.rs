use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, PluginCategory, PluginDeclaration, PluginId},
    ecs::schedule::IntoScheduleConfigs,
    gameplay::{
        GameplayCommandIngressSource, GameplayCommandSet, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick,
    },
    prelude::{App, CoreStage, Plugin, PluginError, Res, ResMut, Resource},
    project_host::{HeadlessRun, HeadlessRunOutcome},
};

use crate::gameplay::{GameSnapshot, Session};

#[derive(Resource, Clone, Default)]
struct ObservedInput(Vec<(u64, bool, f32)>);

#[derive(Default)]
struct PreparationInputProbe;
impl Plugin for PreparationInputProbe {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-preparation-input"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[crate::gameplay::PLUGIN_ID]);
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ObservedInput::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                preparation_phase.before(GameplayCommandSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                observe.after(GameplayCommandSet::Consume),
            )?;
        Ok(())
    }
}

// Model the finite preparation interval around actual public semantic-command admission. This
// test qualifies the game's input owner, not asynchronous file loading or a completed travel.
fn preparation_phase(time: Res<FixedTime>, mut session: ResMut<Session>) {
    if time.tick() == 1 {
        session.phase = super::GamePhase::Preparing;
        session.held[0] = true;
    } else if time.tick() == 3 {
        session.phase = super::GamePhase::Running;
    }
}
fn observe(
    time: Res<FixedTime>,
    session: Res<Session>,
    snapshot: Res<GameSnapshot>,
    mut observed: ResMut<ObservedInput>,
) {
    observed.0.push((
        time.tick(),
        session.held.iter().any(|held| *held),
        snapshot.player.x,
    ));
}

#[test]
fn ignored_press_during_preparation_does_not_move_until_release_and_a_fresh_press() {
    let drafts = [
        (1, 0, false),
        (1, 1, true),
        (4, 1, false),
        (5, 1, true),
        (6, 1, false),
    ];
    let commands = drafts
        .into_iter()
        .enumerate()
        .map(|(index, (tick, direction, pressed))| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick).unwrap(),
                GameplayCommandIngressSource::external("two-level-game.preparation-input").unwrap(),
                GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                super::movement(direction, pressed),
            )
        })
        .collect();
    let recipe = crate::product_recipe()
        .add_plugin::<PreparationInputProbe>()
        .unwrap();
    let mut run = HeadlessRun::<ObservedInput>::from_recipe(
        crate::open_project_root().unwrap(),
        recipe,
        NonZeroU32::new(6).unwrap(),
        commands,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let observed = loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(observed) => break observed.clone(),
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now()
            }
            _ => panic!("input consumer failed: {:?}", report.diagnostics()),
        }
    };
    assert_eq!(observed.0.len(), 6);
    assert!(
        observed.0[..4]
            .iter()
            .all(|(_, held, x)| !held && *x == -120.0)
    );
    assert!(observed.0[4].1);
    assert!(observed.0[4].2 > observed.0[3].2);
    assert!(!observed.0[5].1);
    assert_eq!(observed.0[5].2, observed.0[4].2);
}
