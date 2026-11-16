//! Finite original-operation wall observations for the ordinary native consumer.

use std::time::Duration;

use nara::{
    project_host::{DesktopStartupPhase, DesktopStartupTimings},
    render_wgpu::{WgpuNativeWorkStats, WgpuRuntimeFramePhase, WgpuRuntimeFrameTimings},
};
use serde::Serialize;
use serde_json::{Value, json};

pub(super) const RUNTIME_PHASE_NAMES: [&str; 5] = [
    "namespace_preparation",
    "surface_acquisition",
    "candidate_preparation",
    "program_execution",
    "surface_presentation",
];

/// Disjoint original Host operation spans inside one matching App-to-post-Host frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct RuntimeFrameTiming {
    phase_wall_ns: [u64; 5],
    measured_phase_wall_ns: u64,
    other_app_frame_wall_ns: Option<u64>,
}

impl RuntimeFrameTiming {
    pub(super) fn capture(
        timing: WgpuRuntimeFrameTimings,
        app_to_post_host_ns: Option<u64>,
    ) -> Result<Self, String> {
        Self::from_durations(
            WgpuRuntimeFramePhase::ALL.map(|phase| timing.get(phase)),
            app_to_post_host_ns,
        )
    }

    pub(super) fn from_durations(
        phases: [Option<Duration>; 5],
        app_to_post_host_ns: Option<u64>,
    ) -> Result<Self, String> {
        let mut phase_wall_ns = [0; 5];
        let mut measured_phase_wall_ns = 0_u64;
        for (index, phase) in phases.into_iter().enumerate() {
            let phase = phase.ok_or_else(|| {
                format!(
                    "successful Runtime presentation lacks {} wall observation",
                    RUNTIME_PHASE_NAMES[index]
                )
            })?;
            let value = super::observe::duration_ns(phase)?;
            phase_wall_ns[index] = value;
            measured_phase_wall_ns = measured_phase_wall_ns
                .checked_add(value)
                .ok_or("Runtime phase wall observation sum exceeds u64 nanoseconds")?;
        }
        let other_app_frame_wall_ns = app_to_post_host_ns
            .map(|elapsed| {
                elapsed
                    .checked_sub(measured_phase_wall_ns)
                    .ok_or("Runtime operation wall spans exceed their original App frame")
            })
            .transpose()?;
        Ok(Self {
            phase_wall_ns,
            measured_phase_wall_ns,
            other_app_frame_wall_ns,
        })
    }
}

pub(super) fn startup_snapshot(timing: DesktopStartupTimings) -> Result<Value, String> {
    let mut phases = serde_json::Map::new();
    for (phase, name) in DesktopStartupPhase::ALL.into_iter().zip([
        "content_preparation",
        "runtime_startup",
        "native_session_construction",
        "first_native_poll",
        "native_polling_to_first_presentation",
    ]) {
        let elapsed = timing
            .get(phase)
            .ok_or_else(|| format!("first native presentation lacks startup phase {name}"))?;
        phases.insert(name.into(), json!(super::observe::duration_ns(elapsed)?));
    }
    Ok(json!({
        "operation_wall_ns": phases,
        "scope": "original DesktopRun content/startup/session and native-poll operations; first_native_poll is included in native_polling_to_first_presentation; caller gaps excluded; spans are not additive and do not establish CPU/GPU execution or scanout",
    }))
}

pub(super) fn finish_startup_report(
    mut report: Value,
    first: Option<DesktopStartupTimings>,
    retired: Option<DesktopStartupTimings>,
) -> Result<Value, Value> {
    let observed = (|| {
        let first = first.ok_or("original startup observations unavailable")?;
        if retired != Some(first) {
            return Err("completed startup observations changed during original retirement".into());
        }
        startup_snapshot(first)
    })();
    match observed {
        Ok(startup) => {
            report["startup"] = startup;
            report["startup_frozen_after_first_presentation"] = json!(true);
            Ok(report)
        }
        Err(error) => {
            report["failure"] = json!(error);
            report["accepted"] = json!(false);
            Err(report)
        }
    }
}

pub(super) fn native_work(work: WgpuNativeWorkStats) -> Value {
    json!({
        "issued_buffer_write_calls": work.issued_buffer_write_calls,
        "issued_buffer_write_bytes": work.issued_buffer_write_bytes,
        "issued_texture_write_calls": work.issued_texture_write_calls,
        "issued_texture_write_bytes": work.issued_texture_write_bytes,
        "initialized_buffer_calls": work.initialized_buffer_calls,
        "initialized_buffer_payload_bytes": work.initialized_buffer_payload_bytes,
        "initialized_buffer_storage_bytes": work.initialized_buffer_storage_bytes,
        "encoded_buffer_copy_commands": work.encoded_buffer_copy_commands,
        "encoded_buffer_copy_bytes": work.encoded_buffer_copy_bytes,
        "encoded_texture_copy_commands": work.encoded_texture_copy_commands,
        "encoded_texture_copy_bytes": work.encoded_texture_copy_bytes,
    })
}
