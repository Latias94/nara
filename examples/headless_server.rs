use std::{error::Error, time::Duration};

use nara::prelude::*;

#[derive(Default, Resource)]
struct SimulationTicks(u32);

fn simulate(mut ticks: ResMut<SimulationTicks>) {
    ticks.0 += 1;
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless())?
        .init_resource::<SimulationTicks>()?
        .add_systems(FixedUpdate, simulate)?;

    let interval = Duration::from_nanos(16_666_667);
    for _ in 0..60 {
        app.step_fixed(interval)?;
    }
    assert_eq!(app.world().resource::<SimulationTicks>().0, 60);
    Ok(())
}
