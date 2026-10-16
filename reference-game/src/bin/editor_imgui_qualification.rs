use std::{path::Path, process::ExitCode};

use nara_editor::{EditorLaunchOptions, run_imgui_qualification, run_trusted_compiled};
use nara_reference_game::wave_game_view_intent;

fn main() -> ExitCode {
    let options = EditorLaunchOptions::new(Path::new(env!("CARGO_MANIFEST_DIR")));
    let result = if std::env::args().nth(1).as_deref() == Some("--egui") {
        run_trusted_compiled(options, wave_game_view_intent())
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
