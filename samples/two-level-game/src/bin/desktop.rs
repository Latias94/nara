use std::{path::PathBuf, process::ExitCode};

use nara::project_host::{DesktopProductLaunch, DesktopRunOutcome};
use two_level_game::RunOptions;

#[cfg(feature = "gamepad")]
struct GamepadConfiguration(nara_gilrs::GilrsInputPlugin);

#[cfg(feature = "gamepad")]
impl nara::ProductConfiguration for GamepadConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"two-level-game.gilrs/v1\0");
        match self.0.default_route {
            Some(route) => {
                output.push(1);
                output.extend_from_slice(&route.window.get().to_le_bytes());
                output.extend_from_slice(&route.seat.get().to_le_bytes());
            }
            None => output.push(0),
        }
        for limit in [self.0.max_devices, self.0.max_events_per_frame] {
            output.extend_from_slice(
                &u64::try_from(limit)
                    .expect("gamepad configuration limit fits u64")
                    .to_le_bytes(),
            );
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut verify = false;
    let mut gamepad = false;
    let mut save_path = None;
    while let Some(argument) = args.next() {
        if argument == "--verify-first-present" && !verify {
            verify = true;
        } else if argument == "--gamepad" && !gamepad && cfg!(feature = "gamepad") {
            gamepad = true;
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
    #[cfg(feature = "gamepad")]
    let recipe = if gamepad {
        let configuration = GamepadConfiguration(nara_gilrs::GilrsInputPlugin::new(Some(
            nara_gilrs::GamepadRoute::new(
                nara::input::InputWindowId::new(1),
                nara::input::InputSeatId::PRIMARY,
            ),
        )));
        match recipe.add_configured_plugin(configuration, |config| config.0) {
            Ok(recipe) => recipe,
            Err(error) => {
                eprintln!("gamepad configuration: {error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        recipe
    };
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
    if cfg!(feature = "gamepad") {
        eprintln!("usage: desktop [--save ABSOLUTE_PATH] [--verify-first-present] [--gamepad]");
    } else {
        eprintln!("usage: desktop [--save ABSOLUTE_PATH] [--verify-first-present]");
    }
    ExitCode::FAILURE
}
