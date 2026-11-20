//! Ordinary audible gameplay and finite native callback qualification.

#![forbid(unsafe_code)]

#[cfg(windows)]
#[path = "audio_desktop/oracle.rs"]
mod oracle;
#[cfg(windows)]
#[path = "audio_desktop/project.rs"]
mod project;
#[cfg(windows)]
#[path = "audio_desktop/workflow.rs"]
mod workflow;

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    workflow::main()
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("audio_desktop: unsupported platform; this composition requires Windows WASAPI");
    std::process::ExitCode::FAILURE
}
