#![cfg(all(
    windows,
    feature = "cook",
    feature = "desktop",
    feature = "development",
    feature = "audio-cpal"
))]
#![deny(unsafe_op_in_unsafe_fn)]

use two_level_game::development::protocol;

#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod continuity;
#[path = "../../../tests/support/cooked_audio/mod.rs"]
mod support;

#[test]
#[ignore = "actual release cook --runtime audio-desktop, WASAPI/hardware desktop and physical input/WM_CLOSE; run with all features, serial Cargo jobs=1, offline Rust 1.95.0, absolute shared root target, idle desktop and all game keys Up"]
fn production_cooked_audio_player_matches_source_samples_and_retires_original_native_owners() {
    support::accept(std::path::Path::new(env!("CARGO_BIN_EXE_cook")));
}

#[test]
#[ignore = "private sealed worker for production_cooked_audio_player_matches_source_samples_and_retires_original_native_owners"]
fn cooked_audio_sealed_player_worker() {
    support::worker();
}
