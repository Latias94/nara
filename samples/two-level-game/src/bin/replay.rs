use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use two_level_game::replay::{
    ReplayError, decode_program, decode_recording, encode_recording, record_program,
    replay_recording,
};

#[path = "replay/publication.rs"]
mod publication;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const RESULT_PREFIX: &str = "TWO_LEVEL_REPLAY_RESULT ";
const USAGE: &str = "usage: replay record --project-root ABS --program ABS --output ABS | replay play --project-root ABS --recording ABS";

enum Arguments {
    Record {
        project_root: PathBuf,
        program: PathBuf,
        output: PathBuf,
    },
    Play {
        project_root: PathBuf,
        recording: PathBuf,
    },
}

struct Failure {
    operation: &'static str,
    stage: &'static str,
    message: String,
    runtime: Option<ReplayError>,
    publication: Option<Box<publication::PublicationFailure>>,
}

impl Failure {
    fn new(operation: &'static str, stage: &'static str, message: impl ToString) -> Self {
        Self {
            operation,
            stage,
            message: message.to_string(),
            runtime: None,
            publication: None,
        }
    }

    fn runtime(operation: &'static str, mut error: ReplayError) -> Self {
        let deadline = Instant::now() + Duration::from_secs(10);
        while error.cleanup_pending() && Instant::now() < deadline {
            error.retry_cleanup();
            if error.cleanup_pending() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Self {
            operation,
            stage: operation,
            message: error.to_string(),
            runtime: Some(error),
            publication: None,
        }
    }

    fn publication(mut error: publication::PublicationFailure) -> Self {
        error.retry_cleanup();
        Self {
            operation: "record",
            stage: "output",
            message: error.to_string(),
            runtime: None,
            publication: Some(Box::new(error)),
        }
    }

    fn result(&self) -> Value {
        let mut result = json!({
            "operation": self.operation,
            "ok": false,
            "cleanup_pending": self.runtime.as_ref().is_some_and(ReplayError::cleanup_pending)
                || self.publication.as_ref().is_some_and(|failure| failure.cleanup_pending()),
            "error": { "stage": self.stage, "message": self.message },
        });
        if self.operation == "record" {
            result["publication"] = self.publication.as_ref().map_or_else(
                || publication::Publication::not_published().result(),
                |failure| failure.publication().result(),
            );
        }
        result
    }
}

/// Keeps unresolved temporary-file custody alive until process exit, including failed stdout.
impl Drop for Failure {
    fn drop(&mut self) {
        if let Some(publication) = self.publication.take() {
            (*publication).retain_cleanup_until_exit();
        }
    }
}

impl Arguments {
    fn parse(mut arguments: impl Iterator<Item = OsString>) -> Result<Self, Failure> {
        let operation = match arguments.next().as_deref().and_then(|value| value.to_str()) {
            Some("record") => "record",
            Some("play") => "play",
            _ => return Err(Failure::new("arguments", "arguments", USAGE)),
        };
        let mut project_root = None;
        let mut program = None;
        let mut output = None;
        let mut recording = None;
        while let Some(flag) = arguments.next() {
            let slot = match flag.to_str() {
                Some("--project-root") => &mut project_root,
                Some("--program") if operation == "record" => &mut program,
                Some("--output") if operation == "record" => &mut output,
                Some("--recording") if operation == "play" => &mut recording,
                _ => {
                    return Err(Failure::new(
                        operation,
                        "arguments",
                        format!("unexpected argument {}; {USAGE}", flag.to_string_lossy()),
                    ));
                }
            };
            if slot.is_some() {
                return Err(Failure::new(
                    operation,
                    "arguments",
                    format!("duplicate argument {}", flag.to_string_lossy()),
                ));
            }
            let path = PathBuf::from(arguments.next().ok_or_else(|| {
                Failure::new(
                    operation,
                    "arguments",
                    format!("{} requires an absolute path", flag.to_string_lossy()),
                )
            })?);
            if !path.is_absolute() {
                return Err(Failure::new(
                    operation,
                    "arguments",
                    format!("{} requires an absolute path", flag.to_string_lossy()),
                ));
            }
            *slot = Some(path);
        }
        let required = |path: Option<PathBuf>, flag: &str| {
            path.ok_or_else(|| {
                Failure::new(operation, "arguments", format!("missing {flag}; {USAGE}"))
            })
        };
        let project_root = required(project_root, "--project-root")?;
        if operation == "record" {
            Ok(Self::Record {
                project_root,
                program: required(program, "--program")?,
                output: required(output, "--output")?,
            })
        } else {
            Ok(Self::Play {
                project_root,
                recording: required(recording, "--recording")?,
            })
        }
    }
}

fn read_input(path: &Path, operation: &'static str) -> Result<Vec<u8>, Failure> {
    let file = File::open(path).map_err(|error| {
        Failure::new(
            operation,
            "input",
            format!("open {}: {error}", path.display()),
        )
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            Failure::new(
                operation,
                "input",
                format!("read {}: {error}", path.display()),
            )
        })?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(Failure::new(operation, "input", "input exceeds 16 MiB"));
    }
    Ok(bytes)
}

fn run(arguments: Arguments) -> Result<Value, Failure> {
    match arguments {
        Arguments::Record {
            project_root,
            program,
            output,
        } => {
            let output_value = serde_json::to_value(&output)
                .map_err(|error| Failure::new("record", "output", error))?;
            let bytes = read_input(&program, "record")?;
            let program = decode_program(&bytes)
                .map_err(|error| Failure::new("record", "decode_program", error))?;
            let recording = record_program(&project_root, &program)
                .map_err(|error| Failure::runtime("record", error))?;
            let bytes = encode_recording(&recording)
                .map_err(|error| Failure::new("record", "encode_recording", error))?;
            let final_state = recording
                .steps
                .last()
                .map_or(&recording.initial, |step| &step.after);
            let publication =
                publication::publish(&output, &bytes).map_err(Failure::publication)?;
            Ok(json!({
                "operation": "record",
                "ok": true,
                "cleanup_pending": false,
                "output": output_value,
                "publication": publication.result(),
                "report": {
                    "steps": recording.steps.len(),
                    "commands": recording.steps.iter().map(|step| step.commands.len()).sum::<usize>(),
                    "final_state": final_state,
                },
            }))
        }
        Arguments::Play {
            project_root,
            recording,
        } => {
            let bytes = read_input(&recording, "play")?;
            let recording = decode_recording(&bytes)
                .map_err(|error| Failure::new("play", "decode_recording", error))?;
            let report = replay_recording(&project_root, &recording)
                .map_err(|error| Failure::runtime("play", error))?;
            let report = serde_json::to_value(report)
                .map_err(|error| Failure::new("play", "result", error))?;
            Ok(json!({
                "operation": "play",
                "ok": true,
                "cleanup_pending": false,
                "report": report,
            }))
        }
    }
}

fn write_outcome(
    outcome: &Result<Value, Failure>,
    stdout: &mut impl Write,
) -> std::io::Result<ExitCode> {
    let failure_result;
    let (result, status) = match outcome {
        Ok(result) => (result, ExitCode::SUCCESS),
        Err(failure) => {
            failure_result = failure.result();
            (&failure_result, ExitCode::FAILURE)
        }
    };
    writeln!(stdout, "{RESULT_PREFIX}{result}")?;
    stdout.flush()?;
    Ok(status)
}

fn main() -> ExitCode {
    let outcome = Arguments::parse(std::env::args_os().skip(1)).and_then(run);
    let mut stdout = std::io::stdout().lock();
    match write_outcome(&outcome, &mut stdout) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("write replay result: {error}");
            ExitCode::FAILURE
        }
    }
}
