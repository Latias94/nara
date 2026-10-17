use std::{path::Path, process::ExitCode};

use nara_editor::{EditorLaunchOptions, run_imgui_qualification, run_trusted_compiled};
use nara_reference_game::wave_game_view_intent;

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let mode = arguments.next();
    let options = EditorLaunchOptions::new(Path::new(env!("CARGO_MANIFEST_DIR")));
    let result = if mode.as_deref() == Some(std::ffi::OsStr::new("--egui")) {
        run_trusted_compiled(options, wave_game_view_intent())
    } else if mode.as_deref() == Some(std::ffi::OsStr::new("--generic")) {
        let options = arguments
            .next()
            .map_or_else(EditorLaunchOptions::project_manager, |path| {
                EditorLaunchOptions::new(std::path::PathBuf::from(path))
            });
        run_imgui_qualification(options, None)
    } else if mode.as_deref() == Some(std::ffi::OsStr::new("--trusted")) {
        let options = arguments.next().map_or(options, |path| {
            EditorLaunchOptions::new(std::path::PathBuf::from(path))
        });
        run_imgui_qualification(options, Some(wave_game_view_intent()))
    } else {
        run_imgui_qualification(options, Some(wave_game_view_intent()))
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("editor_imgui_qualification: {error}");
            ExitCode::FAILURE
        }
    }
}
