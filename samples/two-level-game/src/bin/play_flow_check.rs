//! Bounded physical-keyboard acceptance on the original Windows play-flow DesktopRun.

#[cfg(all(windows, feature = "play-flow", feature = "development"))]
#[path = "play_flow_check/desktop.rs"]
mod desktop;

fn main() -> std::process::ExitCode {
    #[cfg(all(windows, feature = "play-flow", feature = "development"))]
    {
        desktop::run()
    }
    #[cfg(not(all(windows, feature = "play-flow", feature = "development")))]
    {
        eprintln!("play_flow_check requires Windows and features development,play-flow");
        std::process::ExitCode::FAILURE
    }
}
