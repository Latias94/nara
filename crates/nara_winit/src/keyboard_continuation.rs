use super::*;
use nara_input::{
    __private::InputEndpointClaim, InputAdmission, KeyboardActionBinding,
    KeyboardContinuationError, KeyboardHold,
};

/// Native activation is separate from visibility, focus, and Runtime adoption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinitInputActivation {
    Isolated,
    AwaitingFocus,
    ReconciliationPending,
    Active,
    Unavailable,
}

/// A keyboard operation preserves its admission or rejection alongside a subsequent fault.
#[derive(Debug, Clone, PartialEq)]
pub enum WinitKeyboardContinuationError {
    Unavailable,
    NotPaused,
    NotIsolated,
    IncompletePausedFrame,
    Unsupported,
    Input(KeyboardContinuationError),
    Scope(RuntimeScopeError),
    Faulted {
        admission: Option<InputAdmission>,
        rejection: Option<KeyboardContinuationError>,
        fault: RuntimeFault,
    },
}

impl std::fmt::Display for WinitKeyboardContinuationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native keyboard continuation refused: {self:?}")
    }
}

impl std::error::Error for WinitKeyboardContinuationError {}

pub(super) struct WinitKeyboardContinuation {
    pub(super) isolated: bool,
    pub(super) activation: WinitInputActivation,
    pub(super) claim: Option<InputEndpointClaim>,
    bindings: Vec<KeyboardActionBinding>,
    error: Option<WinitKeyboardContinuationError>,
    admission: Option<InputAdmission>,
    needs_step: bool,
    step: Option<RuntimeControlTicket>,
}

impl Default for WinitKeyboardContinuation {
    fn default() -> Self {
        Self {
            isolated: false,
            activation: WinitInputActivation::Active,
            claim: None,
            bindings: Vec::new(),
            error: None,
            admission: None,
            needs_step: false,
            step: None,
        }
    }
}

impl WinitSession {
    /// Captures the declared keyboard inventory at a healthy completed paused boundary.
    pub fn capture_keyboard_holds(
        &self,
        bindings: &[KeyboardActionBinding],
    ) -> Result<Vec<KeyboardHold>, WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_ref()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_boundary()?;
        app.capture_keyboard_inventory(bindings)
    }

    /// Requests a full paused App frame through this session's original frame owner.
    ///
    /// This invalidates the previous paused boundary without advancing fixed simulation,
    /// changing time continuation, or manufacturing a control ticket. The caller continues
    /// driving the existing session to observe completion.
    pub fn request_paused_maintenance(&mut self) -> Result<(), WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_mut()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_live_paused()?;
        app.request_frame();
        Ok(())
    }

    /// Freezes ordinary native ingress without cancelling the declared keyboard intent.
    ///
    /// Capture validates the original endpoint, mapping, consumers, and complete held inventory
    /// before the gate changes. Original focus remains observationally unchanged. The caller must
    /// complete paused maintenance before capturing again and later use native focus/physical-key
    /// reconciliation to reactivate the same session.
    pub fn isolate_keyboard_input(
        &mut self,
        bindings: &[KeyboardActionBinding],
    ) -> Result<Vec<KeyboardHold>, WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_mut()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_boundary()?;
        let holds = app.capture_keyboard_inventory(bindings)?;
        app.keyboard_continuation.bindings = bindings.to_vec();
        app.keyboard_continuation.isolated = true;
        app.keyboard_continuation.activation = WinitInputActivation::Isolated;
        app.keyboard_continuation.error = None;
        app.keyboard_continuation.admission = None;
        app.keyboard_continuation.needs_step = false;
        app.keyboard_continuation.step = None;
        app.request_frame();
        Ok(holds)
    }

    /// Admits one controlled synthetic edge while the original native endpoint stays isolated.
    ///
    /// Multiple bounded edges may be admitted before maintenance. Capture remains unavailable
    /// until the original action-to-command tick and paused maintenance settle every edge.
    pub fn admit_isolated_keyboard_input(
        &mut self,
        input: ButtonDriverInput<KeyCode>,
    ) -> Result<InputAdmission, WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_mut()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_live_paused()?;
        if !app.keyboard_continuation.isolated
            || app.keyboard_continuation.activation != WinitInputActivation::Isolated
        {
            return Err(WinitKeyboardContinuationError::NotIsolated);
        }
        if app.keyboard_continuation.bindings.is_empty()
            || match input {
                ButtonDriverInput::Press(key) | ButtonDriverInput::Release(key) => !app
                    .keyboard_continuation
                    .bindings
                    .iter()
                    .any(|binding| binding.key == key),
                ButtonDriverInput::ReleaseAll => false,
            }
        {
            return Err(WinitKeyboardContinuationError::Input(
                KeyboardContinuationError::BindingMismatch,
            ));
        }
        let claim = app
            .keyboard_continuation
            .claim
            .clone()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.apply_keyboard_operation(|scope| {
            nara_input::apply_claimed_keyboard_driver_input(scope, claim, input).map_err(|error| {
                match error {
                    ButtonInputDriverError::WorldAccess(error) => error.into(),
                    ButtonInputDriverError::Timeline(error) => error.into(),
                }
            })
        })
    }

    /// Restores logical keyboard members while native ingress remains isolated.
    pub fn restore_keyboard_holds(
        &mut self,
        holds: Vec<KeyboardHold>,
    ) -> Result<InputAdmission, WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_mut()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_boundary()?;
        if !app.keyboard_continuation.isolated
            || app.keyboard_continuation.activation != WinitInputActivation::Isolated
        {
            return Err(WinitKeyboardContinuationError::NotIsolated);
        }
        let limit = app
            .runtime
            .world()
            .get_resource::<InputTimeline>()
            .ok_or(WinitKeyboardContinuationError::Input(
                KeyboardContinuationError::MissingResources,
            ))?
            .limits()
            .members();
        if holds.is_empty() || holds.len() > limit {
            return Err(WinitKeyboardContinuationError::Input(
                KeyboardContinuationError::InvalidCount,
            ));
        }
        let claim = app
            .keyboard_continuation
            .claim
            .clone()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        let bindings = holds
            .iter()
            .map(|hold| hold.binding.clone())
            .collect::<Vec<_>>();
        let result = app.apply_keyboard_operation(|scope| {
            nara_input::restore_keyboard_holds(scope, claim, holds)
        });
        if result.is_ok()
            || matches!(
                &result,
                Err(WinitKeyboardContinuationError::Faulted {
                    admission: Some(_),
                    ..
                })
            )
        {
            app.keyboard_continuation.bindings = bindings;
        }
        result
    }

    /// Requests focus on the original visible window without resuming the Runtime.
    pub fn request_input_activation(
        &mut self,
    ) -> Result<WinitInputActivation, WinitKeyboardContinuationError> {
        let app = self
            .app
            .as_mut()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        app.check_keyboard_boundary()?;
        if !app.keyboard_continuation.isolated {
            return Err(WinitKeyboardContinuationError::NotIsolated);
        }
        if app.keyboard_continuation.activation == WinitInputActivation::Active {
            return Ok(WinitInputActivation::Active);
        }
        if app.keyboard_continuation.activation == WinitInputActivation::Isolated
            && !app.keyboard_continuation.bindings.is_empty()
        {
            app.capture_keyboard_inventory(&app.keyboard_continuation.bindings)?;
        }
        let window = app
            .original_keyboard_window()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        if !app.initial_visibility || window.is_visible() != Some(true) {
            return Err(WinitKeyboardContinuationError::Unavailable);
        }
        if app.keyboard_continuation.bindings.is_empty()
            || app
                .keyboard_continuation
                .bindings
                .iter()
                .any(|binding| physical_key(binding.key).is_none())
            || !cfg!(target_os = "windows")
        {
            app.keyboard_continuation.activation = WinitInputActivation::Unavailable;
            app.keyboard_continuation.error = Some(WinitKeyboardContinuationError::Unsupported);
            return Err(WinitKeyboardContinuationError::Unsupported);
        }
        app.keyboard_continuation.activation = WinitInputActivation::AwaitingFocus;
        app.keyboard_continuation.error = None;
        window.focus_window();
        if app.input_gate.focused && window.has_focus() {
            app.acknowledge_keyboard_focus().map_err(|_| {
                app.keyboard_continuation
                    .error
                    .clone()
                    .unwrap_or(WinitKeyboardContinuationError::Unavailable)
            })?;
        }
        app.request_frame();
        match &app.keyboard_continuation.error {
            Some(error) => Err(error.clone()),
            None => Ok(app.keyboard_continuation.activation),
        }
    }

    /// Reports native activation; a focused candidate can still await game consumption.
    #[must_use]
    pub fn input_activation(&self) -> WinitInputActivation {
        self.app
            .as_ref()
            .map_or(WinitInputActivation::Unavailable, |app| {
                if app.is_healthy_live() {
                    app.keyboard_continuation.activation
                } else {
                    WinitInputActivation::Unavailable
                }
            })
    }

    /// Retains the complete asynchronous activation refusal or post-operation fault.
    #[must_use]
    pub fn input_activation_error(&self) -> Option<&WinitKeyboardContinuationError> {
        self.app
            .as_ref()
            .and_then(|app| app.keyboard_continuation.error.as_ref())
    }
}

impl WinitApp {
    pub(super) fn configure_input_isolation(&mut self, isolated: bool) {
        self.keyboard_continuation.isolated = isolated;
        self.keyboard_continuation.activation = if isolated {
            self.input_gate = WinitInputGate::reset_for_focus(false);
            WinitInputActivation::Isolated
        } else {
            WinitInputActivation::Active
        };
    }

    pub(super) fn ordinary_input_enabled(&self) -> bool {
        self.keyboard_continuation.activation == WinitInputActivation::Active
    }

    fn check_keyboard_live_paused(&self) -> Result<(), WinitKeyboardContinuationError> {
        if let Some(fault) = self
            .runtime
            .fault()
            .cloned()
            .or_else(|| self.runtime.fault_reporter().fault())
        {
            return Err(WinitKeyboardContinuationError::Faulted {
                admission: None,
                rejection: None,
                fault,
            });
        }
        if !self.is_healthy_live() {
            return Err(WinitKeyboardContinuationError::Unavailable);
        }
        if self.runtime.state() != RuntimeState::Paused {
            return Err(WinitKeyboardContinuationError::NotPaused);
        }
        Ok(())
    }

    fn check_keyboard_boundary(&self) -> Result<(), WinitKeyboardContinuationError> {
        self.check_keyboard_live_paused()?;
        if !self.paused_frame_complete {
            return Err(WinitKeyboardContinuationError::IncompletePausedFrame);
        }
        Ok(())
    }

    fn capture_keyboard_inventory(
        &self,
        bindings: &[KeyboardActionBinding],
    ) -> Result<Vec<KeyboardHold>, WinitKeyboardContinuationError> {
        let claim = self
            .keyboard_continuation
            .claim
            .as_ref()
            .ok_or(WinitKeyboardContinuationError::Unavailable)?;
        let timeline = self.runtime.world().get_resource::<InputTimeline>().ok_or(
            WinitKeyboardContinuationError::Input(KeyboardContinuationError::MissingResources),
        )?;
        let current =
            nara_input::__private::claim_input_endpoint(timeline, claim.window(), claim.seat())
                .map_err(|error| WinitKeyboardContinuationError::Input(error.into()))?;
        if &current != claim {
            return Err(WinitKeyboardContinuationError::Input(
                KeyboardContinuationError::StaleEndpoint,
            ));
        }
        nara_input::capture_keyboard_holds(
            self.runtime.world(),
            claim.window(),
            claim.seat(),
            bindings,
        )
        .map_err(WinitKeyboardContinuationError::Input)
    }

    pub(super) fn apply_keyboard_operation(
        &mut self,
        operation: impl FnOnce(
            &mut RuntimeDriverScope<'_>,
        ) -> Result<InputAdmission, KeyboardContinuationError>,
    ) -> Result<InputAdmission, WinitKeyboardContinuationError> {
        let outcome = self
            .runtime
            .with_driver_scope(operation)
            .map_err(WinitKeyboardContinuationError::Scope)?;
        if outcome.output.is_ok() || outcome.post_operation_fault.is_some() {
            self.request_frame();
        }
        if let Some(fault) = outcome.post_operation_fault {
            let (admission, rejection) = match outcome.output {
                Ok(admission) => (Some(admission), None),
                Err(error) => (None, Some(error)),
            };
            return Err(WinitKeyboardContinuationError::Faulted {
                admission,
                rejection,
                fault,
            });
        }
        outcome
            .output
            .map_err(WinitKeyboardContinuationError::Input)
    }

    pub(super) fn keyboard_control_allowed(&self, control: RuntimeControl) -> bool {
        if !self.keyboard_continuation.isolated || self.ordinary_input_enabled() {
            return true;
        }
        match control {
            RuntimeControl::Resume => false,
            RuntimeControl::StepFixedTick => match self.keyboard_continuation.activation {
                WinitInputActivation::Isolated => true,
                WinitInputActivation::ReconciliationPending => {
                    self.keyboard_continuation.needs_step
                        && self.keyboard_continuation.step.is_none()
                }
                _ => false,
            },
            _ => true,
        }
    }

    pub(super) fn note_keyboard_control(
        &mut self,
        control: RuntimeControl,
        ticket: RuntimeControlTicket,
    ) {
        if control == RuntimeControl::StepFixedTick
            && self.keyboard_continuation.activation == WinitInputActivation::ReconciliationPending
        {
            self.keyboard_continuation.step = Some(ticket);
        }
    }

    fn original_keyboard_window(&self) -> Option<Arc<WinitWindow>> {
        let window = self.platform_windows.get(&WindowId::PRIMARY)?;
        (self.owned_window_ids.contains(&WindowId::PRIMARY)
            && self.nara_windows_by_winit.get(&window.id()) == Some(&WindowId::PRIMARY)
            && self
                .backend_windows
                .snapshot(WindowId::PRIMARY)
                .is_ok_and(|snapshot| snapshot.phase == WindowTargetPhase::Active))
        .then(|| window.clone())
    }

    pub(super) fn acknowledge_keyboard_focus(&mut self) -> Result<(), AppRunError> {
        if !self.input_gate.focused {
            self.keyboard_continuation.activation = WinitInputActivation::AwaitingFocus;
            return Ok(());
        }
        if self.runtime.state() != RuntimeState::Paused {
            return self
                .retain_keyboard_activation_error(WinitKeyboardContinuationError::NotPaused);
        }
        let Some(window) = self.original_keyboard_window() else {
            return self
                .retain_keyboard_activation_error(WinitKeyboardContinuationError::Unavailable);
        };
        let bindings = self.keyboard_continuation.bindings.clone();
        let down = match query_pressed_keys(&window, &bindings) {
            Ok(Some(down)) => down,
            Ok(None) => {
                self.keyboard_continuation.activation = WinitInputActivation::AwaitingFocus;
                return Ok(());
            }
            Err(error) => return self.retain_keyboard_activation_error(error),
        };
        self.reconcile_keyboard_activation(down)
    }

    pub(super) fn reconcile_keyboard_activation(
        &mut self,
        down: Vec<KeyCode>,
    ) -> Result<(), AppRunError> {
        let holds = match self.capture_keyboard_inventory(&self.keyboard_continuation.bindings) {
            Ok(holds) => holds,
            Err(WinitKeyboardContinuationError::Input(KeyboardContinuationError::PendingInput)) => {
                self.keyboard_continuation.needs_step |= self
                    .runtime
                    .world()
                    .resource::<InputTimeline>()
                    .entries()
                    .any(|entry| {
                        self.keyboard_continuation
                            .claim
                            .as_ref()
                            .is_some_and(|claim| {
                                entry.route().window() == claim.window()
                                    && entry.route().seat() == claim.seat()
                                    && matches!(
                                        entry.action_transition(),
                                        Some((_, nara_input::ButtonTransitionPhase::Released))
                                    )
                            })
                    });
                self.keyboard_continuation.activation = WinitInputActivation::ReconciliationPending;
                return Ok(());
            }
            Err(error) => return self.retain_keyboard_activation_error(error),
        };
        self.input_gate.suppressed_keys.retain(|key, suppression| {
            if down.contains(key) {
                *suppression = KeySuppression::ActivationReconciliation;
                return true;
            }
            !self
                .keyboard_continuation
                .bindings
                .iter()
                .any(|binding| binding.key == *key)
        });
        self.input_gate
            .suppressed_keys
            .extend(down.iter().copied().filter_map(|key| {
                (!holds
                    .iter()
                    .any(|hold| hold.held && hold.binding.key == key))
                .then_some((key, KeySuppression::ActivationReconciliation))
            }));
        let missing = holds
            .iter()
            .filter(|hold| hold.held && !down.contains(&hold.binding.key))
            .map(|hold| hold.binding.key)
            .collect::<Vec<_>>();
        self.keyboard_continuation.activation = WinitInputActivation::ReconciliationPending;
        if missing.is_empty() {
            return Ok(());
        }
        let Some(claim) = self.keyboard_continuation.claim.clone() else {
            return self
                .retain_keyboard_activation_error(WinitKeyboardContinuationError::Unavailable);
        };
        let result = self.apply_keyboard_operation(|scope| {
            nara_input::reconcile_keyboard_holds(scope, claim, missing)
        });
        match result {
            Ok(admission) => {
                self.keyboard_continuation.admission = Some(admission);
                self.keyboard_continuation.needs_step = !admission.is_ignored();
                Ok(())
            }
            Err(error) => self.retain_keyboard_activation_error(error),
        }
    }

    fn retain_keyboard_activation_error(
        &mut self,
        error: WinitKeyboardContinuationError,
    ) -> Result<(), AppRunError> {
        let fault = match &error {
            WinitKeyboardContinuationError::Faulted {
                admission, fault, ..
            } => {
                self.keyboard_continuation.admission = *admission;
                Some(fault.clone())
            }
            WinitKeyboardContinuationError::Scope(RuntimeScopeError::Faulted { fault }) => {
                Some(fault.clone())
            }
            _ => None,
        };
        self.keyboard_continuation.error = Some(error);
        self.keyboard_continuation.activation = WinitInputActivation::Unavailable;
        match fault {
            Some(fault) => Err(runtime_fault_error(&fault)),
            None => Ok(()),
        }
    }

    pub(super) fn finish_keyboard_activation(&mut self) -> Result<(), AppRunError> {
        if self.keyboard_continuation.activation != WinitInputActivation::ReconciliationPending
            || !self.paused_frame_complete
            || !self.is_healthy_live()
        {
            return Ok(());
        }
        if let Some(ticket) = self.keyboard_continuation.step {
            match self.runtime.control_status(ticket) {
                Some(RuntimeControlStatus::Applied) => {
                    self.keyboard_continuation.step = None;
                    self.keyboard_continuation.needs_step = false;
                }
                Some(RuntimeControlStatus::Failed(_)) => {
                    return self.retain_keyboard_activation_error(
                        WinitKeyboardContinuationError::Unavailable,
                    );
                }
                _ => return Ok(()),
            }
        }
        if self.keyboard_continuation.needs_step {
            return Ok(());
        }
        self.acknowledge_keyboard_focus()?;
        if self.keyboard_continuation.activation == WinitInputActivation::ReconciliationPending
            && !self.keyboard_continuation.needs_step
            && self
                .capture_keyboard_inventory(&self.keyboard_continuation.bindings)
                .is_ok()
        {
            self.keyboard_continuation.activation = WinitInputActivation::Active;
        }
        Ok(())
    }
}

pub(super) fn physical_key(key: KeyCode) -> Option<WinitKeyCode> {
    let key = match key {
        KeyCode::Escape => WinitKeyCode::Escape,
        KeyCode::Space => WinitKeyCode::Space,
        KeyCode::Enter => WinitKeyCode::Enter,
        KeyCode::ArrowUp => WinitKeyCode::ArrowUp,
        KeyCode::ArrowDown => WinitKeyCode::ArrowDown,
        KeyCode::ArrowLeft => WinitKeyCode::ArrowLeft,
        KeyCode::ArrowRight => WinitKeyCode::ArrowRight,
        KeyCode::Character('a') => WinitKeyCode::KeyA,
        KeyCode::Character('b') => WinitKeyCode::KeyB,
        KeyCode::Character('c') => WinitKeyCode::KeyC,
        KeyCode::Character('d') => WinitKeyCode::KeyD,
        KeyCode::Character('e') => WinitKeyCode::KeyE,
        KeyCode::Character('f') => WinitKeyCode::KeyF,
        KeyCode::Character('g') => WinitKeyCode::KeyG,
        KeyCode::Character('h') => WinitKeyCode::KeyH,
        KeyCode::Character('i') => WinitKeyCode::KeyI,
        KeyCode::Character('j') => WinitKeyCode::KeyJ,
        KeyCode::Character('k') => WinitKeyCode::KeyK,
        KeyCode::Character('l') => WinitKeyCode::KeyL,
        KeyCode::Character('m') => WinitKeyCode::KeyM,
        KeyCode::Character('n') => WinitKeyCode::KeyN,
        KeyCode::Character('o') => WinitKeyCode::KeyO,
        KeyCode::Character('p') => WinitKeyCode::KeyP,
        KeyCode::Character('q') => WinitKeyCode::KeyQ,
        KeyCode::Character('r') => WinitKeyCode::KeyR,
        KeyCode::Character('s') => WinitKeyCode::KeyS,
        KeyCode::Character('t') => WinitKeyCode::KeyT,
        KeyCode::Character('u') => WinitKeyCode::KeyU,
        KeyCode::Character('v') => WinitKeyCode::KeyV,
        KeyCode::Character('w') => WinitKeyCode::KeyW,
        KeyCode::Character('x') => WinitKeyCode::KeyX,
        KeyCode::Character('y') => WinitKeyCode::KeyY,
        KeyCode::Character('z') => WinitKeyCode::KeyZ,
        KeyCode::Character('0') => WinitKeyCode::Digit0,
        KeyCode::Character('1') => WinitKeyCode::Digit1,
        KeyCode::Character('2') => WinitKeyCode::Digit2,
        KeyCode::Character('3') => WinitKeyCode::Digit3,
        KeyCode::Character('4') => WinitKeyCode::Digit4,
        KeyCode::Character('5') => WinitKeyCode::Digit5,
        KeyCode::Character('6') => WinitKeyCode::Digit6,
        KeyCode::Character('7') => WinitKeyCode::Digit7,
        KeyCode::Character('8') => WinitKeyCode::Digit8,
        KeyCode::Character('9') => WinitKeyCode::Digit9,
        _ => return None,
    };

    Some(key)
}

#[cfg(target_os = "windows")]
fn query_pressed_keys(
    window: &WinitWindow,
    bindings: &[KeyboardActionBinding],
) -> Result<Option<Vec<KeyCode>>, WinitKeyboardContinuationError> {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetFocus, GetKeyboardLayout, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
        },
        WindowsAndMessaging::GetForegroundWindow,
    };
    use winit::{
        platform::scancode::PhysicalKeyExtScancode,
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
    };
    let handle = window
        .window_handle()
        .map_err(|_| WinitKeyboardContinuationError::Unavailable)?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(WinitKeyboardContinuationError::Unsupported);
    };
    let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    if !window.has_focus() || unsafe { GetFocus() != hwnd || GetForegroundWindow() != hwnd } {
        return Ok(None);
    }
    let layout = unsafe { GetKeyboardLayout(0) };
    let mut down = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let key = physical_key(binding.key).ok_or(WinitKeyboardContinuationError::Unsupported)?;
        if convert_key_code(key) != Some(binding.key) {
            return Err(WinitKeyboardContinuationError::Unsupported);
        }
        let scan = key
            .to_scancode()
            .ok_or(WinitKeyboardContinuationError::Unsupported)?;
        let vk = unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, layout) };
        if vk == 0 {
            return Err(WinitKeyboardContinuationError::Unsupported);
        }
        if unsafe { GetAsyncKeyState(vk as i32) } < 0 {
            down.push(binding.key);
        }
    }
    if unsafe {
        GetFocus() != hwnd || GetForegroundWindow() != hwnd || GetKeyboardLayout(0) != layout
    } {
        return Ok(None);
    }
    Ok(Some(down))
}

#[cfg(not(target_os = "windows"))]
fn query_pressed_keys(
    _window: &WinitWindow,
    _bindings: &[KeyboardActionBinding],
) -> Result<Option<Vec<KeyCode>>, WinitKeyboardContinuationError> {
    Err(WinitKeyboardContinuationError::Unsupported)
}
