use std::{env, path::PathBuf, process::ExitCode};

use nara_editor::{EditorLaunchOptions, EditorRunError};

fn main() -> ExitCode {
    match parse_arguments().and_then(nara_editor::run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn parse_arguments() -> Result<EditorLaunchOptions, EditorRunError> {
    let mut project_root = None;
    let mut profile = None;
    let mut arguments = env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--profile" {
            let Some(value) = arguments.next() else {
                return Err(EditorRunError::usage("--profile requires a value"));
            };
            if profile
                .replace(value.to_string_lossy().into_owned())
                .is_some()
            {
                return Err(EditorRunError::usage("--profile may be supplied only once"));
            }
            continue;
        }
        if argument.to_string_lossy().starts_with('-') {
            return Err(EditorRunError::usage(format!(
                "unknown option: {}",
                argument.to_string_lossy()
            )));
        }
        if project_root.replace(PathBuf::from(argument)).is_some() {
            return Err(EditorRunError::usage(
                "only one project root may be supplied",
            ));
        }
    }

    let options = match project_root {
        Some(project_root) => EditorLaunchOptions::new(project_root),
        None => EditorLaunchOptions::project_manager(),
    };
    Ok(options.with_optional_profile(profile))
}

fn report_error(error: &EditorRunError) {
    eprintln!("nara-editor: {error}");
    if let Some(diagnostics) = error.diagnostics() {
        for diagnostic in diagnostics.iter() {
            eprintln!(
                "{}: {}",
                diagnostic.code().as_str(),
                diagnostic.summary().as_str()
            );
        }
    }
}
