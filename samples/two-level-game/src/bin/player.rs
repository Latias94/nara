//! Finite source-free Player with independently compiled content expectations.

#![forbid(unsafe_code)]

use std::process::ExitCode;

#[path = "player/arguments.rs"]
mod arguments;
#[cfg(nara_cooked_player)]
#[path = "player/evidence.rs"]
mod evidence;
#[cfg(nara_cooked_player)]
mod generated {
    include!("player/generated.rs");
}
#[path = "player/output.rs"]
mod output;
#[cfg(nara_cooked_player)]
#[path = "player/runtime.rs"]
mod runtime;

fn main() -> ExitCode {
    let options = match arguments::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(message) => return output::emit(output::failure(None, message)),
    };
    #[cfg(nara_cooked_player)]
    {
        runtime::run(options)
    }
    #[cfg(not(nara_cooked_player))]
    {
        output::emit(output::failure(
            Some(options.scenario),
            "Player was built without a generated cooked-content expectation; build and publish it with the two-level game cooker",
        ))
    }
}
