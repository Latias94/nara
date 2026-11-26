use nara::prelude::{Component, Schema};

#[derive(Component, Default, Schema)]
#[schema(id = "game.Spin", version = 2, default, alias = "Spin")]
pub struct Spin {
    #[schema(alias = "Radians per second")]
    pub radians_per_second: f32,
}
