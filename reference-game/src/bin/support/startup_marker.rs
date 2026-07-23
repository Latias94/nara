use std::{
    env, fmt,
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const STARTUP_MARKER_ENV: &str = "NARA_REFERENCE_GAME_STARTUP_MARKER";
pub(crate) const STARTUP_MARKER_SCHEMA: &str = "nara-reference-game.startup-marker-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupMarkerError {
    InvalidConfiguration,
    InvalidEvent,
    OutputFailed,
    Missing,
}

impl StartupMarkerError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "reference-game.startup-marker.invalid-configuration",
            Self::InvalidEvent => "reference-game.startup-marker.invalid-event",
            Self::OutputFailed => "reference-game.startup-marker.output-failed",
            Self::Missing => "reference-game.startup-marker.missing",
        }
    }
}

impl fmt::Display for StartupMarkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary = match self {
            Self::InvalidConfiguration => "Startup marker configuration is invalid",
            Self::InvalidEvent => "Startup marker event is invalid",
            Self::OutputFailed => "Startup marker could not be written",
            Self::Missing => "Startup marker was not observed before success",
        };
        formatter.write_str(summary)
    }
}

fn is_valid_event(event: &str) -> bool {
    matches!(
        event,
        "headless_first_authoritative_tick" | "desktop_first_playable_present"
    )
}

fn write_marker(writer: &mut impl Write, event: &str) -> io::Result<()> {
    writeln!(
        writer,
        "{{\"schema\":\"{}\",\"event\":\"{}\"}}",
        STARTUP_MARKER_SCHEMA, event,
    )?;
    writer.flush()
}

/// Emits one opt-in, static startup boundary marker for a measurement-only child process.
///
/// The marker never carries user or host values. It is intentionally disabled unless the caller
/// sets the exact `NARA_REFERENCE_GAME_STARTUP_MARKER=1` environment value.
#[derive(Debug)]
pub(crate) struct StartupMarker {
    event: &'static str,
    enabled: bool,
    emitted: AtomicBool,
    output_failed: AtomicBool,
}

impl StartupMarker {
    pub(crate) fn from_environment(event: &'static str) -> Result<Self, StartupMarkerError> {
        if !is_valid_event(event) {
            return Err(StartupMarkerError::InvalidEvent);
        }
        let enabled = match env::var_os(STARTUP_MARKER_ENV) {
            None => false,
            Some(value) if value == "1" => true,
            Some(_) => return Err(StartupMarkerError::InvalidConfiguration),
        };
        Ok(Self {
            event,
            enabled,
            emitted: AtomicBool::new(false),
            output_failed: AtomicBool::new(false),
        })
    }

    /// Records the semantic boundary once and flushes it so the parent can timestamp receipt.
    pub(crate) fn emit(&self) -> Result<(), StartupMarkerError> {
        if !self.enabled || self.emitted.swap(true, Ordering::AcqRel) {
            return Ok(());
        }

        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let write_result = write_marker(&mut stdout, self.event);
        if write_result.is_err() {
            self.output_failed.store(true, Ordering::Release);
            return Err(StartupMarkerError::OutputFailed);
        }
        Ok(())
    }

    /// Makes a successful product result invalid when an enabled marker never reached its parent.
    pub(crate) fn verify_success(&self) -> Result<(), StartupMarkerError> {
        if !self.enabled {
            return Ok(());
        }
        if self.output_failed.load(Ordering::Acquire) {
            return Err(StartupMarkerError::OutputFailed);
        }
        if !self.emitted.load(Ordering::Acquire) {
            return Err(StartupMarkerError::Missing);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_declared_events_are_valid() {
        assert!(is_valid_event("headless_first_authoritative_tick"));
        assert!(is_valid_event("desktop_first_playable_present"));
        assert!(!is_valid_event("arbitrary_startup_event"));
        assert!(matches!(
            StartupMarker::from_environment("arbitrary_startup_event"),
            Err(StartupMarkerError::InvalidEvent)
        ));
    }

    #[test]
    fn marker_wire_format_is_canonical() {
        let mut output = Vec::new();

        write_marker(&mut output, "headless_first_authoritative_tick").unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "{\"schema\":\"nara-reference-game.startup-marker-v1\",\"event\":\"headless_first_authoritative_tick\"}\n"
        );
    }

    #[test]
    fn disabled_marker_never_requires_an_observation() {
        let marker = StartupMarker {
            event: "headless_first_authoritative_tick",
            enabled: false,
            emitted: AtomicBool::new(false),
            output_failed: AtomicBool::new(false),
        };

        assert_eq!(marker.emit(), Ok(()));
        assert_eq!(marker.verify_success(), Ok(()));
    }

    #[test]
    fn enabled_marker_requires_a_successful_emission() {
        let marker = StartupMarker {
            event: "headless_first_authoritative_tick",
            enabled: true,
            emitted: AtomicBool::new(false),
            output_failed: AtomicBool::new(false),
        };

        assert_eq!(marker.verify_success(), Err(StartupMarkerError::Missing));
        marker.emitted.store(true, Ordering::Release);
        assert_eq!(marker.verify_success(), Ok(()));
        marker.output_failed.store(true, Ordering::Release);
        assert_eq!(
            marker.verify_success(),
            Err(StartupMarkerError::OutputFailed)
        );
    }
}
