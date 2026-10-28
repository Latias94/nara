use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use nara_asset::{AssetImportError, AssetImportLimits, AssetImportSource, StableAssetId};
use nara_core::ByteLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_tasks::TaskPoolConfig;

use super::*;
use crate::project_host::runtime::editor::persistence::HostCustody;

const WAIT: Duration = Duration::from_secs(5);
const SOURCE: &str = "source.txt";
const METADATA: &str = "source.txt.meta";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Domain;

impl SourceAssetImporter for Domain {
    type Settings = ();
    type Product = ();
    const SOURCE_KIND: &'static str = "test.source-return";

    fn limits() -> AssetImportLimits {
        let limit = ByteLimit::new(4096).unwrap();
        AssetImportLimits::new(limit, limit, limit)
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        assert_eq!(source.bytes(), b"source import return fixture");
        Ok(())
    }
}

impl EditorSourceAssetDomain for Domain {
    type Domain = Self;

    fn default_settings() {}
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_source_import_return_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join(SOURCE), b"source import return fixture").unwrap();
        Self { root }
    }

    fn start(
        &self,
        pools: &TaskPools,
        gate: Option<SourceImportTestGate>,
    ) -> EditorSourceAssetImportHost {
        let assets = DirectoryCapability::from_host_handle(
            open_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        let expected_source = assets
            .open_file_exact(&RelativePath::new(SOURCE).unwrap())
            .unwrap()
            .identity();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id(),
                AssetPath::new(SOURCE).unwrap(),
                AssetSourceKind::Other(Domain::SOURCE_KIND.into()),
            ),
            (),
        );
        let encoded = metadata.to_json_string().unwrap();
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned(
            encoded.clone().into_bytes().into_boxed_slice(),
        )
        .unwrap();
        let publication = FilePublicationHost::bind_missing(&assets, METADATA, 4096).unwrap();
        let publication = HostCustody::default()
            .retain_publication(publication)
            .unwrap();
        let pending = PendingSourceImport::start(
            pools,
            publication,
            SourceImportWork {
                provider: Arc::new(TypedImporter::<Domain>(PhantomData)),
                assets,
                metadata,
                encoded,
                expected_source,
                budget: ProjectContentBudgetHost::new(Default::default()),
                gate,
            },
        )
        .unwrap();
        EditorSourceAssetImportHost {
            pending: Some(pending),
            latest: Some((SOURCE.into(), EditorSourceAssetImportAttempt::Preparing)),
            ..Default::default()
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_source_import_return_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn open_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
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

fn stable_id() -> StableAssetId {
    StableAssetId::parse_str("d40e7bc1-83f4-4da0-81d2-f3054ad061c8").unwrap()
}

struct Control {
    entered: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl Control {
    fn new(phase: SourceImportTestPhase) -> (SourceImportTestGate, Self) {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        (
            SourceImportTestGate {
                phase,
                entered: entered_tx,
                release: release_rx,
            },
            Self {
                entered,
                release: Some(release),
            },
        )
    }

    fn wait(&self) {
        self.entered.recv_timeout(WAIT).unwrap();
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[test]
fn queued_cancellation_retains_pending_until_executor_disposal() {
    let fixture = Fixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut host = fixture.start(&pools, None);
    let returned = host.pending.as_ref().unwrap().task.return_observer();
    host.cancel();
    assert!(host.pending.as_ref().unwrap().task.is_finished());
    assert_eq!(returned.state(), TaskReturnState::Pending);
    assert!(host.schema_adoption_pending());
    assert!(host.poll().is_none());
    assert_eq!(pools.run_pending_for_tests().cancelled_before_start, 1);
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert!(matches!(
        host.poll(),
        Some(EditorSourceAssetImportAttempt::Failed { .. })
    ));
    assert!(!host.schema_adoption_pending());
    assert!(!fixture.root.join(METADATA).exists());
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn running_cancellation_retains_pending_and_never_publishes() {
    let fixture = Fixture::new();
    let pools = Arc::new(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap());
    let (gate, control) = Control::new(SourceImportTestPhase::BeforeWork);
    let mut host = fixture.start(&pools, Some(gate));
    let returned = host.pending.as_ref().unwrap().task.return_observer();
    let runner_pools = Arc::clone(&pools);
    let runner = thread::spawn(move || runner_pools.run_pending_for_tests());
    control.wait();
    host.cancel();
    host.cancel();
    assert!(host.pending.as_ref().unwrap().task.is_finished());
    assert_eq!(returned.state(), TaskReturnState::Pending);
    let started = Instant::now();
    assert!(host.poll().is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(host.schema_adoption_pending());
    drop(control);
    assert_eq!(runner.join().unwrap().executed, 1);
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert!(matches!(
        host.poll(),
        Some(EditorSourceAssetImportAttempt::Failed { .. })
    ));
    assert!(!host.has_pending_work());
    assert!(!fixture.root.join(METADATA).exists());
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn publication_survives_normal_return_and_late_cancellation() {
    for cancel in [false, true] {
        let fixture = Fixture::new();
        let pools = Arc::new(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap());
        let (gate, control) = Control::new(SourceImportTestPhase::AfterWork);
        let mut host = fixture.start(&pools, Some(gate));
        let pending = host.pending.as_ref().unwrap();
        let returned = pending.task.return_observer();
        let publication = pending.publication.clone();
        let runner_pools = Arc::clone(&pools);
        let runner = thread::spawn(move || runner_pools.run_pending_for_tests());
        control.wait();
        let published = fs::read(fixture.root.join(METADATA)).unwrap();
        assert!(
            publication
                .try_with(|host| host.accepted_source_observation().is_some())
                .unwrap()
        );
        if cancel {
            host.cancel();
            assert!(host.pending.as_ref().unwrap().task.is_finished());
        }
        assert_eq!(returned.state(), TaskReturnState::Pending);
        assert!(host.poll().is_none());
        assert!(host.schema_adoption_pending());
        drop(control);
        assert_eq!(runner.join().unwrap().executed, 1);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        let expected = EditorSourceAssetImportAttempt::MetadataPublished {
            stable_id: stable_id(),
        };
        assert_eq!(host.poll(), Some(expected.clone()));
        assert_eq!(host.latest.as_ref().unwrap().1, expected);
        assert!(!host.schema_adoption_pending());
        assert!(publication.try_with(|_| ()).is_none());
        assert_eq!(fs::read(fixture.root.join(METADATA)).unwrap(), published);
        assert!(pools.shutdown_blocking().unwrap().is_complete());
    }
}

#[test]
fn cancellation_after_failed_publication_retains_cleanup_and_terminal_result() {
    let fixture = Fixture::new();
    let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut host = fixture.start(&pools, None);
    let pending = host.pending.as_ref().unwrap();
    let returned = pending.task.return_observer();
    let publication = pending.publication.clone();
    let cleanup = publication
        .try_with(|host| host.test_fail_after_write())
        .unwrap();
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert_eq!(returned.state(), TaskReturnState::Returned);
    host.cancel();
    assert!(host.poll().is_none());
    let expected = host.pending.as_ref().unwrap().outcome.clone().unwrap();
    assert!(matches!(
        &expected,
        EditorSourceAssetImportAttempt::Failed { .. }
    ));
    assert!(publication.try_with(|host| host.has_cleanup()).unwrap());
    assert!(host.schema_adoption_pending());
    host.cancel();
    assert!(host.poll().is_none());
    cleanup.release();
    assert_eq!(host.poll(), Some(expected.clone()));
    assert_eq!(host.latest.as_ref().unwrap().1, expected);
    assert!(!host.schema_adoption_pending());
    assert!(publication.try_with(|_| ()).is_none());
    assert!(!fixture.root.join(METADATA).exists());
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}
