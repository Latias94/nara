use nara_engine::input::{AxisValue, KeyCode};

use super::{
    recording::{Admission, Control, DeviceOrigin, RawOperation, RawRecording},
    runtime::{Bindings, ConfigurationChange, Intent, Player, Seat, Window, fixed_nanos},
};

fn axis(device: u8, value: f32) -> RawOperation {
    RawOperation::Axis {
        device,
        axis: 0,
        value: AxisValue::new(value).unwrap(),
        admission: Admission::Accept,
    }
}

fn button(device: u8, pressed: bool) -> RawOperation {
    RawOperation::Button {
        device,
        button: 0,
        pressed,
        admission: Admission::Accept,
    }
}

fn key(key: KeyCode, pressed: bool) -> RawOperation {
    RawOperation::Key {
        window: Window::Play,
        seat: Seat::Keyboard,
        key,
        pressed,
    }
}

fn configure(configuration: ConfigurationChange) -> RawOperation {
    RawOperation::Configure { configuration }
}

fn frame(checkpoint: &str, ticks: u32) -> RawOperation {
    RawOperation::Frame {
        real_delta_nanos: fixed_nanos(ticks),
        steps: ticks,
        checkpoint: checkpoint.to_owned(),
    }
}

pub(super) fn complete_trajectory() -> RawRecording {
    use ConfigurationChange as Config;
    use RawOperation as Op;

    let positive = Bindings::OriginalV1.positive();
    let negative = Bindings::OriginalV1.negative();
    let fire = Bindings::OriginalV1.fire();
    RawRecording::new(vec![
        Op::Endpoint {
            window: Window::Play,
            seat: Seat::Keyboard,
        },
        Op::Endpoint {
            window: Window::Guest,
            seat: Seat::Controller,
        },
        configure(Config::Present {
            window: Window::Play,
            seat: Seat::Keyboard,
        }),
        configure(Config::Present {
            window: Window::Guest,
            seat: Seat::Controller,
        }),
        frame("routes", 0),
        Op::Focus {
            window: Window::Play,
            seat: Seat::Keyboard,
            focused: true,
        },
        Op::Focus {
            window: Window::Guest,
            seat: Seat::Controller,
            focused: true,
        },
        Op::Connect {
            device: 0,
            window: Window::Guest,
            seat: Seat::Controller,
            origin: DeviceOrigin::Platform,
        },
        Op::Connect {
            device: 1,
            window: Window::Guest,
            seat: Seat::Controller,
            origin: DeviceOrigin::Synthetic,
        },
        axis(0, 0.125),
        frame("dead-zone-interior", 0),
        axis(0, 0.25),
        frame("dead-zone-boundary", 0),
        key(positive, true),
        key(fire, true),
        axis(0, 0.625),
        button(0, true),
        Op::External {
            tick: 1,
            sequence: 1,
            player: Player::Copper,
            intent: Intent::Axis(AxisValue::new(0.25).unwrap()),
        },
        frame("staged-two-seats", 0),
        frame("first-three-ticks", 3),
        axis(1, 1.0),
        button(1, true),
        key(negative, true),
        frame("summed-contributors", 1),
        axis(1, 0.625),
        axis(0, -0.625),
        key(negative, false),
        frame("opposing-contributors", 1),
        Op::Disconnect {
            device: 0,
            admission: Admission::Accept,
        },
        key(positive, false),
        frame("disconnect-preserves-survivor", 2),
        Op::Control {
            control: Control::Pause,
        },
        frame("paused", 0),
        key(positive, true),
        axis(1, -0.625),
        Op::Frame {
            real_delta_nanos: fixed_nanos(1),
            steps: 0,
            checkpoint: "paused-input".to_owned(),
        },
        Op::Control {
            control: Control::ExactStep,
        },
        Op::Frame {
            real_delta_nanos: 123,
            steps: 1,
            checkpoint: "exact-step-before-resume".to_owned(),
        },
        Op::Control {
            control: Control::Resume,
        },
        frame("resumed-input", 0),
        frame("resumed-two-ticks", 2),
        configure(Config::Rebind {
            seat: Seat::Keyboard,
            bindings: Bindings::ReboundV1,
        }),
        frame("observed-held-rebind", 1),
        configure(Config::Rebind {
            seat: Seat::Keyboard,
            bindings: Bindings::OriginalV1,
        }),
        frame("restored-bindings-do-not-adopt-held", 1),
        key(positive, false),
        key(fire, false),
        frame("old-key-members-ended", 0),
        key(positive, true),
        frame("new-key-member", 1),
        configure(Config::Context {
            seat: Seat::Controller,
            enabled: false,
        }),
        frame("controller-context-disabled", 1),
        configure(Config::Context {
            seat: Seat::Controller,
            enabled: true,
        }),
        frame("controller-context-enabled", 0),
        axis(1, 0.8125),
        frame("old-axis-after-context-loss", 1),
        axis(1, 0.0),
        button(1, false),
        frame("raw-neutral-after-context-loss", 0),
        axis(1, 0.25),
        frame("new-axis-starts-inside-dead-zone", 0),
        axis(1, 0.625),
        frame("adopted-zero-contribution-changes", 1),
        configure(Config::Remove {
            seat: Seat::Controller,
        }),
        frame("controller-map-removed", 1),
        axis(1, 0.8125),
        frame("axis-updated-without-map", 0),
        configure(Config::Restore {
            seat: Seat::Controller,
        }),
        frame("original-owned-map-restored", 0),
        axis(1, -0.625),
        frame("old-axis-after-map-restoration", 1),
        axis(1, 0.0),
        frame("raw-neutral-after-map-restoration", 0),
        axis(1, 0.625),
        frame("fresh-axis-after-map-restoration", 1),
        configure(Config::Capture {
            seat: Some(Seat::Keyboard),
        }),
        frame("keyboard-ui-capture", 0),
        key(fire, true),
        frame("ui-consumed-fire-press", 0),
        configure(Config::Capture { seat: None }),
        frame("ui-capture-returned", 0),
        key(fire, false),
        frame("ui-owned-release-does-not-leak", 1),
        key(fire, true),
        frame("fresh-gameplay-fire", 1),
        Op::Focus {
            window: Window::Play,
            seat: Seat::Keyboard,
            focused: false,
        },
        frame("keyboard-focus-lost", 1),
        Op::Focus {
            window: Window::Play,
            seat: Seat::Keyboard,
            focused: true,
        },
        key(positive, true),
        frame("keyboard-focus-returned", 1),
        Op::Disconnect {
            device: 1,
            admission: Admission::Accept,
        },
        frame("last-controller-disconnected", 1),
        Op::Connect {
            device: 2,
            window: Window::Guest,
            seat: Seat::Controller,
            origin: DeviceOrigin::Synthetic,
        },
        frame("same-label-reconnected", 0),
        Op::Axis {
            device: 1,
            axis: 0,
            value: AxisValue::new(1.0).unwrap(),
            admission: Admission::Expired,
        },
        Op::Button {
            device: 1,
            button: 0,
            pressed: true,
            admission: Admission::Expired,
        },
        Op::Disconnect {
            device: 1,
            admission: Admission::Expired,
        },
        Op::ForeignAxis {
            value: AxisValue::new(1.0).unwrap(),
        },
        frame("stale-and-foreign-refused", 1),
        axis(2, -0.625),
        button(2, true),
        frame("new-controller-controls-player", 2),
    ])
}
