use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use nara_app::{AppExit, RuntimeGeneration};
use nara_render::gpu::execution::WgpuPresentedFrameReceipt;
use nara_window::winit::WinitPresentationProgress;

use super::super::StartupContentClosureReceipt;

const FIRST_PRESENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Product-level proof that one checked startup closure reached the real desktop presentation path.
///
/// The receipt retains the content authority instead of copying its manifest, schema, and member
/// facts. The WGPU receipt is process-local and proves submit plus present for one exact live
/// backend transaction; neither receipt is serializable evidence by itself.
#[derive(Clone)]
pub struct DesktopPresentationVerificationReceipt {
    startup_content_closure: StartupContentClosureReceipt,
    presented_frame: WgpuPresentedFrameReceipt,
}

impl DesktopPresentationVerificationReceipt {
    pub(in crate::project_host) const fn new(
        startup_content_closure: StartupContentClosureReceipt,
        presented_frame: WgpuPresentedFrameReceipt,
    ) -> Self {
        Self {
            startup_content_closure,
            presented_frame,
        }
    }

    #[must_use]
    pub const fn startup_content_closure(&self) -> &StartupContentClosureReceipt {
        &self.startup_content_closure
    }
}

impl fmt::Debug for DesktopPresentationVerificationReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopPresentationVerificationReceipt")
            .field("startup_content_closure", &self.startup_content_closure)
            .field("presented_frame", &self.presented_frame)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct DesktopFirstPresentVerification {
    outcome: Arc<Mutex<DesktopFirstPresentOutcome>>,
}

impl DesktopFirstPresentVerification {
    pub(in crate::project_host) fn new() -> Self {
        let deadline = Instant::now()
            .checked_add(FIRST_PRESENT_TIMEOUT)
            .expect("the bounded first-present timeout fits in Instant");
        Self {
            outcome: Arc::new(Mutex::new(DesktopFirstPresentOutcome::Awaiting {
                deadline,
            })),
        }
    }

    /// Observes presentation or schedules an idle wake at the verification deadline.
    pub(super) fn observe(
        &self,
        runtime_generation: RuntimeGeneration,
        presented: Option<WgpuPresentedFrameReceipt>,
        now: Instant,
    ) -> WinitPresentationProgress {
        let mut outcome = self.lock_outcome();
        let (exit, next_wake) = if let DesktopFirstPresentOutcome::Awaiting { deadline } = *outcome
        {
            let exact_presentation =
                presented.filter(|receipt| receipt.runtime_generation() == runtime_generation);
            match first_present_decision(deadline, now, exact_presentation) {
                DesktopFirstPresentDecision::Await => (None, Some(deadline)),
                DesktopFirstPresentDecision::Accept(receipt) => {
                    *outcome = DesktopFirstPresentOutcome::Accepted(receipt);
                    (Some(AppExit::Success), None)
                }
                DesktopFirstPresentDecision::TimedOut => {
                    *outcome = DesktopFirstPresentOutcome::TimedOut;
                    (Some(AppExit::Requested), None)
                }
            }
        } else {
            (None, None)
        };
        WinitPresentationProgress { exit, next_wake }
    }

    pub(in crate::project_host) fn outcome(&self) -> DesktopFirstPresentOutcome {
        *self.lock_outcome()
    }

    fn lock_outcome(&self) -> MutexGuard<'_, DesktopFirstPresentOutcome> {
        self.outcome
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::project_host) enum DesktopFirstPresentOutcome {
    Awaiting { deadline: Instant },
    Accepted(WgpuPresentedFrameReceipt),
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesktopFirstPresentDecision<T> {
    Await,
    Accept(T),
    TimedOut,
}

fn first_present_decision<T>(
    deadline: Instant,
    now: Instant,
    exact_presentation: Option<T>,
) -> DesktopFirstPresentDecision<T> {
    if now >= deadline {
        DesktopFirstPresentDecision::TimedOut
    } else if let Some(presentation) = exact_presentation {
        DesktopFirstPresentDecision::Accept(presentation)
    } else {
        DesktopFirstPresentDecision::Await
    }
}

#[cfg(test)]
mod tests {
    use nara_app::{
        App, RuntimeCandidate, RuntimeCandidateRetirementState, RuntimeClosePolicy,
        RuntimeObligationLedger,
    };

    use super::*;

    fn runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeCandidate::admit(
            App::new().seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
        let runtime = candidate.complete_startup().unwrap().promote();
        let generation = runtime.generation();
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        generation
    }

    #[test]
    fn idle_post_host_maintenance_wakes_at_deadline_and_exits_once() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(1);
        let generation = runtime_generation();
        let verification = DesktopFirstPresentVerification {
            outcome: Arc::new(Mutex::new(DesktopFirstPresentOutcome::Awaiting {
                deadline,
            })),
        };

        let waiting = verification.observe(generation, None, now);
        assert_eq!(waiting.exit, None);
        assert_eq!(waiting.next_wake, Some(deadline));
        let timed_out = verification.observe(generation, None, deadline);
        assert_eq!(timed_out.exit, Some(AppExit::Requested));
        assert_eq!(timed_out.next_wake, None);
        assert_eq!(verification.outcome(), DesktopFirstPresentOutcome::TimedOut);
        let settled = verification.observe(generation, None, deadline);
        assert_eq!(settled.exit, None);
        assert_eq!(settled.next_wake, None);
    }

    #[test]
    fn presentation_at_first_present_deadline_is_rejected() {
        let deadline = Instant::now();

        assert_eq!(
            first_present_decision(deadline, deadline, Some(())),
            DesktopFirstPresentDecision::TimedOut
        );
    }

    #[test]
    fn presentation_after_first_present_deadline_is_rejected() {
        let deadline = Instant::now();
        let after_deadline = deadline
            .checked_add(Duration::from_nanos(1))
            .expect("one nanosecond fits in Instant");

        assert_eq!(
            first_present_decision(deadline, after_deadline, Some(())),
            DesktopFirstPresentDecision::TimedOut
        );
    }
}
