use std::time::Duration;

use nara_app::AppExit;
use nara_winit::{WinitRunProgress, WinitSession};

use super::{DesktopRun, DesktopRunState};

/// Progress of the original desktop aggregate, independently of a terminal run report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRunProgress {
    /// Source or a supplied Runtime is retained, but the native session has not started.
    NotStarted,
    /// The original game is live, including a paused game waiting for caller operations.
    Running,
    /// Runtime, presentation or native owners are retiring under the original close drive.
    Retiring,
    /// The original owners need an explicit cleanup retry; ordinary polling does not renew it.
    CleanupIncomplete,
    Completed(AppExit),
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::project_host) enum DesktopDriveMode {
    Blocking,
    Poll(Duration),
}

impl DesktopDriveMode {
    pub(super) fn drive(self, session: &mut WinitSession) -> WinitRunProgress {
        match self {
            Self::Blocking => session.execute(),
            Self::Poll(max_wait) => session.poll(max_wait),
        }
    }
}

impl DesktopRun {
    /// Starts or polls the original desktop Runtime, native event loop and presentation owner.
    ///
    /// `max_wait` bounds native waiting during this poll, not synchronous source/startup work or
    /// the duration of a callback, managed frame or graphics operation. Repeated calls keep the
    /// same live session. Native thread affinity and Winit's main-thread requirements apply.
    /// An incomplete cleanup retains all owners; [`Self::execute`] explicitly retries that drive.
    pub fn poll(&mut self, max_wait: Duration) -> DesktopRunProgress {
        self.advance(DesktopDriveMode::Poll(max_wait));
        self.progress()
    }

    /// Fences ordinary operations and starts close of the original aggregate on its next poll.
    ///
    /// Continue polling until terminal. A source product closed before startup admits no Runtime
    /// or native window; an already supplied Runtime still retires through its original owners.
    pub fn request_close(&mut self) {
        self.close_requested = true;
        if let DesktopRunState::Native(native) = &mut self.state {
            native.session.request_close();
            if native.session.is_retired() {
                native.progress = WinitRunProgress::Retired;
            } else if native.progress != WinitRunProgress::Incomplete {
                native.progress = WinitRunProgress::Retiring;
            }
        }
    }

    #[must_use]
    pub fn progress(&self) -> DesktopRunProgress {
        match &self.state {
            DesktopRunState::Native(native) => match native.progress {
                WinitRunProgress::Running => DesktopRunProgress::Running,
                WinitRunProgress::Retiring => DesktopRunProgress::Retiring,
                WinitRunProgress::Incomplete => DesktopRunProgress::CleanupIncomplete,
                WinitRunProgress::Retired => DesktopRunProgress::Retiring,
            },
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Cleaning { hosts, .. } => {
                if hosts.iter().any(|host| {
                    host.cleanup_state()
                        == Some(nara_app::RuntimeCandidateRetirementState::RetirementIncomplete)
                }) {
                    DesktopRunProgress::CleanupIncomplete
                } else {
                    DesktopRunProgress::Retiring
                }
            }
            DesktopRunState::Completed(exit) => DesktopRunProgress::Completed(*exit),
            DesktopRunState::Failed => DesktopRunProgress::Failed,
            DesktopRunState::FreshRuntime { .. } => DesktopRunProgress::NotStarted,
            DesktopRunState::Executing => DesktopRunProgress::Running,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            DesktopRunState::Fresh(_) => DesktopRunProgress::NotStarted,
        }
    }
}
