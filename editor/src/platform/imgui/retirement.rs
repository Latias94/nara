//! Concrete Editor graphics retirement; this owner is independent of Play runtimes.

use super::*;

#[cfg(test)]
std::thread_local! {
    // Exercise the SDK's actual attach refusal after a renderer has been constructed.
    static REJECT_NEXT_NATIVE_ATTACH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn reject_next_native_attach() {
    REJECT_NEXT_NATIVE_ATTACH.with(|reject| reject.set(true));
}

pub(super) fn enable_native_platform_for_construction() -> bool {
    #[cfg(test)]
    if REJECT_NEXT_NATIVE_ATTACH.with(|reject| reject.replace(false)) {
        return false;
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum NativeRetirementAction {
    #[default]
    Hold,
    Install,
    InstallForExit,
    AwaitingExitReady,
    Exit,
}

impl NativeRetirementAction {
    pub(super) fn needs_exit_maintenance(self) -> bool {
        matches!(
            self,
            Self::InstallForExit | Self::AwaitingExitReady | Self::Exit
        )
    }
}

#[derive(Debug, Default)]
pub(super) struct NativeCloseErrors {
    messages: Vec<String>,
    omitted: usize,
}

impl NativeCloseErrors {
    pub(super) fn record(&mut self, error: impl std::fmt::Display) {
        let message = error.to_string();
        let mut characters = message.chars();
        let mut truncated = characters.by_ref().take(4096).collect::<String>();
        if characters.next().is_some() {
            truncated.push_str(" [truncated]");
        }
        if self.messages.contains(&truncated) {
            return;
        }
        if self.messages.len() < 32 {
            self.messages.push(truncated);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }

    pub(super) fn append(&mut self, other: Self) {
        for message in other.messages {
            self.record(message);
        }
        self.omitted = self.omitted.saturating_add(other.omitted);
    }

    pub(super) fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.omitted == 0
    }
}

impl std::fmt::Display for NativeCloseErrors {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, message) in self.messages.iter().enumerate() {
            if index != 0 {
                formatter.write_str("; ")?;
            }
            formatter.write_str(message)?;
        }
        if self.omitted != 0 {
            write!(
                formatter,
                "; {} additional close errors omitted",
                self.omitted
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Open,
    Drain,
    Renderer,
    Views,
    Platform,
    Complete,
}

#[derive(Debug, Default)]
pub(super) struct NativeClose {
    phase: Phase,
    pub(super) drain: Option<WgpuEditorGpuRetirement>,
    pub(super) errors: NativeCloseErrors,
    pub(super) saved_layout: Option<String>,
    pub(super) device_lost: bool,
}

impl NativeClose {
    pub(super) fn is_complete(&self) -> bool {
        self.phase == Phase::Complete
    }
}

impl NativeImgui {
    /// Drives unfinished phases once, retaining the complete aggregate on every refusal.
    pub(super) fn poll_graphics_close(&mut self) -> bool {
        if self.close.phase == Phase::Open {
            let mut layout = String::new();
            self.context.save_ini_settings(&mut layout);
            self.close.saved_layout = Some(layout);
            self.input.invalidate(true);
            self.context.end_frame();
            if let Some(drain) = &mut self.close.drain {
                drain.begin(self.close.device_lost);
            }
            self.close.phase = Phase::Drain;
        }
        if self.close.phase == Phase::Drain {
            if let Some(drain) = &mut self.close.drain {
                match drain.poll() {
                    Ok(true) => {}
                    Ok(false) => return false,
                    Err(error) => {
                        self.close.errors.record(format!("GPU drain: {error}"));
                        return false;
                    }
                }
            }
            self.close.phase = Phase::Renderer;
        }
        if self.close.phase == Phase::Renderer {
            if let Err(error) = self.renderer.shutdown(&mut self.context) {
                self.close.errors.record(format!("Renderer close: {error}"));
                return false;
            }
            self.close.phase = Phase::Views;
        }
        if self.close.phase == Phase::Views {
            // Successful bulk shutdown already revoked texture registrations and adapter bindings.
            // No fallible per-texture unregister is needed at the final aggregate boundary.
            for target in [&mut self.scene, &mut self.game] {
                if let Some(view) = target.view {
                    self.gpu.retire_view(view);
                }
                *target = ImguiTarget::default();
            }
            self.close.phase = Phase::Platform;
        }
        if self.close.phase == Phase::Platform {
            if let Err(error) = self.platform.shutdown(&mut self.context) {
                self.close.errors.record(format!("Platform close: {error}"));
                return false;
            }
            self.close.phase = Phase::Complete;
        }
        self.close.phase == Phase::Complete
    }

    /// Explicit finite driver for test cleanup and a terminal event-loop failure.
    /// Responsive production close uses `poll_graphics_close` while its event loop remains alive.
    pub(super) fn shutdown_graphics(&mut self) -> Result<(), EditorRunError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.poll_graphics_close() {
            if Instant::now() >= deadline {
                return Err(imgui_error(format!(
                    "native graphics retirement remains incomplete in {:?}: {}",
                    self.close.phase, self.close.errors
                )));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.close.errors.is_empty() {
            Ok(())
        } else {
            Err(imgui_error(format!(
                "native graphics retired with errors: {}",
                self.close.errors
            )))
        }
    }
}

/// A failed construction retains the native owners still returned to the first-party caller.
/// SDK-internal rollback may already have released allocations before returning its error.
pub(super) struct NativeInitializationFailure {
    error: EditorRunError,
    native: Option<Box<NativeImgui>>,
}

impl NativeInitializationFailure {
    pub(super) fn retained(error: EditorRunError, native: NativeImgui) -> Self {
        Self {
            error,
            native: Some(Box::new(native)),
        }
    }

    pub(super) fn into_parts(self) -> (EditorRunError, Option<NativeImgui>) {
        (self.error, self.native.map(|native| *native))
    }
}

impl From<EditorRunError> for NativeInitializationFailure {
    fn from(error: EditorRunError) -> Self {
        Self {
            error,
            native: None,
        }
    }
}

impl std::fmt::Display for NativeInitializationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, formatter)
    }
}

impl std::fmt::Debug for NativeInitializationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeInitializationFailure")
            .field("error", &self.error)
            .field("retains_native", &self.native.is_some())
            .finish()
    }
}

/// Native tests must consume a construction refusal's custody before projecting its message.
#[cfg(test)]
pub(super) fn initialize_native_for_test(
    event_loop: &ActiveEventLoop,
    window: Arc<Window>,
) -> Result<NativeImgui, String> {
    NativeImgui::new(event_loop, window).map_err(|failure| {
        let (error, native) = failure.into_parts();
        let cleanup = native.map(|mut native| finish_graphics_for_test(&mut native));
        match cleanup {
            Some(Err(cleanup)) => format!("{error}; initialization cleanup: {cleanup}"),
            _ => error.to_string(),
        }
    })
}

#[cfg(test)]
pub(super) fn finish_graphics_for_test(native: &mut NativeImgui) -> Result<(), String> {
    let result = native.shutdown_graphics();
    if !native.close.is_complete() {
        eprintln!("native test cannot release unfinished graphics custody: {result:?}");
        std::process::abort();
    }
    result.map_err(|error| error.to_string())
}

/// Finish both slots before a test inspects close facts, drops the Host, or closes its Session.
#[cfg(test)]
pub(super) fn finish_host_graphics_for_test(host: &mut NativeEditorHost) -> Result<(), String> {
    let mut errors = NativeCloseErrors::default();
    for native in [&mut host.native, &mut host.retiring].into_iter().flatten() {
        if let Err(error) = finish_graphics_for_test(native) {
            errors.record(error);
        }
        native.publish_render_views(host.application.panel_workspace_mut());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.to_string())
    }
}
