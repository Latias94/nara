//! Finite source-game packets shared by the development Host and standalone game.
//!
//! The Host privately owns the control directory and process/build identities. Runtime
//! generations below are process-local observations, never cross-process authority.
//! Both peers enforce byte/shape limits before DTO decoding; derived Serde is not that check.
//! Replies use Serde's ordinary `Result` representation: `{"Ok": reply}` or `{"Err": message}`.
//! The Host consumes replies before retrying their filenames; the child never overwrites one.

pub use crate::command::SemanticCommand;
use crate::resume::ResumeState;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 6;
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
    SourceDesktopAudio,
}

impl DevelopmentPlacement {
    pub const fn argument(self) -> &'static str {
        match self {
            Self::SourceHeadless => "headless",
            Self::SourceDesktop => "desktop",
            Self::SourceDesktopAudio => "audio-desktop",
        }
    }

    pub const fn is_desktop(self) -> bool {
        matches!(self, Self::SourceDesktop | Self::SourceDesktopAudio)
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

/// Original child audio diagnostics, without PCM, native handles or control authority.
///
/// Runtime generations are process-local observations. Physical audio flags do not replace
/// the original native process cleanup receipt or grant permission to enable output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioStatus {
    pub runtime_generation: u64,
    pub reset_on_rebuild: bool,
    pub output_enabled: bool,
    pub suppressed_effects: u64,
    pub accepted: u64,
    pub started: u64,
    pub completed: u64,
    pub cancelled: u64,
    pub faults: u64,
    pub failed: bool,
    pub callbacks: u64,
    pub output_frames: u64,
    pub admission_open: bool,
    pub physical_complete: bool,
    pub stream_drop_returned: bool,
    pub data_callback_released: bool,
    pub error_callback_released: bool,
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
    AudioStatus,
    EnableAudio,
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
    Audio {
        status: AudioStatus,
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
    AudioClosed {
        failed: bool,
        status: AudioStatus,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn game_audio_placement_and_commands_have_explicit_wire_values() {
        assert_eq!(PROTOCOL_VERSION, 6);
        assert_eq!(
            DevelopmentPlacement::default(),
            DevelopmentPlacement::SourceHeadless
        );
        assert!(!DevelopmentPlacement::SourceHeadless.is_desktop());
        assert!(DevelopmentPlacement::SourceDesktop.is_desktop());
        assert!(DevelopmentPlacement::SourceDesktopAudio.is_desktop());
        assert_eq!(
            DevelopmentPlacement::SourceDesktopAudio.argument(),
            "audio-desktop"
        );
        assert_eq!(
            serde_json::to_value(DevelopmentPlacement::SourceDesktopAudio).unwrap(),
            json!("source_desktop_audio"),
        );
        for (command, operation) in [
            (Command::AudioStatus, "audio_status"),
            (Command::EnableAudio, "enable_audio"),
        ] {
            let wire = json!({ "operation": operation });
            assert_eq!(serde_json::to_value(&command).unwrap(), wire);
            assert_eq!(serde_json::from_value::<Command>(wire).unwrap(), command);
        }
    }

    #[test]
    fn audio_diagnostics_preserve_game_failure_without_native_faults() {
        let wire = json!({
            "runtime_generation": 1,
            "reset_on_rebuild": true,
            "output_enabled": false,
            "suppressed_effects": 3,
            "accepted": 0,
            "started": 0,
            "completed": 0,
            "cancelled": 0,
            "faults": 0,
            "failed": true,
            "callbacks": 2,
            "output_frames": 1024,
            "admission_open": false,
            "physical_complete": true,
            "stream_drop_returned": true,
            "data_callback_released": true,
            "error_callback_released": true,
        });
        let status: AudioStatus = serde_json::from_value(wire.clone()).unwrap();
        let copied = status;
        assert_eq!(copied, status);
        assert_eq!(serde_json::to_value(status).unwrap(), wire);
        assert!(status.failed);
        assert_eq!(status.faults, 0);
        for reply in [
            Reply::Audio { status },
            Reply::AudioClosed {
                failed: true,
                status,
            },
            Reply::Closed { failed: false },
        ] {
            let encoded = serde_json::to_vec(&reply).unwrap();
            assert_eq!(serde_json::from_slice::<Reply>(&encoded).unwrap(), reply);
        }
        let mut missing_failure = wire.clone();
        missing_failure.as_object_mut().unwrap().remove("failed");
        assert!(serde_json::from_value::<AudioStatus>(missing_failure).is_err());
        for unknown in ["samples", "pcm", "error", "native_handle"] {
            let mut invalid = wire.clone();
            invalid[unknown] = json!([]);
            assert!(serde_json::from_value::<AudioStatus>(invalid).is_err());
        }
    }
}
