use std::{env, path::PathBuf};

use nara_editor::{EditorLaunchOptions, EditorRunError};

fn main() -> Result<(), EditorRunError> {
    let mut arguments = env::args_os().skip(1);
    let project_root = arguments
        .next()
        .ok_or_else(|| EditorRunError::usage("usage: drawn_editor <project-root>"))?;
    if arguments.next().is_some() {
        return Err(EditorRunError::usage(
            "only one project root may be supplied",
        ));
    }

    nara_editor::run_drawn(EditorLaunchOptions::new(PathBuf::from(project_root)))
}
