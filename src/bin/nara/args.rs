use std::{ffi::OsString, path::PathBuf};

use thiserror::Error;

pub(crate) enum Command {
    Help,
    Version,
    New(NewArguments),
}

pub(crate) struct NewArguments {
    pub(crate) slug: String,
    pub(crate) parent: PathBuf,
    pub(crate) dependency: DependencyArguments,
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
        Some("new") => parse_new(arguments),
        _ => Err(error("unknown command")),
    }
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
