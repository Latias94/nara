#![forbid(unsafe_code)]

#[path = "player/arguments.rs"]
mod arguments;
#[cfg(any(nara_cooked_player, test))]
#[path = "server/clock.rs"]
mod clock;
#[cfg(any(nara_cooked_player, test))]
#[path = "server/control.rs"]
mod control;
#[cfg(nara_cooked_player)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/nara-player-bootstrap.rs"));
}
#[cfg(nara_cooked_player)]
#[path = "server/output.rs"]
mod output;
#[cfg(nara_cooked_player)]
#[path = "server/regions.rs"]
mod regions;
#[cfg(nara_cooked_player)]
#[path = "server/runtime.rs"]
mod runtime;
#[cfg(nara_cooked_player)]
#[path = "server/service.rs"]
mod service;

use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use serde_json::{Value, json};
use two_level_game::{RunOptions, Scenario};

const USAGE: &str = "usage: server [--serve [--paused] [--save ABSOLUTE_PATH] | --serve --paused --resume-state ABSOLUTE_PATH | --verify-travel | --verify-save ABSOLUTE_PATH | --verify-restore ABSOLUTE_PATH | --verify-regions]";
const RESULT: &str = "TWO_LEVEL_SERVER_RESULT";

enum Invocation {
    Player(RunOptions),
    Serve(#[cfg_attr(not(nara_cooked_player), allow(dead_code))] ServeOptions),
    Regions,
}

#[derive(Default)]
#[cfg_attr(not(nara_cooked_player), allow(dead_code))]
struct ServeOptions {
    save_path: Option<PathBuf>,
    paused: bool,
    resume_state: Option<PathBuf>,
}

impl Invocation {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut arguments = arguments.into_iter().peekable();
        if arguments.next_if(|flag| flag == "--serve").is_some() {
            return ServeOptions::parse(arguments).map(Self::Serve);
        }
        if arguments
            .next_if(|flag| flag == "--verify-regions")
            .is_some()
        {
            return if arguments.next().is_none() {
                Ok(Self::Regions)
            } else {
                Err(USAGE)
            };
        }
        arguments::parse(arguments)
            .map(Self::Player)
            .map_err(|message| {
                if message.starts_with("usage:") {
                    USAGE
                } else {
                    message
                }
            })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Player(options) => match options.scenario {
                Scenario::Interactive => "interactive",
                Scenario::Travel => "travel",
                Scenario::Save => "save",
                Scenario::Restore => "restore",
            },
            Self::Serve(_) => "serve",
            Self::Regions => "regions",
        }
    }
}

impl ServeOptions {
    fn parse(mut arguments: impl Iterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut options = Self::default();
        while let Some(flag) = arguments.next() {
            if flag == "--paused" && !options.paused {
                options.paused = true;
            } else if flag == "--save" && options.save_path.is_none() {
                options.save_path =
                    Some(arguments::parse_save_path(arguments.next().ok_or(USAGE)?)?);
            } else if flag == "--resume-state" && options.resume_state.is_none() {
                let path = PathBuf::from(arguments.next().ok_or(USAGE)?);
                if !path.is_absolute() {
                    return Err("Server resume state path must be absolute");
                }
                options.resume_state = Some(path);
            } else {
                return Err(USAGE);
            }
        }
        if options.resume_state.is_some() && (!options.paused || options.save_path.is_some()) {
            return Err("Server --resume-state requires --paused and conflicts with --save");
        }
        Ok(options)
    }
}

struct Report {
    scenario: Option<&'static str>,
    stage: &'static str,
    ready: bool,
    close: &'static str,
    cleanup_complete: bool,
    state: Option<Value>,
    visited_levels: Value,
    checkpoint: Option<Value>,
    server_evidence: Option<Value>,
}

impl Report {
    fn new(scenario: Option<&'static str>, stage: &'static str) -> Self {
        Self {
            scenario,
            stage,
            ready: false,
            close: "not_started",
            cleanup_complete: true,
            state: None,
            visited_levels: json!([]),
            checkpoint: None,
            server_evidence: None,
        }
    }

    fn emit(self, status: &'static str, message: Option<&str>) -> ExitCode {
        let result = self.into_record(status, message);
        if let Some(message) = result["message"].as_str() {
            let _ = writeln!(io::stderr().lock(), "two_level_server: {message}");
        }
        let mut output = io::stdout().lock();
        let written = writeln!(output, "{RESULT} {result}").and_then(|()| output.flush());
        if status == "ok" && written.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }

    fn into_record(self, status: &'static str, message: Option<&str>) -> Value {
        let message = message.map(|message| message.chars().take(2048).collect::<String>());
        let initialized = self
            .state
            .as_ref()
            .is_some_and(|state| state["initialized"] == true);
        json!({
            "status": status,
            "success": status == "ok",
            "scenario": self.scenario,
            "stage": self.stage,
            "ready": self.ready,
            "initialized": initialized,
            "close": self.close,
            "cleanup_complete": self.cleanup_complete,
            "source_free": true,
            "processed_runtime": self.ready,
            "state": self.state,
            "visited_levels": self.visited_levels,
            "checkpoint": self.checkpoint,
            "server_evidence": self.server_evidence,
            "message": message,
            "error": message,
        })
    }
}

fn main() -> ExitCode {
    let invocation = match Invocation::parse(std::env::args_os().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => return Report::new(None, "arguments").emit("rejected", Some(message)),
    };
    #[cfg(nara_cooked_player)]
    {
        runtime::run(invocation)
    }
    #[cfg(not(nara_cooked_player))]
    {
        Report::new(Some(invocation.name()), "bootstrap").emit(
            "rejected",
            Some("Server requires a generated cooked-content expectation from the two-level game cooker"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_mode_keeps_checkpoint_selection_explicit() {
        let path = std::env::temp_dir().join("server-checkpoint.json");
        let Ok(Invocation::Serve(options)) =
            Invocation::parse(["--serve".into(), "--save".into(), path.clone().into()])
        else {
            panic!("expected sustained operation with a checkpoint");
        };
        assert_eq!(options.save_path, Some(path));
        assert!(!options.paused);
        assert!(options.resume_state.is_none());
        assert!(matches!(
            Invocation::parse(["--serve".into()]),
            Ok(Invocation::Serve(ServeOptions {
                save_path: None,
                paused: false,
                resume_state: None,
            }))
        ));
        for arguments in [
            vec!["--serve", "--save"],
            vec!["--serve", "--save", "relative.json"],
            vec!["--serve", "--verify-regions"],
            vec!["--verify-regions", "--serve"],
        ] {
            assert!(Invocation::parse(arguments.into_iter().map(OsString::from)).is_err());
        }
    }

    #[test]
    fn exact_continuation_requires_an_explicit_paused_service_without_a_save_slot() {
        let path = std::env::temp_dir().join("server-continuation.json");
        let Ok(Invocation::Serve(options)) = Invocation::parse([
            "--serve".into(),
            "--paused".into(),
            "--resume-state".into(),
            path.clone().into(),
        ]) else {
            panic!("expected a paused continuation candidate");
        };
        assert!(options.paused);
        assert_eq!(options.resume_state.as_deref(), Some(path.as_path()));
        assert!(options.save_path.is_none());

        for arguments in [
            vec![
                "--serve".into(),
                "--resume-state".into(),
                path.clone().into(),
            ],
            vec!["--serve".into(), "--paused".into(), "--resume-state".into()],
            vec![
                "--serve".into(),
                "--paused".into(),
                "--resume-state".into(),
                "relative.json".into(),
            ],
            vec![
                "--serve".into(),
                "--paused".into(),
                "--resume-state".into(),
                path.clone().into(),
                "--save".into(),
                path.clone().into(),
            ],
            vec!["--serve".into(), "--paused".into(), "--paused".into()],
        ] {
            assert!(Invocation::parse(arguments).is_err());
        }
    }
}
