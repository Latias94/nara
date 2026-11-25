use std::{path::PathBuf, process::ExitCode};

use nara::project_host::{DesktopProductLaunch, DesktopRunOutcome};
use two_level_game::RunOptions;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut verify = false;
    let mut gamepad = false;
    let mut play_flow = false;
    let mut save_path = None;
    while let Some(argument) = args.next() {
        if argument == "--verify-first-present" && !verify {
            verify = true;
        } else if argument == "--gamepad" && !gamepad && cfg!(all(windows, feature = "gamepad")) {
            gamepad = true;
        } else if argument == "--play-flow" && !play_flow && cfg!(feature = "play-flow") {
            play_flow = true;
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
    #[cfg(feature = "play-flow")]
    let recipe = if play_flow {
        use two_level_game::flow::{PlayFlowControls, with_play_flow, with_play_flow_hud};

        let controls = PlayFlowControls::Keyboard;
        #[cfg(all(windows, feature = "gamepad"))]
        let controls = if gamepad {
            PlayFlowControls::KeyboardAndGamepad
        } else {
            controls
        };
        match with_play_flow(recipe, controls).and_then(with_play_flow_hud) {
            Ok(recipe) => recipe,
            Err(error) => {
                eprintln!("play flow configuration: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        recipe
    };
    #[cfg(all(windows, feature = "gamepad"))]
    let recipe = if gamepad {
        match two_level_game::controller::with_gilrs_input(recipe) {
            Ok(recipe) => recipe,
            Err(error) => {
                eprintln!("gamepad configuration: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        recipe
    };
    let launch = DesktopProductLaunch::new(env!("CARGO_MANIFEST_DIR"), recipe);
    let launch = if verify {
        launch.with_first_present_verification()
    } else {
        launch
    };
    let launch = if play_flow {
        launch.with_profile("play-flow")
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
        eprintln!("desktop failed: {:?}", report.diagnostics());
        return ExitCode::FAILURE;
    }
    if verify {
        if report.startup_content_closure().is_none() {
            eprintln!("missing startup evidence");
            return ExitCode::FAILURE;
        }
        println!("two_level_desktop: ok first_present_verified=true");
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprint!("usage: desktop [--save ABSOLUTE_PATH] [--verify-first-present]");
    if cfg!(all(windows, feature = "gamepad")) {
        eprint!(" [--gamepad]");
    }
    if cfg!(feature = "play-flow") {
        eprint!(" [--play-flow]");
    }
    eprintln!();
    ExitCode::FAILURE
}
