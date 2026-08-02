use nara::{advanced_prelude::StartupSceneActivation, prelude::Commands};

fn remove_activation(mut commands: Commands) {
    commands.remove_resource::<StartupSceneActivation<'static>>();
}

fn main() {}
