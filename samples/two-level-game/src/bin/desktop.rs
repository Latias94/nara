use std::{
    path::PathBuf,
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::project_host::{
    DesktopRun, DesktopRunIntent, DesktopRunOutcome, ProductSupervisedDesktopLaunchCandidate,
    SupervisedDesktopProductIdentity,
};
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
    let candidate = match ProductSupervisedDesktopLaunchCandidate::read_from_inherited_bootstrap() {
        Ok(candidate) => candidate,
        Err(error) => {
            eprintln!("supervised launch: {error}");
            return ExitCode::FAILURE;
        }
    };
    if verify && candidate.is_some() {
        eprintln!("first-present verification cannot combine with supervised launch");
        return ExitCode::FAILURE;
    }
    let root = match candidate.as_ref() {
        Some(candidate) => {
            two_level_game::open_explicit_project_root(candidate.context().project_root())
        }
        None => two_level_game::open_project_root(),
    };
    let root = match root {
        Ok(root) => root,
        Err(error) => {
            eprintln!("project root: {error}");
            return ExitCode::FAILURE;
        }
    };
    let session = match candidate {
        Some(candidate) => match candidate.verify_project_source(&root) {
            Ok(session) => Some(session),
            Err(error) => {
                eprintln!("supervised source: {error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let profile = session
        .as_ref()
        .map(|session| session.context().profile_name())
        .unwrap_or("desktop")
        .to_owned();
    let recipe = two_level_game::configured_recipe(RunOptions {
        save_path,
        ..Default::default()
    });
    let intent = DesktopRunIntent::new()
        .with_profile(profile)
        .with_recipe(recipe);
    let intent = if let Some(session) = session {
        let identity = match SupervisedDesktopProductIdentity::from_cargo_product(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            "desktop",
        ) {
            Ok(identity) => identity,
            Err(error) => {
                eprintln!("product identity: {error}");
                return ExitCode::FAILURE;
            }
        };
        intent.with_supervised_desktop_activation(session, identity)
    } else if verify {
        intent.with_first_present_verification()
    } else {
        intent
    };
    let mut run = DesktopRun::new(root, intent);
    let mut deadline = None;
    loop {
        let report = run.execute();
        match report.outcome() {
            DesktopRunOutcome::Completed(_) if !verify => return ExitCode::SUCCESS,
            DesktopRunOutcome::Completed(_) if report.presentation_verification().is_some() => {
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
                return ExitCode::SUCCESS;
            }
            DesktopRunOutcome::CleanupIncomplete => {
                if Instant::now()
                    >= *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(10))
                {
                    eprintln!("desktop cleanup deadline exceeded");
                    return ExitCode::FAILURE;
                }
                std::thread::yield_now();
            }
            _ => {
                eprintln!("desktop failed: {:?}", report.diagnostics());
                return ExitCode::FAILURE;
            }
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: desktop [--save ABSOLUTE_PATH] [--verify-first-present]");
    ExitCode::FAILURE
}
