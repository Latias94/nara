use std::ffi::OsString;

use two_level_game::RunOptions;

const USAGE: &str = "usage: player_audio [--observe] [--save ABSOLUTE_PATH] | [--observe] --verify";

#[cfg_attr(not(nara_cooked_player), allow(dead_code))]
pub(super) struct Options {
    pub game: RunOptions,
    pub observe: bool,
    pub verify_first_present: bool,
    pub verify_audio: bool,
}

pub(super) fn parse(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Options, &'static str> {
    let mut arguments = arguments.into_iter();
    let mut desktop = Vec::new();
    let mut verify = false;
    while let Some(argument) = arguments.next() {
        if argument == "--verify" && !verify {
            verify = true;
            desktop.push(OsString::from("--verify-travel"));
        } else if argument == "--observe" {
            desktop.push(argument);
        } else if argument == "--save" {
            desktop.push(argument);
            desktop.push(arguments.next().ok_or(USAGE)?);
        } else {
            return Err(USAGE);
        }
    }
    let parsed = super::desktop_arguments::parse(desktop).map_err(|_| USAGE)?;
    Ok(Options {
        game: parsed.game,
        observe: parsed.observe,
        verify_first_present: parsed.verify_first_present,
        verify_audio: verify,
    })
}
