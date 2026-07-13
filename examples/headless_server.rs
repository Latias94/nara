use std::{error::Error, time::Duration};

use nara::{
    advanced_prelude::{GameplayCommandBatch, GameplayCommandQueue, GameplayCommandSet},
    ecs::schedule::IntoScheduleConfigs,
    prelude::*,
};

#[derive(Debug, Default, Resource)]
struct ObservedCommands(Vec<GameplayCommandKey>);

fn observe_commands(batch: Res<GameplayCommandBatch>, mut observed: ResMut<ObservedCommands>) {
    observed
        .0
        .extend(batch.iter().map(|command| command.key().clone()));
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(ServerPlugins)?
        .insert_resource(ObservedCommands::default())?
        .add_systems(
            CoreStage::FixedUpdate,
            observe_commands.in_set(GameplayCommandSet::Consume),
        )?;

    let command = GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).expect("the first authoritative tick is non-zero"),
        GameplayCommandIngressSource::external("example-server")?,
        GameplayCommandSourceSequence::new(1).expect("the first source sequence is non-zero"),
        GameplayCommandDraft::new(GameplayCommandTypeId::new("server.tick")?),
    );
    app.world_mut()?
        .resource_mut::<GameplayCommandQueue>()
        .submit(command)?;

    let outcome = app.run_once(Duration::from_secs_f64(1.0 / 60.0))?;

    assert_eq!(outcome.exit, None);
    assert_eq!(app.world().resource::<ObservedCommands>().0.len(), 1);
    assert_eq!(
        app.world().resource::<ObservedCommands>().0[0].tick().get(),
        1
    );
    assert!(app.world().resource::<GameplayCommandQueue>().is_idle());
    assert!(!app.world().contains_resource::<nara::input::PointerState>());
    assert!(
        !app.world()
            .contains_resource::<nara::window::WindowEvents>()
    );

    Ok(())
}
