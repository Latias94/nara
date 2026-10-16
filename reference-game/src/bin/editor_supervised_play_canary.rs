use std::{env, ffi::OsStr, path::Path, process::ExitCode};

use nara_editor::{EditorLaunchOptions, run};

const CANARY_ENV: &str = "NARA_EDITOR_SUPERVISED_PLAY_CANARY";

fn main() -> ExitCode {
    if env::var_os(CANARY_ENV).as_deref() != Some(OsStr::new("1")) {
        eprintln!(
            "editor_supervised_play_canary: set {CANARY_ENV}=1 before launching the native canary"
        );
        return ExitCode::FAILURE;
    }

    match run(
        EditorLaunchOptions::new(Path::new(env!("CARGO_MANIFEST_DIR"))).with_profile("desktop"),
    ) {
        Ok(()) => {
            println!("editor_supervised_play_canary: ok");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("editor_supervised_play_canary: {error}");
            ExitCode::FAILURE
        }
    }
}
