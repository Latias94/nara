//! The Two-Level Game's finite edit/build/process qualification Host.
//!
//! This owns only its copied project, captured executable installations and direct children.
//! It deliberately uses the public Cargo adapter and the game's ordinary process protocol.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command as NativeCommand, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    fs::{
        ContentDigest, FileCapability, RelativeComponent, RelativePath,
        RestrictiveCapabilityHandleLease,
    },
    project_tooling::{
        cargo_build::{
            CapturedCargoExecutable, CargoBuildFailure, CargoBuildLimits, CargoBuildRequest,
            CargoBuildTools, build_executable,
        },
        host_fs,
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
const MAX_LOG: u64 = 2 * 1024 * 1024;
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
        };
        fixture.host = fixture.probe_host();
        for directory in ["src", "assets", "scenes", "prefabs", "schema"] {
            copy_tree(&sample.join(directory), &fixture.project.join(directory));
        }
        for name in ["Cargo.toml", "Cargo.lock", "nara.toml"] {
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
        fixture.write(
            "Cargo.toml",
            &manifest.replacen(original, &format!("path = {absolute}"), 1),
        );
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
                .any(|path| path == "src/bin/continuity/protocol.rs")
        );
        fixture
    }

    fn probe_host(&self) -> String {
        let mut command =
            NativeCommand::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        command
            .current_dir(&self.repository)
            .args(["-vV"])
            .env("RUSTUP_AUTO_INSTALL", "0");
        let directory = self.directory("toolchain-observation");
        let mut child = DirectChild::spawn(command, &directory).unwrap();
        let status = child.wait(REQUEST_TIMEOUT).unwrap();
        assert!(status.success(), "{}", child.diagnostics());
        let text =
            String::from_utf8(read_bounded(&child.stdout, MAX_LOG as usize).unwrap()).unwrap();
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
        let selection = CargoBuildSelection::new(
            "two-level-game",
            "continuity",
            "dev",
            &self.host,
            std::iter::empty::<&str>(),
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
        let old = format!(
            "#[derive(Resource, Default)]{newline}pub(super) struct Session {{{newline}    held: [bool; 4],"
        );
        assert_eq!(
            source.matches(&old).count(),
            1,
            "the private baseline Session field"
        );
        assert!(!source.contains("struct MovementIntent"));
        let replacement = format!(
            "#[derive(Default)]{newline}struct MovementIntent {{{newline}    held: [bool; 4],{newline}}}{newline}{newline}\
             #[derive(Resource, Default)]{newline}pub(super) struct Session {{{newline}    movement: MovementIntent,",
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
        assert!(self.read(GAMEPLAY).contains("direction * 80.0 * delta"));
        changed
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
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
            file,
            _read_lease: read_lease,
            _directory_lease: directory_lease,
        };
        installed.assert_exact(captured);
        installed
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
    native: DirectChild,
    _executable: &'a Executable,
    control: PathBuf,
    session: String,
    generation: String,
    next: u64,
    pub ready: Observation,
}

impl<'a> GameProcess<'a> {
    pub fn spawn(
        fixture: &Fixture,
        executable: &'a Executable,
        label: &str,
    ) -> Result<Self, String> {
        Self::spawn_in(fixture, executable, label, &fixture.project)
    }

    pub fn spawn_in(
        fixture: &Fixture,
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
        let mut command = NativeCommand::new(&executable.path);
        command
            .current_dir(&control)
            .arg("--project-root")
            .arg(project)
            .arg("--control-dir")
            .arg(&control)
            .arg("--session")
            .arg(&session)
            .arg("--generation")
            .arg(&generation);
        command
            .env_clear()
            .env("PATH", control.join("no-cargo-on-path"));
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut native = DirectChild::spawn(command, &control)?;
        let ready_bytes = native.wait_file(&control.join("ready.json"), START_TIMEOUT)?;
        let ready_response = decode_response(&ready_bytes)?;
        let ready = match checked_reply(ready_response, &session, &generation, 0)? {
            Reply::Observed { observation } => observation,
            reply => {
                return Err(format!(
                    "bootstrap did not observe the paused game: {reply:?}"
                ));
            }
        };
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
        self.native.child.as_ref().unwrap().id()
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
        checked_reply(response, &self.session, &self.generation, request)
    }

    fn receive(&mut self, packet: &Request) -> Result<Response, String> {
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
        let actual = checked_reply(response.clone(), &self.session, &self.generation, request)
            .expect(
                "the actual restore reply must pass identity before its stale replay is tested",
            );
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
        match self.request(Command::Close).unwrap() {
            Reply::Closed { failed } => assert!(!failed, "physical normal Close reported failure"),
            reply => panic!("Close must acknowledge physical retirement: {reply:?}"),
        }
        let status = self.native.wait(RETIRE_TIMEOUT).unwrap();
        assert!(
            status.success(),
            "normal Close must exit successfully: {}",
            self.native.diagnostics()
        );
        assert!(
            self.native.child.is_none(),
            "normal Close must reap the direct child"
        );
    }
}

fn checked_reply(
    response: Response,
    session: &str,
    generation: &str,
    request: u64,
) -> Result<Reply, String> {
    if response.version != PROTOCOL_VERSION
        || response.session != session
        || response.generation != generation
        || response.request != request
    {
        return Err(format!(
            "stale protocol reply: expected ({session}, {generation}, {request}), got ({}, {}, {}, {})",
            response.version, response.session, response.generation, response.request
        ));
    }
    response.result
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

struct DirectChild {
    child: Option<Child>,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl DirectChild {
    fn spawn(mut command: NativeCommand, directory: &Path) -> Result<Self, String> {
        let stdout = directory.join("stdout.log");
        let stderr = directory.join("stderr.log");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                File::create_new(&stdout).map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(
                File::create_new(&stderr).map_err(|error| error.to_string())?,
            ));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command
            .spawn()
            .map_err(|error| format!("{command:?}: {error}"))?;
        Ok(Self {
            child: Some(child),
            stdout,
            stderr,
        })
    }

    fn logs_bounded(&self) -> Result<(), String> {
        for path in [&self.stdout, &self.stderr] {
            let length = fs::metadata(path).map_err(|error| error.to_string())?.len();
            if length > MAX_LOG {
                return Err(format!(
                    "native diagnostic log exceeded {MAX_LOG} bytes: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn diagnostics(&self) -> String {
        let text = |path: &Path| match read_bounded(path, MAX_LOG as usize) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => error,
        };
        format!(
            "stdout:\n{}\nstderr:\n{}",
            text(&self.stdout),
            text(&self.stderr)
        )
    }

    fn check(&mut self) -> Result<(), String> {
        self.logs_bounded()?;
        if let Some(status) = self
            .child
            .as_mut()
            .unwrap()
            .try_wait()
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "native child exited before the requested result ({status}): {}",
                self.diagnostics()
            ));
        }
        Ok(())
    }

    fn wait_file(&mut self, path: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.logs_bounded()?;
            if path.exists() {
                return read_bounded(path, MAX_MESSAGE_BYTES);
            }
            if let Err(error) = self.check() {
                if path.exists() {
                    return read_bounded(path, MAX_MESSAGE_BYTES);
                }
                return Err(error);
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

    fn wait(&mut self, timeout: Duration) -> Result<ExitStatus, String> {
        let deadline = Instant::now() + timeout;
        loop {
            self.logs_bounded()?;
            let child = self.child.as_mut().unwrap();
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                child.wait().map_err(|error| error.to_string())?;
                self.child = None;
                return Ok(status);
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
}

impl Drop for DirectChild {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        let kill = child.kill();
        let deadline = Instant::now() + RETIRE_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    let _ = child.wait();
                    return;
                }
                Err(error) => {
                    eprintln!(
                        "continuity fallback could not reap child {}: {error}; kill={kill:?}",
                        child.id()
                    );
                    return;
                }
                Ok(None) if Instant::now() >= deadline => {
                    eprintln!(
                        "continuity fallback could not confirm retirement of child {}; kill={kill:?}",
                        child.id()
                    );
                    return;
                }
                Ok(None) => std::thread::sleep(POLL),
            }
        }
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
