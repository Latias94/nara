use nara::prelude::{Component, PersistentComponent};

#[derive(Component, PersistentComponent)]
#[nara(
    id = "game.Spin",
    version = 1,
    alias = "Spin",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Spin {
    #[nara(id = "radians-per-second", alias = "Radians per second")]
    pub radians_per_second: f32,
}
