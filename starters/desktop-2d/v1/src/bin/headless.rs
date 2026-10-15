use std::{num::NonZeroU32, process::ExitCode, time::Instant};

use nara::app::RuntimeFrameStatus;
use nara::project_host::{HeadlessRun, HeadlessRunOutcome};
use nara_game::open_project_root;

const CLEANUP_DEADLINE_SECONDS: u64 = 5;

fn main() -> ExitCode {
    if let Some(exit) = nara::project_tooling::run_project_tool_mode(
        std::env::args_os().skip(1),
        || Ok::<_, std::convert::Infallible>(nara_game::product_recipe()),
    ) {
        return exit;
    }
    let project_root = match open_project_root() {
        Ok(project_root) => project_root,
        Err(error) => {
            eprintln!("failed to open project source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut run = HeadlessRun::<RuntimeFrameStatus>::from_recipe(
        project_root,
        nara_game::product_recipe(),
        NonZeroU32::new(1).expect("the starter tick bound is non-zero"),
        Vec::new(),
    );
    let deadline = Instant::now() + std::time::Duration::from_secs(CLEANUP_DEADLINE_SECONDS);

    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) if outcome.fixed_tick == 1 => {
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
                println!("starter_headless: ok fixed_ticks={}", outcome.fixed_tick);
                return ExitCode::SUCCESS;
            }
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            HeadlessRunOutcome::Completed(_) | HeadlessRunOutcome::Failed => {
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
