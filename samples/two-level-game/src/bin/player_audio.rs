//! Explicit audible playback and callback verification of the executable-relative cooked game.

#![forbid(unsafe_code)]

use std::process::ExitCode;

#[path = "player_audio/arguments.rs"]
mod arguments;
#[path = "player_desktop/arguments.rs"]
mod desktop_arguments;
#[path = "player_desktop/output.rs"]
#[allow(dead_code)]
mod desktop_output;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player/evidence.rs"]
#[allow(dead_code)]
mod evidence;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player_audio/observation.rs"]
mod observation;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player_audio/oracle.rs"]
mod oracle;
#[path = "player/output.rs"]
#[allow(dead_code)]
mod output;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/nara-player-bootstrap.rs"));
}
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player_desktop/presentation.rs"]
mod presentation;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player_desktop/runtime.rs"]
mod runtime;
#[cfg(all(
    nara_cooked_player,
    windows,
    feature = "desktop",
    feature = "audio-cpal"
))]
#[path = "player_desktop/verification.rs"]
mod verification;

fn main() -> ExitCode {
    let options = match arguments::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(message) => return desktop_output::refuse(None, message),
    };
    #[cfg(all(
        nara_cooked_player,
        windows,
        feature = "desktop",
        feature = "audio-cpal"
    ))]
    {
        if options.verify_audio
            && (!matches!(
                std::env::var("WGPU_BACKEND").as_deref(),
                Ok("dx12" | "vulkan")
            ) || std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() != Ok("0"))
        {
            return desktop_output::refuse(
                Some(options.game.scenario),
                "player_audio --verify requires WGPU_BACKEND=dx12 or vulkan and NARA_WGPU_FORCE_FALLBACK=0; a real WASAPI output device is required",
            );
        }
        let observed = observation::AudioObservation::new(options.verify_audio);
        runtime::run_with(options, observed)
    }
    #[cfg(not(all(
        nara_cooked_player,
        windows,
        feature = "desktop",
        feature = "audio-cpal"
    )))]
    {
        desktop_output::refuse(
            Some(options.game.scenario),
            "player_audio requires Windows, desktop and audio-cpal features, and a generated audio-desktop cooked-content expectation; publish it with the two-level game cooker",
        )
    }
}
