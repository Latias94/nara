use std::{ffi::OsString, path::PathBuf};

use nara_project::delivery::DeliveryOperation;

#[derive(Debug)]
pub(super) enum Command {
    Help,
    Version,
    New {
        destination: PathBuf,
        package: String,
        checkout: PathBuf,
    },
    Check {
        project: PathBuf,
        profile: Option<String>,
    },
    Schema {
        project: PathBuf,
        refresh: bool,
    },
    Build {
        project: PathBuf,
        options: BuildOptions,
        game_args: Option<Vec<OsString>>,
    },
    Delivery {
        project: PathBuf,
        operation: DeliveryOperation,
        options: DeliveryOptions,
    },
}

#[derive(Debug, Default)]
pub(super) struct BuildOptions {
    pub(super) binary: Option<String>,
    pub(super) features: Vec<String>,
    pub(super) release: bool,
}

#[derive(Debug)]
pub(super) struct DeliveryOptions {
    pub(super) build: BuildOptions,
    pub(super) recipe: String,
    pub(super) target: String,
    pub(super) output_parent: PathBuf,
    pub(super) name: String,
    pub(super) cooker_args: Vec<OsString>,
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
        "new" | "check" | "schema" | "build" | "play" | "cook" | "export" => {}
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
            let profile = match arguments.next() {
                Some(option) if option == "--profile" => {
                    Some(text_arg(&mut arguments, "--profile")?)
                }
                Some(option) => return Err(unknown_option(&option)),
                None => None,
            };
            require_end(arguments)?;
            Ok(Command::Check {
                project: path,
                profile,
            })
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
        "build" | "play" => parse_build(path, command == "play", arguments),
        "cook" => parse_delivery(path, DeliveryOperation::Cook, arguments),
        "export" => parse_delivery(path, DeliveryOperation::Export, arguments),
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

fn parse_build(
    project: PathBuf,
    play: bool,
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Command, String> {
    let mut trusted = false;
    let mut options = BuildOptions::default();
    let mut game_args = play.then(Vec::new);
    while let Some(option) = arguments.next() {
        match option.to_str() {
            Some("--trusted") if !trusted => trusted = true,
            Some("--release") if !options.release => options.release = true,
            Some("--bin") if options.binary.is_none() => {
                options.binary = Some(text_arg(&mut arguments, "--bin")?)
            }
            Some("--features") => parse_features(&mut arguments, &mut options.features)?,
            Some("--") if play => {
                game_args = Some(arguments.collect());
                break;
            }
            _ => return Err(unknown_option(&option)),
        }
    }
    if !trusted {
        return Err("build and play require --trusted to execute project code".into());
    }
    Ok(Command::Build {
        project,
        options,
        game_args,
    })
}

fn parse_delivery(
    project: PathBuf,
    operation: DeliveryOperation,
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Command, String> {
    let mut trusted = false;
    let mut build = BuildOptions::default();
    let (mut recipe, mut target, mut output_parent, mut name) = (None, None, None, None);
    let mut cooker_args = Vec::new();
    while let Some(option) = arguments.next() {
        match option.to_str() {
            Some("--trusted") if !trusted => trusted = true,
            Some("--release") if !build.release => build.release = true,
            Some("--bin") if build.binary.is_none() => {
                build.binary = Some(text_arg(&mut arguments, "--bin")?)
            }
            Some("--features") => parse_features(&mut arguments, &mut build.features)?,
            Some("--recipe") if recipe.is_none() => {
                recipe = Some(text_arg(&mut arguments, "--recipe")?)
            }
            Some("--target") if target.is_none() => {
                target = Some(text_arg(&mut arguments, "--target")?)
            }
            Some("--output-parent") if output_parent.is_none() => {
                output_parent = Some(path_arg(&mut arguments, "--output-parent")?)
            }
            Some("--name") if name.is_none() => name = Some(text_arg(&mut arguments, "--name")?),
            Some("--") => {
                cooker_args = arguments.collect();
                break;
            }
            _ => return Err(unknown_option(&option)),
        }
    }
    if !trusted {
        return Err("cook and export require --trusted to execute project code".into());
    }
    Ok(Command::Delivery {
        project,
        operation,
        options: DeliveryOptions {
            build,
            recipe: recipe.ok_or("cook and export require --recipe <name>")?,
            target: target.ok_or("cook and export require --target <triple>")?,
            output_parent: output_parent.ok_or("cook and export require --output-parent <dir>")?,
            name: name.ok_or("cook and export require --name <name>")?,
            cooker_args,
        },
    })
}

fn parse_features(
    arguments: &mut impl Iterator<Item = OsString>,
    features: &mut Vec<String>,
) -> Result<(), String> {
    let list = text_arg(arguments, "--features")?;
    for feature in list.split(',').map(str::trim) {
        if feature.is_empty() {
            return Err("--features requires a comma-separated list of feature names".into());
        }
        features.push(feature.to_owned());
    }
    Ok(())
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
    let path = arguments
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing {label}"))?;
    Ok(path.into())
}

fn text_arg(arguments: &mut impl Iterator<Item = OsString>, label: &str) -> Result<String, String> {
    arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| format!("missing or invalid value for {label}"))
}

fn unknown_option(option: &std::ffi::OsStr) -> String {
    format!("unexpected or repeated argument {option:?}")
}

#[cfg(test)]
mod tests {
    use super::{Command, DeliveryOperation, parse};

    #[test]
    fn only_explicit_trust_enables_execution() {
        for command in ["build", "play"] {
            assert!(parse([command, "."].map(Into::into)).is_err());
            assert!(parse([command, ".", "--trusted"].map(Into::into)).is_ok());
        }
        assert!(parse(["play", ".", "--", "--trusted"].map(Into::into)).is_err());
        for command in ["cook", "export"] {
            let arguments = [
                command,
                ".",
                "--recipe",
                "headless",
                "--target",
                "native-triple",
                "--output-parent",
                "output",
                "--name",
                "game",
            ];
            assert!(parse(arguments.map(Into::into)).is_err());
            assert!(parse(arguments.into_iter().chain(["--trusted"]).map(Into::into)).is_ok());
            assert!(
                parse(
                    arguments
                        .into_iter()
                        .chain(["--", "--trusted"])
                        .map(Into::into)
                )
                .is_err()
            );
        }
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
        let Command::Build {
            project,
            options,
            game_args: Some(game_args),
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

    #[test]
    fn export_keeps_cooker_arguments_separate_from_delivery_options() {
        let command = parse(
            [
                "export",
                "game folder",
                "--trusted",
                "--recipe",
                "desktop",
                "--target",
                "native-triple",
                "--output-parent",
                "output folder",
                "--name",
                "Player",
                "--features",
                "desktop,overlay",
                "--",
                "--flow",
                "round",
            ]
            .map(Into::into),
        )
        .unwrap();
        let Command::Delivery {
            project,
            operation: DeliveryOperation::Export,
            options,
        } = command
        else {
            panic!("expected an Export command");
        };
        assert_eq!(project.to_str(), Some("game folder"));
        assert_eq!(options.recipe, "desktop");
        assert_eq!(options.target, "native-triple");
        assert_eq!(options.output_parent.to_str(), Some("output folder"));
        assert_eq!(options.name, "Player");
        assert_eq!(options.build.features, ["desktop", "overlay"]);
        assert_eq!(options.cooker_args, ["--flow", "round"]);
    }
}
