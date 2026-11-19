#![cfg(all(
    windows,
    feature = "cook",
    feature = "development",
    feature = "play-flow"
))]
#![deny(unsafe_op_in_unsafe_fn)]

use two_level_game::{delivery::RuntimeKind, development::protocol};

#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod continuity;
#[path = "../../../tests/support/cooked_round_flow.rs"]
mod support;

#[test]
#[ignore = "actual release cook --runtime desktop --flow round, relocated source-free Windows/DX12 hardware physical full round; serial Cargo jobs=1, offline Rust 1.95.0, absolute shared root target, idle desktop and all acceptance keys Up"]
fn production_cooked_round_desktop_accepts_physical_complete_round() {
    support::accept(
        std::path::Path::new(env!("CARGO_BIN_EXE_cook")),
        RuntimeKind::Desktop,
    );
}

#[test]
#[cfg(feature = "audio-cpal")]
#[ignore = "actual release cook --runtime audio-desktop --flow round, source-free Windows/DX12 hardware and WASAPI full physical round with original stream retirement; serial Cargo jobs=1, offline Rust 1.95.0, absolute shared root target, idle desktop and all acceptance keys Up"]
fn production_cooked_round_audio_desktop_accepts_physical_complete_round() {
    support::accept(
        std::path::Path::new(env!("CARGO_BIN_EXE_cook")),
        RuntimeKind::AudioDesktop,
    );
}

#[test]
#[ignore = "private sealed worker for production cooked round-flow acceptance"]
fn cooked_round_flow_sealed_player_worker() {
    support::worker();
}
