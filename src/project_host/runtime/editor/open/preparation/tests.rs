use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{atomic::AtomicU64, mpsc},
    time::{Duration, Instant},
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::SceneDocument;

use super::super::{
    EditorProjectOpenAttempt, EditorProjectOpenAttemptState, EditorProjectOpenFailure,
    EditorProjectOpenFailureImpact, EditorProjectOpenFailureSource, EditorProjectOpenHost,
    EditorProjectOpenPhase, EditorProjectOpenProgress,
};
use super::*;

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);

struct StartupProject {
    root: PathBuf,
}

impl StartupProject {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_owned_startup_{}_{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1
[project]
name = "Owned Startup"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
"#,
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            AuthoringSchemaBundle::new([])
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn ready(&self, host: &mut EditorProjectOpenHost) -> EditorProjectOpenAttempt {
        let capability = DirectoryCapability::from_host_handle(
            open_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap();
        let attempt = host.begin(capability, None);
        let attempt = expect_pending(host.advance(attempt));
        assert_eq!(
            attempt.phase(),
            EditorProjectOpenPhase::AuthoringSchemaBundle
        );
        let attempt = expect_pending(host.advance(attempt));
        assert_eq!(attempt.phase(), EditorProjectOpenPhase::StartupClosure);
        attempt
    }
}

impl Drop for StartupProject {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_owned_startup_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn open_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    }
    #[cfg(target_os = "linux")]
    {
        File::open(path).unwrap()
    }
}

struct GateControl {
    started: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl GateControl {
    fn install(
        host: &mut EditorProjectOpenHost,
        phase: StartupTestPhase,
        panic_after_release: bool,
    ) -> Self {
        let (started_tx, started) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        host.preparation.gate = Some(StartupTestGate {
            phase,
            started: started_tx,
            release: release_rx,
            panic_after_release,
        });
        Self {
            started,
            release: Some(release),
        }
    }

    fn wait(&self) {
        self.started.recv_timeout(Duration::from_secs(5)).unwrap();
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for GateControl {
    fn drop(&mut self) {
        self.release();
    }
}

fn expect_pending(progress: EditorProjectOpenProgress) -> EditorProjectOpenAttempt {
    match progress {
        EditorProjectOpenProgress::Pending(attempt) => attempt,
        EditorProjectOpenProgress::Rejected(failure) => {
            panic!("unexpected startup rejection: {failure:?}")
        }
        EditorProjectOpenProgress::Opened(_) => {
            panic!("startup preparation published a session early")
        }
    }
}

fn drive_startup(
    host: &mut EditorProjectOpenHost,
    mut attempt: EditorProjectOpenAttempt,
) -> EditorProjectOpenProgress {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = host.advance(attempt);
        match progress {
            EditorProjectOpenProgress::Pending(next)
                if next.phase() == EditorProjectOpenPhase::StartupClosure =>
            {
                assert!(
                    Instant::now() < deadline,
                    "startup worker did not terminate"
                );
                attempt = next;
                std::thread::sleep(Duration::from_millis(1));
            }
            terminal => return terminal,
        }
    }
}

fn wait_retired(active: &AtomicUsize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while active.load(Ordering::Acquire) != 0 {
        assert!(
            Instant::now() < deadline,
            "startup captures did not physically retire"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_unavailable(progress: EditorProjectOpenProgress) {
    let EditorProjectOpenProgress::Rejected(failure) = progress else {
        panic!("unavailable startup work gained publication authority");
    };
    assert_safe_unavailable(&failure);
}

fn assert_safe_unavailable(failure: &EditorProjectOpenFailure) {
    assert_eq!(failure.phase(), EditorProjectOpenPhase::StartupClosure);
    assert_eq!(
        failure.source(),
        EditorProjectOpenFailureSource::ProjectSource
    );
    assert_eq!(
        failure.impact(),
        EditorProjectOpenFailureImpact::CandidateNotPublished
    );
    assert!(failure.relative_source().is_none());
    assert_eq!(
        failure.diagnostics().iter().next().unwrap().code().as_str(),
        "project.editor.startup-preparation-unavailable"
    );
}

#[test]
fn held_real_worker_keeps_owner_poll_pending_and_attempt_transportable() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    assert!(host.preparation.pools.is_none());
    let ready = project.ready(&mut host);
    assert!(host.preparation.pools.is_none());
    let mut gate = GateControl::install(&mut host, StartupTestPhase::BeforeRead, false);
    let attempt = expect_pending(host.advance(ready));
    gate.wait();
    let mut foreign_host = EditorProjectOpenHost::new();
    // The worker is still held: this owner call must return without waiting for its read.
    let attempt = expect_pending(foreign_host.advance(attempt));
    assert_eq!(attempt.phase(), EditorProjectOpenPhase::StartupClosure);
    assert!(foreign_host.preparation.pools.is_none());
    gate.release();
    let attempt = expect_pending(drive_startup(&mut foreign_host, attempt));
    assert_eq!(attempt.phase(), EditorProjectOpenPhase::SessionPublication);
    let EditorProjectOpenAttemptState::SessionPublication(receipt) = attempt.state else {
        unreachable!()
    };
    assert_eq!(receipt.snapshot().source_facts().len(), 1);
    wait_retired(&host.preparation.active);
}

#[test]
fn dropped_attempt_and_host_return_before_running_capture_retires() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    let ready = project.ready(&mut host);
    let mut gate = GateControl::install(&mut host, StartupTestPhase::AfterPreparation, false);
    let attempt = expect_pending(host.advance(ready));
    gate.wait();
    let active = Arc::clone(&host.preparation.active);
    let EditorProjectOpenAttemptState::StartupPreparing(ref preparation) = attempt.state else {
        unreachable!()
    };
    let cancellation = preparation.task.cancellation_token();
    drop(attempt);
    assert!(cancellation.is_cancelled());
    drop(host);
    assert_eq!(active.load(Ordering::Acquire), 1);
    gate.release();
    wait_retired(&active);
}

#[test]
fn cancelled_physical_captures_keep_admission_bounded_until_worker_return() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    let ready = project.ready(&mut host);
    let mut gate = GateControl::install(&mut host, StartupTestPhase::AfterPreparation, false);
    let running = expect_pending(host.advance(ready));
    gate.wait();
    let mut attempts = vec![running];
    for _ in 0..STARTUP_PENDING_LIMIT {
        let ready = project.ready(&mut host);
        attempts.push(expect_pending(host.advance(ready)));
    }
    assert_eq!(
        host.preparation.active.load(Ordering::Acquire),
        STARTUP_CAPTURE_LIMIT
    );
    for attempt in &attempts {
        let EditorProjectOpenAttemptState::StartupPreparing(preparation) = &attempt.state else {
            unreachable!()
        };
        assert!(preparation.task.cancel());
        assert!(preparation.task.is_finished());
    }
    assert_unavailable(host.advance(attempts.remove(0)));
    assert_eq!(
        host.preparation.active.load(Ordering::Acquire),
        STARTUP_CAPTURE_LIMIT
    );
    let ready = project.ready(&mut host);
    assert_unavailable(host.advance(ready));
    gate.release();
    wait_retired(&host.preparation.active);
    for attempt in attempts {
        assert_unavailable(host.advance(attempt));
    }
    let ready = project.ready(&mut host);
    let attempt = expect_pending(host.advance(ready));
    assert_eq!(
        expect_pending(drive_startup(&mut host, attempt)).phase(),
        EditorProjectOpenPhase::SessionPublication
    );
}

#[test]
fn queued_cancellation_never_executes_startup_preparation() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    host.preparation.pools =
        Some(TaskPools::inline_for_tests(startup_pool_config().unwrap()).unwrap());
    let ready = project.ready(&mut host);
    let attempt = expect_pending(host.advance(ready));
    let EditorProjectOpenAttemptState::StartupPreparing(ref preparation) = attempt.state else {
        unreachable!()
    };
    assert!(preparation.task.cancel());
    assert_unavailable(host.advance(attempt));
    assert_eq!(host.preparation.active.load(Ordering::Acquire), 1);
    let report = host
        .preparation
        .pools
        .as_ref()
        .unwrap()
        .run_pending_for_tests();
    assert_eq!(report.executed, 0);
    assert_eq!(report.cancelled_before_start, 1);
    assert_eq!(host.preparation.active.load(Ordering::Acquire), 0);
}

#[test]
fn closed_pool_rejection_is_safe_and_releases_the_capture() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    let pools = TaskPools::inline_for_tests(startup_pool_config().unwrap()).unwrap();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
    host.preparation.pools = Some(pools);
    let ready = project.ready(&mut host);
    assert_unavailable(host.advance(ready));
    assert_eq!(host.preparation.active.load(Ordering::Acquire), 0);
}

#[test]
fn worker_panic_returns_safe_startup_failure_without_a_candidate() {
    let project = StartupProject::new();
    let mut host = EditorProjectOpenHost::new();
    let ready = project.ready(&mut host);
    let mut gate = GateControl::install(&mut host, StartupTestPhase::BeforeRead, true);
    let attempt = expect_pending(host.advance(ready));
    gate.wait();
    gate.release();
    assert_unavailable(drive_startup(&mut host, attempt));
    wait_retired(&host.preparation.active);
}
