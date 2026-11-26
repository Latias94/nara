use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use nara_core::{Diagnostic, DiagnosticSeverity};
use nara_editor_core::play::{PlayControl, PlayError, PlayModeSession, PlayState};
use nara_project::{
    EditorPlayMode,
    play::{BuildError, BuildEvent, BuildRequest, OutputLine, OutputStream},
    workflow::{
        CargoPackage, ProjectTrust, SCHEMA_BINARY, SchemaCacheError, refresh_schema_interruptible,
    },
};
use nara_reflect::ComponentSchemaCatalog;
use nara_scene::authoring::SceneDocument;

#[derive(Debug)]
pub enum Message {
    Log(Diagnostic),
    Schema(ComponentSchemaCatalog),
    Play {
        mode: EditorPlayMode,
        state: PlayState,
        pending: Option<PlayControl>,
        steps: u64,
    },
    PlayControlResult {
        control: PlayControl,
        result: Result<(), String>,
    },
    Finished(Result<(), String>),
}

pub struct Job {
    pub messages: Receiver<Message>,
    controls: Sender<PlayControl>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Job {
    pub fn refresh(project: PathBuf, trust: ProjectTrust) -> io::Result<Self> {
        Self::spawn(move |cancel, _, messages| {
            let package = CargoPackage::read(&project)?;
            let features = package
                .required_features(SCHEMA_BINARY)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let request = BuildRequest {
                features: &features,
                ..BuildRequest::new(&project, package.name(), SCHEMA_BINARY)
            };
            let schema = match refresh_schema_interruptible(
                &request,
                trust,
                || cancel.load(Ordering::Relaxed),
                |event| build_message(messages, event),
            ) {
                Err(
                    SchemaCacheError::Interrupted
                    | SchemaCacheError::Build(BuildError::Interrupted),
                ) => return Ok(()),
                result => result?,
            };
            let _ = messages.send(Message::Schema(schema));
            Ok(())
        })
    }

    pub fn play(
        project: PathBuf,
        document: SceneDocument,
        trust: ProjectTrust,
    ) -> io::Result<Self> {
        Self::spawn(move |cancel, controls, messages| {
            let mut play = match PlayModeSession::start(
                &project,
                &document,
                trust,
                || cancel.load(Ordering::Relaxed),
                |event| build_message(messages, event),
            ) {
                Err(PlayError::Cancelled) => return Ok(()),
                result => result?,
            };
            let mode = play.mode();
            let mut previous = None;
            let result = loop {
                if cancel.load(Ordering::Relaxed) {
                    break Ok(());
                }
                let delivered =
                    deliver_controls(&controls, messages, |control| play.control(control));
                let state = match play.poll(|line| process_line(messages, line)) {
                    Ok(state) => state,
                    Err(error) => break Err(error.into()),
                };
                let now = (state, play.pending_control(), play.completed_steps());
                if delivered || previous != Some(now) {
                    let _ = messages.send(Message::Play {
                        mode,
                        state,
                        pending: now.1,
                        steps: now.2,
                    });
                    previous = Some(now);
                }
                if let PlayState::Exited(status) = state {
                    break if status.success() {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("Game exited with {status}"))
                    };
                }
                thread::sleep(Duration::from_millis(16));
            };
            if !matches!(play.state(), PlayState::Exited(_)) {
                play.stop(|line| process_line(messages, line))?;
            }
            result
        })
    }

    fn spawn(
        work: impl FnOnce(&AtomicBool, Receiver<PlayControl>, &Sender<Message>) -> anyhow::Result<()>
        + Send
        + 'static,
    ) -> io::Result<Self> {
        let (messages, output) = mpsc::channel();
        let (controls, input) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        let worker = thread::Builder::new()
            .name("nara-editor-project".into())
            .spawn(move || {
                let result = work(&token, input, &messages).map_err(|error| format!("{error:#}"));
                let _ = messages.send(Message::Finished(result));
            })?;
        Ok(Self {
            messages: output,
            controls,
            cancel,
            thread: Some(worker),
        })
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn control(&self, control: PlayControl) -> io::Result<()> {
        self.controls
            .send(control)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the Play worker has stopped"))
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

fn deliver_controls(
    controls: &Receiver<PlayControl>,
    messages: &Sender<Message>,
    mut deliver: impl FnMut(PlayControl) -> Result<(), PlayError>,
) -> bool {
    let mut delivered = false;
    for control in controls.try_iter() {
        let result = deliver(control).map_err(|error| error.to_string());
        delivered |= result.is_ok();
        let _ = messages.send(Message::PlayControlResult { control, result });
    }
    delivered
}

fn build_message(messages: &Sender<Message>, event: BuildEvent) {
    match event {
        BuildEvent::Output(line) => process_line(messages, line),
        BuildEvent::Diagnostic { diagnostic, .. } => {
            let _ = messages.send(Message::Log(diagnostic.diagnostic()));
        }
    }
}

fn process_line(messages: &Sender<Message>, line: OutputLine) {
    let severity = match line.stream {
        OutputStream::Stdout => DiagnosticSeverity::Info,
        OutputStream::Stderr => DiagnosticSeverity::Warning,
    };
    log(messages, severity, line.text);
}

fn log(messages: &Sender<Message>, severity: DiagnosticSeverity, message: String) {
    let _ = messages.send(Message::Log(Diagnostic {
        severity,
        message,
        code: None,
        location: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_delivery_reports_rejections_even_without_play_progress() {
        let (input, controls) = mpsc::channel();
        let (messages, output) = mpsc::channel();
        input.send(PlayControl::Step).unwrap();
        let delivered = deliver_controls(&controls, &messages, |control| {
            Err(PlayError::ControlUnavailable(control, PlayState::Running))
        });
        assert!(!delivered);
        assert!(matches!(
            output.try_recv().unwrap(),
            Message::PlayControlResult {
                control: PlayControl::Step,
                result: Err(_),
            }
        ));

        input.send(PlayControl::Pause).unwrap();
        assert!(deliver_controls(&controls, &messages, |_| Ok(())));
        assert!(matches!(
            output.try_recv().unwrap(),
            Message::PlayControlResult {
                control: PlayControl::Pause,
                result: Ok(()),
            }
        ));
    }

    #[test]
    fn control_queue_reports_a_disconnected_worker() {
        let timeout = Duration::from_secs(5);
        let job = Job::spawn(move |_, controls, _| {
            assert_eq!(controls.recv_timeout(timeout)?, PlayControl::Pause);
            Ok(())
        })
        .unwrap();
        job.control(PlayControl::Pause).unwrap();
        assert!(matches!(
            job.messages.recv_timeout(timeout).unwrap(),
            Message::Finished(Ok(()))
        ));
        assert_eq!(
            job.control(PlayControl::Pause).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
