use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let argument = arguments.next();
    if arguments.next().is_some() {
        return Err("usage: game [--verify-first-present]".into());
    }
    match argument.as_deref().and_then(std::ffi::OsStr::to_str) {
        None if argument.is_none() => nara_reference_game::desktop::run(false),
        Some("--verify-first-present") => nara_reference_game::desktop::run(true),
        Some("--help" | "-h") => {
            println!("Usage: game [--verify-first-present]");
            Ok(())
        }
        _ => Err("usage: game [--verify-first-present]".into()),
    }
}
