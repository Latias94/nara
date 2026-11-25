use std::{
    error::Error,
    io::{self, Write},
};

use game_components::Spin;
use nara::{
    prelude::{App, DefaultPlugins, RegisterSchema},
    reflect::schema_json,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless())?
        .register_schema::<Spin>()?;
    writeln!(io::stdout().lock(), "{}", schema_json(&app)?)?;
    Ok(())
}
