#![forbid(unsafe_code)]

#[path = "player/arguments.rs"]
mod arguments;
#[cfg(nara_cooked_player)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/nara-player-bootstrap.rs"));
}
#[cfg(nara_cooked_player)]
#[path = "server/regions.rs"]
mod regions;
#[cfg(nara_cooked_player)]
#[path = "server/runtime.rs"]
mod runtime;

use std::{
    ffi::OsString,
    io::{self, Write},
    process::ExitCode,
};

use serde_json::{Value, json};
use two_level_game::{RunOptions, Scenario};

const USAGE: &str = "usage: server [--verify-travel | --verify-save ABSOLUTE_PATH | --verify-restore ABSOLUTE_PATH | --verify-regions]";

enum Invocation {
    Player(RunOptions),
    Regions,
}

impl Invocation {
    fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut arguments = arguments.into_iter().peekable();
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
            Self::Regions => "regions",
        }
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
        let message = message.map(|message| message.chars().take(2048).collect::<String>());
        if let Some(message) = &message {
            let _ = writeln!(io::stderr().lock(), "two_level_server: {message}");
        }
        let success = status == "ok";
        let initialized = self
            .state
            .as_ref()
            .is_some_and(|state| state["initialized"] == true);
        let result = json!({
            "status": status,
            "success": success,
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
        });
        let mut output = io::stdout().lock();
        let written =
            writeln!(output, "TWO_LEVEL_SERVER_RESULT {result}").and_then(|()| output.flush());
        if success && written.is_ok() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
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
