use std::{error::Error, path::Path};

const USAGE: &str = "game [--verify-first-present | --editor-play <scene-path>]";

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let argument = arguments.next();
    let option = argument.as_deref().and_then(std::ffi::OsStr::to_str);
    let scene = arguments.next();
    match (option, scene) {
        (None, None) if argument.is_none() => nara_reference_game::desktop::run(false),
        (Some("--verify-first-present"), None) => nara_reference_game::desktop::run(true),
        (Some("--editor-play"), Some(scene)) if arguments.next().is_none() => {
            nara_reference_game::desktop::run_editor_play(Path::new(&scene))
        }
        (Some("--help" | "-h"), None) => {
            println!("Usage: {USAGE}");
            Ok(())
        }
        _ => Err(format!("usage: {USAGE}").into()),
    }
}
