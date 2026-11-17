use std::time::{Duration, Instant};

use nara::fs::ContentDigest;
use two_level_game::{
    development::{
        ActiveGeneration, DevelopmentHost, InputActivation, ProbeStep,
        protocol::{DesktopPresentation, ExecutionState, ExecutionStatus, Observation},
    },
    resume::{ResumeState, encode_resume},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyboardLayout, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
            KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, SendInput,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
        },
    },
};

use crate::acceptance::{assert_preserved, movement, probe_steps};

const POLL: Duration = Duration::from_millis(10);
const DEADLINE: Duration = Duration::from_secs(5);
const MAX_STATUS_POLLS: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWindow {
    handle: HWND,
    process: u32,
    thread: u32,
}

impl NativeWindow {
    fn observe(handle: HWND) -> Option<Self> {
        if handle.is_null() || unsafe { IsWindow(handle) } == 0 {
            return None;
        }
        let mut process = 0;
        let thread = unsafe { GetWindowThreadProcessId(handle, &mut process) };
        (thread != 0 && process != 0).then_some(Self {
            handle,
            process,
            thread,
        })
    }

    fn foreground() -> Option<Self> {
        Self::observe(unsafe { GetForegroundWindow() })
    }

    fn still_matches(self) -> bool {
        Self::observe(self.handle) == Some(self)
    }
}

/// Owns only its matched injected hold and the original observed foreground restoration.
pub struct NativeInput {
    original: Option<NativeWindow>,
    target: Option<NativeWindow>,
    virtual_key: u32,
    movement_keys: [u32; 4],
    down: bool,
}

impl NativeInput {
    pub fn new() -> Self {
        let layout = unsafe { GetKeyboardLayout(0) };
        let movement_keys = [0x1e, 0x20, 0x11, 0x1f]
            .map(|scan| unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, layout) });
        assert!(
            movement_keys.iter().all(|key| *key != 0),
            "physical A/D/W/S require mapped scan codes",
        );
        let held = movement_keys.map(|key| unsafe { GetAsyncKeyState(key as i32) < 0 });
        assert!(
            held == [false; 4],
            "physical A/D/W/S must initially be Up; native movement keys held: {held:?}",
        );
        Self {
            original: NativeWindow::foreground(),
            target: None,
            virtual_key: movement_keys[1],
            movement_keys,
            down: false,
        }
    }

    pub fn bind_foreground(&mut self, generation: ActiveGeneration) {
        assert!(
            !self.down,
            "finish the prior matched hold before changing targets"
        );
        assert!(
            !self.physical_down(),
            "physical D must be Up before targeting a child"
        );
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if let Some(window) = NativeWindow::foreground()
                && window.process == generation.pid
            {
                self.target = Some(window);
                return;
            }
            std::thread::sleep(POLL);
        }
        panic!(
            "the exact active child {} never became the foreground window: {:?}",
            generation.pid,
            NativeWindow::foreground(),
        );
    }

    pub fn assert_foreground(&self) {
        assert!(
            self.target.is_some(),
            "bind the actual activated child first"
        );
        assert_eq!(
            NativeWindow::foreground(),
            self.target,
            "native input must remain on the exact observed active window",
        );
    }

    pub fn physical_down(&self) -> bool {
        unsafe { GetAsyncKeyState(self.virtual_key as i32) < 0 }
    }

    fn physical_movement(&self) -> [bool; 4] {
        self.movement_keys
            .map(|key| unsafe { GetAsyncKeyState(key as i32) < 0 })
    }

    pub fn press(&mut self) {
        self.assert_foreground();
        assert!(
            !self.down && !self.physical_down(),
            "a matched hold starts Up"
        );
        self.down = true;
        assert!(send_d(false), "matched native D Down admission failed");
        let deadline = Instant::now() + DEADLINE;
        while !self.physical_down() {
            assert!(
                Instant::now() < deadline,
                "native D Down never became physical"
            );
            self.assert_foreground();
            std::thread::sleep(POLL);
        }
    }

    pub fn release(&mut self) -> bool {
        if !self.down {
            return !self.physical_down();
        }
        if !send_d(true) {
            return false;
        }
        let deadline = Instant::now() + DEADLINE;
        while self.physical_down() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(POLL);
        }
        self.down = false;
        true
    }

    pub fn restore_foreground(&mut self) -> bool {
        let Some(original) = self.original else {
            return true;
        };
        if !original.still_matches() {
            self.original = None;
            return true;
        }
        unsafe { SetForegroundWindow(original.handle) };
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if NativeWindow::foreground() == Some(original) {
                self.original = None;
                return true;
            }
            if !original.still_matches() {
                self.original = None;
                return true;
            }
            std::thread::sleep(POLL);
        }
        false
    }
}

impl Drop for NativeInput {
    fn drop(&mut self) {
        if !self.release() {
            eprintln!("continuous desktop cleanup could not release its matched native D hold");
        }
        if !self.restore_foreground() {
            eprintln!("continuous desktop cleanup could not restore the original foreground");
        }
    }
}

fn send_d(up: bool) -> bool {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: 0x20,
                dwFlags: KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}

pub fn status(
    host: &mut DevelopmentHost,
    generation: ActiveGeneration,
    expected: ExecutionState,
) -> ExecutionStatus {
    assert_eq!(host.active(), Some(generation));
    let status = host.execution_status().unwrap();
    assert_eq!(host.active(), Some(generation));
    assert_eq!(status.state, expected, "{status:?}");
    assert_eq!(status.observation.phase, "running", "{status:?}");
    assert!(!status.observation.restoring, "{status:?}");
    assert!(status.observation.error.is_none(), "{status:?}");
    status
}

pub fn poll_running(
    host: &mut DevelopmentHost,
    generation: ActiveGeneration,
    ready: impl Fn(&Observation) -> bool,
) -> ExecutionStatus {
    let deadline = Instant::now() + DEADLINE;
    for _ in 0..MAX_STATUS_POLLS {
        let current = status(host, generation, ExecutionState::Running);
        assert_eq!(
            current.input_activation,
            InputActivation::Active,
            "{current:?}"
        );
        if ready(&current.observation) {
            return current;
        }
        assert!(
            Instant::now() < deadline,
            "continuous observation deadline expired"
        );
        std::thread::sleep(POLL);
    }
    panic!("continuous observation exceeded the finite status request budget");
}

pub fn activate_and_resume(
    host: &mut DevelopmentHost,
    input: &mut NativeInput,
    generation: ActiveGeneration,
) -> ExecutionStatus {
    let activation = host.activate_active().unwrap();
    assert_eq!(activation.active, generation);
    assert_eq!(
        activation.activation,
        InputActivation::Active,
        "{activation:?}"
    );
    assert!(activation.error.is_none(), "{activation:?}");
    input.bind_foreground(generation);
    let resumed = host.resume_active().unwrap();
    assert_eq!(resumed.state, ExecutionState::Running, "{resumed:?}");
    assert_eq!(
        resumed.input_activation,
        InputActivation::Active,
        "{resumed:?}"
    );
    assert_eq!(host.active(), Some(generation));
    resumed
}

pub fn pause_and_capture(host: &mut DevelopmentHost, generation: ActiveGeneration) -> ResumeState {
    let paused = host.pause_active().unwrap();
    assert_eq!(paused.state, ExecutionState::Paused, "{paused:?}");
    assert_eq!(host.active(), Some(generation));
    host.isolate_active_input().unwrap();
    let captured = host.capture().unwrap();
    assert_eq!(captured.held, [false; 4]);
    let paused = status(host, generation, ExecutionState::Paused);
    assert_eq!(paused.input_activation, InputActivation::Isolated);
    assert_eq!(paused.observation.logical_tick, captured.clock.ticks);
    std::thread::sleep(POLL);
    assert_preserved(host, &generation, &captured);
    captured
}

pub fn neutral_probes(base: &ResumeState, speed: f32) -> Vec<ProbeStep> {
    assert_eq!(base.held, [false; 4]);
    let mut held = base.clone();
    held.held = [false, true, false, false];
    let mut probes = probe_steps(&held, speed);
    probes[0].commands = vec![movement(true)];
    probes
}

pub fn assert_presentation(
    presentation: &DesktopPresentation,
    canonical: &ResumeState,
    backend: Option<&str>,
) {
    assert_eq!(
        presentation.state_digest,
        *ContentDigest::of_bytes(&encode_resume(canonical).unwrap()).as_bytes(),
    );
    assert!(presentation.runtime_generation > 0 && presentation.app_frame > 0);
    assert!(presentation.render_frame > 0 && presentation.window > 0);
    assert!(presentation.surface_generation > 0 && presentation.backend_instance > 0);
    assert!(presentation.device_epoch > 0);
    assert!(matches!(
        presentation.adapter_backend.as_str(),
        "dx12" | "vulkan"
    ));
    assert_ne!(presentation.adapter_type, "cpu");
    if let Some(backend) = backend {
        assert_eq!(presentation.adapter_backend, backend);
    }
}

fn assert_free_corridor(player: [f32; 2]) {
    assert!(
        (-260.0..260.0).contains(&player[0]) && (38.0..42.0).contains(&player[1]),
        "rightward speed evidence must stay within the free Y=40 corridor and finite X budget: {player:?}",
    );
}

/// Status observes naturally completed ticks. This path never requests Step or a frame.
pub fn continuous_native_motion(
    host: &mut DevelopmentHost,
    input: &mut NativeInput,
    generation: ActiveGeneration,
    canonical: &ResumeState,
    speed: f32,
) -> (ResumeState, Duration) {
    let started = Instant::now();
    assert!(speed.is_finite() && speed > 0.0);
    assert_free_corridor(canonical.player.translation);
    let initial = status(host, generation, ExecutionState::Running);
    assert_eq!(initial.observation.player, canonical.player.translation);
    assert_eq!(
        initial.observation.held,
        [false; 4],
        "continuous input requires an idle desktop; physical A/D/W/S={:?}",
        input.physical_movement(),
    );
    let runtime = initial.observation.runtime_generation;
    for _ in 0..8 {
        std::thread::sleep(POLL);
    }
    let idle = poll_running(host, generation, |observation| {
        observation.logical_tick >= initial.observation.logical_tick + 2
    });
    assert_eq!(idle.observation.runtime_generation, runtime);
    assert_eq!(idle.observation.player, canonical.player.translation);
    input.press();
    let held = poll_running(host, generation, |observation| {
        input.assert_foreground();
        assert!(input.physical_down());
        assert_free_corridor(observation.player);
        observation.held == [false, true, false, false]
    });
    assert_eq!(held.observation.player[1], canonical.player.translation[1]);
    assert!(held.observation.player[0] >= canonical.player.translation[0]);
    let advanced = poll_running(host, generation, |observation| {
        input.assert_foreground();
        assert!(input.physical_down());
        assert_free_corridor(observation.player);
        observation.logical_tick >= held.observation.logical_tick + 4
    });
    assert!(input.release(), "matched native D Up must complete");
    assert_eq!(advanced.observation.runtime_generation, runtime);
    assert_eq!(advanced.observation.held, [false, true, false, false]);
    let ticks = advanced.observation.logical_tick - held.observation.logical_tick;
    assert!(
        ticks <= 128,
        "movement exceeded the finite oracle budget: {ticks}"
    );
    assert_eq!(
        advanced.observation.logical_elapsed_nanos - held.observation.logical_elapsed_nanos,
        ticks * canonical.clock.timestep_nanos,
    );
    let delta = Duration::from_nanos(canonical.clock.timestep_nanos).as_secs_f32();
    let mut expected = held.observation.player;
    for _ in 0..ticks {
        expected[0] += speed * delta;
    }
    assert_free_corridor(expected);
    assert_eq!(
        advanced.observation.player, expected,
        "actual completed fixed ticks must execute the selected binary's movement law",
    );
    assert!(advanced.observation.player[0] > held.observation.player[0]);
    let released = poll_running(host, generation, |observation| {
        input.assert_foreground();
        assert_free_corridor(observation.player);
        observation.held == [false; 4]
    });
    assert!(released.observation.player[0] >= advanced.observation.player[0]);
    assert_eq!(
        released.observation.player[1],
        advanced.observation.player[1]
    );
    let stopped = poll_running(host, generation, |observation| {
        assert_free_corridor(observation.player);
        observation.logical_tick >= released.observation.logical_tick + 2
    });
    assert_eq!(stopped.observation.runtime_generation, runtime);
    assert_eq!(stopped.observation.held, [false; 4]);
    assert_eq!(stopped.observation.player, released.observation.player);
    let captured = pause_and_capture(host, generation);
    assert_free_corridor(captured.player.translation);
    assert_eq!(captured.player.translation, stopped.observation.player);
    assert!(captured.clock.ticks >= stopped.observation.logical_tick);
    assert!(captured.clock.ticks > canonical.clock.ticks);
    assert!(!input.physical_down());
    (captured, started.elapsed())
}
