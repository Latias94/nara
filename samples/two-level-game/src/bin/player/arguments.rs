use std::{ffi::OsString, path::PathBuf};

use two_level_game::{RunOptions, Scenario};

const USAGE: &str = "usage: player [--verify-travel | --verify-save ABSOLUTE_PATH | --verify-restore ABSOLUTE_PATH]";
const MAX_SAVE_PATH_BYTES: usize = 4096;

pub(super) fn parse(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<RunOptions, &'static str> {
    let mut arguments = arguments.into_iter();
    let Some(flag) = arguments.next() else {
        return Ok(RunOptions::default());
    };
    let scenario = if flag == "--verify-travel" {
        Scenario::Travel
    } else if flag == "--verify-save" {
        Scenario::Save
    } else if flag == "--verify-restore" {
        Scenario::Restore
    } else {
        return Err(USAGE);
    };
    let save_path = if scenario == Scenario::Travel {
        None
    } else {
        Some(parse_save_path(arguments.next().ok_or(USAGE)?)?)
    };
    if arguments.next().is_some() {
        return Err(USAGE);
    }
    Ok(RunOptions {
        scenario,
        save_path,
    })
}

pub(super) fn parse_save_path(value: OsString) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(value);
    let text = path.to_str().ok_or("Player save path must be UTF-8")?;
    if text.len() > MAX_SAVE_PATH_BYTES
        || text.contains('\0')
        || !path.is_absolute()
        || path.file_name().is_none()
    {
        return Err(
            "Player save path must be absolute with a file name and at most 4096 UTF-8 bytes; create its parent explicitly",
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_path() -> PathBuf {
        std::env::temp_dir().join("two-level-player-checkpoint.json")
    }

    #[test]
    fn accepts_only_the_four_finite_forms() {
        let interactive = parse([]).unwrap();
        assert_eq!(interactive.scenario, Scenario::Interactive);
        assert!(interactive.save_path.is_none());
        let travel = parse(["--verify-travel".into()]).unwrap();
        assert_eq!(travel.scenario, Scenario::Travel);
        assert!(travel.save_path.is_none());
        for (flag, scenario) in [
            ("--verify-save", Scenario::Save),
            ("--verify-restore", Scenario::Restore),
        ] {
            let path = absolute_path();
            let options = parse([flag.into(), path.clone().into_os_string()]).unwrap();
            assert_eq!(options.scenario, scenario);
            assert_eq!(options.save_path, Some(path));
        }
    }

    #[test]
    fn rejects_overrides_unknown_flags_and_extra_arguments() {
        for arguments in [
            vec!["--pack", "content.pack"],
            vec!["--catalog-hash", "1234"],
            vec!["--project", "."],
            vec!["--help"],
            vec!["--verify-save"],
            vec!["--verify-restore"],
            vec!["--verify-travel", "--verify-travel"],
            vec!["--verify-travel", "extra"],
        ] {
            assert!(parse(arguments.into_iter().map(OsString::from)).is_err());
        }
        assert!(
            parse([
                "--verify-save".into(),
                absolute_path().into_os_string(),
                "extra".into(),
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_relative_paths_roots_nuls_and_oversized_utf8() {
        let nul = format!("{}\0.json", absolute_path().display());
        let oversized = std::env::temp_dir().join(format!("{}.json", "界".repeat(1366)));
        let root = absolute_path().ancestors().last().unwrap().to_path_buf();
        for path in [
            PathBuf::from("checkpoint.json"),
            root,
            PathBuf::from(nul),
            oversized,
        ] {
            assert!(parse(["--verify-save".into(), path.into_os_string()]).is_err());
        }
    }

    #[test]
    fn bounds_utf8_bytes_without_rejecting_unicode_paths() {
        let prefix = std::env::temp_dir().join("two-level-player-path-boundary");
        let prefix_bytes = prefix.to_str().unwrap().len() + 1;
        let path = prefix.join(format!("界{}", "a".repeat(4096 - prefix_bytes - 3)));
        assert_eq!(path.to_str().unwrap().len(), 4096);
        assert!(parse(["--verify-save".into(), path.clone().into_os_string()]).is_ok());
        let oversized = OsString::from(format!("{}a", path.display()));
        assert!(parse(["--verify-save".into(), oversized]).is_err());
    }
}
