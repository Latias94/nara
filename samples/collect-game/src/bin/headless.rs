use std::{num::NonZeroU32, process::ExitCode, time::Instant};

use collect_gameplay::{GameSnapshot, demonstration_commands};
use nara::project_host::{HeadlessRun, HeadlessRunOutcome};
use nara_game::open_project_root;

const CLEANUP_DEADLINE_SECONDS: u64 = 5;

fn main() -> ExitCode {
    if let Some(exit) =
        nara::project_tooling::run_project_tool_mode(std::env::args_os().skip(1), || {
            Ok::<_, std::convert::Infallible>(nara_game::product_recipe())
        })
    {
        return exit;
    }
    let verify_reset = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [argument] if argument == "--verify-reset" => true,
        _ => {
            eprintln!("usage: headless [--verify-reset]");
            return ExitCode::FAILURE;
        }
    };
    let project_root = match open_project_root() {
        Ok(project_root) => project_root,
        Err(error) => {
            eprintln!("failed to open project source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let ticks = if verify_reset { 66 } else { 62 };
    let mut run = HeadlessRun::<GameSnapshot>::from_recipe(
        project_root,
        nara_game::product_recipe(),
        NonZeroU32::new(ticks).expect("the demonstration tick bound is non-zero"),
        demonstration_commands(verify_reset),
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(CLEANUP_DEADLINE_SECONDS);

    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                let valid = outcome.tick == u64::from(ticks)
                    && outcome.target_count == 3
                    && if verify_reset {
                        !outcome.won
                            && outcome.collected == 0
                            && outcome.resets == 1
                            && outcome.visible_targets == 3
                            && !outcome.victory_color
                            && outcome.player == nara::core::Vec2::new(-120.0, 0.0)
                    } else {
                        outcome.won
                            && outcome.collected == 3
                            && outcome.resets == 0
                            && outcome.visible_targets == 0
                            && outcome.victory_color
                    };
                if !valid {
                    eprintln!("unexpected collection-game outcome: {outcome:?}");
                    return ExitCode::FAILURE;
                }
                let Some(closure) = report.startup_content_closure() else {
                    eprintln!("headless run completed without startup-content evidence");
                    return ExitCode::FAILURE;
                };
                if let Err(error) = nara_game::record_product_observation(
                    nara_game::ProductObservationTarget::Headless,
                    closure,
                ) {
                    eprintln!("failed to record headless product observation: {error}");
                    return ExitCode::FAILURE;
                }
                println!(
                    "collect_headless: ok tick={} collected={} targets={} won={} resets={} player=({:.1},{:.1})",
                    outcome.tick,
                    outcome.collected,
                    outcome.target_count,
                    outcome.won,
                    outcome.resets,
                    outcome.player.x,
                    outcome.player.y
                );
                return ExitCode::SUCCESS;
            }
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            HeadlessRunOutcome::Failed => {
                eprintln!("{:#?}", report.diagnostics());
                return ExitCode::FAILURE;
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                eprintln!("headless cleanup exceeded its bounded deadline");
                return ExitCode::FAILURE;
            }
        }
    }
}
