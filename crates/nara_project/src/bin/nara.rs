use std::process::ExitCode;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[path = "nara/cli.rs"]
mod cli;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn main() -> ExitCode {
    cli::run()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn main() -> ExitCode {
    eprintln!("The Nara CLI is supported on Windows, macOS, and Linux.");
    ExitCode::FAILURE
}
