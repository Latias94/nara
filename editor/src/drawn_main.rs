use std::{env, path::PathBuf, process::ExitCode};

use nara_editor::{EditorLaunchOptions, EditorRunError};

fn main() -> ExitCode {
    match arguments().and_then(nara_editor::run_drawn) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nara-editor-drawn: {error}");
            ExitCode::FAILURE
        }
    }
}

fn arguments() -> Result<EditorLaunchOptions, EditorRunError> {
    let mut root = None;
    let mut profile = None;
    let mut arguments = env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--profile" {
            let value = arguments
                .next()
                .ok_or_else(|| EditorRunError::usage("--profile requires a value"))?;
            if profile
                .replace(value.to_string_lossy().into_owned())
                .is_some()
            {
                return Err(EditorRunError::usage("--profile may be supplied only once"));
            }
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(EditorRunError::usage(format!(
                "unknown option: {}",
                argument.to_string_lossy()
            )));
        } else if root.replace(PathBuf::from(argument)).is_some() {
            return Err(EditorRunError::usage(
                "only one project root may be supplied",
            ));
        }
    }
    Ok(EditorLaunchOptions::new(root.ok_or_else(|| {
        EditorRunError::usage("usage: nara-editor-drawn <project-root> [--profile <profile>]")
    })?)
    .with_optional_profile(profile))
}
