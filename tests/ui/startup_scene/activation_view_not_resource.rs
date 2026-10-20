use nara::{advanced_prelude::CurrentScene, prelude::Commands};

fn remove_activation(mut commands: Commands) {
    commands.remove_resource::<CurrentScene<'static>>();
}

fn main() {}
