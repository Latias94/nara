//! Supervised external Play, with an isolated authoring snapshot for opt-in games.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    path::Path,
    process::ExitStatus,
    time::{Duration, Instant},
};

use nara_core::AssetId;
use nara_project::{
    EditorPlayMode,
    play::{
        BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream, PlayProcess,
        build_interruptible,
    },
    workflow::{CargoPackage, ProjectCheckError, ProjectTrust, read_project_manifest},
};
use nara_reflect::{ComponentSchema, ComponentSchemaCatalog};
use nara_scene::authoring::{
    SceneComponent, SceneDocument, SceneDocumentError,
    prefab::{PrefabError, expand_prefabs_for_editor},
};
use tempfile::TempDir;
use thiserror::Error;

pub use nara_project::play::PlayControl;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Starting,
    Running,
    Paused,
    Exited(ExitStatus),
}

#[derive(Debug, Error)]
pub enum PlayError {
    #[error(transparent)]
    Project(#[from] ProjectCheckError),
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Scene(#[from] SceneDocumentError),
    #[error(transparent)]
    Prefab(#[from] PrefabError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("launching a game requires a trusted project")]
    Untrusted,
    #[error("Play was cancelled")]
    Cancelled,
    #[error("{0:?} requires a project that opts into snapshot Play")]
    SnapshotControlRequired(PlayControl),
    #[error("{0:?} is unavailable while Play is {1:?} or awaiting an acknowledgment")]
    ControlUnavailable(PlayControl, PlayState),
    #[error("the game did not acknowledge the editor control")]
    NoAcknowledgment,
}

/// Flattens the captured authoring inputs so a later prefab edit cannot change this Play launch.
///
/// `None` means that prefab-local opaque values still need a project Schema before their
/// expanded forms can be serialized with a single asset table.
pub fn flatten_snapshot(
    captured: &SceneDocument,
    prefabs: &BTreeMap<AssetId, SceneDocument>,
    catalog: &ComponentSchemaCatalog,
) -> Result<Option<SceneDocument>, PlayError> {
    if !captured
        .entities
        .iter()
        .any(|entity| entity.prefab.is_some())
    {
        return Ok(Some(captured.clone()));
    }
    let mut source = captured.clone();
    source.resolve_unknown_components(catalog.components())?;
    let expanded = expand_prefabs_for_editor(&source, catalog, |id| {
        let mut source = prefabs
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("prefab {id} was not captured"))?;
        source
            .resolve_unknown_components(catalog.components())
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(source)
    })?;
    if !expanded.deferred_overrides.is_empty()
        || expanded.document.entities.iter().any(|row| {
            expanded.provenance.contains_key(&row.id)
                && row
                    .components
                    .values()
                    .any(|component| matches!(component, SceneComponent::Unknown(_)))
        })
    {
        return Ok(None);
    }
    Ok(Some(expanded.document))
}

/// Owns a supervised game and any temporary authoring snapshot it opens.
pub struct PlayModeSession {
    process: PlayProcess,
    snapshot: Option<Snapshot>,
    state: PlayState,
    completed_steps: u64,
}

struct Snapshot {
    _directory: TempDir,
    reply: SnapshotReply,
}

#[derive(Clone, Copy)]
enum SnapshotReply {
    Ready(Instant),
    Control(PlayControl, Instant),
    Idle,
    Closed,
}

impl PlayModeSession {
    /// Builds and launches the project's default binary according to its Play setting.
    /// Call on a background worker. `poll` delivers game output and handles snapshot replies.
    pub fn start(
        project: &Path,
        scene: &SceneDocument,
        schemas: &[ComponentSchema],
        trust: ProjectTrust,
        mut cancel: impl FnMut() -> bool,
        emit: impl FnMut(BuildEvent),
    ) -> Result<Self, PlayError> {
        if trust != ProjectTrust::Trusted {
            return Err(PlayError::Untrusted);
        }
        let mode = read_project_manifest(project)?.editor.play;
        let package = CargoPackage::read(project)?;
        let binary = package.default_binary();
        let features: Vec<_> = package
            .required_features(binary)
            .iter()
            .map(String::as_str)
            .collect();
        let request = BuildRequest {
            features: &features,
            ..BuildRequest::new(project, package.name(), binary)
        };
        let executable = match build_interruptible(&request, trust, &mut cancel, emit) {
            Err(BuildError::Interrupted) => return Err(PlayError::Cancelled),
            result => result?,
        };
        if cancel() {
            return Err(PlayError::Cancelled);
        }
        Self::launch(project, &executable, scene, schemas, trust, mode)
    }

    fn launch(
        project: &Path,
        executable: &Path,
        scene: &SceneDocument,
        schemas: &[ComponentSchema],
        trust: ProjectTrust,
        mode: EditorPlayMode,
    ) -> Result<Self, PlayError> {
        match mode {
            EditorPlayMode::External => {
                let process = PlayProcess::launch(project, executable, &[], trust)?;
                Ok(Self::new(process, None))
            }
            EditorPlayMode::Snapshot => {
                let snapshot = tempfile::Builder::new()
                    .prefix("nara-editor-play-")
                    .tempdir()?;
                let path = snapshot.path().join("play.scene.ron");
                scene.write_with(&path, schemas)?;
                let arguments = [OsString::from("--editor-play"), path.into_os_string()];
                let process =
                    PlayProcess::launch_controlled(project, executable, &arguments, trust)?;
                Ok(Self::new(process, Some(snapshot)))
            }
        }
    }

    fn new(process: PlayProcess, directory: Option<TempDir>) -> Self {
        let snapshot = directory.map(|directory| Snapshot {
            _directory: directory,
            reply: SnapshotReply::Ready(Instant::now() + RESPONSE_TIMEOUT),
        });
        let state = if snapshot.is_some() {
            PlayState::Starting
        } else {
            PlayState::Running
        };
        Self {
            process,
            snapshot,
            state,
            completed_steps: 0,
        }
    }

    pub fn mode(&self) -> EditorPlayMode {
        if self.snapshot.is_some() {
            EditorPlayMode::Snapshot
        } else {
            EditorPlayMode::External
        }
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn pending_control(&self) -> Option<PlayControl> {
        match self.snapshot.as_ref()?.reply {
            SnapshotReply::Control(control, _) => Some(control),
            _ => None,
        }
    }

    pub fn completed_steps(&self) -> u64 {
        self.completed_steps
    }

    pub fn control(&mut self, control: PlayControl) -> Result<(), PlayError> {
        let Some(snapshot) = &mut self.snapshot else {
            return Err(PlayError::SnapshotControlRequired(control));
        };
        if !matches!(snapshot.reply, SnapshotReply::Idle)
            || !matches!(
                (self.state, control),
                (PlayState::Running, PlayControl::Pause)
                    | (PlayState::Paused, PlayControl::Resume | PlayControl::Step)
            )
        {
            return Err(PlayError::ControlUnavailable(control, self.state));
        }
        self.process.send_control(control)?;
        snapshot.reply = SnapshotReply::Control(control, Instant::now() + RESPONSE_TIMEOUT);
        Ok(())
    }

    pub fn poll(&mut self, mut emit: impl FnMut(OutputLine)) -> Result<PlayState, PlayError> {
        let status = self.process.poll(|line| {
            if line.stream == OutputStream::Stdout
                && let Some(snapshot) = &mut self.snapshot
            {
                let state = match (snapshot.reply, line.text.as_str()) {
                    (SnapshotReply::Ready(_), "NARA_EDITOR ready") => Some(PlayState::Running),
                    (SnapshotReply::Control(PlayControl::Pause, _), "NARA_EDITOR paused") => {
                        Some(PlayState::Paused)
                    }
                    (SnapshotReply::Control(PlayControl::Resume, _), "NARA_EDITOR running") => {
                        Some(PlayState::Running)
                    }
                    (SnapshotReply::Control(PlayControl::Step, _), "NARA_EDITOR stepped") => {
                        self.completed_steps = self.completed_steps.saturating_add(1);
                        Some(PlayState::Paused)
                    }
                    _ => None,
                };
                if let Some(state) = state {
                    self.state = state;
                    snapshot.reply = SnapshotReply::Idle;
                    return;
                }
            }
            emit(line);
        });
        match status {
            Ok(Some(status)) => {
                self.state = PlayState::Exited(status);
                self.close_controls();
            }
            Ok(None) => {}
            Err(error) => {
                self.close_controls();
                return Err(error.into());
            }
        }
        if let Some(snapshot) = &mut self.snapshot
            && let SnapshotReply::Ready(deadline) | SnapshotReply::Control(_, deadline) =
                snapshot.reply
            && Instant::now() >= deadline
        {
            snapshot.reply = SnapshotReply::Closed;
            return Err(PlayError::NoAcknowledgment);
        }
        Ok(self.state)
    }

    /// Terminates and reaps the complete game process tree.
    pub fn stop(&mut self, emit: impl FnMut(OutputLine)) -> Result<ExitStatus, PlayError> {
        self.close_controls();
        let status = self.process.stop(emit)?;
        self.state = PlayState::Exited(status);
        Ok(status)
    }

    fn close_controls(&mut self) {
        if let Some(snapshot) = &mut self.snapshot {
            snapshot.reply = SnapshotReply::Closed;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{BufRead, Write},
        thread,
    };

    use nara_scene::authoring::SceneEntity;

    use super::*;

    const CONTINUE_PAUSE: &str = "continue-pause";

    #[test]
    fn snapshot_resolves_prefabs_from_the_captured_source_cut() {
        use nara_scene::{
            authoring::{PrefabInstance, builtin_component_schemas},
            spatial::Transform2d,
        };

        let catalog =
            ComponentSchemaCatalog::from_components(builtin_component_schemas().unwrap()).unwrap();
        let asset = "01951a29-2e18-7000-9000-000000000003".parse().unwrap();
        let instance = "e66193e0-2e80-4a44-934a-08330b93cb75".parse().unwrap();
        let mut scene = SceneDocument::new("97270f99-fd97-483f-a605-a1e7acaea745".parse().unwrap());
        let mut root = SceneEntity::new(instance, "Instance");
        root.prefab = Some(PrefabInstance::new(asset));
        scene.entities.push(root);

        let mut prefab =
            SceneDocument::new("981296f8-b92f-4868-a674-ec7b643f9953".parse().unwrap());
        let mut row = SceneEntity::new(
            "a6086d96-7d01-4142-8129-2afa8585b463".parse().unwrap(),
            "Prefab",
        );
        row.set(&Transform2d::from_xy(20.0, 0.0)).unwrap();
        prefab.entities.push(row);
        let captured = BTreeMap::from([(asset, prefab)]);
        let mut live = captured.clone();
        live.get_mut(&asset).unwrap().entities[0]
            .set(&Transform2d::from_xy(90.0, 0.0))
            .unwrap();

        let snapshot = flatten_snapshot(&scene, &captured, &catalog)
            .unwrap()
            .unwrap();
        let saved = snapshot.to_ron_with(catalog.components()).unwrap();
        let roundtrip = SceneDocument::parse_with(&saved, catalog.components()).unwrap();
        assert_eq!(roundtrip.entities[0].id, instance);
        assert!(roundtrip.entities[0].prefab.is_none());
        let pose = roundtrip.entities[0]
            .typed::<Transform2d>()
            .unwrap()
            .unwrap();
        assert_eq!(pose.translation.x, 20.0);
        let changed = flatten_snapshot(&scene, &live, &catalog).unwrap().unwrap();
        assert_ne!(snapshot, changed);
    }

    #[test]
    fn external_play_streams_output_without_waiting_for_a_handshake() {
        let project = tempfile::tempdir().unwrap();
        let process = PlayProcess::launch(
            project.path(),
            &std::env::current_exe().unwrap(),
            &helper_arguments("play::tests::external_game_helper"),
            ProjectTrust::Trusted,
        )
        .unwrap();
        let mut play = PlayModeSession::new(process, None);

        assert_eq!(play.mode(), EditorPlayMode::External);
        assert_eq!(play.state(), PlayState::Running);
        assert!(matches!(
            play.control(PlayControl::Pause),
            Err(PlayError::SnapshotControlRequired(PlayControl::Pause))
        ));
        wait_for(&mut play, |_, output| {
            output.iter().any(|line| line.text == "NARA_EDITOR ready")
        });
        assert_eq!(play.state(), PlayState::Running);
        assert_eq!(play.pending_control(), None);
        let stopped = play.stop(|_| {}).unwrap();
        assert_eq!(play.poll(|_| {}).unwrap(), PlayState::Exited(stopped));
    }

    #[test]
    fn stopped_game_releases_its_snapshot_without_changing_the_editor_document() {
        let mut source =
            SceneDocument::new("97270f99-fd97-483f-a605-a1e7acaea745".parse().unwrap());
        source.entities.push(SceneEntity::new(
            "e66193e0-2e80-4a44-934a-08330b93cb75".parse().unwrap(),
            "Pilot",
        ));
        let (mut play, path) = launch_snapshot_helper(&source);
        assert_eq!(play.mode(), EditorPlayMode::Snapshot);
        assert_eq!(play.state(), PlayState::Starting);

        wait_for(&mut play, |play, _| play.state == PlayState::Running);
        assert!(matches!(
            play.control(PlayControl::Step),
            Err(PlayError::ControlUnavailable(..))
        ));
        assert_eq!(play.pending_control(), None);
        play.control(PlayControl::Pause).unwrap();
        wait_until_pause_is_requested(&mut play);
        assert_eq!(play.state(), PlayState::Running);
        assert_eq!(play.pending_control(), Some(PlayControl::Pause));
        fs::write(path.with_file_name(CONTINUE_PAUSE), []).unwrap();
        wait_for(&mut play, |play, _| play.state == PlayState::Paused);
        assert_eq!(play.pending_control(), None);
        play.control(PlayControl::Step).unwrap();
        assert_eq!(play.pending_control(), Some(PlayControl::Step));
        wait_for(&mut play, |play, _| play.completed_steps() == 1);
        assert_eq!(play.state(), PlayState::Paused);
        assert_eq!(play.pending_control(), None);
        assert_eq!(
            SceneDocument::read(&path).unwrap().entities[0].name,
            "Simulated pilot"
        );
        play.control(PlayControl::Resume).unwrap();
        wait_for(&mut play, |play, _| play.state == PlayState::Running);
        play.stop(|_| {}).unwrap();
        assert!(matches!(play.poll(|_| {}).unwrap(), PlayState::Exited(_)));
        drop(play);
        assert!(!path.exists());
        assert_eq!(source.entities[0].name, "Pilot");
    }

    #[test]
    fn an_unanswered_control_clears_pending_and_ignores_its_late_acknowledgment() {
        let source = SceneDocument::new("97270f99-fd97-483f-a605-a1e7acaea745".parse().unwrap());
        let (mut play, path) = launch_snapshot_helper(&source);
        wait_for(&mut play, |play, _| play.state == PlayState::Running);
        play.control(PlayControl::Pause).unwrap();
        wait_until_pause_is_requested(&mut play);
        play.snapshot.as_mut().unwrap().reply =
            SnapshotReply::Control(PlayControl::Pause, Instant::now());

        assert!(matches!(
            play.poll(|_| {}),
            Err(PlayError::NoAcknowledgment)
        ));
        assert_eq!(play.pending_control(), None);
        assert!(matches!(
            play.control(PlayControl::Pause),
            Err(PlayError::ControlUnavailable(..))
        ));
        fs::write(path.with_file_name(CONTINUE_PAUSE), []).unwrap();
        wait_for(&mut play, |_, output| {
            output.iter().any(|line| {
                line.stream == OutputStream::Stdout && line.text == "NARA_EDITOR paused"
            })
        });
        assert_eq!(play.state(), PlayState::Running);
        play.stop(|_| {}).unwrap();
    }

    fn launch_snapshot_helper(source: &SceneDocument) -> (PlayModeSession, std::path::PathBuf) {
        let snapshot = tempfile::tempdir().unwrap();
        let path = snapshot.path().join("play.scene.ron");
        source.write(&path).unwrap();
        let process = PlayProcess::launch_controlled(
            snapshot.path(),
            &std::env::current_exe().unwrap(),
            &helper_arguments("play::tests::snapshot_game_helper"),
            ProjectTrust::Trusted,
        )
        .unwrap();
        (PlayModeSession::new(process, Some(snapshot)), path)
    }

    fn helper_arguments(test: &str) -> [OsString; 5] {
        ["--exact", test, "--ignored", "--nocapture", "--quiet"].map(OsString::from)
    }

    fn wait_until_pause_is_requested(play: &mut PlayModeSession) {
        wait_for(play, |_, output| {
            output.iter().any(|line| {
                line.stream == OutputStream::Stdout && line.text == "NARA_EDITOR running"
            }) && output.iter().any(|line| {
                line.stream == OutputStream::Stderr && line.text == "NARA_EDITOR paused"
            })
        });
    }

    fn wait_for(
        play: &mut PlayModeSession,
        ready: impl Fn(&PlayModeSession, &[OutputLine]) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = Vec::new();
        loop {
            let state = play.poll(|line| output.push(line)).unwrap();
            assert!(
                !matches!(state, PlayState::Exited(_)),
                "helper exited: {output:?}"
            );
            if ready(play, &output) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for game acknowledgment"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "launched as a contained subprocess by the editor Play test"]
    fn external_game_helper() {
        assert!(io::stdin().lock().lines().next().is_none());
        println!("NARA_EDITOR ready");
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    #[ignore = "launched as a contained subprocess by the editor Play test"]
    fn snapshot_game_helper() {
        let path = Path::new("play.scene.ron");
        let mut simulated = SceneDocument::read(path).unwrap();
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "NARA_EDITOR ready").unwrap();
        stdout.flush().unwrap();
        for line in io::stdin().lock().lines() {
            let event = match line.unwrap().as_str() {
                "pause" => {
                    writeln!(stdout, "NARA_EDITOR running").unwrap();
                    stdout.flush().unwrap();
                    eprintln!("NARA_EDITOR paused");
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !Path::new(CONTINUE_PAUSE).is_file() {
                        assert!(Instant::now() < deadline, "the test did not release Pause");
                        thread::sleep(Duration::from_millis(10));
                    }
                    "paused"
                }
                "resume" => "running",
                "step" => {
                    simulated.entities[0].name = "Simulated pilot".to_owned();
                    simulated.write(path).unwrap();
                    "stepped"
                }
                other => panic!("unexpected input: {other}"),
            };
            writeln!(stdout, "NARA_EDITOR {event}").unwrap();
            stdout.flush().unwrap();
        }
    }
}
