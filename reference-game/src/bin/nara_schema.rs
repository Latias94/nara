use std::{
    error::Error,
    io::{self, Write},
};

use nara::{
    prelude::{App, MinimalPlugins, RegisterSchema},
    reflect::schema_json,
};
use nara_reference_game::RunRules;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)?
        .register_schema::<RunRules>()?;
    writeln!(io::stdout().lock(), "{}", schema_json(&app)?)?;
    Ok(())
}
