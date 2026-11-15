#![forbid(unsafe_code)]

use std::process::ExitCode;

#[path = "player_desktop/arguments.rs"]
#[cfg_attr(not(nara_cooked_player), allow(dead_code))]
mod arguments;
#[path = "player_desktop/output.rs"]
#[cfg_attr(not(nara_cooked_player), allow(dead_code))]
mod desktop_output;
#[cfg(nara_cooked_player)]
#[path = "player/evidence.rs"]
#[allow(dead_code)]
mod evidence;
#[cfg(nara_cooked_player)]
mod generated {
    include!("player/generated.rs");
}
#[path = "player/output.rs"]
#[allow(dead_code)]
mod output;
#[cfg(nara_cooked_player)]
#[path = "player_desktop/presentation.rs"]
mod presentation;
#[cfg(nara_cooked_player)]
#[path = "player_desktop/runtime.rs"]
mod runtime;
#[cfg(nara_cooked_player)]
#[path = "player_desktop/verification.rs"]
mod verification;

fn main() -> ExitCode {
    let options = match arguments::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(message) => return desktop_output::refuse(None, message),
    };
    #[cfg(nara_cooked_player)]
    {
        runtime::run(options)
    }
    #[cfg(not(nara_cooked_player))]
    {
        desktop_output::refuse(
            Some(options.game.scenario),
            "desktop Player was built without a generated cooked-content expectation; build and publish it with the two-level game cooker",
        )
    }
}
