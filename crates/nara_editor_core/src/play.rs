//! External Play of an isolated snapshot of the authored scene.

use std::{
    ffi::OsString,
    io,
    path::Path,
    process::ExitStatus,
    time::{Duration, Instant},
};

use nara_project::{
    play::{
        BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream, PlayProcess,
        build_interruptible,
    },
    workflow::{CargoPackage, ProjectCheckError, ProjectTrust},
};
use nara_scene::authoring::{SceneDocument, SceneDocumentError};
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
    Io(#[from] io::Error),
    #[error("launching a game requires a trusted project")]
    Untrusted,
    #[error("Play was cancelled")]
    Cancelled,
    #[error("{0:?} is unavailable while Play is {1:?} or awaiting an acknowledgment")]
    ControlUnavailable(PlayControl, PlayState),
    #[error("the game did not acknowledge the editor control")]
    NoAcknowledgment,
}

/// Owns a supervised game and its temporary authoring snapshot. The editor keeps its own
/// document and history while the game opens the snapshot in a separate process.
pub struct PlayModeSession {
    process: PlayProcess,
    _snapshot: TempDir,
    state: PlayState,
    pending: Option<PlayControl>,
    deadline: Option<Instant>,
    completed_steps: u64,
}

impl PlayModeSession {
    /// Incrementally builds the project's default binary, then starts it with `--editor-play`.
    /// Call on a background worker. `poll` completes startup and delivers game output.
    pub fn start(
        project: &Path,
        scene: &SceneDocument,
        trust: ProjectTrust,
        mut cancel: impl FnMut() -> bool,
        emit: impl FnMut(BuildEvent),
    ) -> Result<Self, PlayError> {
        if trust != ProjectTrust::Trusted {
            return Err(PlayError::Untrusted);
        }
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
        Self::launch(project, &executable, scene, trust)
    }

    fn launch(
        project: &Path,
        executable: &Path,
        scene: &SceneDocument,
        trust: ProjectTrust,
    ) -> Result<Self, PlayError> {
        let snapshot = tempfile::Builder::new()
            .prefix("nara-editor-play-")
            .tempdir()?;
        let path = snapshot.path().join("play.nara.scene.ron");
        scene.write(&path)?;
        let arguments = [OsString::from("--editor-play"), path.into_os_string()];
        let process = PlayProcess::launch_controlled(project, executable, &arguments, trust)?;
        Ok(Self::starting(process, snapshot))
    }

    fn starting(process: PlayProcess, snapshot: TempDir) -> Self {
        Self {
            process,
            _snapshot: snapshot,
            state: PlayState::Starting,
            pending: None,
            deadline: Some(Instant::now() + RESPONSE_TIMEOUT),
            completed_steps: 0,
        }
    }

    pub fn state(&self) -> PlayState {
        self.state
    }

    pub fn pending_control(&self) -> Option<PlayControl> {
        self.pending
    }

    pub fn completed_steps(&self) -> u64 {
        self.completed_steps
    }

    pub fn control(&mut self, control: PlayControl) -> Result<(), PlayError> {
        if self.pending.is_some()
            || !matches!(
                (self.state, control),
                (PlayState::Running, PlayControl::Pause)
                    | (PlayState::Paused, PlayControl::Resume | PlayControl::Step)
            )
        {
            return Err(PlayError::ControlUnavailable(control, self.state));
        }
        self.process.send_control(control)?;
        self.pending = Some(control);
        self.deadline = Some(Instant::now() + RESPONSE_TIMEOUT);
        Ok(())
    }

    pub fn poll(&mut self, mut emit: impl FnMut(OutputLine)) -> Result<PlayState, PlayError> {
        let mut replies = Vec::new();
        let status = self.process.poll(|line| {
            if line.stream == OutputStream::Stdout && line.text.starts_with("NARA_EDITOR ") {
                replies.push(line);
            } else {
                emit(line);
            }
        })?;
        for line in replies {
            let state = match (self.state, self.pending, line.text.as_str()) {
                (PlayState::Starting, None, "NARA_EDITOR ready") => Some(PlayState::Running),
                (_, Some(PlayControl::Pause), "NARA_EDITOR paused") => Some(PlayState::Paused),
                (_, Some(PlayControl::Resume), "NARA_EDITOR running") => Some(PlayState::Running),
                (_, Some(PlayControl::Step), "NARA_EDITOR stepped") => {
                    self.completed_steps = self.completed_steps.saturating_add(1);
                    Some(PlayState::Paused)
                }
                _ => None,
            };
            if let Some(state) = state {
                self.state = state;
                self.pending = None;
                self.deadline = None;
            } else {
                emit(line);
            }
        }
        if let Some(status) = status {
            self.set_exited(status);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(PlayError::NoAcknowledgment);
        }
        Ok(self.state)
    }

    /// Terminates and reaps the complete game process tree.
    pub fn stop(&mut self, emit: impl FnMut(OutputLine)) -> Result<ExitStatus, PlayError> {
        let status = self.process.stop(emit)?;
        self.set_exited(status);
        Ok(status)
    }

    fn set_exited(&mut self, status: ExitStatus) {
        self.state = PlayState::Exited(status);
        self.pending = None;
        self.deadline = None;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, Write},
        thread,
    };

    use nara_scene::authoring::SceneEntity;

    use super::*;

    #[test]
    fn stopped_game_releases_its_snapshot_without_changing_the_editor_document() {
        let mut source =
            SceneDocument::new("97270f99-fd97-483f-a605-a1e7acaea745".parse().unwrap());
        source.entities.push(SceneEntity::new(
            "e66193e0-2e80-4a44-934a-08330b93cb75".parse().unwrap(),
            "Pilot",
        ));
        let snapshot = tempfile::tempdir().unwrap();
        let path = snapshot.path().join("play.nara.scene.ron");
        source.write(&path).unwrap();
        let arguments = [
            "--exact",
            "play::tests::game_helper",
            "--ignored",
            "--nocapture",
            "--quiet",
        ]
        .map(OsString::from);
        let process = PlayProcess::launch_controlled(
            snapshot.path(),
            &std::env::current_exe().unwrap(),
            &arguments,
            ProjectTrust::Trusted,
        )
        .unwrap();
        let mut play = PlayModeSession::starting(process, snapshot);

        wait_for(&mut play, |play| play.state == PlayState::Running);
        assert!(matches!(
            play.control(PlayControl::Step),
            Err(PlayError::ControlUnavailable(..))
        ));
        play.control(PlayControl::Pause).unwrap();
        wait_for(&mut play, |play| play.state == PlayState::Paused);
        play.control(PlayControl::Step).unwrap();
        wait_for(&mut play, |play| play.completed_steps == 1);
        assert_eq!(play.state(), PlayState::Paused);
        assert_eq!(
            SceneDocument::read(&path).unwrap().entities[0].name,
            "Simulated pilot"
        );
        play.control(PlayControl::Resume).unwrap();
        wait_for(&mut play, |play| play.state == PlayState::Running);
        play.stop(|_| {}).unwrap();
        assert!(matches!(play.poll(|_| {}).unwrap(), PlayState::Exited(_)));
        drop(play);
        assert!(!path.exists());
        assert_eq!(source.entities[0].name, "Pilot");
    }

    fn wait_for(play: &mut PlayModeSession, ready: impl Fn(&PlayModeSession) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut output = Vec::new();
            let state = play.poll(|line| output.push(line)).unwrap();
            assert!(
                !matches!(state, PlayState::Exited(_)),
                "helper exited: {output:?}"
            );
            if ready(play) {
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
    fn game_helper() {
        let path = Path::new("play.nara.scene.ron");
        let mut simulated = SceneDocument::read(path).unwrap();
        assert_eq!(simulated.entities[0].name, "Pilot");
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "NARA_EDITOR ready").unwrap();
        stdout.flush().unwrap();
        for line in io::stdin().lock().lines() {
            let event = match line.unwrap().as_str() {
                "pause" => "paused",
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
