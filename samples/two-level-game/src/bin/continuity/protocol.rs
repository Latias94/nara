//! Finite source-game packets shared with the actual-build test Host via `#[path]`.
//!
//! The Host privately owns the control directory and process/build identities. Runtime
//! generations below are process-local observations, never cross-process authority.
//! Both peers enforce byte/shape limits before DTO decoding; derived Serde is not that check.
//! Replies use Serde's ordinary `Result` representation: `{"Ok": reply}` or `{"Err": message}`.
//! The Host consumes replies before retrying their filenames; the child never overwrites one.

use serde::{Deserialize, Serialize};
use two_level_game::resume::ResumeState;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub const MAX_IDENTITY_BYTES: usize = 128;
pub const MAX_STEP_COMMANDS: usize = 8;
pub const READY_FILE: &str = "ready.json";
pub const REQUEST_FILE: &str = "request.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub session: String,
    pub generation: String,
    pub request: u64,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Observe,
    Step { commands: Vec<SemanticCommand> },
    Frame,
    Capture,
    Restore { state: ResumeState },
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticCommand {
    Move { direction: usize, pressed: bool },
    Marker,
    Travel,
    Instance,
    Retry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub session: String,
    pub generation: String,
    pub request: u64,
    pub result: Result<Reply, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Observed { observation: Observation },
    Captured { state: ResumeState },
    Closed { failed: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub runtime_generation: u64,
    pub engine_tick: u64,
    pub logical_tick: u64,
    pub logical_elapsed_nanos: u64,
    pub player: [f32; 2],
    pub held: [bool; 4],
    pub score: u32,
    pub collected: usize,
    pub markers: Vec<u64>,
    /// One of `running`, `preparing`, `reconstructing`, or `degraded`.
    pub phase: String,
    pub restoring: bool,
    pub error: Option<String>,
}
