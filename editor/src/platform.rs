use std::time::{Duration, Instant};

use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    render::RenderWindowPacket,
    render_wgpu::__private::WgpuEditorExecutionError,
    tooling::{EditorPersistenceOperation, EditorPlayState},
    window::{PresentMode, WindowId as NaraWindowId, WindowResolution},
};
use thiserror::Error;
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

use crate::{EditorApplicationOpenError, EditorLaunchOptions};

mod composition;
mod game_input;
mod imgui;
pub use composition::{EditorCompositionError, EditorNativeComposition};

const EDITOR_TITLE: &str = "Nara Editor";
const EDITOR_GPU_UNAVAILABLE_TITLE: &str = "Nara Editor - GPU unavailable";
const PLAY_LIFECYCLE_POLL_DELAY: Duration = Duration::from_millis(16);
const IMAGE_IMPORT_POLL_DELAY: Duration = Duration::from_millis(16);
const PERSISTENCE_POLL_DELAY: Duration = Duration::from_millis(16);

#[derive(Debug, Error)]
pub enum EditorRunError {
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    TrustedOpen(#[from] EditorApplicationOpenError),
    #[error("failed to create or drive the native event loop: {0}")]
    EventLoop(String),
    #[error("native Editor platform failed: {0}")]
    Platform(String),
    #[error("native Editor GPU execution failed: {0}")]
    Gpu(#[source] WgpuEditorExecutionError),
    #[error(transparent)]
    Composition(#[from] EditorCompositionError),
}

impl EditorRunError {
    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    #[must_use]
    pub fn diagnostics(&self) -> Option<&nara::diagnostic::DiagnosticReport> {
        match self {
            Self::TrustedOpen(error) => error.diagnostics(),
            Self::Usage(_)
            | Self::EventLoop(_)
            | Self::Platform(_)
            | Self::Gpu(_)
            | Self::Composition(_) => None,
        }
    }

    fn requires_gpu_replacement(&self) -> bool {
        matches!(self, Self::Gpu(error) if error.requires_owner_replacement())
    }
}

impl From<crate::workspace::EditorPanelRegistrationError> for EditorRunError {
    fn from(error: crate::workspace::EditorPanelRegistrationError) -> Self {
        Self::Composition(error.into())
    }
}

pub fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    EditorNativeComposition::new().run(options)
}

/// Runs one private first-party Editor with an explicitly trusted compiled product intent.
#[doc(hidden)]
pub fn run_trusted_compiled(
    options: EditorLaunchOptions,
    intent: TrustedCompiledEditorProjectIntent,
) -> Result<(), EditorRunError> {
    EditorNativeComposition::new().run_trusted_compiled(options, intent)
}

fn earliest_deadline(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

fn play_lifecycle_poll_deadline(now: Instant, state: EditorPlayState) -> Option<Instant> {
    matches!(
        state,
        EditorPlayState::PreparingPlay
            | EditorPlayState::Starting
            | EditorPlayState::RetiringPlay
            | EditorPlayState::Stopping
    )
    .then_some(now + PLAY_LIFECYCLE_POLL_DELAY)
}

fn image_import_poll_deadline(now: Instant, pending: bool) -> Option<Instant> {
    pending.then_some(now + IMAGE_IMPORT_POLL_DELAY)
}

fn persistence_poll_deadline(
    now: Instant,
    operation: EditorPersistenceOperation,
) -> Option<Instant> {
    (operation != EditorPersistenceOperation::Idle).then_some(now + PERSISTENCE_POLL_DELAY)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorGpuRecovery {
    Ready,
    AwaitingRuntimePause,
    AutomaticReplacementAttempted,
    AwaitingExplicitRetry,
}

impl EditorGpuRecovery {
    fn begin_runtime_pause(&mut self) -> bool {
        if *self != Self::Ready {
            return false;
        }
        *self = Self::AwaitingRuntimePause;
        true
    }

    fn finish_runtime_pause(&mut self) {
        debug_assert_eq!(*self, Self::AwaitingRuntimePause);
        *self = Self::AutomaticReplacementAttempted;
    }

    fn begin_automatic_replacement(&mut self) -> bool {
        if *self != Self::Ready {
            return false;
        }
        *self = Self::AutomaticReplacementAttempted;
        true
    }

    fn begin_explicit_retry(&mut self) -> bool {
        if *self != Self::AwaitingExplicitRetry {
            return false;
        }
        *self = Self::AutomaticReplacementAttempted;
        true
    }

    fn require_explicit_retry(&mut self) {
        *self = Self::AwaitingExplicitRetry;
    }

    fn mark_presentation_ready(&mut self) {
        *self = Self::Ready;
    }
}

fn explicit_gpu_retry_requested(event: &KeyEvent) -> bool {
    event.state == ElementState::Pressed
        && !event.repeat
        && event.physical_key == PhysicalKey::Code(KeyCode::F5)
}

fn editor_window_packet(window: &Window) -> RenderWindowPacket {
    let size = window.inner_size();
    RenderWindowPacket {
        id: NaraWindowId::PRIMARY,
        resolution: WindowResolution::new(size.width, size.height)
            .with_scale_factor(window.scale_factor()),
        present_mode: PresentMode::AutoVsync,
    }
}

fn platform_gpu_error(error: WgpuEditorExecutionError) -> EditorRunError {
    EditorRunError::Gpu(error)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nara::tooling::{EditorPersistenceOperation, EditorPlayState};

    use super::{
        EditorGpuRecovery, earliest_deadline, image_import_poll_deadline,
        persistence_poll_deadline, play_lifecycle_poll_deadline,
    };

    #[test]
    fn transitional_runtime_lifecycle_schedules_bounded_follow_up_drive() {
        let now = Instant::now();

        for state in [
            EditorPlayState::PreparingPlay,
            EditorPlayState::Starting,
            EditorPlayState::RetiringPlay,
            EditorPlayState::Stopping,
        ] {
            assert_eq!(
                play_lifecycle_poll_deadline(now, state),
                Some(now + Duration::from_millis(16))
            );
        }

        assert_eq!(
            play_lifecycle_poll_deadline(now, EditorPlayState::Running),
            None
        );
        assert_eq!(
            play_lifecycle_poll_deadline(now, EditorPlayState::CloseIncomplete),
            None
        );
    }

    #[test]
    fn image_import_pending_work_uses_a_bounded_poll_deadline() {
        let now = Instant::now();

        assert_eq!(
            image_import_poll_deadline(now, true),
            Some(now + Duration::from_millis(16))
        );
        assert_eq!(image_import_poll_deadline(now, false), None);
    }

    #[test]
    fn pending_persistence_uses_maintenance_until_idle() {
        let now = Instant::now();
        let document = nara::tooling::EditorDocumentId::from_raw(1);
        let revision =
            nara::scene::SceneAuthoringSession::new(nara::scene::SceneDocument::default())
                .revision();
        for operation in [
            EditorPersistenceOperation::Saving {
                document,
                captured_revision: revision,
            },
            EditorPersistenceOperation::Opening { document },
        ] {
            assert_eq!(
                persistence_poll_deadline(now, operation),
                Some(now + Duration::from_millis(16))
            );
        }
        assert_eq!(
            persistence_poll_deadline(now, EditorPersistenceOperation::Idle),
            None
        );
    }

    #[test]
    fn earlier_wake_source_wins_over_image_import_polling() {
        let now = Instant::now();
        let earlier = now + Duration::from_millis(4);

        assert_eq!(
            earliest_deadline(Some(earlier), image_import_poll_deadline(now, true)),
            Some(earlier)
        );
    }

    #[test]
    fn gpu_recovery_requires_explicit_retry_after_one_automatic_replacement() {
        let mut recovery = EditorGpuRecovery::Ready;

        assert!(recovery.begin_automatic_replacement());
        assert!(!recovery.begin_automatic_replacement());

        recovery.require_explicit_retry();
        assert!(!recovery.begin_automatic_replacement());

        recovery.require_explicit_retry();
        assert!(recovery.begin_explicit_retry());
        assert!(!recovery.begin_explicit_retry());

        recovery.mark_presentation_ready();
        assert!(recovery.begin_automatic_replacement());

        recovery.mark_presentation_ready();
        assert!(recovery.begin_runtime_pause());
        assert!(!recovery.begin_automatic_replacement());
        recovery.finish_runtime_pause();
        assert_eq!(recovery, EditorGpuRecovery::AutomaticReplacementAttempted);
    }
}
