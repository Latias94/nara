//! Finite source-game packets shared by the development Host and standalone game.
//!
//! The Host privately owns the control directory and process/build identities. Runtime
//! generations below are process-local observations, never cross-process authority.
//! Both peers enforce byte/shape limits before DTO decoding; derived Serde is not that check.
//! Replies use Serde's ordinary `Result` representation: `{"Ok": reply}` or `{"Err": message}`.
//! The Host consumes replies before retrying their filenames; the child never overwrites one.

use crate::resume::ResumeState;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 5;
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub const MAX_IDENTITY_BYTES: usize = 128;
pub const MAX_STEP_COMMANDS: usize = 8;
pub const READY_FILE: &str = "ready.json";
pub const REQUEST_FILE: &str = "request.json";

/// Explicit execution placement; compilation features do not select a live native owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DevelopmentPlacement {
    #[default]
    SourceHeadless,
    SourceDesktop,
}

impl DevelopmentPlacement {
    pub const fn argument(self) -> &'static str {
        match self {
            Self::SourceHeadless => "headless",
            Self::SourceDesktop => "desktop",
        }
    }
}

/// Observation of the original child's native activation, never cross-process authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputActivation {
    Isolated,
    AwaitingFocus,
    ReconciliationPending,
    Active,
    Unavailable,
}

/// The original managed execution state, independent of the game's travel phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Running,
    Paused,
}

/// Read-only current facts, without a paused capture or presentation guarantee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStatus {
    pub state: ExecutionState,
    pub input_activation: InputActivation,
    pub observation: Observation,
}

/// Original presentation joined to the exact sealed canonical game state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPresentation {
    pub runtime_generation: u64,
    pub app_frame: u64,
    pub render_frame: u64,
    pub window: u64,
    pub surface_generation: u64,
    pub backend_instance: u64,
    pub device_epoch: u64,
    pub state_digest: [u8; 32],
    pub adapter_backend: String,
    pub adapter_type: String,
}

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
    ExecutionStatus,
    Pause,
    Resume,
    Step { commands: Vec<SemanticCommand> },
    Frame,
    Capture,
    Restore { state: ResumeState },
    ConfirmPresentation,
    IsolateInput,
    ActivateInput,
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

/// Diagnostic control-service observations from one original child response.
///
/// These are not canonical game state, control authority, proof of native cleanup or a
/// performance guarantee. Counters may saturate, and flags do not replace owner results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlServiceObservation {
    pub inbox_checks: u64,
    pub idle_wait_calls: u64,
    pub idle_wait_nanos: u64,
    pub hints: u64,
    pub callbacks: u64,
    pub coalesced_hints: u64,
    pub native_wake_delivery_failures: u64,
    pub watcher_started: bool,
    pub watcher_joined: bool,
    pub counters_saturated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u32,
    pub placement: DevelopmentPlacement,
    pub session: String,
    pub generation: String,
    pub request: u64,
    pub control_service: ControlServiceObservation,
    pub result: Result<Reply, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Execution {
        status: ExecutionStatus,
    },
    Observed {
        observation: Observation,
    },
    Captured {
        state: ResumeState,
    },
    Presented {
        presentation: DesktopPresentation,
    },
    Activated {
        activation: InputActivation,
        error: Option<String>,
    },
    Closing,
    Closed {
        failed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub runtime_generation: u64,
    /// Original process-local App sequence, independent of simulation and canonical state.
    pub app_frame: u64,
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
