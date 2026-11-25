use nara::{
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId},
    prelude::{App, CoreStage, FixedTime, Plugin, Res},
};

const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PluginId::new("my-game.first-step"), PluginCategory::Runtime);

#[derive(Default)]
pub struct FirstStepPlugin;

impl Plugin for FirstStepPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::FixedUpdate, first_step)?;
        Ok(())
    }
}

fn first_step(time: Res<FixedTime>) {
    if time.tick() == 1 {
        println!("first_fixed_step: tick=1");
    }
}
