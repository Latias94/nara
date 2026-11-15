use std::{ffi::OsString, path::PathBuf};

use two_level_game::{RunOptions, Scenario};

const USAGE: &str = "usage: player_desktop [--observe] [--save ABSOLUTE_PATH] [--verify-first-present] | [--observe] (--verify-travel | --verify-save ABSOLUTE_PATH | --verify-restore ABSOLUTE_PATH)";
const MAX_SAVE_PATH_BYTES: usize = 4096;

pub(super) struct Options {
    pub game: RunOptions,
    pub observe: bool,
    pub verify_first_present: bool,
}

pub(super) fn parse(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Options, &'static str> {
    let mut arguments = arguments.into_iter();
    let mut scenario = None;
    let mut save_path = None;
    let mut observe = false;
    let mut verify_first_present = false;
    let mut interactive_save = false;
    while let Some(argument) = arguments.next() {
        if argument == "--observe" && !observe {
            observe = true;
        } else if argument == "--verify-first-present" && !verify_first_present {
            verify_first_present = true;
        } else if argument == "--save" && save_path.is_none() {
            interactive_save = true;
            save_path = Some(read_path(arguments.next().ok_or(USAGE)?)?);
        } else if argument == "--verify-travel" && scenario.is_none() {
            scenario = Some(Scenario::Travel);
        } else if (argument == "--verify-save" || argument == "--verify-restore")
            && scenario.is_none()
            && save_path.is_none()
        {
            scenario = Some(if argument == "--verify-save" {
                Scenario::Save
            } else {
                Scenario::Restore
            });
            save_path = Some(read_path(arguments.next().ok_or(USAGE)?)?);
        } else {
            return Err(USAGE);
        }
    }
    if scenario.is_some() && (verify_first_present || interactive_save) {
        return Err(USAGE);
    }
    Ok(Options {
        game: RunOptions {
            scenario: scenario.unwrap_or(Scenario::Interactive),
            save_path,
        },
        observe,
        verify_first_present,
    })
}

fn read_path(argument: OsString) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(argument);
    let text = path
        .to_str()
        .ok_or("desktop Player save path must be UTF-8")?;
    if text.len() > MAX_SAVE_PATH_BYTES
        || text.contains('\0')
        || !path.is_absolute()
        || path.file_name().is_none()
    {
        return Err(
            "desktop Player save path must be absolute with a file name and at most 4096 UTF-8 bytes; create its parent explicitly",
        );
    }
    Ok(path)
}
