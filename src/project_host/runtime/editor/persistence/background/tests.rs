use std::{
    fs::{self, File},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_reflect::ComponentRegistry;
use nara_scene::{SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord};
use nara_tasks::{TaskPoolConfig, TaskTerminal};
use nara_tooling::{EditorPersistenceAuthority, EditorWorkspace};

use super::*;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct SaveFixture {
    directory: PathBuf,
    original: Vec<u8>,
    workspace: EditorWorkspace,
    authority: EditorPersistenceAuthority,
    document: EditorDocumentId,
}

impl SaveFixture {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "nara_background_save_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir(&directory).unwrap();
        let original = SceneDocument::default()
            .to_json_string()
            .unwrap()
            .into_bytes();
        fs::write(directory.join("current.scene.json"), &original).unwrap();
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("captured-entity").unwrap(),
        )]);
        let (mut workspace, authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_scene_session("current.scene.json", SceneAuthoringSession::new(scene))
            .unwrap()
            .opened_document
            .unwrap();
        Self {
            directory,
            original,
            workspace,
            authority,
            document,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        fs::read(self.directory.join("current.scene.json")).unwrap()
    }

    fn candidate(&mut self) -> SceneSaveCandidate {
        SceneSaveCandidate {
            checkpoint: self
                .authority
                .capture(&self.workspace, self.document)
                .unwrap(),
        }
    }

    fn start(
        &mut self,
        pools: &TaskPools,
        custody: &HostCustody,
        gate: Option<SaveTestGate>,
    ) -> BackgroundSave {
        let directory = DirectoryCapability::from_host_handle(
            open_directory(&self.directory),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let opened =
            ScenePersistenceHost::open(&directory, "current.scene.json", &registry).unwrap();
        let lane = SaveLane::Scene(EditorScenePersistenceLane {
            project_path: Arc::from("scenes/current.scene.json"),
            document: self.document,
            host: custody.retain_scene(opened.host).unwrap(),
        });
        BackgroundSave::start(
            lane,
            PendingSave::Scene(self.candidate()),
            None,
            pools,
            gate,
        )
    }

    fn assert_retry_can_publish(&mut self, lane: SaveLane) {
        let SaveLane::Scene(lane) = lane else {
            panic!("the fixture owns a Scene Host")
        };
        assert!(matches!(
            lane.host
                .try_with(|host| host.save_owned(self.candidate(), None))
                .unwrap(),
            PersistenceSaveOutcome::Saved { .. }
        ));
        assert!(
            String::from_utf8(self.bytes())
                .unwrap()
                .contains("captured-entity")
        );
    }
}

impl Drop for SaveFixture {
    fn drop(&mut self) {
        let directory = self.directory.canonicalize().unwrap();
        assert!(directory.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            directory
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_background_save_")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

fn open_directory(path: &std::path::Path) -> File {
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
fn pool_rejection_returns_the_unchanged_host_without_reconciliation() {
    let mut fixture = SaveFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    assert!(pools.shutdown_blocking().unwrap().is_complete());
    let custody = HostCustody::default();
    let mut save = fixture.start(&pools, &custody, None);
    assert!(save.task.is_none());
    let (lane, outcome) = save
        .poll()
        .expect("a rejected task retains its original Host");
    assert!(matches!(
        outcome,
        PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::Busy)
    ));
    assert_eq!(lane.document(), fixture.document);
    assert_eq!(fixture.bytes(), fixture.original);
    fixture.assert_retry_can_publish(lane);
}

#[test]
fn queued_logical_cancellation_returns_the_host_without_publishing() {
    let mut fixture = SaveFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let custody = HostCustody::default();
    let mut save = fixture.start(&pools, &custody, None);
    assert!(save.task.as_ref().unwrap().cancel());
    let (lane, outcome) = save
        .poll()
        .expect("cancelled queued work must return its Host");
    assert!(matches!(
        outcome,
        PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::Busy)
    ));
    let execution = pools.run_pending_for_tests();
    assert_eq!(execution.executed, 0);
    assert_eq!(execution.cancelled_before_start, 1);
    assert_eq!(fixture.bytes(), fixture.original);
    fixture.assert_retry_can_publish(lane);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn running_logical_cancellation_waits_for_the_host_and_preserves_the_published_outcome() {
    let mut fixture = SaveFixture::new();
    let pools = Arc::new(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap());
    let custody = HostCustody::default();
    let (gate, control) = SaveTestGate::new(SaveTestPhase::AfterPublication);
    let mut save = fixture.start(&pools, &custody, Some(gate));
    let runner_pools = Arc::clone(&pools);
    let runner = std::thread::spawn(move || runner_pools.run_pending_for_tests());
    assert!(control.wait_until_started());
    let published = fixture.bytes();
    assert!(String::from_utf8_lossy(&published).contains("captured-entity"));
    assert!(save.task.as_ref().unwrap().cancel());
    assert!(save.task.as_ref().unwrap().is_finished());
    assert!(
        save.poll().is_none(),
        "a logical terminal cannot release a borrowed file Host"
    );

    control.release();
    assert_eq!(runner.join().unwrap().executed, 1);
    assert!(matches!(
        save.task.as_mut().unwrap().try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    let (lane, outcome) = save
        .poll()
        .expect("the worker returned its physical file Host");
    let PersistenceSaveOutcome::Saved { commit, evidence } = outcome else {
        panic!("the actual published outcome must survive logical cancellation")
    };
    assert_eq!(lane.document(), fixture.document);
    assert_eq!(evidence.document(), fixture.document);
    assert!(
        fixture
            .authority
            .commit(&mut fixture.workspace, commit)
            .applied
    );
    assert_eq!(fixture.bytes(), published);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn worker_panic_recovers_the_poisoned_host_and_blocks_blind_retry() {
    let mut fixture = SaveFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let custody = HostCustody::default();
    let mut save = fixture.start(&pools, &custody, None);
    // Break the worker's one-shot request invariant to unwind through its actual mutex guard.
    save.work.lock().unwrap().as_mut().unwrap().candidate = None;
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(save.work.is_poisoned());
    assert!(matches!(
        save.task.as_mut().unwrap().try_take(),
        Some(TaskTerminal::Failed(_))
    ));
    let (lane, outcome) = save
        .poll()
        .expect("panic must not destroy the only file Host");
    assert!(matches!(
        outcome,
        PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
    ));
    let rejection = EditorPersistenceResult::Rejected {
        request: EditorPersistenceRequestKind::Save,
        document: Some(fixture.document),
        reason: EditorPersistenceRejection::RequiresReconcile,
    };
    assert!(!super::super::persistence_result_allows_retry(
        rejection,
        EditorPersistenceRequestKind::Save,
        fixture.document,
    ));
    assert!(super::super::persistence_result_allows_retry(
        rejection,
        EditorPersistenceRequestKind::Reopen,
        fixture.document,
    ));
    assert!(!super::super::persistence_result_allows_retry(
        rejection,
        EditorPersistenceRequestKind::Reopen,
        EditorDocumentId::from_raw(fixture.document.raw() + 1),
    ));
    let SaveLane::Scene(lane) = lane else {
        panic!("the fixture owns a Scene Host")
    };
    assert!(matches!(
        lane.host
            .try_with(|host| host.save_owned(fixture.candidate(), None))
            .unwrap(),
        PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
    ));
    assert_eq!(fixture.bytes(), fixture.original);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn queued_save_rejects_when_its_original_host_retires_before_execution() {
    let mut fixture = SaveFixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let custody = HostCustody::default();
    let mut save = fixture.start(&pools, &custody, None);
    let host = {
        let work = save.work.lock().unwrap();
        let SaveLane::Scene(lane) = &work.as_ref().unwrap().lane else {
            panic!("the fixture owns a Scene Host")
        };
        lane.host.clone()
    };
    custody.close_admission();
    assert!(host.retire());

    assert_eq!(pools.run_pending_for_tests().executed, 1);
    let (lane, outcome) = save
        .poll()
        .expect("a queued Save must finish after its Host retires");
    assert_eq!(lane.document(), fixture.document);
    assert!(matches!(
        outcome,
        PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::Busy)
    ));
    assert!(
        host.try_with(|_| ()).is_none(),
        "a retired Host cannot regain file authority"
    );
    assert_eq!(fixture.bytes(), fixture.original);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}
