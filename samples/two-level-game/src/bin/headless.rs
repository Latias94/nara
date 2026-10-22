use std::{num::NonZeroU32, path::PathBuf, process::ExitCode, time::Duration};

use nara::project_host::{HeadlessProductRun, HeadlessRunIntent, HeadlessRunOutcome};
use two_level_game::{GamePhase, GameSnapshot, RunOptions, Scenario};

fn main() -> ExitCode {
    if let Some(exit) =
        nara::project_tooling::run_project_tool_mode(std::env::args_os().skip(1), || {
            Ok::<_, std::convert::Infallible>(two_level_game::product_recipe())
        })
    {
        return exit;
    }
    let options = match arguments() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let scripted = options.scenario != Scenario::Interactive;
    let intent = HeadlessRunIntent::frames(
        NonZeroU32::new(if scripted { 200_000 } else { 1 }).unwrap(),
        Duration::from_millis(20),
    )
    .with_recipe(two_level_game::configured_recipe(options.clone()))
    .stop_when(move |snapshot: &GameSnapshot| {
        snapshot.error.is_some()
            || if scripted {
                snapshot.verified
            } else {
                snapshot.initialized
            }
    });
    let report = match HeadlessProductRun::<GameSnapshot>::new(
        env!("CARGO_MANIFEST_DIR"),
        intent,
        Vec::new(),
    )
    .run()
    {
        Ok(report) => report,
        Err(error) => {
            eprintln!("project source: {error}");
            return ExitCode::FAILURE;
        }
    };
    match report.outcome() {
        HeadlessRunOutcome::Completed(snapshot) => {
            if snapshot.error.is_some()
                || !snapshot.initialized
                || snapshot.phase != GamePhase::Running
                || (scripted && !snapshot.verified)
            {
                eprintln!("unexpected gameplay result: {snapshot:?}");
                return ExitCode::FAILURE;
            }
            if options.scenario == Scenario::Save {
                let mut slot = match two_level_game::save::io::SaveSlot::open(
                    options.save_path.as_ref().unwrap(),
                ) {
                    Ok(slot) => slot,
                    Err(error) => {
                        eprintln!("save readback: {error}");
                        return ExitCode::FAILURE;
                    }
                };
                let saved = match slot.load() {
                    Ok(saved) => saved.checkpoint,
                    Err(error) => {
                        eprintln!("save decode: {error}");
                        return ExitCode::FAILURE;
                    }
                };
                if saved.score != 30
                    || saved.collected.len() != 3
                    || saved.markers.len() != 3
                    || saved.next_marker_id.get() != 4
                    || saved.location.level
                        != two_level_game::levels::LevelKey::main(
                            two_level_game::levels::LevelId::Cavern,
                        )
                {
                    eprintln!("published checkpoint disagrees with gameplay: {saved:?}");
                    return ExitCode::FAILURE;
                }
            }
            let Some(closure) = report.startup_content_closure() else {
                eprintln!("missing startup evidence");
                return ExitCode::FAILURE;
            };
            if let Err(error) = two_level_game::record_product_observation(
                two_level_game::ProductObservationTarget::Headless,
                closure,
            ) {
                eprintln!("product observation: {error}");
                return ExitCode::FAILURE;
            }
            println!(
                "two_level_headless: ok scenario={:?} level={:?} score={} collected={} markers={:?} travels={} saves={} restores={} verified={}",
                options.scenario,
                snapshot.level,
                snapshot.score,
                snapshot.collected,
                snapshot.markers,
                snapshot.travel_count,
                snapshot.saves,
                snapshot.restores,
                snapshot.verified
            );
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("headless run failed: {:?}", report.diagnostics());
            return ExitCode::FAILURE;
        }
    }
}

fn arguments() -> Result<RunOptions, &'static str> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let (scenario, save_path) = match arguments.as_slice() {
        [] => (Scenario::Interactive, None),
        [flag] if flag == "--verify-travel" => (Scenario::Travel, None),
        [flag, path] if flag == "--verify-save" || flag == "--verify-restore" => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err("save path must be absolute; create its parent explicitly");
            }
            (
                if flag == "--verify-save" {
                    Scenario::Save
                } else {
                    Scenario::Restore
                },
                Some(path),
            )
        }
        _ => {
            return Err(
                "usage: headless [--verify-travel | --verify-save ABSOLUTE_PATH | --verify-restore ABSOLUTE_PATH]",
            );
        }
    };
    Ok(RunOptions {
        scenario,
        save_path,
    })
}
