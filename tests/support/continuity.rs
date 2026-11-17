//! The Two-Level Game's finite edit/build/process qualification Host.
//!
//! This owns its copied project, captured executable installations and shared process owners.
//! It deliberately uses the public Cargo adapter and the game's ordinary process protocol.
//! Diagnostics retain a bounded in-memory tail, separately from file-based game packets.

use std::{
    cell::RefCell,
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::{
        ContentDigest, DirectoryCapability, FileCapability, RelativeComponent, RelativePath,
        RestrictiveCapabilityHandleLease,
    },
    project_tooling::{
        cargo_build::{
            CapturedCargoExecutable, CargoBuildFailure, CargoBuildLimits, CargoBuildRequest,
            CargoBuildTools, build_executable,
        },
        host_fs,
        process::{
            ExactExecutableLaunchAuthority, ProcessLogLimits, ProcessOutputStream,
            SupervisedProcessCommand, SupervisedProcessOwner,
            SupervisedProcessState as ProcessState, SupervisedProductEnvironmentPolicy,
            SupervisedProductProcessCommand,
        },
    },
    project_workflow::cargo::{CargoBuildError, CargoBuildSelection},
};
use two_level_game::resume::{ResumeState, encode_resume};

use crate::protocol::{
    Command, MAX_MESSAGE_BYTES, Observation, PROTOCOL_VERSION, Reply, Request, Response,
    SemanticCommand,
};

const TEMP_PREFIX: &str = "nara-two-level-continuity-";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const START_TIMEOUT: Duration = Duration::from_secs(60);
const RETIRE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_LOG_RECORDS: usize = 256;
const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOOL_STDOUT: usize = 16 * 1024;
const POLL: Duration = Duration::from_millis(10);
pub const GAMEPLAY: &str = "src/gameplay/mod.rs";
pub const COMPILE_ERROR: &str = "intentional R12 Two-Level Game candidate failure";

/// One exclusive temporary tree, never an existing checkout or a Cargo output directory.
pub struct Fixture {
    parent: PathBuf,
    pub root: PathBuf,
    pub project: PathBuf,
    pub repository: PathBuf,
    target: PathBuf,
    tools: CargoBuildTools,
    host: String,
    inputs: Vec<String>,
    pending_cleanup: RefCell<Vec<SupervisedProcessOwner>>,
}

impl Fixture {
    pub fn new() -> Self {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .unwrap();
        assert_eq!(sample.file_name().unwrap(), "two-level-game");
        let repository = sample.join("../..").canonicalize().unwrap();
        assert!(repository.join("crates/nara_app/Cargo.toml").is_file());
        let toolchain = std::env::var("RUSTUP_TOOLCHAIN")
            .expect("run qualification with RUSTUP_TOOLCHAIN=1.95.0");
        assert!(
            toolchain == "1.95.0" || toolchain.starts_with("1.95.0-"),
            "qualification requires the installed Rust 1.95.0 baseline: {toolchain}",
        );
        assert_eq!(
            std::env::var("CARGO_NET_OFFLINE").as_deref(),
            Ok("true"),
            "nested builds must use the existing lockfile and cached dependencies",
        );
        let target = PathBuf::from(
            std::env::var_os("CARGO_TARGET_DIR")
                .expect("set the absolute root CARGO_TARGET_DIR and run nested Cargo serially"),
        );
        assert!(target.is_absolute());
        let target = target.canonicalize().unwrap();
        assert_eq!(target, repository.join("target").canonicalize().unwrap());

        let parent = std::env::temp_dir().canonicalize().unwrap();
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!(
            "{TEMP_PREFIX}{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let tools = CargoBuildTools::discover_in(&repository).unwrap();
        let mut fixture = Self {
            parent,
            root,
            project,
            repository,
            target,
            tools,
            host: String::new(),
            inputs: Vec::new(),
            pending_cleanup: RefCell::new(Vec::new()),
        };
        fixture.host = fixture.probe_host();
        for directory in ["src", "assets", "scenes", "prefabs", "schema"] {
            copy_tree(&sample.join(directory), &fixture.project.join(directory));
        }
        for name in ["Cargo.toml", "Cargo.lock", "nara.toml", "build.rs"] {
            fs::copy(sample.join(name), fixture.project.join(name)).unwrap();
        }
        let manifest = fixture.read("Cargo.toml");
        let original = "path = \"../..\"";
        assert_eq!(
            manifest.matches(original).count(),
            1,
            "the one nara path dependency"
        );
        let absolute = serde_json::to_string(fixture.repository.to_str().unwrap()).unwrap();
        let mut manifest = manifest.replacen(original, &format!("path = {absolute}"), 1);
        let original = "path = \"../../crates/nara_rapier2d\"";
        assert_eq!(
            manifest.matches(original).count(),
            1,
            "the direct nara_rapier2d path dependency"
        );
        let path = fixture
            .repository
            .join("crates/nara_rapier2d")
            .canonicalize()
            .unwrap();
        assert!(path.join("Cargo.toml").is_file());
        let absolute = serde_json::to_string(path.to_str().unwrap()).unwrap();
        manifest = manifest.replacen(original, &format!("path = {absolute}"), 1);
        for dependency in ["winit", "notify"] {
            let original = format!("path = \"../../vendor/{dependency}\"");
            assert_eq!(
                manifest.matches(&original).count(),
                1,
                "the selected {dependency} source"
            );
            let path = fixture.repository.join("vendor").join(dependency);
            assert!(path.join("Cargo.toml").is_file());
            let absolute = serde_json::to_string(path.to_str().unwrap()).unwrap();
            manifest = manifest.replacen(&original, &format!("path = {absolute}"), 1);
        }
        fixture.write("Cargo.toml", &manifest);
        assert_eq!(
            fs::read(fixture.project.join("Cargo.lock")).unwrap(),
            fs::read(sample.join("Cargo.lock")).unwrap(),
            "preparation must copy the existing lockfile without resolving dependencies",
        );
        fixture.inputs = inventory(&fixture.project);
        assert!(
            fixture
                .inputs
                .iter()
                .any(|path| path == "src/bin/continuity.rs")
        );
        assert!(
            fixture
                .inputs
                .iter()
                .any(|path| path == "src/development/protocol.rs")
        );
        fixture
    }

    fn probe_host(&self) -> String {
        let command = SupervisedProcessCommand::new(self.tools.rustc_path(), &self.repository)
            .arguments(["-vV"])
            .environment("RUSTUP_AUTO_INSTALL", "0")
            .bounded_stdout(NonZeroUsize::new(MAX_TOOL_STDOUT).unwrap());
        let owner = SupervisedProcessOwner::spawn(&command, log_limits()).unwrap();
        let mut native = FiniteProcess::new(self, owner);
        let bytes = match native.capture_stdout(REQUEST_TIMEOUT, MAX_TOOL_STDOUT) {
            Ok(bytes) => bytes,
            Err(error) => panic!("{}", native.fail(error)),
        };
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.lines().any(|line| line == "release: 1.95.0"), "{text}");
        text.lines()
            .find_map(|line| line.strip_prefix("host: "))
            .expect("the actual pinned compiler reports its explicit host triple")
            .to_owned()
    }

    pub fn directory(&self, name: &str) -> PathBuf {
        assert!(RelativeComponent::new(name).is_ok());
        let path = self.root.join(name);
        fs::create_dir(&path).unwrap();
        let path = path.canonicalize().unwrap();
        assert_eq!(path.parent(), Some(self.root.as_path()));
        path
    }

    pub fn read(&self, relative: &str) -> String {
        assert!(RelativePath::new(relative).is_ok());
        fs::read_to_string(self.project.join(relative)).unwrap()
    }

    pub fn write(&self, relative: &str, source: &str) {
        assert!(RelativePath::new(relative).is_ok());
        let path = self.project.join(relative);
        assert!(path.canonicalize().unwrap().starts_with(&self.project));
        assert!(
            !fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::write(path, source).unwrap();
    }

    pub fn request(&self) -> CargoBuildRequest {
        self.request_features(&["development"])
    }

    pub fn desktop_request(&self) -> CargoBuildRequest {
        self.request_features(&["desktop", "development"])
    }

    pub fn audio_desktop_request(&self) -> CargoBuildRequest {
        self.request_features(&["audio-cpal", "desktop", "development"])
    }

    pub fn assert_player_feature_closure(&self, desktop: bool) {
        let features = if desktop {
            "cooked-player,desktop"
        } else {
            "cooked-player"
        };
        let limit = 256 * 1024;
        let command = SupervisedProcessCommand::new(self.tools.cargo_path(), &self.project)
            .arguments([
                "tree",
                "--locked",
                "--offline",
                "--target",
                &self.host,
                "--no-default-features",
                "--features",
                features,
                "--edges",
                "normal,build",
                "--prefix",
                "none",
                "--format",
                "{p}|{f}",
            ])
            .environment("RUSTUP_AUTO_INSTALL", "0")
            .bounded_stdout(NonZeroUsize::new(limit).unwrap());
        let owner = SupervisedProcessOwner::spawn(&command, log_limits()).unwrap();
        let mut native = FiniteProcess::new(self, owner);
        let bytes = match native.capture_stdout(REQUEST_TIMEOUT, limit) {
            Ok(bytes) => bytes,
            Err(error) => panic!("{}", native.fail(error)),
        };
        let graph = String::from_utf8(bytes).unwrap();
        let mut game = false;
        let mut runtime = false;
        let mut renderer = false;
        for line in graph.lines() {
            let (package, features) = line.split_once('|').expect("Cargo's selected tree format");
            let name = package.split_whitespace().next().unwrap();
            let features = features
                .trim_end_matches(" (*)")
                .split(',')
                .collect::<Vec<_>>();
            assert!(
                !matches!(
                    name,
                    "notify" | "nara_asset_watch" | "nara_tooling" | "nara-editor" | "cpal"
                ) && !name.starts_with("nara_ui")
                    && !name.starts_with("dear-imgui"),
                "shipping runtime inherited development or UI package: {line}",
            );
            match name {
                "two-level-game" => {
                    game = true;
                    let expected: &[&str] = if desktop {
                        &["cooked-player", "desktop"]
                    } else {
                        &["cooked-player"]
                    };
                    assert_eq!(features, expected, "{line}");
                }
                "nara" => {
                    runtime = true;
                    for forbidden in [
                        "default",
                        "project-tooling",
                        "project-workflow",
                        "tooling",
                        "asset-watch",
                        "runtime-ui",
                        "presentation-ui",
                    ] {
                        assert!(!features.contains(&forbidden), "{line}");
                    }
                }
                "nara_render_wgpu" | "nara_winit" | "wgpu" | "winit" => {
                    renderer = true;
                    assert!(
                        desktop,
                        "headless shipping runtime inherited presentation: {line}"
                    );
                }
                _ => {}
            }
        }
        assert!(
            game && runtime,
            "Cargo must report the actual game and Nara nodes"
        );
        assert_eq!(renderer, desktop);
        println!(
            "TWO_LEVEL_PLAYER_FEATURE_CLOSURE desktop={desktop} development=false tooling=false watcher=false ui=false",
        );
    }

    fn request_features(&self, features: &[&str]) -> CargoBuildRequest {
        let selection = CargoBuildSelection::new(
            "two-level-game",
            "continuity",
            "dev",
            &self.host,
            features.iter().copied(),
            false,
        )
        .unwrap();
        CargoBuildRequest::new(
            &self.project,
            RelativePath::new("Cargo.toml").unwrap(),
            selection,
            self.tools.clone(),
            self.inputs
                .iter()
                .map(|path| RelativePath::new(path).unwrap()),
            CargoBuildLimits::default(),
        )
        .unwrap()
        .with_target_directory(&self.target)
        .unwrap()
    }

    pub fn build(&self) -> CapturedCargoExecutable {
        let captured = build_executable(&self.request(), |_| Ok(()))
            .expect("build the actual selected Two-Level Game continuity executable");
        let provenance = captured.provenance();
        assert_eq!(provenance.selection(), self.request().selection());
        assert_eq!(provenance.toolchain().host_target(), self.host);
        assert_eq!(captured.digest(), ContentDigest::of_bytes(captured.bytes()));
        assert_eq!(
            provenance
                .declared_inputs()
                .iter()
                .map(|input| input.path())
                .collect::<Vec<_>>(),
            self.inputs.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        for input in provenance.declared_inputs() {
            assert_eq!(
                input.digest(),
                ContentDigest::of_bytes(&fs::read(self.project.join(input.path())).unwrap()),
                "{}",
                input.path()
            );
        }
        captured
    }

    pub fn fail_compilation(&self) {
        let error = build_executable(&self.request(), |_| Ok(())).unwrap_err();
        assert!(
            matches!(&error, CargoBuildFailure::Compilation { status, diagnostics, protocol }
                if !status.success() && matches!(protocol, Some(CargoBuildError::BuildFailed))
                    && diagnostics.entries().iter().any(|entry| entry.message() == COMPILE_ERROR)),
            "the intentional Rust error must preserve structured compiler diagnostics: {error:?}",
        );
    }

    pub fn edit_body(&self) {
        let source = self.read(GAMEPLAY);
        let old = "direction * 160.0 * delta";
        assert_eq!(
            source.matches(old).count(),
            1,
            "the real unmodified movement law"
        );
        assert!(!source.contains("direction * 80.0 * delta"));
        self.write(
            GAMEPLAY,
            &source.replacen(old, "direction * 80.0 * delta", 1),
        );
    }

    /// Change only the known private Session representation and its textual field accesses.
    /// No Rust parser, inferred receiver graph or public DTO rewrite belongs in this fixture.
    pub fn edit_structure(&self) -> BTreeSet<String> {
        let dto = self.read("src/resume/mod.rs");
        let progress = self.read("src/save/dto.rs");
        let source = self.read(GAMEPLAY);
        let snapshot = struct_text(&source, "pub struct GameSnapshot {").to_owned();
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let session = struct_text(&source, "pub(super) struct Session {");
        let field = "    held: [bool; 4],";
        assert_eq!(
            session.matches(field).count(),
            1,
            "the private baseline Session field"
        );
        let old = format!("#[derive(Resource, Default)]{newline}{session}");
        assert_eq!(
            source.matches(&old).count(),
            1,
            "the private Session declaration"
        );
        assert!(!source.contains("struct MovementIntent"));
        let rewritten = session.replacen(field, "    movement: MovementIntent,", 1);
        let replacement = format!(
            "#[derive(Default)]{newline}struct MovementIntent {{{newline}    held: [bool; 4],{newline}}}{newline}{newline}\
             #[derive(Resource, Default)]{newline}{rewritten}",
        );
        let mut changed = BTreeSet::new();
        let mut accesses = 0;
        for relative in self
            .inputs
            .iter()
            .filter(|path| path.starts_with("src/") && path.ends_with(".rs"))
        {
            let before = self.read(relative);
            let mut after = if relative == GAMEPLAY {
                before.replacen(&old, &replacement, 1)
            } else {
                before.clone()
            };
            for receiver in [
                "session",
                ".resource::<Session>()",
                ".resource_mut::<Session>()",
            ] {
                let (rewritten, count) = group_held_access(&after, receiver);
                after = rewritten;
                accesses += count;
                assert_eq!(
                    group_held_access(&after, receiver).1,
                    0,
                    "unconverted {receiver}.held in {relative}"
                );
            }
            if before != after {
                self.write(relative, &after);
                changed.insert(relative.clone());
            }
        }
        assert!(
            accesses >= 8,
            "the actual Session capture, restore, movement and travel accesses must change"
        );
        for required in [
            GAMEPLAY,
            "src/gameplay/travel.rs",
            "src/gameplay/resume.rs",
            "src/gameplay/resume/capture.rs",
        ] {
            assert!(
                changed.contains(required),
                "the real private Session consumer must be rewritten: {required}"
            );
        }
        assert_eq!(
            self.read("src/resume/mod.rs"),
            dto,
            "the public resume DTO must stay byte-identical"
        );
        assert_eq!(
            self.read("src/save/dto.rs"),
            progress,
            "progress DTOs must stay byte-identical"
        );
        assert_eq!(
            struct_text(&self.read(GAMEPLAY), "pub struct GameSnapshot {"),
            snapshot
        );
        let after = self.read(GAMEPLAY);
        for law in ["direction * 160.0 * delta", "direction * 80.0 * delta"] {
            assert_eq!(
                after.matches(law).count(),
                source.matches(law).count(),
                "private structural editing must preserve the movement implementation"
            );
        }
        changed
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.pending_cleanup.get_mut().retain_mut(|owner| {
            if let Err(error) = cleanup_process(owner) {
                eprintln!(
                    "continuity outer cleanup still owns native process {}: {error}; {}",
                    owner.process_id(),
                    diagnostics(owner),
                );
                true
            } else {
                false
            }
        });
        if !self.pending_cleanup.get_mut().is_empty() {
            let pending = std::mem::take(self.pending_cleanup.get_mut());
            let count = pending.len();
            std::mem::forget(pending);
            eprintln!(
                "continuity retains {count} unresolved native owners and their tree at {} until test-process exit",
                self.root.display(),
            );
            if !std::thread::panicking() {
                panic!("continuity native cleanup remains incomplete after outer retry");
            }
            return;
        }
        let Ok(resolved) = self.root.canonicalize() else {
            return;
        };
        if resolved != self.root
            || resolved.parent() != Some(self.parent.as_path())
            || !resolved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(TEMP_PREFIX)
        {
            eprintln!(
                "refusing to remove an unverified continuity tree: {}",
                self.root.display()
            );
            return;
        }
        if let Err(error) = fs::remove_dir_all(&resolved) {
            eprintln!(
                "continuity temporary cleanup failed at {}: {error}",
                resolved.display()
            );
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    assert!(
        !fs::symlink_metadata(source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(
            !kind.is_symlink(),
            "copy only ordinary sample source/content"
        );
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()));
        } else {
            assert!(kind.is_file());
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn inventory(root: &Path) -> Vec<String> {
    fn visit(root: &Path, directory: &Path, paths: &mut Vec<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink());
            if kind.is_dir() {
                visit(root, &entry.path(), paths);
            } else {
                assert!(kind.is_file());
                paths.push(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut paths = Vec::new();
    visit(root, root, &mut paths);
    paths.sort();
    paths
}

fn struct_text<'a>(source: &'a str, start: &str) -> &'a str {
    let begin = source
        .find(start)
        .expect("the public GameSnapshot declaration");
    let end = source[begin..]
        .find("\n}")
        .expect("the public DTO's closing brace");
    &source[begin..begin + end + 2]
}

fn group_held_access(source: &str, receiver: &str) -> (String, usize) {
    let mut result = String::new();
    let mut consumed = 0;
    let mut count = 0;
    for (begin, _) in source.match_indices(receiver) {
        if receiver == "session"
            && source[..begin]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            continue;
        }
        let end = begin + receiver.len();
        let whitespace =
            source[end..].len() - source[end..].trim_start_matches(char::is_whitespace).len();
        let field = end + whitespace;
        if !source[field..].starts_with(".held")
            || source[field + 5..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            continue;
        }
        assert_eq!(&source[field..field + 5], ".held");
        result.push_str(&source[consumed..field]);
        result.push_str(".movement.held");
        consumed = field + 5;
        count += 1;
    }
    result.push_str(&source[consumed..]);
    (result, count)
}

/// A unique installation independent of mutable Cargo output, with no surviving writable handle.
pub struct Executable {
    path: PathBuf,
    directory: DirectoryCapability,
    file: FileCapability,
    _read_lease: RestrictiveCapabilityHandleLease,
    _directory_lease: RestrictiveCapabilityHandleLease,
}

impl Executable {
    pub fn install(fixture: &Fixture, label: &str, captured: &CapturedCargoExecutable) -> Self {
        let root = fixture.directory(label);
        let directory = host_fs::writable_directory(&root).unwrap();
        let name =
            RelativeComponent::new(format!("continuity{}", std::env::consts::EXE_SUFFIX)).unwrap();
        let path = root.join(name.to_str().unwrap());
        let mut output = directory.create_child_executable_file(&name).unwrap();
        output
            .writer()
            .unwrap()
            .write_all(captured.bytes())
            .unwrap();
        output.sync().unwrap();
        drop(output);
        drop(directory);
        let directory = host_fs::readable_directory(&root).unwrap();
        let file = directory.open_child_file(&name).unwrap();
        let read_lease = file.duplicate_restrictive_host_handle().unwrap();
        let directory_lease = directory.duplicate_restrictive_host_handle().unwrap();
        #[cfg(windows)]
        assert!(
            fs::OpenOptions::new().write(true).open(&path).is_err(),
            "the retained immutable executable lease must refuse a new writer",
        );
        let installed = Self {
            path,
            directory,
            file,
            _read_lease: read_lease,
            _directory_lease: directory_lease,
        };
        installed.assert_exact(captured);
        installed
    }

    fn launch_authority(&self) -> Result<ExactExecutableLaunchAuthority, String> {
        let executable = self
            .file
            .duplicate_restrictive_executable_host_handle()
            .map_err(|error| error.to_string())?;
        let directory = self
            .directory
            .duplicate_restrictive_host_handle()
            .map_err(|error| error.to_string())?;
        ExactExecutableLaunchAuthority::from_verified_handles(executable, directory)
            .map_err(|error| error.to_string())
    }

    pub fn assert_exact(&self, captured: &CapturedCargoExecutable) {
        self.file
            .verify_digest(captured.digest(), captured.bytes().len() as u64)
            .unwrap();
        assert_eq!(
            self.file
                .read_to_end_bounded(captured.bytes().len() as u64)
                .unwrap(),
            captured.bytes()
        );
        let current = host_fs::readable_file(&self.path).unwrap();
        assert!(current.same_object(&self.file).unwrap());
        current
            .verify_digest(captured.digest(), captured.bytes().len() as u64)
            .unwrap();
    }
}

#[derive(Debug)]
pub struct CapturedState {
    pub state: ResumeState,
    pub bytes: Vec<u8>,
}

impl CapturedState {
    pub fn assert_exact(&self, other: &Self) {
        assert_eq!(
            self.state, other.state,
            "every game-owned semantic field must match"
        );
        assert_eq!(
            self.bytes, other.bytes,
            "canonical game capture bytes must also match"
        );
    }
}

/// Only the declared finite control protocol; this is not a runtime supervisor or Editor bridge.
pub struct GameProcess<'a> {
    native: FiniteProcess<'a>,
    _executable: &'a Executable,
    control: PathBuf,
    session: String,
    generation: String,
    next: u64,
    pub ready: Observation,
}

impl<'a> GameProcess<'a> {
    pub fn spawn(
        fixture: &'a Fixture,
        executable: &'a Executable,
        label: &str,
    ) -> Result<Self, String> {
        Self::spawn_in(fixture, executable, label, &fixture.project)
    }

    pub fn spawn_in(
        fixture: &'a Fixture,
        executable: &'a Executable,
        label: &str,
        project: &Path,
    ) -> Result<Self, String> {
        assert!(project.is_absolute());
        let control = fixture.directory(label);
        let session = format!(
            "{}-{label}",
            fixture.root.file_name().unwrap().to_str().unwrap()
        );
        let generation = format!("build-{label}");
        #[cfg(windows)]
        let environment = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
        #[cfg(not(windows))]
        let environment = SupervisedProductEnvironmentPolicy::Sealed;
        let command = SupervisedProductProcessCommand::new(environment, &control).arguments([
            std::ffi::OsStr::new("--project-root"),
            project.as_os_str(),
            std::ffi::OsStr::new("--control-dir"),
            control.as_os_str(),
            std::ffi::OsStr::new("--session"),
            std::ffi::OsStr::new(&session),
            std::ffi::OsStr::new("--generation"),
            std::ffi::OsStr::new(&generation),
        ]);
        let owner = SupervisedProcessOwner::spawn_exact(
            executable.launch_authority()?,
            &command,
            log_limits(),
        )
        .map_err(|error| error.to_string())?;
        let mut native = FiniteProcess::new(fixture, owner);
        let bootstrap = (|| {
            let bytes = native.wait_file(&control.join("ready.json"), START_TIMEOUT)?;
            let response = decode_response(&bytes)?;
            match checked_reply(response, &session, &generation, 0)? {
                Reply::Observed { observation } => Ok(observation),
                reply => Err(format!(
                    "bootstrap did not observe the paused game: {reply:?}"
                )),
            }
        })();
        let ready = match bootstrap {
            Ok(ready) => ready,
            Err(error) => return Err(native.fail(error)),
        };
        if ready.phase != "running"
            || ready.restoring
            || ready.error.is_some()
            || ready.engine_tick != 0
            || ready.logical_tick != 0
            || ready.logical_elapsed_nanos != 0
            || ready.runtime_generation == 0
        {
            return Err(native.fail(format!(
                "bootstrap did not produce a fresh initialized paused game: {ready:?}"
            )));
        }
        assert_running(&ready);
        assert_eq!(
            ready.engine_tick, 0,
            "start/pause must not advance simulation"
        );
        assert_eq!(ready.logical_tick, 0);
        assert_eq!(ready.logical_elapsed_nanos, 0);
        assert_ne!(ready.runtime_generation, 0);
        Ok(Self {
            native,
            _executable: executable,
            control,
            session,
            generation,
            next: 1,
            ready,
        })
    }

    pub fn pid(&self) -> u32 {
        self.native.owner().unwrap().process_id()
    }

    pub fn request(&mut self, command: Command) -> Result<Reply, String> {
        let request = self.next;
        self.next = self.next.checked_add(1).unwrap();
        let packet = Request {
            version: PROTOCOL_VERSION,
            session: self.session.clone(),
            generation: self.generation.clone(),
            request,
            command,
        };
        let response = self.receive(&packet)?;
        response.result
    }

    fn receive(&mut self, packet: &Request) -> Result<Response, String> {
        let result = self.exchange(packet).and_then(|response| {
            check_reply_identity(&response, &self.session, &self.generation, packet.request)?;
            if let Ok(reply) = &response.result
                && !matches!(
                    (&packet.command, reply),
                    (
                        Command::Observe
                            | Command::Step { .. }
                            | Command::Frame
                            | Command::Restore { .. },
                        Reply::Observed { .. }
                    ) | (Command::Capture, Reply::Captured { .. })
                        | (Command::Close, Reply::Closed { .. })
                )
            {
                return Err(format!(
                    "unexpected game reply for {:?}: {reply:?}",
                    packet.command,
                ));
            }
            Ok(response)
        });
        result.map_err(|error| self.native.fail(error))
    }

    fn exchange(&mut self, packet: &Request) -> Result<Response, String> {
        self.native.check()?;
        let bytes = serde_json::to_vec(packet).map_err(|error| error.to_string())?;
        assert!(bytes.len() <= MAX_MESSAGE_BYTES);
        let inbox = self.control.join("request.json");
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while inbox.exists() {
            self.native.check()?;
            if Instant::now() >= deadline {
                return Err("previous inbox was not consumed before its deadline".into());
            }
            std::thread::sleep(POLL);
        }
        let temporary = self
            .control
            .join(format!("host-request-{}-{}.tmp", packet.request, self.next));
        let mut file = File::create_new(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, &inbox).map_err(|error| error.to_string())?;
        let response_path = self
            .control
            .join(format!("response-{}.json", packet.request));
        let bytes = self.native.wait_file(
            &response_path,
            deadline.saturating_duration_since(Instant::now()),
        )?;
        fs::remove_file(response_path).map_err(|error| error.to_string())?;
        decode_response(&bytes)
    }

    pub fn observe(&mut self) -> Observation {
        observed(self.request(Command::Observe).unwrap())
    }

    pub fn step(&mut self, commands: Vec<SemanticCommand>) -> Observation {
        observed(self.request(Command::Step { commands }).unwrap())
    }

    pub fn settle(&mut self, mut observation: Observation) -> Observation {
        let deadline = Instant::now() + REQUEST_TIMEOUT;
        while observation.phase != "running" || observation.restoring {
            assert!(observation.error.is_none(), "{observation:?}");
            assert!(
                Instant::now() < deadline,
                "Scene/reconstruction did not finish: {observation:?}"
            );
            observation = observed(self.request(Command::Frame).unwrap());
            std::thread::sleep(POLL);
        }
        assert_running(&observation);
        observation
    }

    pub fn settled_step(&mut self, commands: Vec<SemanticCommand>) -> Observation {
        let observation = self.step(commands);
        self.settle(observation)
    }

    pub fn capture(&mut self) -> CapturedState {
        match self.request(Command::Capture).unwrap() {
            Reply::Captured { state } => {
                let bytes = encode_resume(&state).unwrap();
                CapturedState { state, bytes }
            }
            reply => panic!("capture must return the exact game DTO: {reply:?}"),
        }
    }

    pub fn restore(&mut self, captured: &CapturedState) -> Result<Observation, String> {
        let observation = observed(self.request(Command::Restore {
            state: captured.state.clone(),
        })?);
        assert_running(&observation);
        captured.assert_exact(&self.capture());
        Ok(observation)
    }

    /// Feed stale mutation requests to the actual child, then prove complete state stayed put.
    pub fn reject_stale_requests(&mut self) {
        for kind in ["session", "generation", "request"] {
            let baseline = self.capture();
            let request = if kind == "request" {
                self.next - 1
            } else {
                self.next
            };
            if kind != "request" {
                self.next += 1;
            }
            let mut packet = Request {
                version: PROTOCOL_VERSION,
                session: self.session.clone(),
                generation: self.generation.clone(),
                request,
                command: Command::Step {
                    commands: vec![SemanticCommand::Marker],
                },
            };
            if kind == "session" {
                packet.session.push_str("-stale");
            }
            if kind == "generation" {
                packet.generation.push_str("-stale");
            }
            let response = self.receive(&packet).unwrap();
            assert!(
                response.result.is_err(),
                "stale {kind} must be refused before mutation"
            );
            assert!(checked_reply(response, &self.session, &self.generation, request).is_err());
            baseline.assert_exact(&self.capture());
        }
    }

    /// A fresh real candidate restores successfully, but a stale reply must fail the ordinary
    /// acceptance gate before its state can be considered for promotion. The caller closes it.
    pub fn restore_with_stale_reply(
        &mut self,
        captured: &CapturedState,
        kind: &str,
    ) -> Result<(), String> {
        let request = self.next;
        self.next = self.next.checked_add(1).unwrap();
        let packet = Request {
            version: PROTOCOL_VERSION,
            session: self.session.clone(),
            generation: self.generation.clone(),
            request,
            command: Command::Restore {
                state: captured.state.clone(),
            },
        };
        let mut response = self.receive(&packet)?;
        let actual = match checked_reply(response.clone(), &self.session, &self.generation, request)
        {
            Ok(reply) => reply,
            Err(error) => return Err(self.native.fail(error)),
        };
        if !matches!(actual, Reply::Observed { .. }) {
            return Err(self.native.fail(format!(
                "the real restore must succeed before testing a stale reply: {response:?}"
            )));
        }
        assert!(
            matches!(actual, Reply::Observed { .. }),
            "the real restore must succeed before testing a stale reply: {response:?}"
        );
        match kind {
            "session" => response.session.push_str("-retired"),
            "generation" => response.generation.push_str("-retired"),
            "request" => response.request = request - 1,
            _ => panic!("unknown stale reply case: {kind}"),
        }
        checked_reply(response, &self.session, &self.generation, request).map(|_| ())
    }

    pub fn close(mut self) {
        let reply = match self.request(Command::Close) {
            Ok(reply) => reply,
            Err(error) => panic!("{}", self.native.fail(error)),
        };
        if !matches!(reply, Reply::Closed { failed: false }) {
            let error =
                format!("Close must acknowledge physical retirement without failure: {reply:?}");
            panic!("{}", self.native.fail(error));
        }
        if let Err(error) = self.native.wait(RETIRE_TIMEOUT) {
            panic!("{}", self.native.fail(error));
        }
        let owner = self.native.owner().unwrap();
        let receipt = owner.cleanup_receipt().unwrap();
        assert!(
            receipt.exit().success(),
            "normal Close must exit successfully: {}",
            self.native.diagnostics()
        );
        assert!(
            receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained(),
            "normal Close must reap the child, empty native containment, stop readers and drain output"
        );
        assert!(
            !self.native.fallback_terminate_requested,
            "fallback termination cannot establish a clean game Close"
        );
    }
}

fn checked_reply(
    response: Response,
    session: &str,
    generation: &str,
    request: u64,
) -> Result<Reply, String> {
    check_reply_identity(&response, session, generation, request)?;
    response.result
}

fn check_reply_identity(
    response: &Response,
    session: &str,
    generation: &str,
    request: u64,
) -> Result<(), String> {
    if response.version != PROTOCOL_VERSION
        || response.placement != crate::protocol::DevelopmentPlacement::SourceHeadless
        || response.session != session
        || response.generation != generation
        || response.request != request
    {
        return Err(format!(
            "stale protocol reply: expected ({session}, {generation}, {request}), got ({}, {}, {}, {})",
            response.version, response.session, response.generation, response.request
        ));
    }
    Ok(())
}

fn observed(reply: Reply) -> Observation {
    match reply {
        Reply::Observed { observation } => observation,
        reply => panic!("expected full game observation, got {reply:?}"),
    }
}

pub fn assert_running(observation: &Observation) {
    assert_eq!(observation.phase, "running", "{observation:?}");
    assert!(!observation.restoring, "{observation:?}");
    assert!(observation.error.is_none(), "{observation:?}");
}

struct FiniteProcess<'a> {
    owner: Option<SupervisedProcessOwner>,
    fixture: &'a Fixture,
    fallback_terminate_requested: bool,
}

impl<'a> FiniteProcess<'a> {
    fn new(fixture: &'a Fixture, owner: SupervisedProcessOwner) -> Self {
        Self {
            owner: Some(owner),
            fixture,
            fallback_terminate_requested: false,
        }
    }

    fn owner(&self) -> Result<&SupervisedProcessOwner, String> {
        self.owner
            .as_ref()
            .ok_or_else(|| "native cleanup custody was retained by the fixture".into())
    }

    fn diagnostics(&self) -> String {
        self.owner().map(diagnostics).unwrap_or_else(|error| error)
    }

    fn advance(&mut self) -> Result<bool, String> {
        self.owner
            .as_mut()
            .ok_or("native cleanup custody was retained by the fixture")?
            .advance(Instant::now())
            .map(|observation| observation.exit().is_some())
            .map_err(|error| error.to_string())
    }

    fn check(&mut self) -> Result<(), String> {
        if self.advance()? {
            return Err(format!(
                "native process exited before the requested result: {}",
                self.diagnostics()
            ));
        }
        Ok(())
    }

    fn wait_file(&mut self, path: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let exited = self.advance()?;
            if path.exists() {
                return read_bounded(path, MAX_MESSAGE_BYTES);
            }
            if exited {
                if path.exists() {
                    return read_bounded(path, MAX_MESSAGE_BYTES);
                }
                return Err(format!(
                    "native process exited before the requested result: {}",
                    self.diagnostics()
                ));
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "native request exceeded {timeout:?}: {}; {}",
                    path.display(),
                    self.diagnostics()
                ));
            }
            std::thread::sleep(POLL);
        }
    }

    fn wait(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let owner = self
                .owner
                .as_mut()
                .ok_or("native cleanup custody was retained by the fixture")?;
            owner
                .advance(Instant::now())
                .map_err(|error| error.to_string())?;
            if owner.state() == ProcessState::Complete {
                return check_cleanup(owner);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "native child did not retire within {timeout:?}: {}",
                    self.diagnostics()
                ));
            }
            std::thread::sleep(POLL);
        }
    }

    fn capture_stdout(&mut self, timeout: Duration, maximum: usize) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + timeout;
        let mut bytes = Vec::new();
        loop {
            let owner = self
                .owner
                .as_mut()
                .ok_or("native cleanup custody was retained by the fixture")?;
            let (_, outputs) = owner
                .advance_with_output(Instant::now())
                .map_err(|error| error.to_string())?;
            for output in outputs {
                if output.stream() != ProcessOutputStream::Stdout {
                    continue;
                }
                if output.loss() || output.bytes().len() > maximum - bytes.len() {
                    return Err("the tool probe lost its bounded exact stdout".into());
                }
                bytes.extend_from_slice(output.bytes());
            }
            if owner.state() == ProcessState::Complete {
                check_cleanup(owner)?;
                if !owner.cleanup_receipt().unwrap().exit().success() {
                    return Err(format!("tool probe failed: {}", self.diagnostics()));
                }
                return Ok(bytes);
            }
            if Instant::now() >= deadline {
                return Err(format!("tool probe exceeded {timeout:?}"));
            }
            std::thread::sleep(POLL);
        }
    }

    fn fail(&mut self, error: String) -> String {
        let Some(owner) = self.owner.as_mut() else {
            return format!("{error}; {}", self.diagnostics());
        };
        self.fallback_terminate_requested |= owner.state() != ProcessState::Complete;
        match cleanup_process(owner) {
            Ok(()) => format!(
                "{error}; native cleanup complete, fallback_terminate_requested={}; {}",
                self.fallback_terminate_requested,
                self.diagnostics(),
            ),
            Err(cleanup) => {
                let tail = self.diagnostics();
                self.fixture
                    .pending_cleanup
                    .borrow_mut()
                    .push(self.owner.take().unwrap());
                format!(
                    "{error}; native cleanup retained for outer retry: {cleanup}; fallback_terminate_requested={}; {tail}",
                    self.fallback_terminate_requested,
                )
            }
        }
    }
}

impl Drop for FiniteProcess<'_> {
    fn drop(&mut self) {
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.state() != ProcessState::Complete)
        {
            let error = self.fail("finite native owner dropped without completed Close".into());
            eprintln!("continuity fallback: {error}");
        }
    }
}

fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::new(
        NonZeroUsize::new(MAX_LOG_RECORDS).unwrap(),
        NonZeroUsize::new(MAX_LOG_BYTES).unwrap(),
    )
}

fn diagnostics(owner: &SupervisedProcessOwner) -> String {
    let mut text = format!(
        "bounded diagnostic tail (records<={MAX_LOG_RECORDS}, bytes<={MAX_LOG_BYTES}): {:?}; output_loss={}; cleanup_receipt={:?}",
        owner.logs().stats(),
        owner.output_loss_observed(),
        owner.cleanup_receipt(),
    );
    for entry in owner.logs().iter() {
        text.push_str(&format!(
            "\n{:?} truncated_prefix_bytes={}: {}",
            entry.stream(),
            entry.truncated_prefix_bytes(),
            String::from_utf8_lossy(entry.bytes()),
        ));
    }
    text
}

fn check_cleanup(owner: &SupervisedProcessOwner) -> Result<(), String> {
    let receipt = owner
        .cleanup_receipt()
        .ok_or("native cleanup has not published a receipt")?;
    if owner.state() != ProcessState::Complete
        || !receipt.containment_empty()
        || !receipt.readers_stopped()
        || !receipt.output_drained()
    {
        return Err(
            "native cleanup has not emptied containment, stopped readers and drained output".into(),
        );
    }
    Ok(())
}

fn cleanup_process(owner: &mut SupervisedProcessOwner) -> Result<(), String> {
    if owner.state() == ProcessState::Complete {
        return check_cleanup(owner);
    }
    let now = Instant::now();
    let deadline = now + RETIRE_TIMEOUT;
    let mut last_error = owner
        .request_terminate(now)
        .err()
        .map(|error| error.to_string());
    let mut retry_at = now;
    loop {
        let now = Instant::now();
        if let Err(error) = owner.advance(now) {
            last_error = Some(error.to_string());
        }
        if owner.state() == ProcessState::Complete {
            return check_cleanup(owner);
        }
        if now >= deadline {
            return Err(format!(
                "native cleanup exceeded {RETIRE_TIMEOUT:?}: state={:?}, last_error={last_error:?}",
                owner.state(),
            ));
        }
        if last_error.is_some() && now >= retry_at {
            last_error = owner
                .retry_cleanup(now)
                .err()
                .map(|error| error.to_string());
            retry_at = now + Duration::from_secs(1);
        }
        std::thread::sleep(POLL);
    }
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if file.metadata().map_err(|error| error.to_string())?.len() > limit as u64 {
        return Err(format!("{} exceeds its {limit}-byte limit", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!(
            "{} grew beyond its {limit}-byte limit",
            path.display()
        ));
    }
    Ok(bytes)
}

fn decode_response(bytes: &[u8]) -> Result<Response, String> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("continuity response exceeded its encoded-byte limit".into());
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(20).unwrap(),
        ItemLimit::new(8192).unwrap(),
        ItemLimit::new(256).unwrap(),
        ByteLimit::new(16 * 1024).unwrap(),
        ByteLimit::new(64 * 1024).unwrap(),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}
