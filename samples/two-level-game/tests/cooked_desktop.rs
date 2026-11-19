#![cfg(all(
    windows,
    feature = "cook",
    feature = "desktop",
    feature = "development"
))]
#![deny(unsafe_op_in_unsafe_fn)]

use two_level_game::{delivery::ControllerBackend, development::protocol};

#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod continuity;
#[path = "../../../tests/support/cooked_desktop.rs"]
mod support;

#[test]
#[ignore = "production release cook and source-free Windows/DX12 physical play; serial Cargo jobs=1, offline Rust 1.95.0, shared root target, an idle desktop and all game keys Up are required"]
fn production_cooked_desktop_accepts_physical_play_save_and_fresh_restore() {
    support::accept(
        std::path::Path::new(env!("CARGO_BIN_EXE_cook")),
        ControllerBackend::Disabled,
    );
}

#[test]
#[cfg(feature = "gamepad")]
#[ignore = "complete source-free Windows/DX12 keyboard play/save/restore with Gilrs WGI inventory; zero connected controllers is valid; serial Cargo jobs=1, offline Rust 1.95.0, shared root target, an idle desktop and all game keys Up are required"]
fn production_cooked_desktop_with_gilrs_accepts_physical_play_save_and_fresh_restore() {
    support::accept(
        std::path::Path::new(env!("CARGO_BIN_EXE_cook")),
        ControllerBackend::Gilrs,
    );
}

#[test]
#[ignore = "private sealed worker for production_cooked_desktop_accepts_physical_play_save_and_fresh_restore"]
fn cooked_desktop_sealed_player_worker() {
    support::worker();
}
