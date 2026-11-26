use std::{
    error::Error,
    io::{self, Write},
};

use nara::{
    prelude::{App, MinimalPlugins},
    reflect::schema_json,
};
use nara_reference_game::register_game_schema;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)?;
    register_game_schema(&mut app)?;
    writeln!(io::stdout().lock(), "{}", schema_json(&app)?)?;
    Ok(())
}
