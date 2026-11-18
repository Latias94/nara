#![forbid(unsafe_code)]

use std::{num::NonZeroU32, process::ExitCode};

#[cfg(nara_cooked_player)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/nara-player-bootstrap.rs"));
}

const MAX_FRAMES: u32 = 200_000;

fn main() -> ExitCode {
    match frames().and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("starter player: {message}");
            ExitCode::FAILURE
        }
    }
}

fn frames() -> Result<NonZeroU32, String> {
    let mut arguments = std::env::args_os().skip(1);
    let Some(flag) = arguments.next() else {
        return Ok(NonZeroU32::new(1).expect("the default frame count is nonzero"));
    };
    if flag != "--frames" {
        return Err("usage: player [--frames N]".into());
    }
    let count = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u32>().ok())
        .and_then(NonZeroU32::new)
        .filter(|count| count.get() <= MAX_FRAMES)
        .ok_or_else(|| format!("--frames requires an integer from 1 to {MAX_FRAMES}"))?;
    if arguments.next().is_some() {
        return Err("usage: player [--frames N]".into());
    }
    Ok(count)
}

#[cfg(not(nara_cooked_player))]
fn run(_frames: NonZeroU32) -> Result<(), String> {
    Err(
        "Player was built without a generated cooked-content expectation; build and publish it with this project's cook binary"
            .into(),
    )
}

#[cfg(nara_cooked_player)]
fn run(frames: NonZeroU32) -> Result<(), String> {
    use std::{io::Write, time::Duration};

    use nara::{
        app::RuntimeFrameStatus,
        fs::{ContentDigest, RelativePath},
        project_host::{
            HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome,
            open_executable_relative_runtime_pack,
        },
        runtime_content::{RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentTarget},
    };

    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(generated::CATALOG_LENGTH, generated::CATALOG_HASH),
        RuntimeContentTarget::new(generated::CONTENT_TARGET, generated::CONTENT_PROFILE)
            .map_err(|error| error.to_string())?,
    );
    let base = open_executable_relative_runtime_pack(
        &RelativePath::new(generated::PACK_FILE).map_err(|error| error.to_string())?,
        &expected,
        nara_game::product_recipe().map_err(|error| error.to_string())?,
        RuntimeContentLimits::default(),
    )
    .map_err(|error| error.to_string())?;
    let delta = base.plan().settings().runtime().fixed_time().timestep();
    let options = HeadlessRunOptions::frames(frames, delta);
    let mut run =
        HeadlessRun::<RuntimeFrameStatus>::from_runtime_content(base, options, Vec::new());
    let mut reported = false;
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) if !report.diagnostics().has_errors() => {
                println!(
                    "starter_player: ok frame_limit={} fixed_ticks={}",
                    frames.get(),
                    outcome.fixed_tick,
                );
                return Ok(());
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                if !reported {
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "starter player: retaining the original Runtime while cleanup completes",
                    );
                    reported = true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            HeadlessRunOutcome::Completed(_) | HeadlessRunOutcome::Failed => {
                return Err(format!("Runtime failed: {:#?}", report.diagnostics()));
            }
        }
    }
}
