use std::error::Error;

use nara_reference_game::run_headless_ticks;

fn main() -> Result<(), Box<dyn Error>> {
    let snapshot = run_headless_ticks(3)?;
    println!(
        "tick={} enemy_hp={}",
        snapshot.tick, snapshot.enemy_hit_points
    );
    Ok(())
}
