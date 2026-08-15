use std::{env, ffi::OsString, process::ExitCode, time::Instant};

use nara::project_host::{DesktopRun, DesktopRunIntent, DesktopRunOutcome};
use nara_game::open_project_root;

const VERIFY_FIRST_PRESENT_ARGUMENT: &str = "--verify-first-present";
const CLEANUP_DEADLINE_SECONDS: u64 = 5;

fn main() -> ExitCode {
    let verify_first_present = match parse_arguments(env::args_os().skip(1)) {
        Some(verify_first_present) => verify_first_present,
        None => {
            eprintln!("usage: desktop [{VERIFY_FIRST_PRESENT_ARGUMENT}]");
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
    let intent = DesktopRunIntent::new()
        .with_profile("desktop")
        .with_recipe(nara_game::product_recipe());
    let intent = if verify_first_present {
        intent.with_first_present_verification()
    } else {
        intent
    };
    let mut run = DesktopRun::new(project_root, intent);
    let deadline = Instant::now() + std::time::Duration::from_secs(CLEANUP_DEADLINE_SECONDS);

    loop {
        let report = run.execute();
        match report.outcome() {
            DesktopRunOutcome::Completed(_) if !verify_first_present => return ExitCode::SUCCESS,
            DesktopRunOutcome::Completed(_) if report.presentation_verification().is_some() => {
                println!("starter_desktop: ok first_present_verified=true");
                return ExitCode::SUCCESS;
            }
            DesktopRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            DesktopRunOutcome::Completed(_) | DesktopRunOutcome::Failed => {
                eprintln!("{:#?}", report.diagnostics());
                return ExitCode::FAILURE;
            }
            DesktopRunOutcome::CleanupIncomplete => {
                eprintln!("desktop cleanup exceeded its bounded deadline");
                return ExitCode::FAILURE;
            }
        }
    }
}

fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Option<bool> {
    let mut arguments = arguments.into_iter();
    match (arguments.next(), arguments.next()) {
        (None, None) => Some(false),
        (Some(argument), None) if argument == VERIFY_FIRST_PRESENT_ARGUMENT => Some(true),
        _ => None,
    }
}
