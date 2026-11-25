use std::{
    error::Error,
    io::{self, Write},
};

use nara::{
    advanced_prelude::StartupSceneActivationPlugin,
    prelude::{App, DefaultPlugins},
    reflect::schema_json,
};
use nara_reference_game::project_recipe;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins::headless(),
        StartupSceneActivationPlugin,
        project_recipe()?,
    ))?;
    writeln!(io::stdout().lock(), "{}", schema_json(&app)?)?;
    Ok(())
}
