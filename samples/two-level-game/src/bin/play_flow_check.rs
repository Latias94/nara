//! Bounded physical-keyboard acceptance on the original Windows play-flow DesktopRun.

#[cfg(all(windows, feature = "play-flow", feature = "development"))]
#[path = "play_flow_check/desktop.rs"]
mod desktop;
#[cfg(all(windows, feature = "play-flow-check"))]
#[path = "play_flow_check/readiness.rs"]
mod readiness;

fn main() -> std::process::ExitCode {
    #[cfg(all(windows, feature = "play-flow", feature = "development"))]
    {
        #[cfg(feature = "play-flow-check")]
        if std::env::args_os()
            .skip(1)
            .eq([std::ffi::OsString::from("--readiness-after-start")])
        {
            return readiness::run();
        }
        desktop::run()
    }
    #[cfg(not(all(windows, feature = "play-flow", feature = "development")))]
    {
        eprintln!("play_flow_check requires Windows and features development,play-flow");
        std::process::ExitCode::FAILURE
    }
}
