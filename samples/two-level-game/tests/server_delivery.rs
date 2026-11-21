#![cfg(all(
    any(windows, all(target_os = "linux", target_arch = "x86_64")),
    feature = "cook",
    feature = "development"
))]
#![forbid(unsafe_code)]

use two_level_game::development::protocol;

#[path = "../../../tests/support/continuity.rs"]
#[allow(dead_code)]
mod continuity;
#[path = "../../../tests/support/cooked_delivery.rs"]
#[allow(dead_code)]
mod cooked_delivery;
#[path = "support/server_delivery/mod.rs"]
mod server_delivery;

#[test]
#[ignore = "actual Two-Level server Cook and relocated native execution; requires locked offline Rust 1.95.0, serial Cargo jobs=1 and the absolute shared root target"]
fn production_cooker_delivers_the_source_free_two_level_server() {
    server_delivery::accept(std::path::Path::new(env!("CARGO_BIN_EXE_cook")));
}

#[test]
#[ignore = "actual Two-Level Server Cook and scaled process execution; requires locked offline Rust 1.95.0, serial Cargo jobs=1 and the absolute shared root target"]
fn production_cooker_delivers_scaled_two_level_server_timing() {
    server_delivery::accept_scaled(std::path::Path::new(env!("CARGO_BIN_EXE_cook")));
}

#[test]
#[ignore = "private sealed-environment worker for production_cooker_delivers_the_source_free_two_level_server"]
fn cooked_delivery_sealed_player_worker() {
    cooked_delivery::native::player_worker();
}
