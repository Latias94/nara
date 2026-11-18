use std::{ffi::OsString, iter::Peekable, path::PathBuf};

use thiserror::Error;

pub(crate) enum Command {
    Help,
    ScopedHelp(HelpTopic),
    Version,
    Check(CheckArguments),
    Schema(SchemaArguments),
    New(NewArguments),
    Export(ExportArguments),
    Repair(RepairArguments),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    Check,
    Schema,
    New,
    Export,
    Repair,
}

pub(crate) struct CheckArguments {
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
}

pub(crate) struct SchemaArguments {
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
    pub(crate) write: bool,
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

/// Parses the finite project commands without accessing project files or build tools.
///
/// Subcommand `-h` and `--help` must be terminal, at a positional or option boundary.
/// Help may omit remaining required arguments, but supplied options must have values and
/// cannot be duplicated or conflict. Options follow the command's positional arguments.
/// Values starting with `-` are rejected; use a path such as `./--help` for a literal name.
/// Bare `help` remains a top-level command and is otherwise an ordinary value.
pub(crate) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(Command::Help);
    };
    match command.to_str() {
        Some("--help" | "-h" | "help") => ensure_terminal(arguments, Command::Help),
        Some("--version" | "-V") => ensure_terminal(arguments, Command::Version),
        Some("check") => parse_check(arguments),
        Some("schema") => parse_schema(arguments),
        Some("new") => parse_new(arguments),
        Some("export") => parse_export(arguments),
        Some("repair") => parse_repair(arguments),
        _ => Err(error("unknown command")),
    }
}

fn parse_check(arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.peekable();
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::Check);
    }
    let project = PathBuf::from(required_value(
        arguments.next(),
        "missing project directory",
    )?);
    let mut profile = None;
    while let Some(flag) = arguments.next() {
        match flag.to_str() {
            Some("--help" | "-h") => return scoped_help(arguments, HelpTopic::Check),
            Some("--profile") => set_once(
                &mut profile,
                required_unicode(arguments.next(), "missing profile name")?,
            )?,
            _ => return Err(error("unknown check-command option")),
        }
    }
    Ok(Command::Check(CheckArguments { project, profile }))
}

fn parse_schema(arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.peekable();
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::Schema);
    }
    let project = PathBuf::from(required_value(
        arguments.next(),
        "missing project directory",
    )?);
    let mut profile = None;
    let mut write = None;
    while let Some(flag) = arguments.next() {
        match flag.to_str() {
            Some("--help" | "-h") => return scoped_help(arguments, HelpTopic::Schema),
            Some("--check") => set_once(&mut write, false)?,
            Some("--write") => set_once(&mut write, true)?,
            Some("--profile") => set_once(
                &mut profile,
                required_unicode(arguments.next(), "missing profile name")?,
            )?,
            _ => return Err(error("unknown schema-command option")),
        }
    }
    Ok(Command::Schema(SchemaArguments {
        project,
        profile,
        write: write.ok_or_else(|| error("choose exactly one of --check or --write"))?,
    }))
}

fn parse_repair(arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.peekable();
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::Repair);
    }
    let repair = required_unicode(arguments.next(), "missing repair kind")?;
    if repair != "startup-scene" {
        return Err(error("unsupported repair kind"));
    }
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::Repair);
    }
    let project = PathBuf::from(required_value(
        arguments.next(),
        "missing project directory",
    )?);
    let mut profile = None;
    let mut replacement = None;
    while let Some(flag) = arguments.next() {
        match flag.to_str() {
            Some("--help" | "-h") => return scoped_help(arguments, HelpTopic::Repair),
            Some("--profile") => set_once(
                &mut profile,
                required_unicode(arguments.next(), "missing profile name")?,
            )?,
            Some("--replacement") => set_once(
                &mut replacement,
                required_unicode(arguments.next(), "missing replacement Scene path")?,
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

fn parse_export(arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.peekable();
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::Export);
    }
    let project = PathBuf::from(required_value(
        arguments.next(),
        "missing project directory",
    )?);
    let mut parent = None;
    let mut destination = None;
    while let Some(flag) = arguments.next() {
        match flag.to_str() {
            Some("--help" | "-h") => return scoped_help(arguments, HelpTopic::Export),
            Some("--parent") => set_once(
                &mut parent,
                PathBuf::from(required_value(
                    arguments.next(),
                    "missing parent directory",
                )?),
            )?,
            Some("--destination") => set_once(
                &mut destination,
                required_value(arguments.next(), "missing destination name")?,
            )?,
            _ => return Err(error("unknown export-command option")),
        }
    }
    Ok(Command::Export(ExportArguments {
        project,
        parent: parent.ok_or_else(|| error("missing --parent directory"))?,
        destination: destination.ok_or_else(|| error("missing --destination name"))?,
    }))
}

fn parse_new(arguments: impl Iterator<Item = OsString>) -> Result<Command, ParseError> {
    let mut arguments = arguments.peekable();
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::New);
    }
    let starter = required_unicode(arguments.next(), "missing starter id")?;
    if starter != "desktop-2d/v1" {
        return Err(error("unsupported starter id"));
    }
    if take_help(&mut arguments) {
        return scoped_help(arguments, HelpTopic::New);
    }
    let slug = required_unicode(arguments.next(), "missing project slug")?;

    let mut parent = None;
    let mut local_checkout = None;
    let mut repository = None;
    let mut revision = None;
    while let Some(flag) = arguments.next() {
        match flag.to_str() {
            Some("--help" | "-h") => {
                if local_checkout.is_some() && (repository.is_some() || revision.is_some()) {
                    return Err(error("choose exactly one Nara dependency source"));
                }
                return scoped_help(arguments, HelpTopic::New);
            }
            Some("--parent") => set_once(
                &mut parent,
                PathBuf::from(required_value(
                    arguments.next(),
                    "missing parent directory",
                )?),
            )?,
            Some("--local-checkout") => {
                set_once(
                    &mut local_checkout,
                    PathBuf::from(required_value(
                        arguments.next(),
                        "missing checkout directory",
                    )?),
                )?;
            }
            Some("--git") => set_once(
                &mut repository,
                required_unicode(arguments.next(), "missing Git repository")?,
            )?,
            Some("--rev") => set_once(
                &mut revision,
                required_unicode(arguments.next(), "missing Git revision")?,
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

fn take_help(arguments: &mut Peekable<impl Iterator<Item = OsString>>) -> bool {
    arguments
        .next_if(|value| matches!(value.to_str(), Some("--help" | "-h")))
        .is_some()
}

fn scoped_help(
    arguments: impl Iterator<Item = OsString>,
    topic: HelpTopic,
) -> Result<Command, ParseError> {
    ensure_terminal(arguments, Command::ScopedHelp(topic))
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
    required_value(value, missing)?
        .into_string()
        .map_err(|_| error("command value must be Unicode"))
}

fn required_value(value: Option<OsString>, missing: &'static str) -> Result<OsString, ParseError> {
    let value = value.ok_or_else(|| error(missing))?;
    if value.as_encoded_bytes().starts_with(b"-") {
        Err(error(missing))
    } else {
        Ok(value)
    }
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
    fn top_level_help_and_version_remain_terminal() {
        assert!(matches!(parse([]).unwrap(), Command::Help));
        for flag in ["help", "--help", "-h"] {
            assert!(matches!(
                parse([OsString::from(flag)]).unwrap(),
                Command::Help
            ));
            assert!(parse([flag, "check"].map(OsString::from)).is_err());
        }
        for flag in ["--version", "-V"] {
            assert!(matches!(
                parse([OsString::from(flag)]).unwrap(),
                Command::Version
            ));
            assert!(parse([flag, "--help"].map(OsString::from)).is_err());
        }
        assert_eq!(
            parse(["unknown", "--help"].map(OsString::from))
                .err()
                .unwrap()
                .to_string(),
            "unknown command"
        );
    }

    #[test]
    fn scoped_help_can_replace_remaining_required_arguments() {
        for (prefix, topic) in [
            (vec!["check"], HelpTopic::Check),
            (vec!["schema"], HelpTopic::Schema),
            (vec!["new"], HelpTopic::New),
            (vec!["new", "desktop-2d/v1"], HelpTopic::New),
            (vec!["export"], HelpTopic::Export),
            (vec!["repair"], HelpTopic::Repair),
            (vec!["repair", "startup-scene"], HelpTopic::Repair),
            (
                vec!["check", "missing", "--profile", "desktop"],
                HelpTopic::Check,
            ),
            (vec!["schema", "missing"], HelpTopic::Schema),
            (vec!["schema", "missing", "--write"], HelpTopic::Schema),
            (vec!["new", "desktop-2d/v1", "game"], HelpTopic::New),
            (
                vec!["export", "missing", "--parent", "absent"],
                HelpTopic::Export,
            ),
            (
                vec!["repair", "startup-scene", "missing"],
                HelpTopic::Repair,
            ),
        ] {
            for flag in ["-h", "--help"] {
                let parsed =
                    parse(prefix.iter().copied().chain([flag]).map(OsString::from)).unwrap();
                assert!(matches!(parsed, Command::ScopedHelp(actual) if actual == topic));
            }
        }
    }

    #[test]
    fn help_does_not_hide_unknown_duplicate_or_conflicting_arguments() {
        for arguments in [
            vec!["check", "--help", "extra"],
            vec!["check", "project", "-h", "--help"],
            vec!["check", "project", "--unknown", "value", "--help"],
            vec!["check", "project", "extra", "--help"],
            vec![
                "check",
                "project",
                "--profile",
                "a",
                "--profile",
                "b",
                "--help",
            ],
            vec!["schema", "project", "--check", "--write", "--help"],
            vec!["schema", "project", "--write", "--write", "--help"],
            vec!["new", "unknown-starter", "--help"],
            vec![
                "new",
                "desktop-2d/v1",
                "game",
                "--parent",
                ".",
                "--parent",
                ".",
                "--help",
            ],
            vec![
                "new",
                "desktop-2d/v1",
                "game",
                "--local-checkout",
                ".",
                "--git",
                "url",
                "--help",
            ],
            vec![
                "new",
                "desktop-2d/v1",
                "game",
                "--rev",
                "revision",
                "--local-checkout",
                ".",
                "--help",
            ],
            vec![
                "export",
                "project",
                "--destination",
                "a",
                "--destination",
                "b",
                "--help",
            ],
            vec!["repair", "unknown-kind", "--help"],
            vec![
                "repair",
                "startup-scene",
                "project",
                "--replacement",
                "a",
                "--replacement",
                "b",
                "--help",
            ],
        ] {
            assert!(
                parse(arguments.iter().copied().map(OsString::from)).is_err(),
                "accepted malformed arguments: {arguments:?}"
            );
        }
    }

    #[test]
    fn help_tokens_cannot_supply_missing_option_values() {
        for prefix in [
            vec!["check", "project", "--profile"],
            vec!["schema", "project", "--profile"],
            vec!["new", "desktop-2d/v1", "game", "--parent"],
            vec!["new", "desktop-2d/v1", "game", "--local-checkout"],
            vec!["new", "desktop-2d/v1", "game", "--git"],
            vec!["new", "desktop-2d/v1", "game", "--rev"],
            vec!["export", "project", "--parent"],
            vec!["export", "project", "--destination"],
            vec!["repair", "startup-scene", "project", "--profile"],
            vec!["repair", "startup-scene", "project", "--replacement"],
        ] {
            for flag in ["-h", "--help", "--unknown"] {
                let arguments = prefix.iter().copied().chain([flag]).map(OsString::from);
                assert!(parse(arguments).is_err(), "accepted {prefix:?} {flag}");
            }
        }
    }

    #[test]
    fn ordinary_help_text_and_unicode_values_are_preserved() {
        let Command::Check(check) =
            parse(["check", "./--help", "--profile", "帮助"].map(OsString::from)).unwrap()
        else {
            panic!("a literal path must select Check");
        };
        assert_eq!(check.project, PathBuf::from("./--help"));
        assert_eq!(check.profile.as_deref(), Some("帮助"));

        let Command::Export(export) = parse(
            [
                "export",
                "项目/游戏",
                "--parent",
                "help",
                "--destination",
                "发行",
            ]
            .map(OsString::from),
        )
        .unwrap() else {
            panic!("Unicode paths must select Export");
        };
        assert_eq!(export.project, PathBuf::from("项目/游戏"));
        assert_eq!(export.parent, PathBuf::from("help"));
        assert_eq!(export.destination, OsString::from("发行"));

        let Command::New(new) = parse(
            [
                "new",
                "desktop-2d/v1",
                "game",
                "--parent",
                "项目",
                "--git",
                "help",
                "--rev",
                "帮助",
            ]
            .map(OsString::from),
        )
        .unwrap() else {
            panic!("ordinary option values must select New");
        };
        assert_eq!(new.parent, PathBuf::from("项目"));
        assert!(matches!(
            new.dependency,
            DependencyArguments::ImmutableGit { repository, revision }
                if repository == "help" && revision == "帮助"
        ));
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn native_paths_remain_lossless_but_text_arguments_require_unicode() {
        #[cfg(unix)]
        let native = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'p', 0xff])
        };
        #[cfg(windows)]
        let native = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[u16::from(b'p'), 0xd800])
        };
        assert!(native.to_str().is_none());
        let Command::Check(check) = parse([OsString::from("check"), native.clone()]).unwrap()
        else {
            panic!("a native path must select Check");
        };
        assert_eq!(check.project.as_os_str(), native.as_os_str());

        let Command::Export(export) = parse([
            OsString::from("export"),
            native.clone(),
            OsString::from("--parent"),
            native.clone(),
            OsString::from("--destination"),
            native.clone(),
        ])
        .unwrap() else {
            panic!("native paths must select Export");
        };
        assert_eq!(export.project.as_os_str(), native.as_os_str());
        assert_eq!(export.parent.as_os_str(), native.as_os_str());
        assert_eq!(export.destination, native);

        for prefix in [
            vec!["check", "project", "--profile"],
            vec!["schema", "project", "--profile"],
            vec!["new", "desktop-2d/v1"],
            vec!["new", "desktop-2d/v1", "game", "--git"],
            vec!["new", "desktop-2d/v1", "game", "--rev"],
            vec!["repair", "startup-scene", "project", "--replacement"],
        ] {
            let arguments = prefix
                .into_iter()
                .map(OsString::from)
                .chain([native.clone(), OsString::from("--help")]);
            assert_eq!(
                parse(arguments).err().unwrap().to_string(),
                "command value must be Unicode"
            );
        }
    }

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

    #[test]
    fn schema_requires_one_explicit_mode() {
        for arguments in [
            vec!["schema", "project"],
            vec!["schema", "project", "--check", "--write"],
            vec!["schema", "project", "--write", "--write"],
            vec!["schema", "project", "--check", "--profile"],
        ] {
            assert!(parse(arguments.into_iter().map(OsString::from)).is_err());
        }
        let Command::Schema(arguments) =
            parse(["schema", "project", "--write", "--profile", "desktop"].map(OsString::from))
                .unwrap()
        else {
            panic!("schema selects its own command");
        };
        assert!(arguments.write);
        assert_eq!(arguments.profile.as_deref(), Some("desktop"));
    }
}
