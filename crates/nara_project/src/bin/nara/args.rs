use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

#[derive(Debug)]
pub(super) enum Command {
    Help,
    Version,
    New {
        destination: PathBuf,
        package: String,
        checkout: PathBuf,
    },
    Check(PathBuf),
    Schema {
        project: PathBuf,
        refresh: bool,
    },
    Play {
        project: PathBuf,
        options: PlayOptions,
        game_args: Vec<OsString>,
    },
}

#[derive(Debug, Default)]
pub(super) struct PlayOptions {
    pub(super) binary: Option<String>,
    pub(super) features: Vec<String>,
    pub(super) release: bool,
}

pub(super) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(Command::Help);
    };
    let command = command.to_str().ok_or("expected a command")?;
    match command {
        "-h" | "--help" | "help" if arguments.next().is_none() => return Ok(Command::Help),
        "-V" | "--version" if arguments.next().is_none() => return Ok(Command::Version),
        "new" | "check" | "schema" | "play" => {}
        _ => return Err(format!("unknown command {command:?}")),
    }
    let path = path_arg(
        &mut arguments,
        if command == "new" {
            "destination"
        } else {
            "project directory"
        },
    )?;
    if matches!(path.to_str(), Some("--help" | "-h")) {
        return Ok(Command::Help);
    }
    match command {
        "new" => parse_new(path, arguments),
        "check" => {
            require_end(arguments)?;
            Ok(Command::Check(path))
        }
        "schema" => {
            let refresh = match arguments.next() {
                Some(option) if option == "--trusted" => true,
                Some(option) => return Err(unknown_option(&option)),
                None => false,
            };
            require_end(arguments)?;
            Ok(Command::Schema {
                project: path,
                refresh,
            })
        }
        "play" => parse_play(path, arguments),
        _ => Err(format!("unknown command {command:?}")),
    }
}

fn parse_new(
    destination: PathBuf,
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Command, String> {
    let mut package = None;
    let mut checkout = None;
    while let Some(option) = arguments.next() {
        match option.to_str() {
            Some("--package") if package.is_none() => {
                package = Some(text_arg(&mut arguments, "--package")?)
            }
            Some("--nara-checkout") if checkout.is_none() => {
                checkout = Some(path_arg(&mut arguments, "--nara-checkout")?)
            }
            _ => return Err(unknown_option(&option)),
        }
    }
    Ok(Command::New {
        destination,
        package: package.ok_or("new requires --package <name>")?,
        checkout: checkout.ok_or("new requires --nara-checkout <path>")?,
    })
}

fn parse_play(
    project: PathBuf,
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Command, String> {
    let mut trusted = false;
    let mut options = PlayOptions::default();
    let mut game_args = Vec::new();
    while let Some(option) = arguments.next() {
        match option.to_str() {
            Some("--trusted") if !trusted => trusted = true,
            Some("--release") if !options.release => options.release = true,
            Some("--bin") if options.binary.is_none() => {
                options.binary = Some(text_arg(&mut arguments, "--bin")?)
            }
            Some("--features") => {
                let list = text_arg(&mut arguments, "--features")?;
                for feature in list.split(',').map(str::trim) {
                    if feature.is_empty() {
                        return Err(
                            "--features requires a comma-separated list of feature names".into(),
                        );
                    }
                    options.features.push(feature.to_owned());
                }
            }
            Some("--") => {
                game_args = arguments.collect();
                break;
            }
            _ => return Err(unknown_option(&option)),
        }
    }
    if !trusted {
        return Err("play requires --trusted to execute project code".into());
    }
    Ok(Command::Play {
        project,
        options,
        game_args,
    })
}

fn require_end(mut arguments: impl Iterator<Item = OsString>) -> Result<(), String> {
    arguments
        .next()
        .map_or(Ok(()), |option| Err(unknown_option(&option)))
}

fn path_arg(
    arguments: &mut impl Iterator<Item = OsString>,
    label: &str,
) -> Result<PathBuf, String> {
    arguments
        .next()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {label}"))
}

fn text_arg(arguments: &mut impl Iterator<Item = OsString>, label: &str) -> Result<String, String> {
    arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| format!("missing or invalid value for {label}"))
}

fn unknown_option(option: &OsStr) -> String {
    format!("unexpected or repeated argument {option:?}")
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    #[test]
    fn only_explicit_trust_enables_execution() {
        assert!(parse(["play", "."].map(Into::into)).is_err());
        assert!(parse(["play", ".", "--trusted"].map(Into::into)).is_ok());
        assert!(parse(["play", ".", "--", "--trusted"].map(Into::into)).is_err());
        assert!(matches!(
            parse(["schema", "."].map(Into::into)).unwrap(),
            Command::Schema { refresh: false, .. }
        ));
        assert!(matches!(
            parse(["schema", ".", "--trusted"].map(Into::into)).unwrap(),
            Command::Schema { refresh: true, .. }
        ));
    }

    #[test]
    fn play_keeps_game_arguments_separate_from_cargo_options() {
        let command = parse(
            [
                "play",
                "game folder",
                "--trusted",
                "--release",
                "--bin",
                "demo",
                "--features",
                "one,two",
                "--",
                "--release",
                "two words",
            ]
            .map(Into::into),
        )
        .unwrap();
        let Command::Play {
            project,
            options,
            game_args,
        } = command
        else {
            panic!("expected a Play command");
        };
        assert_eq!(project.to_str(), Some("game folder"));
        assert_eq!(options.binary.as_deref(), Some("demo"));
        assert_eq!(options.features, ["one", "two"]);
        assert!(options.release);
        assert_eq!(game_args, ["--release", "two words"]);
    }
}
