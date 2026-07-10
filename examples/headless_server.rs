use std::{error::Error, time::Duration};

use nara::{ecs::schedule::IntoScheduleConfigs, prelude::*};

#[derive(Debug, Default, Resource)]
struct ObservedCommands(Vec<GameplayCommandKey>);

fn observe_commands(batch: Res<GameplayCommandBatch>, mut observed: ResMut<ObservedCommands>) {
    observed
        .0
        .extend(batch.iter().map(|command| command.key().clone()));
}

fn main() -> Result<(), Box<dyn Error>> {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Headless Example"

[profiles.server]
"#,
    );

    if load.has_errors() {
        return Err(format!("invalid manifest diagnostics: {:?}", load.diagnostics).into());
    }

    let settings = load
        .manifest
        .expect("valid manifest load should carry a manifest")
        .resolve_profile(Some("server"))?;

    let mut app = App::new();
    apply_project_settings(&mut app, settings)?
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
