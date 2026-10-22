use std::{num::NonZeroU32, process::ExitCode};

use nara::app::RuntimeFrameStatus;
use nara::project_host::{HeadlessProductRun, HeadlessRunOutcome};

fn main() -> ExitCode {
    if let Some(exit) =
        nara::project_tooling::run_project_tool_mode(std::env::args_os().skip(1), || {
            Ok::<_, std::convert::Infallible>(nara_game::product_recipe())
        })
    {
        return exit;
    }
    let report = match HeadlessProductRun::<RuntimeFrameStatus>::from_recipe(
        env!("CARGO_MANIFEST_DIR"),
        nara_game::product_recipe(),
        NonZeroU32::new(1).expect("the starter tick bound is non-zero"),
        Vec::new(),
    )
    .run()
    {
        Ok(report) => report,
        Err(error) => {
            eprintln!("failed to open project source: {error}");
            return ExitCode::FAILURE;
        }
    };
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
            ExitCode::SUCCESS
        }
        HeadlessRunOutcome::Completed(_) | HeadlessRunOutcome::Failed => {
            eprintln!("{:#?}", report.diagnostics());
            ExitCode::FAILURE
        }
        HeadlessRunOutcome::CleanupIncomplete => {
            eprintln!("headless product returned before cleanup completed");
            ExitCode::FAILURE
        }
    }
}
