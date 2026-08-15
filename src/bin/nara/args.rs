use std::{ffi::OsString, path::PathBuf};

use thiserror::Error;

pub(crate) enum Command {
    Help,
    Version,
    Check(CheckArguments),
    New(NewArguments),
    Export(ExportArguments),
    Repair(RepairArguments),
}

pub(crate) struct CheckArguments {
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
}

pub(crate) struct NewArguments {
    pub(crate) slug: String,
    pub(crate) parent: PathBuf,
    pub(crate) dependency: DependencyArguments,
}

pub(crate) struct ExportArguments {
    pub(crate) project: PathBuf,
    pub(crate) parent: PathBuf,
    pub(crate) destination: OsString,
}

pub(crate) struct RepairArguments {
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
    pub(crate) replacement: String,
}

pub(crate) enum DependencyArguments {
    LocalCheckout {
        checkout: PathBuf,
    },
    ImmutableGit {
        repository: String,
        revision: String,
    },
}

#[derive(Debug, Error)]
#[error("{message}")]
pub(crate) struct ParseError {
    message: &'static str,
}

pub(crate) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(Command::Help);
    };
    match command.to_str() {
        Some("--help" | "-h" | "help") => ensure_terminal(arguments, Command::Help),
        Some("--version" | "-V") => ensure_terminal(arguments, Command::Version),
        Some("check") => parse_check(arguments),
        Some("new") => parse_new(arguments),
        Some("export") => parse_export(arguments),
        Some("repair") => parse_repair(arguments),
        _ => Err(error("unknown command")),
    }
}

fn parse_check(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let project = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| error("missing project directory"))?;
    let mut profile = None;
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| error("command option is missing its value"))?;
        match flag.to_str() {
            Some("--profile") => set_once(
                &mut profile,
                required_unicode(Some(value), "profile must be Unicode")?,
            )?,
            _ => return Err(error("unknown check-command option")),
        }
    }
    Ok(Command::Check(CheckArguments { project, profile }))
}

fn parse_repair(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let repair = required_unicode(arguments.next(), "missing repair kind")?;
    if repair != "startup-scene" {
        return Err(error("unsupported repair kind"));
    }
    let project = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| error("missing project directory"))?;
    let mut profile = None;
    let mut replacement = None;
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| error("command option is missing its value"))?;
        match flag.to_str() {
            Some("--profile") => set_once(
                &mut profile,
                required_unicode(Some(value), "profile must be Unicode")?,
            )?,
            Some("--replacement") => set_once(
                &mut replacement,
                required_unicode(Some(value), "replacement Scene path must be Unicode")?,
            )?,
            _ => return Err(error("unknown repair-command option")),
        }
    }
    Ok(Command::Repair(RepairArguments {
        project,
        profile,
        replacement: replacement.ok_or_else(|| error("missing --replacement Scene path"))?,
    }))
}

fn parse_export(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let project = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| error("missing project directory"))?;
    let mut parent = None;
    let mut destination = None;
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| error("command option is missing its value"))?;
        match flag.to_str() {
            Some("--parent") => set_once(&mut parent, PathBuf::from(value))?,
            Some("--destination") => set_once(&mut destination, value)?,
            _ => return Err(error("unknown export-command option")),
        }
    }
    Ok(Command::Export(ExportArguments {
        project,
        parent: parent.ok_or_else(|| error("missing --parent directory"))?,
        destination: destination.ok_or_else(|| error("missing --destination name"))?,
    }))
}

fn parse_new(mut arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let starter = required_unicode(arguments.next(), "missing starter id")?;
    if starter != "desktop-2d/v1" {
        return Err(error("unsupported starter id"));
    }
    let slug = required_unicode(arguments.next(), "missing project slug")?;

    let mut parent = None;
    let mut local_checkout = None;
    let mut repository = None;
    let mut revision = None;
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| error("command option is missing its value"))?;
        match flag.to_str() {
            Some("--parent") => set_once(&mut parent, PathBuf::from(value))?,
            Some("--local-checkout") => {
                set_once(&mut local_checkout, PathBuf::from(value))?;
            }
            Some("--git") => set_once(
                &mut repository,
                required_unicode(Some(value), "Git repository must be Unicode")?,
            )?,
            Some("--rev") => set_once(
                &mut revision,
                required_unicode(Some(value), "Git revision must be Unicode")?,
            )?,
            _ => return Err(error("unknown new-command option")),
        }
    }

    let parent = parent.ok_or_else(|| error("missing --parent directory"))?;
    let dependency = match (local_checkout, repository, revision) {
        (Some(checkout), None, None) => DependencyArguments::LocalCheckout { checkout },
        (None, Some(repository), Some(revision)) => DependencyArguments::ImmutableGit {
            repository,
            revision,
        },
        _ => return Err(error("choose exactly one Nara dependency source")),
    };
    Ok(Command::New(NewArguments {
        slug,
        parent,
        dependency,
    }))
}

fn ensure_terminal(
    mut arguments: impl Iterator<Item = OsString>,
    command: Command,
) -> Result<Command, ParseError> {
    if arguments.next().is_some() {
        Err(error("command does not accept additional arguments"))
    } else {
        Ok(command)
    }
}

fn required_unicode(value: Option<OsString>, missing: &'static str) -> Result<String, ParseError> {
    value
        .ok_or_else(|| error(missing))?
        .into_string()
        .map_err(|_| error("command value must be Unicode"))
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), ParseError> {
    if slot.replace(value).is_some() {
        Err(error("command option was provided more than once"))
    } else {
        Ok(())
    }
}

const fn error(message: &'static str) -> ParseError {
    ParseError { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_git_requires_both_repository_and_revision() {
        let error = parse(
            [
                "new",
                "desktop-2d/v1",
                "game",
                "--parent",
                "projects",
                "--git",
                "https://example.invalid/nara.git",
            ]
            .map(OsString::from),
        )
        .err()
        .unwrap();

        assert_eq!(
            error.to_string(),
            "choose exactly one Nara dependency source"
        );
    }

    #[test]
    fn duplicate_options_are_rejected() {
        let error = parse(
            [
                "new",
                "desktop-2d/v1",
                "game",
                "--parent",
                "projects",
                "--parent",
                "other",
                "--local-checkout",
                ".",
            ]
            .map(OsString::from),
        )
        .err()
        .unwrap();

        assert_eq!(
            error.to_string(),
            "command option was provided more than once"
        );
    }
}
