use std::{env, ffi::OsString, process::ExitCode, time::Instant};

use nara::project_host::{
    DesktopRun, DesktopRunIntent, DesktopRunOutcome, ProductSupervisedDesktopLaunchCandidate,
    SupervisedDesktopProductIdentity,
};
use nara_game::{open_explicit_project_root, open_project_root};

const VERIFY_FIRST_PRESENT_ARGUMENT: &str = "--verify-first-present";
const CLEANUP_DEADLINE_SECONDS: u64 = 5;
const DESKTOP_BINARY_TARGET: &str = "desktop";

fn main() -> ExitCode {
    let verify_first_present = match parse_arguments(env::args_os().skip(1)) {
        Some(verify_first_present) => verify_first_present,
        None => {
            eprintln!("usage: desktop [{VERIFY_FIRST_PRESENT_ARGUMENT}]");
            return ExitCode::FAILURE;
        }
    };
    let supervised_candidate =
        match ProductSupervisedDesktopLaunchCandidate::read_from_inherited_bootstrap() {
            Ok(candidate) => candidate,
            Err(error) => {
                eprintln!("failed to read supervised desktop launch: {error}");
                return ExitCode::FAILURE;
            }
        };
    if verify_first_present && supervised_candidate.is_some() {
        eprintln!(
            "{VERIFY_FIRST_PRESENT_ARGUMENT} cannot be combined with a supervised desktop launch"
        );
        return ExitCode::FAILURE;
    }
    let project_root = match supervised_candidate.as_ref() {
        Some(candidate) => open_explicit_project_root(candidate.context().project_root()),
        None => open_project_root(),
    };
    let project_root = match project_root {
        Ok(project_root) => project_root,
        Err(error) => {
            eprintln!("failed to open project source: {error}");
            return ExitCode::FAILURE;
        }
    };
    let supervised_session = match supervised_candidate {
        Some(candidate) => match candidate.verify_project_source(&project_root) {
            Ok(session) => Some(session),
            Err(error) => {
                eprintln!("failed to revalidate supervised desktop source facts: {error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let profile = supervised_session
        .as_ref()
        .map(|session| session.context().profile_name().to_owned())
        .unwrap_or_else(|| "desktop".to_owned());
    let intent = DesktopRunIntent::new()
        .with_profile(profile)
        .with_recipe(nara_game::product_recipe());
    let intent = match supervised_session {
        Some(session) => {
            let product_identity = match SupervisedDesktopProductIdentity::from_cargo_product(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                DESKTOP_BINARY_TARGET,
            ) {
                Ok(identity) => identity,
                Err(error) => {
                    eprintln!("failed to construct desktop product identity: {error}");
                    return ExitCode::FAILURE;
                }
            };
            intent.with_supervised_desktop_activation(session, product_identity)
        }
        None if verify_first_present => intent.with_first_present_verification(),
        None => intent,
    };
    let mut run = DesktopRun::new(project_root, intent);
    let deadline = Instant::now() + std::time::Duration::from_secs(CLEANUP_DEADLINE_SECONDS);

    loop {
        let report = run.execute();
        match report.outcome() {
            DesktopRunOutcome::Completed(_) if !verify_first_present => return ExitCode::SUCCESS,
            DesktopRunOutcome::Completed(_) if report.presentation_verification().is_some() => {
                let Some(closure) = report.startup_content_closure() else {
                    eprintln!("desktop run completed without startup-content evidence");
                    return ExitCode::FAILURE;
                };
                if let Err(error) = nara_game::record_product_observation(
                    nara_game::ProductObservationTarget::Desktop,
                    closure,
                ) {
                    eprintln!("failed to record desktop product observation: {error}");
                    return ExitCode::FAILURE;
                }
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
