#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#![forbid(unsafe_code)]

#[cfg(feature = "cook")]
use two_level_game::development::protocol;

#[cfg(feature = "cook")]
#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod continuity;

#[path = "../../../tests/support/cooked_delivery.rs"]
mod support;

#[cfg(feature = "cook")]
#[test]
#[ignore = "actual production cook, serial nested Rust 1.95.0 Cargo, source deletion and native Players; requires offline dependencies, jobs=1 and the absolute shared root target"]
fn production_cooker_delivers_the_source_free_two_level_game() {
    support::native::accept(std::path::Path::new(env!("CARGO_BIN_EXE_cook")));
}

#[cfg(feature = "cook")]
#[test]
#[ignore = "private sealed-environment worker for production_cooker_delivers_the_source_free_two_level_game"]
fn cooked_delivery_sealed_player_worker() {
    support::native::player_worker();
}
