//! Caller-driven source development for the Two-Level Game's explicit execution placement.
//!
//! Usage: develop --project-root ABS --work-parent ABS --target-dir ABS --target TRIPLE
//!                [--profile dev] [--placement headless|desktop]
//! Supplying this project authorizes execution of its trusted Rust and Cargo build inputs.
//! stdout contains one ready record followed by newline-delimited JSON command results.
//! At most 128 commands are accepted. Lifetime checks run between commands and after stdin reads;
//! they cannot interrupt blocked stdin or the existing synchronous trusted Cargo adapter.

#[path = "develop/input.rs"]
mod input;
#[path = "develop/output.rs"]
mod output;

use std::{
    io::{self, Write},
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::project_tooling::cargo_build::{CargoBuildFailure, CargoBuildTools, build_executable};
use serde_json::{Value, json};
use two_level_game::development::{
    ActiveGeneration, DevelopmentHost, PreparedRebuild, ProbeStep, RetirementRole, RetirementStatus,
};

use input::{Arguments, Command};

const MAX_COMMANDS: usize = 128;
const MAX_CLEANUP_PASSES: usize = 3;
const SESSION_ALLOWANCE: Duration = Duration::from_secs(20 * 60);
const IDLE_ALLOWANCE: Duration = Duration::from_secs(10 * 60);

fn main() -> ExitCode {
    let stdout = io::stdout();
    run(&mut stdout.lock())
}

fn run(stdout: &mut impl Write) -> ExitCode {
    let arguments = match Arguments::parse() {
        Ok(arguments) => arguments,
        Err(error) => return startup_error(stdout, output::error("arguments", &error)),
    };
    let tools = match CargoBuildTools::discover_in(&arguments.project_root) {
        Ok(tools) => tools,
        Err(failure) => return startup_build_failure(stdout, failure),
    };
    let request = match arguments.build_request(&tools) {
        Ok(request) => request,
        Err(error) => return startup_error(stdout, output::error("inventory", &error)),
    };
    let captured = match build_executable(&request, |_| Ok(())) {
        Ok(captured) => captured,
        Err(failure) => return startup_build_failure(stdout, failure),
    };
    let mut host = match DevelopmentHost::with_placement(
        &arguments.project_root,
        &arguments.work_parent,
        arguments.placement,
    ) {
        Ok(host) => host,
        Err(error) => return startup_error(stdout, output::development_error(&error, None)),
    };
    let before = host.active();
    let ready = host
        .start(captured)
        .map_err(|error| output::development_error(&error, host.last_build_failure()))
        .and_then(|observation| {
            output::observation(&observation).map_err(|error| output::error("observation", &error))
        });
    let stop = match ready {
        Ok(observation) => {
            let ready = json!({
                "operation": "ready",
                "ok": true,
                "placement": arguments.placement,
                "active_before": output::generation(before),
                "active_after": output::generation(host.active()),
                "observation": observation,
                "selection": {
                    "package": "two-level-game",
                    "bin": "continuity",
                    "profile": arguments.profile,
                    "target": arguments.target,
                    "default_features": false,
                    "features": request.selection().features(),
                },
                "limits": {
                    "commands": MAX_COMMANDS,
                    "line_bytes": input::MAX_LINE_BYTES,
                    "state_bytes": output::MAX_STATE_BYTES,
                    "response_bytes": output::MAX_RESPONSE_BYTES,
                    "probe_steps": input::MAX_PROBES,
                    "cleanup_passes": MAX_CLEANUP_PASSES,
                    "idle_seconds": IDLE_ALLOWANCE.as_secs(),
                    "session_seconds": SESSION_ALLOWANCE.as_secs(),
                    "lifetime_checks": "between_commands_and_after_stdin_reads",
                },
            });
            match output::emit(stdout, &ready) {
                Ok(()) => serve(&mut host, &arguments, &tools, stdout),
                Err(error) => Stop::failed(
                    "output",
                    None,
                    host.active(),
                    output::error("output", &error),
                ),
            }
        }
        Err(error) => Stop::failed("startup", None, before, error),
    };
    finish(host, stdout, stop)
}

struct Stop {
    operation: &'static str,
    command: Option<usize>,
    active_before: Option<ActiveGeneration>,
    error: Option<Value>,
}

impl Stop {
    fn failed(
        operation: &'static str,
        command: Option<usize>,
        active_before: Option<ActiveGeneration>,
        error: Value,
    ) -> Self {
        Self {
            operation,
            command,
            active_before,
            error: Some(error),
        }
    }
}

fn serve(
    host: &mut DevelopmentHost,
    arguments: &Arguments,
    tools: &CargoBuildTools,
    stdout: &mut impl Write,
) -> Stop {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut prepared = None;
    let started = Instant::now();
    let mut activity = started;
    for number in 1..=MAX_COMMANDS {
        if started.elapsed() >= SESSION_ALLOWANCE || activity.elapsed() >= IDLE_ALLOWANCE {
            return Stop::failed(
                "limit",
                None,
                host.active(),
                output::error(
                    "limit",
                    "the finite session or idle allowance was exhausted",
                ),
            );
        }
        let command = match input::read_command(&mut stdin) {
            Ok(Some(command)) => command,
            Ok(None) => {
                return Stop {
                    operation: "eof",
                    command: None,
                    active_before: host.active(),
                    error: None,
                };
            }
            Err(error) => {
                return Stop::failed(
                    "input",
                    Some(number),
                    host.active(),
                    output::error("input", &error),
                );
            }
        };
        if started.elapsed() >= SESSION_ALLOWANCE || activity.elapsed() >= IDLE_ALLOWANCE {
            return Stop::failed(
                "limit",
                Some(number),
                host.active(),
                output::error(
                    "limit",
                    "the finite allowance expired while waiting for stdin; this command was not executed",
                ),
            );
        }
        let before = host.active();
        if matches!(command, Command::Close {}) {
            return Stop {
                operation: "close",
                command: Some(number),
                active_before: before,
                error: None,
            };
        }
        let operation = command.operation();
        let result = execute(host, &mut prepared, arguments, tools, command);
        let mut response = match result {
            Ok(result) => json!({
                "operation": operation,
                "command": number,
                "ok": true,
                "result": result,
            }),
            Err(error) => json!({
                "operation": operation,
                "command": number,
                "ok": false,
                "error": error,
            }),
        };
        response["active_before"] = output::generation(before);
        response["active_after"] = output::generation(host.active());
        response["prepared"] =
            output::generation(prepared.as_ref().map(PreparedRebuild::generation));
        response["cleanup_error"] = host.cleanup_error().map_or(Value::Null, output::text);
        response["retirements"] = output::retirements(host.retirements());
        if let Err(error) = output::emit(stdout, &response) {
            return Stop::failed(
                "output",
                Some(number),
                before,
                output::error("output", &error),
            );
        }
        activity = Instant::now();
    }
    Stop::failed(
        "limit",
        None,
        host.active(),
        output::error("limit", "the finite 128-command allowance was exhausted"),
    )
}

fn execute(
    host: &mut DevelopmentHost,
    prepared: &mut Option<PreparedRebuild>,
    arguments: &Arguments,
    tools: &CargoBuildTools,
    command: Command,
) -> Result<Value, Value> {
    match command {
        Command::Observe {} => Ok(json!({ "capture": capture(host)? })),
        Command::Step { commands } => {
            let observed = host
                .step(commands)
                .map_err(|error| output::development_error(&error, host.last_build_failure()))?;
            let observed = output::observation(&observed)
                .map_err(|error| output::error("observation", &error))?;
            Ok(with_capture(host, json!({ "observation": observed })))
        }
        Command::Prepare { basis, probes } => {
            if prepared.is_some() {
                return Err(output::error(
                    "state",
                    "adopt or cancel the existing prepared candidate before preparing another",
                ));
            }
            let request = arguments
                .build_request(tools)
                .map_err(|error| output::error("inventory", &error))?;
            let probes = probes
                .into_iter()
                .map(|probe| ProbeStep {
                    commands: probe.commands,
                    expected: probe.expected,
                })
                .collect::<Vec<_>>();
            match host.prepare_rebuild(&request, &basis, &probes) {
                Ok(candidate) => {
                    let generation = candidate.generation();
                    let presentation = candidate.presentation().cloned();
                    *prepared = Some(candidate);
                    let basis =
                        output::state(&basis).map_err(|error| output::error("state", &error))?;
                    Ok(with_capture(
                        host,
                        json!({
                            "candidate": output::generation(Some(generation)),
                            "presentation": presentation,
                            "basis": basis,
                            "probe_outcome": {
                                "status": "all_matched",
                                "steps": probes.len(),
                                "comparison": "complete_resume_state",
                            },
                        }),
                    ))
                }
                Err(error) => {
                    let mut failure = output::development_error(&error, host.last_build_failure());
                    failure["probe_outcome"] = json!({
                        "status": "not_qualified",
                        "requested_steps": probes.len(),
                    });
                    Err(with_capture(host, failure))
                }
            }
        }
        Command::Adopt {} => {
            let candidate = prepared
                .as_ref()
                .ok_or_else(|| output::error("state", "there is no prepared candidate to adopt"))?;
            let outcome = host
                .adopt(candidate)
                .map_err(|error| output::development_error(&error, host.last_build_failure()))?;
            *prepared = None;
            Ok(with_capture(
                host,
                json!({
                    "committed": true,
                    "active": output::generation(Some(outcome.active)),
                    "predecessor": output::generation(Some(outcome.predecessor)),
                    "retirement": output::retirement(outcome.retirement),
                }),
            ))
        }
        Command::Cancel {} => {
            let candidate = prepared.as_ref().ok_or_else(|| {
                output::error("state", "there is no prepared candidate to cancel")
            })?;
            let retirement = host
                .cancel(candidate)
                .map_err(|error| output::development_error(&error, host.last_build_failure()))?;
            *prepared = None;
            Ok(json!({ "retirement": output::retirement(retirement) }))
        }
        Command::RetryCleanup {} => Ok(json!({
            "retirement": output::retirement(host.retry_retirement()),
        })),
        Command::IsolateInput {} => {
            let observation = host
                .isolate_active_input()
                .map_err(|error| output::development_error(&error, None))?;
            Ok(with_capture(host, json!({"observation": observation})))
        }
        Command::ConfirmPresentation {} => {
            let presentation = host
                .confirm_active_presentation()
                .map_err(|error| output::development_error(&error, None))?;
            Ok(json!({"presentation": presentation}))
        }
        Command::ActivateInput {} => {
            let result = host
                .activate_active()
                .map_err(|error| output::development_error(&error, None))?;
            Ok(with_capture(
                host,
                json!({
                    "active": output::generation(Some(result.active)),
                    "activation": result.activation,
                    "activation_error": result.error,
                }),
            ))
        }
        Command::Close {} => Err(output::error(
            "state",
            "close must retire the complete development session",
        )),
    }
}

fn capture(host: &mut DevelopmentHost) -> Result<Value, Value> {
    let state = host
        .capture()
        .map_err(|error| output::development_error(&error, host.last_build_failure()))?;
    output::state(&state).map_err(|error| output::error("capture", &error))
}

fn with_capture(host: &mut DevelopmentHost, mut result: Value) -> Value {
    match capture(host) {
        Ok(state) => result["capture"] = state,
        Err(error) => result["capture_error"] = error,
    }
    result
}

fn finish(mut host: DevelopmentHost, stdout: &mut impl Write, stop: Stop) -> ExitCode {
    let active = host.active();
    let mut retirement = RetirementStatus::Pending;
    let mut attempts = 0;
    for _ in 0..MAX_CLEANUP_PASSES {
        attempts += 1;
        retirement = host.close();
        if retirement == RetirementStatus::Complete {
            break;
        }
    }
    let game_cleanup_error = final_cleanup_error(
        active,
        retirement,
        host.retirements().iter().map(|report| {
            (
                report.generation,
                report.role,
                report.game_closed
                    && report.native.exit().success()
                    && !report.terminated
                    && !report.native.force_killed()
                    && report.native.containment_empty()
                    && report.native.readers_stopped()
                    && report.native.output_drained(),
            )
        }),
    );
    let success = stop.error.is_none() && game_cleanup_error.is_none();
    let response = json!({
        "operation": stop.operation,
        "command": stop.command,
        "ok": success,
        "active_before": output::generation(stop.active_before),
        "active_after": output::generation(host.active()),
        "retirement": output::retirement(retirement),
        "cleanup_attempts": attempts,
        "cleanup_error": host.cleanup_error().or(game_cleanup_error).map(output::text),
        "retirements": output::retirements(host.retirements()),
        "build_failure": host.last_build_failure().map(output::build_failure),
        "error": stop.error,
    });
    let emitted = output::emit(stdout, &response);
    if retirement == RetirementStatus::Pending {
        std::mem::forget(host);
    }
    if let Err(error) = emitted {
        eprintln!("develop: {error}");
        return ExitCode::FAILURE;
    }
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn final_cleanup_error(
    active: Option<ActiveGeneration>,
    retirement: RetirementStatus,
    reports: impl IntoIterator<Item = (ActiveGeneration, RetirementRole, bool)>,
) -> Option<&'static str> {
    if retirement == RetirementStatus::Pending {
        return Some("physical cleanup remains incomplete");
    }
    let mut active_retired = active.is_none();
    for (generation, role, normal) in reports {
        if !matches!(role, RetirementRole::Active | RetirementRole::Predecessor) {
            continue;
        }
        if !normal {
            return Some(
                "an active or predecessor game did not retire normally; inspect retirements",
            );
        }
        if role == RetirementRole::Active && Some(generation) == active {
            active_retired = true;
        }
    }
    if !active_retired {
        return Some("the current active game has no normal retirement report");
    }
    None
}

fn startup_error(stdout: &mut impl Write, error: Value) -> ExitCode {
    let response = json!({
        "operation": "startup",
        "ok": false,
        "active_before": null,
        "active_after": null,
        "retirement": null,
        "host_started": false,
        "error": error,
    });
    if let Err(error) = output::emit(stdout, &response) {
        eprintln!("develop: {error}");
    }
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::fs::ContentDigest;

    fn generation(sequence: u64) -> ActiveGeneration {
        ActiveGeneration {
            sequence,
            pid: 1,
            executable: ContentDigest::of_bytes(b"cleanup decision executable"),
            provenance: ContentDigest::of_bytes(b"cleanup decision provenance"),
        }
    }

    #[test]
    fn abnormal_disposable_history_does_not_fail_normal_active_and_predecessor_cleanup() {
        let current = generation(4);
        assert_eq!(
            final_cleanup_error(
                Some(current),
                RetirementStatus::Complete,
                [
                    (generation(1), RetirementRole::Probe, false),
                    (generation(2), RetirementRole::Candidate, false),
                    (generation(3), RetirementRole::Predecessor, true),
                    (current, RetirementRole::Active, true),
                ],
            ),
            None,
        );
    }

    #[test]
    fn abnormal_active_or_predecessor_fails_physically_complete_cleanup() {
        let current = generation(2);
        for reports in [
            [
                (generation(1), RetirementRole::Predecessor, true),
                (current, RetirementRole::Active, false),
            ],
            [
                (generation(1), RetirementRole::Predecessor, false),
                (current, RetirementRole::Active, true),
            ],
        ] {
            assert!(
                final_cleanup_error(Some(current), RetirementStatus::Complete, reports).is_some(),
            );
        }
    }

    #[test]
    fn predecessor_or_disposable_reports_cannot_replace_current_active_retirement() {
        let current = generation(2);
        for reports in [
            vec![],
            vec![(generation(1), RetirementRole::Active, true)],
            vec![(current, RetirementRole::Predecessor, true)],
            vec![(current, RetirementRole::Candidate, true)],
            vec![(current, RetirementRole::Probe, true)],
        ] {
            assert!(
                final_cleanup_error(Some(current), RetirementStatus::Complete, reports).is_some(),
            );
        }
    }

    #[test]
    fn pending_physical_cleanup_never_passes_even_with_normal_game_reports() {
        let current = generation(2);
        assert!(
            final_cleanup_error(
                Some(current),
                RetirementStatus::Pending,
                [
                    (generation(1), RetirementRole::Predecessor, true),
                    (current, RetirementRole::Active, true),
                ],
            )
            .is_some(),
        );
        assert!(final_cleanup_error(None, RetirementStatus::Pending, []).is_some());
    }
}

fn startup_build_failure(stdout: &mut impl Write, mut failure: CargoBuildFailure) -> ExitCode {
    let mut cleanup_error = None;
    let mut attempts = 0;
    if let CargoBuildFailure::CleanupIncomplete(custody) = &mut failure {
        for _ in 0..MAX_CLEANUP_PASSES {
            if custody.process_id().is_none() {
                break;
            }
            attempts += 1;
            match custody.retry_cleanup() {
                Ok(()) => cleanup_error = None,
                Err(error) => cleanup_error = Some(output::text(&error.to_string())),
            }
        }
    }
    let pending = matches!(&failure, CargoBuildFailure::CleanupIncomplete(custody) if custody.process_id().is_some());
    let response = json!({
        "operation": "startup",
        "ok": false,
        "active_before": null,
        "active_after": null,
        "retirement": if pending { "pending" } else { "complete" },
        "cleanup_attempts": attempts,
        "cleanup_error": cleanup_error,
        "error": output::build_failure(&failure),
    });
    if let Err(error) = output::emit(stdout, &response) {
        eprintln!("develop: {error}");
    }
    if pending {
        std::mem::forget(failure);
    }
    ExitCode::FAILURE
}
