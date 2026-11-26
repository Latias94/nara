use nara::{prelude::*, reflect::schema_json};

#[derive(Component, Schema)]
#[schema(
    id = "example.Speed",
    version = 1,
    alias = "Speed",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Speed {
    #[schema(alias = "Units per second", unit = "units/s")]
    units_per_second: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)?;
    app.register_schema::<Speed>()?;
    println!("{}", schema_json(&app)?);
    Ok(())
}
