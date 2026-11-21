#![cfg(feature = "project-workflow-cli")]

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
fn top_level_help_and_version_keep_their_existing_behavior() {
    let root = TestRoot::new();
    let help = successful_stdout(root.run(&[]));
    assert!(help.starts_with("Nara project workflow\n\nUsage:\n"));
    for command in ["check", "schema", "new", "export", "repair startup-scene"] {
        assert!(help.contains(&format!("  nara {command} ")));
    }
    for flag in ["help", "-h", "--help"] {
        assert_eq!(successful_stdout(root.run(&[flag])), help);
    }
    for flag in ["-V", "--version"] {
        assert_eq!(
            successful_stdout(root.run(&[flag])),
            format!("nara {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    assert!(root.entries().is_empty());
}

#[test]
fn scoped_help_has_usable_syntax_and_examples_without_required_operands() {
    let root = TestRoot::new();
    for (prefix, syntax, example) in [
        (
            vec!["check"],
            "nara check <project-directory> [--profile <name>]",
            "nara check ./my-game",
        ),
        (
            vec!["schema"],
            "nara schema <project-directory> (--check | --write) [--profile <name>]",
            "nara schema ./my-game --write",
        ),
        (
            vec!["new"],
            "nara new desktop-2d/v1 <slug> --parent <directory> --local-checkout <directory>",
            "nara new desktop-2d/v1 my-game --parent .. --local-checkout .",
        ),
        (
            vec!["new", "desktop-2d/v1"],
            "nara new desktop-2d/v1 <slug> --parent <directory> --git <https-url> --rev <object-id>",
            "nara new desktop-2d/v1 my-game --parent .. --local-checkout .",
        ),
        (
            vec!["new", "desktop-2d/v2"],
            "nara new desktop-2d/v2 <slug> --parent <directory> --local-checkout <directory>",
            "nara new desktop-2d/v2 my-game --parent .. --local-checkout .",
        ),
        (
            vec!["export"],
            "nara export <project-directory> --parent <directory> --destination <name>",
            "nara export ./my-game --parent . --destination my-game-export",
        ),
        (
            vec!["repair"],
            "nara repair startup-scene <project-directory> [--profile <name>] --replacement <scene-path>",
            "nara repair startup-scene ./my-game --replacement startup.scene.json",
        ),
        (
            vec!["repair", "startup-scene"],
            "nara repair startup-scene <project-directory> [--profile <name>] --replacement <scene-path>",
            "nara repair startup-scene ./my-game --replacement startup.scene.json",
        ),
    ] {
        let mut previous = None;
        for flag in ["-h", "--help"] {
            let mut arguments = prefix.clone();
            arguments.push(flag);
            let help = successful_stdout(root.run(&arguments));
            assert!(help.contains("Usage:\n"), "{help}");
            assert!(help.contains(&format!("\n  {syntax}\n")), "{help}");
            let (_, examples) = help.split_once("Example (").unwrap();
            assert!(examples.contains(&format!("\n  {example}\n")), "{help}");
            assert!(!help.starts_with("Nara project workflow"), "{help}");
            if let Some(previous) = previous {
                assert_eq!(help, previous);
            }
            previous = Some(help);
        }
    }
    assert!(root.entries().is_empty());
}

#[test]
fn trailing_help_never_opens_missing_or_unreachable_paths() {
    let root = TestRoot::new();
    fs::write(root.path.join("not-a-directory"), b"unchanged").unwrap();

    for project in ["missing/项目", "not-a-directory/项目"] {
        for prefix in [
            vec!["check", project, "--profile", "desktop"],
            vec!["schema", project, "--write", "--profile", "desktop"],
            vec![
                "new",
                "desktop-2d/v1",
                "must-stay-absent",
                "--parent",
                project,
                "--local-checkout",
                project,
            ],
            vec![
                "new",
                "desktop-2d/v1",
                "must-stay-absent",
                "--parent",
                project,
                "--git",
                "https://example.invalid/nara.git",
                "--rev",
                "0123456789abcdef0123456789abcdef01234567",
            ],
            vec![
                "export",
                project,
                "--parent",
                project,
                "--destination",
                "must-stay-absent",
            ],
            vec![
                "repair",
                "startup-scene",
                project,
                "--replacement",
                "场景.scene.json",
                "--profile",
                "desktop",
            ],
        ] {
            for flag in ["-h", "--help"] {
                let mut arguments = prefix.clone();
                arguments.push(flag);
                let help = successful_stdout(root.run(&arguments));
                assert!(help.contains(&format!("Usage:\n  nara {} ", prefix[0])));
                assert_eq!(root.entries(), ["not-a-directory"]);
                assert_eq!(
                    fs::read(root.path.join("not-a-directory")).unwrap(),
                    b"unchanged"
                );
            }
        }
    }
}

#[test]
fn help_never_builds_existing_projects_or_creates_project_and_export_output() {
    let root = TestRoot::new();
    let project = root.path.join("project");
    let publication = root.path.join("publication");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&publication).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        b"invalid Cargo manifest; do not build",
    )
    .unwrap();
    fs::write(
        project.join("nara.toml"),
        b"invalid project; do not open or repair",
    )
    .unwrap();

    for prefix in [
        vec!["check", "project"],
        vec!["schema", "project", "--write"],
        vec![
            "new",
            "desktop-2d/v1",
            "must-stay-absent",
            "--parent",
            "publication",
            "--local-checkout",
            "project",
        ],
        vec![
            "export",
            "project",
            "--parent",
            "publication",
            "--destination",
            "must-stay-absent",
        ],
        vec![
            "repair",
            "startup-scene",
            "project",
            "--replacement",
            "startup.scene.json",
        ],
    ] {
        let mut arguments = prefix;
        arguments.push("--help");
        let help = successful_stdout(root.run(&arguments));
        assert!(help.contains(&format!("Usage:\n  nara {} ", arguments[0])));
        assert_eq!(fs::read_dir(&publication).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&project).unwrap().count(), 2);
        assert_eq!(root.entries(), ["project", "publication"]);
        assert_eq!(
            fs::read(project.join("Cargo.toml")).unwrap(),
            b"invalid Cargo manifest; do not build"
        );
        assert_eq!(
            fs::read(project.join("nara.toml")).unwrap(),
            b"invalid project; do not open or repair"
        );
    }
}

#[test]
fn malformed_help_requests_keep_argument_errors_and_have_no_side_effects() {
    let root = TestRoot::new();
    for (arguments, message) in [
        (vec!["unknown", "--help"], "unknown command"),
        (
            vec!["help", "check"],
            "command does not accept additional arguments",
        ),
        (
            vec!["--version", "--help"],
            "command does not accept additional arguments",
        ),
        (
            vec!["new", "--help", "desktop-2d/v1"],
            "command does not accept additional arguments",
        ),
        (
            vec!["check", "missing", "--help", "--profile", "desktop"],
            "command does not accept additional arguments",
        ),
        (
            vec!["check", "missing", "--profile", "--help"],
            "missing profile name",
        ),
        (
            vec!["schema", "missing", "--profile", "-h"],
            "missing profile name",
        ),
        (
            vec!["export", "missing", "--parent", "--help"],
            "missing parent directory",
        ),
        (
            vec!["new", "desktop-2d/v1", "game", "--git", "--help"],
            "missing Git repository",
        ),
        (
            vec![
                "repair",
                "startup-scene",
                "missing",
                "--replacement",
                "--help",
            ],
            "missing replacement Scene path",
        ),
        (
            vec!["check", "missing", "--unknown", "value", "--help"],
            "unknown check-command option",
        ),
        (
            vec![
                "check",
                "missing",
                "--profile",
                "a",
                "--profile",
                "b",
                "--help",
            ],
            "command option was provided more than once",
        ),
        (
            vec!["schema", "missing", "--check", "--write", "--help"],
            "command option was provided more than once",
        ),
        (
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
            "choose exactly one Nara dependency source",
        ),
    ] {
        let output = root.run(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}: {output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.starts_with(&format!("error: {message}\n\n")),
            "{stderr}"
        );
        assert!(root.entries().is_empty());
    }
}

fn successful_stdout(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

struct TestRoot {
    parent: PathBuf,
    path: PathBuf,
}

impl TestRoot {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = parent.join(format!(
            "nara-project-workflow-help-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self { parent, path }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nara"))
            .current_dir(&self.path)
            .args(arguments)
            .env("CARGO", self.path.join("unavailable-cargo"))
            .env("RUSTC", self.path.join("unavailable-rustc"))
            .env("PATH", "")
            .env("CARGO_TARGET_DIR", self.path.join("must-not-build"))
            .output()
            .unwrap()
    }

    fn entries(&self) -> Vec<String> {
        let mut entries = fs::read_dir(&self.path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        entries.sort_unstable();
        entries
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        if fs::canonicalize(&self.path)
            .is_ok_and(|path| path == self.path && path.parent() == Some(self.parent.as_path()))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
