use std::time::{Duration, Instant};

/// Original desktop action operations, observed only when startup timing is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopStartupPhase {
    /// Source checking or processed-base binding, plan creation and original start admission.
    ContentPreparation,
    /// Completing the original Runtime startup attempt before native ownership is transferred.
    RuntimeStartup,
    /// Initial Pause request, presentation driver and original native session construction.
    NativeSessionConstruction,
    /// First nonblocking native poll, including any window, device and frame work it performs.
    FirstNativePoll,
    /// Sum of native poll calls through the original first actual presentation, excluding caller gaps.
    NativePollingToFirstPresentation,
}

impl DesktopStartupPhase {
    pub const ALL: [Self; 5] = [
        Self::ContentPreparation,
        Self::RuntimeStartup,
        Self::NativeSessionConstruction,
        Self::FirstNativePoll,
        Self::NativePollingToFirstPresentation,
    ];
}

/// Fixed completed startup observations from one original desktop action.
///
/// Elapsed operations include existing waits and return processing. They do not certify
/// success, Runtime readiness, full paused-frame completion, presentation or physical cleanup;
/// observe those through their original APIs. Missing phases were not reached or observed.
/// An already-started Runtime has no content/startup observations, and blocking `execute`
/// does not report its lifetime as native startup polling. The polling aggregate is published
/// only after an actual presentation. Entering blocking execution before that point leaves it
/// unavailable. Unwinding does not complete an observation.
/// The spans exclude gaps between operations and are not a total startup, CPU or GPU budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DesktopStartupTimings {
    elapsed: [Option<Duration>; DesktopStartupPhase::ALL.len()],
}

impl DesktopStartupTimings {
    #[must_use]
    pub const fn get(&self, phase: DesktopStartupPhase) -> Option<Duration> {
        self.elapsed[phase as usize]
    }
}

#[derive(Debug)]
pub(in crate::project_host) struct DesktopStartupObservation {
    completed: DesktopStartupTimings,
    pending_native_polls: Option<Duration>,
}

impl Default for DesktopStartupObservation {
    fn default() -> Self {
        Self {
            completed: DesktopStartupTimings::default(),
            pending_native_polls: Some(Duration::ZERO),
        }
    }
}

impl DesktopStartupObservation {
    pub(super) const fn snapshot(&self) -> DesktopStartupTimings {
        self.completed
    }

    pub(super) fn record_native_poll(&mut self, elapsed: Duration, first: bool, presented: bool) {
        if first {
            self.completed.elapsed[DesktopStartupPhase::FirstNativePoll as usize] = Some(elapsed);
        }
        if let Some(pending) = self.pending_native_polls.as_mut() {
            *pending = pending.saturating_add(elapsed);
            if presented {
                self.completed.elapsed
                    [DesktopStartupPhase::NativePollingToFirstPresentation as usize] =
                    self.pending_native_polls.take();
            }
        }
    }

    pub(super) fn stop_native_poll_observation(&mut self) {
        self.pending_native_polls = None;
    }
}

pub(in crate::project_host) struct StartupTimer(Option<Instant>);

impl StartupTimer {
    pub(in crate::project_host) fn start(timings: &Option<DesktopStartupObservation>) -> Self {
        Self(timings.is_some().then(Instant::now))
    }

    pub(super) fn native_poll(timings: &Option<DesktopStartupObservation>, first: bool) -> Self {
        Self(
            timings
                .as_ref()
                .is_some_and(|timings| first || timings.pending_native_polls.is_some())
                .then(Instant::now),
        )
    }

    pub(super) fn elapsed(self) -> Option<Duration> {
        self.0.map(|started| started.elapsed())
    }

    pub(in crate::project_host) fn finish(
        self,
        timings: &mut Option<DesktopStartupObservation>,
        phase: DesktopStartupPhase,
    ) {
        if let (Some(started), Some(timings)) = (self.0, timings.as_mut()) {
            timings.completed.elapsed[phase as usize] = Some(started.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_startup_observation_retains_no_timer_or_snapshot() {
        let mut disabled = None;
        let timer = StartupTimer::start(&disabled);
        assert!(timer.0.is_none());
        timer.finish(&mut disabled, DesktopStartupPhase::RuntimeStartup);
        assert!(disabled.is_none());
    }

    #[test]
    fn completed_operation_does_not_fill_unreached_phases() {
        let mut timings = Some(DesktopStartupObservation::default());
        StartupTimer::start(&timings).finish(&mut timings, DesktopStartupPhase::RuntimeStartup);
        let snapshot = timings.unwrap().snapshot();
        assert!(snapshot.get(DesktopStartupPhase::RuntimeStartup).is_some());
        for phase in [
            DesktopStartupPhase::ContentPreparation,
            DesktopStartupPhase::NativeSessionConstruction,
            DesktopStartupPhase::FirstNativePoll,
            DesktopStartupPhase::NativePollingToFirstPresentation,
        ] {
            assert_eq!(snapshot.get(phase), None);
        }
    }

    #[test]
    fn first_poll_does_not_publish_an_unobserved_presentation_and_later_work_cannot_extend_it() {
        let mut timings = DesktopStartupObservation::default();
        timings.record_native_poll(Duration::from_millis(2), true, false);
        assert_eq!(
            timings.snapshot().get(DesktopStartupPhase::FirstNativePoll),
            Some(Duration::from_millis(2))
        );
        assert_eq!(
            timings
                .snapshot()
                .get(DesktopStartupPhase::NativePollingToFirstPresentation),
            None
        );
        let initial = timings.snapshot();
        timings.record_native_poll(Duration::from_millis(3), false, false);
        assert_eq!(timings.snapshot(), initial);
        timings.record_native_poll(Duration::from_millis(7), false, true);
        assert_eq!(
            timings
                .snapshot()
                .get(DesktopStartupPhase::NativePollingToFirstPresentation),
            Some(Duration::from_millis(12))
        );
        let completed = timings.snapshot();
        timings.record_native_poll(Duration::from_millis(100), false, true);
        assert_eq!(timings.snapshot(), completed);
        assert!(StartupTimer::native_poll(&Some(timings), false).0.is_none());
    }

    #[test]
    fn blocking_execution_does_not_complete_an_interrupted_native_poll_aggregate() {
        let mut timings = DesktopStartupObservation::default();
        timings.record_native_poll(Duration::from_millis(2), true, false);
        timings.stop_native_poll_observation();
        timings.record_native_poll(Duration::from_millis(7), false, true);
        assert_eq!(
            timings
                .snapshot()
                .get(DesktopStartupPhase::NativePollingToFirstPresentation),
            None
        );
        assert!(StartupTimer::native_poll(&Some(timings), false).0.is_none());
        assert!(StartupTimer::native_poll(&None, true).0.is_none());
    }
}
