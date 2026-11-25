use std::{env, ffi::OsString, process::ExitCode};

use nara::project_host::{DesktopProductLaunch, DesktopRunOutcome};

const VERIFY_FIRST_PRESENT_ARGUMENT: &str = "--verify-first-present";

fn main() -> ExitCode {
    let verify_first_present = match parse_arguments(env::args_os().skip(1)) {
        Some(verify_first_present) => verify_first_present,
        None => {
            eprintln!("usage: desktop [{VERIFY_FIRST_PRESENT_ARGUMENT}]");
            return ExitCode::FAILURE;
        }
    };
    let recipe = match nara_game::product_recipe() {
        Ok(recipe) => recipe,
        Err(error) => {
            eprintln!("failed to construct product recipe: {error}");
            return ExitCode::FAILURE;
        }
    };
    let launch = DesktopProductLaunch::new(env!("CARGO_MANIFEST_DIR"), recipe);
    let launch = if verify_first_present {
        launch.with_first_present_verification()
    } else {
        launch
    };
    let report = launch.run();
    let report = match report {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if !matches!(report.outcome(), DesktopRunOutcome::Completed(_)) {
        eprintln!("{:#?}", report.diagnostics());
        return ExitCode::FAILURE;
    }
    if verify_first_present {
        if report.startup_content_closure().is_none() {
            eprintln!("desktop run completed without startup-content evidence");
            return ExitCode::FAILURE;
        }
        println!("starter_desktop: ok first_present_verified=true");
    }
    ExitCode::SUCCESS
}

fn parse_arguments(arguments: impl IntoIterator<Item = OsString>) -> Option<bool> {
    let mut arguments = arguments.into_iter();
    match (arguments.next(), arguments.next()) {
        (None, None) => Some(false),
        (Some(argument), None) if argument == VERIFY_FIRST_PRESENT_ARGUMENT => Some(true),
        _ => None,
    }
}
