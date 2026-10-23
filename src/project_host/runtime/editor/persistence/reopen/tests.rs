use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::ComponentRegistry;
use nara_scene::{SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord};
use nara_tasks::TaskPoolConfig;
use nara_tooling::EditorWorkspace;

use super::*;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct ReopenFixture {
    directory: PathBuf,
    original: Vec<u8>,
    host: RetainedHost<ScenePersistenceHost>,
    registry: ComponentRegistry,
    workspace: EditorWorkspace,
    document: EditorDocumentId,
}

impl ReopenFixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "nara_background_reopen_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let original = scene("persisted-entity")
            .to_json_string()
            .unwrap()
            .into_bytes();
        fs::write(directory.join("current.scene.json"), &original).unwrap();
        let directory_capability = DirectoryCapability::from_host_handle(
            open_directory(&directory),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let host =
            ScenePersistenceHost::open(&directory_capability, "current.scene.json", &registry)
                .unwrap()
                .host;
        let mut workspace = EditorWorkspace::new();
        let document = workspace
            .open_scene_session(
                "current.scene.json",
                SceneAuthoringSession::new(scene("working-entity")),
            )
            .unwrap()
            .opened_document
            .unwrap();
        Self {
            directory,
            original,
            host: HostCustody::default().retain_scene(host).unwrap(),
            registry,
            workspace,
            document,
        }
    }

    fn start(&self, pools: &TaskPools, gate: Option<ReopenTestGate>) -> BackgroundReopen {
        let slot = self.workspace.scene(self.document).unwrap();
        BackgroundReopen::start(
            PendingReopen {
                admission: ReopenAdmission {
                    lane: PersistenceLane::Scene(EditorScenePersistenceLane {
                        project_path: Arc::from("scenes/current.scene.json"),
                        document: self.document,
                        host: self.host.clone(),
                    }),
                    document: self.document,
                    kind: EditorDocumentKind::Scene,
                    revision: slot.revision(),
                    binding: None,
                    project_path: Arc::from("scenes/current.scene.json"),
                    schema: None,
                },
                request: ReopenRequest::Scene(
                    self.host
                        .try_with(|host| host.prepare_reopen(None))
                        .unwrap()
                        .unwrap(),
                ),
            },
            pools,
            gate,
        )
    }

    fn assert_owner_untouched(&self) {
        assert_eq!(
            self.workspace
                .scene(self.document)
                .unwrap()
                .session()
                .document(),
            &scene("working-entity")
        );
        assert_eq!(
            fs::read(self.directory.join("current.scene.json")).unwrap(),
            self.original
        );
    }
}

impl Drop for ReopenFixture {
    fn drop(&mut self) {
        let directory = self.directory.canonicalize().unwrap();
        assert!(directory.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            directory
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_background_reopen_")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

fn scene(entity: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(SceneEntityId::new(entity).unwrap())])
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

#[test]
fn closed_pool_returns_unavailable_without_reading_or_publishing() {
    let fixture = ReopenFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
    let mut reopen = fixture.start(&pools, None);
    assert!(reopen.task.is_none());
    assert!(matches!(reopen.poll(), Some(ReopenCompletion::Unavailable)));
    assert_eq!(pools.run_pending_for_tests().executed, 0);
    fixture.assert_owner_untouched();
}

#[test]
fn queued_logical_cancellation_never_executes_the_read_request() {
    let fixture = ReopenFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut reopen = fixture.start(&pools, None);
    assert!(reopen.task.as_ref().unwrap().cancel());
    assert!(matches!(reopen.poll(), Some(ReopenCompletion::Unavailable)));
    let executed = pools.run_pending_for_tests();
    assert_eq!(executed.executed, 0);
    assert_eq!(executed.cancelled_before_start, 1);
    fixture.assert_owner_untouched();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn running_logical_cancellation_waits_for_physical_return_and_discards_prepared_bytes() {
    let fixture = ReopenFixture::new();
    let pools = Arc::new(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap());
    let (started_tx, started) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let gate = ReopenTestGate {
        phase: ReopenTestPhase::AfterPreparation,
        started: started_tx,
        release: release_rx,
    };
    let control = ReopenTestControl {
        started,
        release: Some(release),
    };
    let mut reopen = fixture.start(&pools, Some(gate));
    let runner_pools = Arc::clone(&pools);
    let runner = std::thread::spawn(move || runner_pools.run_pending_for_tests());
    assert!(control.wait_until_started());
    assert!(reopen.task.as_ref().unwrap().cancel());
    assert!(reopen.task.as_ref().unwrap().is_finished());
    assert!(
        reopen.poll().is_none(),
        "logical cancellation does not release preparation custody"
    );
    fixture.assert_owner_untouched();
    control.release();
    assert_eq!(runner.join().unwrap().executed, 1);
    assert!(matches!(reopen.poll(), Some(ReopenCompletion::Unavailable)));
    fixture.assert_owner_untouched();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn worker_panic_after_preparation_discards_data_without_replacing_owner_state() {
    let fixture = ReopenFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let (started, _) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    // A disconnected test barrier unwinds after preparing bytes, while holding the real guard.
    drop(release);
    let gate = ReopenTestGate {
        phase: ReopenTestPhase::AfterPreparation,
        started,
        release: release_rx,
    };
    let mut reopen = fixture.start(&pools, Some(gate));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(reopen.work.is_poisoned());
    assert!(
        reopen
            .work
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .unwrap()
            .prepared
            .is_some()
    );
    assert!(matches!(reopen.poll(), Some(ReopenCompletion::Unavailable)));
    fixture.assert_owner_untouched();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn completed_preparation_can_be_admitted_only_by_the_owner() {
    let fixture = ReopenFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut reopen = fixture.start(&pools, None);
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    let Some(ReopenCompletion::Scene(prepared)) = reopen.poll() else {
        panic!("a completed read must return its candidate")
    };
    let SceneReopenOutcome::Opened(opened) = prepared.admit(Some(&fixture.registry)) else {
        panic!("the candidate must admit on the owner")
    };
    assert_eq!(opened.session.document(), &scene("persisted-entity"));
    fixture.assert_owner_untouched();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}
