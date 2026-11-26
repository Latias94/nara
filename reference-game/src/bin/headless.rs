use std::error::Error;

use nara_reference_game::headless::{MAX_ROUNDS, run_stress};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let rounds = match arguments.next().as_deref() {
        None => 1,
        Some("--stress") => arguments
            .next()
            .map_or(Ok(1), |count| count.parse::<u32>())?,
        Some("--help" | "-h") => {
            println!("Usage: headless [--stress [ROUNDS]]  (1–{MAX_ROUNDS} rounds, default 1)");
            return Ok(());
        }
        Some(argument) => return Err(format!("unknown argument {argument:?}").into()),
    };
    if let Some(argument) = arguments.next() {
        return Err(format!("unexpected argument {argument:?}").into());
    }
    let report = run_stress(rounds)?;
    println!(
        "Night Relay verified {} round(s) across {} fixed ticks: {} wall contacts per round, \
         shield loss, restart, and escape in {:.2}s",
        report.rounds,
        report.fixed_ticks,
        report.wall_contacts_per_round,
        report.escape_time.as_secs_f32(),
    );
    Ok(())
}
