use std::error::Error;

#[cfg(feature = "source")]
const USAGE: &str = "game [--verify-first-present | --verify-gameplay | --verify-r1 <save-dir> | --editor-play <scene-path>]";
#[cfg(not(feature = "source"))]
const USAGE: &str = "game [--verify-first-present | --verify-gameplay | --verify-r1 <save-dir>]";

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let argument = arguments.next();
    let option = argument.as_deref().and_then(std::ffi::OsStr::to_str);
    let operand = arguments.next();
    match (option, operand) {
        (None, None) if argument.is_none() => nara_reference_game::desktop::run(false),
        (Some("--verify-first-present"), None) => nara_reference_game::desktop::run(true),
        (Some("--verify-gameplay"), None) => nara_reference_game::desktop::verify_gameplay(),
        (Some("--verify-r1"), Some(directory)) if arguments.next().is_none() => {
            nara_reference_game::desktop::verify_r1(std::path::Path::new(&directory))
        }
        #[cfg(feature = "source")]
        (Some("--editor-play"), Some(scene)) if arguments.next().is_none() => {
            nara_reference_game::desktop::run_editor_play(std::path::Path::new(&scene))
        }
        (Some("--help" | "-h"), None) => {
            println!("Usage: {USAGE}");
            Ok(())
        }
        _ => Err(format!("usage: {USAGE}").into()),
    }
}
