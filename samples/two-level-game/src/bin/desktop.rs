use std::{path::PathBuf, process::ExitCode};

use nara::project_host::{DesktopProductLaunch, DesktopRunOutcome};
use two_level_game::RunOptions;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut verify = false;
    let mut save_path = None;
    while let Some(argument) = args.next() {
        if argument == "--verify-first-present" && !verify {
            verify = true;
        } else if argument == "--save" && save_path.is_none() {
            save_path = args.next().map(PathBuf::from);
            if save_path.is_none() {
                return usage();
            }
        } else {
            return usage();
        }
    }
    if save_path.as_ref().is_some_and(|path| !path.is_absolute()) {
        return usage();
    }
    let recipe = two_level_game::configured_recipe(RunOptions {
        save_path,
        ..Default::default()
    });
    let launch = DesktopProductLaunch::from_cargo_product(
        env!("CARGO_MANIFEST_DIR"),
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        "desktop",
        recipe,
    );
    let launch = match launch {
        Ok(launch) if verify => launch.with_first_present_verification(),
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
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
        eprintln!("desktop failed: {:?}", report.diagnostics());
        return ExitCode::FAILURE;
    }
    if verify {
        let Some(closure) = report.startup_content_closure() else {
            eprintln!("missing startup evidence");
            return ExitCode::FAILURE;
        };
        if let Err(error) = two_level_game::record_product_observation(
            two_level_game::ProductObservationTarget::Desktop,
            closure,
        ) {
            eprintln!("product observation: {error}");
            return ExitCode::FAILURE;
        }
        println!("two_level_desktop: ok first_present_verified=true");
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: desktop [--save ABSOLUTE_PATH] [--verify-first-present]");
    ExitCode::FAILURE
}
