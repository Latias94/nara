//! Real source/Tasks/Runtime regressions using the store's original allocation owner.

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_app::RuntimeInstance;
use nara_asset::{
    AssetBuildSource, AssetImportError, AssetImportLimits, AssetImportSource, AssetMeta,
    AssetMetaFileLimits, AssetMetaIdentity, AssetPath, AssetPlugin, AssetRecord,
    AssetReferenceDemand, AssetRuntimeError, AssetRuntimeLimits, AssetSourcePath,
    AssetSourcePlugin, Handle, LoadState, ProjectAssetDatabase, RuntimeAssetCodec,
    SourceAssetImporter, StableAssetId,
};
use nara_core::{ByteLimit, FormatVersion};
use nara_fs::{CapabilityRights, FileCapability, HostCapabilityOptions, TrustMode};
use nara_tasks::{TaskPlugin, TaskPoolConfig};
use serde::Deserialize;

use super::*;
use crate::asset_build::{ArtifactBuildLimits, ArtifactContentDigest};

const SOURCE: &str = "b58cc029-d438-411b-8df7-f1ca876500de";
const PRIMARY: &str = "selection.txt";
const METADATA: &str = "selection.txt.meta";
const DEADLINE: Duration = Duration::from_secs(15);

fn source_id() -> StableAssetId {
    StableAssetId::parse_str(SOURCE).unwrap()
}

fn canonical(name: &str) -> AssetSourcePath {
    AssetSourcePath::new(format!("assets/{name}")).unwrap()
}

fn primary_product(source: StableAssetId) -> AssetProductRef {
    AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap()
}

fn fixture_diagnostic() -> Diagnostic {
    diagnostic(
        "test.source-selection-invalid",
        "The source selection was rejected",
    )
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    source: StableAssetId,
    byte: u8,
}

struct Domain<const CASE: usize>;
struct Codec<const CASE: usize>;

impl<const CASE: usize> AssetDomain for Domain<CASE> {
    type Product = Selection;
}

impl<const CASE: usize> SourceAssetImporter for Domain<CASE> {
    type Settings = ();

    const SOURCE_KIND: &'static str = "source-selection-regression";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(32).unwrap(),
            ByteLimit::new(16 * 1024).unwrap(),
            ByteLimit::new(4096).unwrap(),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<Selection, AssetImportError<E>> {
        let path = selected_path(source.bytes())?;
        let source_id = source.metadata().identity().stable_id();
        let bytes = source.read_dependency(&path, ByteLimit::new(1).unwrap())?;
        selection(source_id, &bytes)
    }
}

fn selected_path<E>(bytes: &[u8]) -> Result<AssetPath, AssetImportError<E>> {
    let path = std::str::from_utf8(bytes)
        .ok()
        .and_then(|path| AssetPath::new(path).ok());
    path.ok_or_else(|| AssetImportError::domain(fixture_diagnostic()))
}

fn selection<E>(source: StableAssetId, bytes: &[u8]) -> Result<Selection, AssetImportError<E>> {
    let [byte] = bytes else {
        return Err(AssetImportError::domain(fixture_diagnostic()));
    };
    Ok(Selection {
        source,
        byte: *byte,
    })
}

impl<const CASE: usize> SourceAssetBuild for Domain<CASE> {
    type Codec = Codec<CASE>;

    const BUILD_ID: &'static str = "test.source-selection-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(_: ()) -> Result<(), Diagnostic> {
        Ok(())
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<Selection, AssetImportError<E>> {
        let probe = probe::<CASE>();
        probe.builds.fetch_add(1, Ordering::SeqCst);
        if let Some(store) = probe.pressure_store.lock().unwrap().take() {
            let limit = store.limits().retained_bytes.get();
            let used = match store.reserve_retained(limit) {
                Err(ArtifactBuildError::Budget {
                    kind: "retained",
                    required,
                    limit,
                }) => required - limit,
                other => panic!("expected existing retained allocations, got {other:?}"),
            };
            let lease = store.reserve_retained(limit - used - 1).unwrap();
            assert!(matches!(
                store.reserve_retained(2),
                Err(ArtifactBuildError::Budget {
                    kind: "retained",
                    ..
                })
            ));
            *probe.retained_pressure.lock().unwrap() = Some(lease);
        }
        let path = selected_path(source.bytes())?;
        let source_id = source.metadata().identity().stable_id();
        let bytes = source.read_dependency(&path, ByteLimit::new(1).unwrap())?;
        selection(source_id, &bytes)
    }

    fn visit_products<E>(
        product: &Selection,
        mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visitor(primary_product(product.source))
    }
}

impl<const CASE: usize> RuntimeAssetCodec<Selection> for Codec<CASE> {
    const RUNTIME_KIND: &'static str = "source_selection_regression";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(256).unwrap(),
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(4096).unwrap(),
        )
    }

    fn encode_runtime(product: &Selection) -> Result<Box<[u8]>, AssetRuntimeError> {
        serde_json::to_vec(product)
            .map(Vec::into_boxed_slice)
            .map_err(|_| AssetRuntimeError::new(fixture_diagnostic()))
    }

    fn decode_runtime(bytes: &[u8]) -> Result<Selection, AssetRuntimeError> {
        let product = serde_json::from_slice(bytes)
            .map_err(|_| AssetRuntimeError::new(fixture_diagnostic()))?;
        let probe = probe::<CASE>();
        probe.decodes.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = probe.decode_gate.lock().unwrap().take() {
            gate.block();
        }
        Ok(product)
    }

    fn runtime_retained_bytes(_: &Selection) -> Result<usize, AssetRuntimeError> {
        Ok(size_of::<Selection>())
    }

    fn visit_runtime_dependencies<E>(
        _: &Selection,
        _: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(product: &Selection, reference: AssetProductRef) -> bool {
        reference.source() == product.source && reference.product() == ImportedProductId::PRIMARY
    }
}

#[derive(Default)]
struct Probe {
    builds: AtomicUsize,
    decodes: AtomicUsize,
    pressure_store: Mutex<Option<Arc<ArtifactBuildStore>>>,
    retained_pressure: Mutex<Option<ArtifactBuildLease>>,
    decode_gate: Mutex<Option<Arc<Gate>>>,
}

fn probe<const CASE: usize>() -> &'static Probe {
    static PROBES: [OnceLock<Probe>; 6] = [const { OnceLock::new() }; 6];
    PROBES[CASE].get_or_init(Probe::default)
}

impl Probe {
    fn calls(&self) -> (usize, usize) {
        (
            self.builds.load(Ordering::SeqCst),
            self.decodes.load(Ordering::SeqCst),
        )
    }
}

struct RetainedPressure<const CASE: usize>;

impl<const CASE: usize> RetainedPressure<CASE> {
    fn install(store: &Arc<ArtifactBuildStore>) -> Self {
        let probe = probe::<CASE>();
        assert!(probe.retained_pressure.lock().unwrap().is_none());
        assert!(
            probe
                .pressure_store
                .lock()
                .unwrap()
                .replace(Arc::clone(store))
                .is_none()
        );
        Self
    }
}

impl<const CASE: usize> Drop for RetainedPressure<CASE> {
    fn drop(&mut self) {
        probe::<CASE>().pressure_store.lock().unwrap().take();
        probe::<CASE>().retained_pressure.lock().unwrap().take();
    }
}

#[derive(Default)]
struct Gate {
    reached: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}

impl Gate {
    fn block(&self) {
        self.reached.store(true, Ordering::SeqCst);
        let (released, _) = self
            .wake
            .wait_timeout_while(
                self.released.lock().unwrap(),
                Duration::from_secs(30),
                |released| !*released,
            )
            .unwrap();
        assert!(
            *released,
            "the consumer must release the bounded decode gate"
        );
    }
}

struct HeldDecode(Arc<Gate>);

impl HeldDecode {
    fn install<const CASE: usize>() -> Self {
        let gate = Arc::new(Gate::default());
        assert!(
            probe::<CASE>()
                .decode_gate
                .lock()
                .unwrap()
                .replace(Arc::clone(&gate))
                .is_none()
        );
        Self(gate)
    }

    fn reached(&self) -> bool {
        self.0.reached.load(Ordering::SeqCst)
    }
}

impl Drop for HeldDecode {
    fn drop(&mut self) {
        *self.0.released.lock().unwrap() = true;
        self.0.wake.notify_all();
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nara-source-budget-retry-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        for name in ["source", "cache"] {
            fs::create_dir(fixture.0.join(name)).unwrap();
        }
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                source_id(),
                AssetPath::new(PRIMARY).unwrap(),
                AssetSourceKind::Other(Domain::<0>::SOURCE_KIND.into()),
            ),
            (),
        )
        .to_json_string()
        .unwrap();
        fixture.write(METADATA, metadata.as_bytes());
        fixture.write(PRIMARY, b"a.txt");
        fixture.write("a.txt", b"A");
        fixture
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.0.join("source").join(name);
        assert_eq!(path.parent(), Some(self.0.join("source").as_path()));
        fs::write(path, bytes).unwrap();
    }

    fn remove_primary(&self) {
        let root = self.0.join("source").canonicalize().unwrap();
        let path = root.join(PRIMARY).canonicalize().unwrap();
        assert_eq!(path.parent(), Some(root.as_path()));
        fs::remove_file(path).unwrap();
    }

    fn store(&self) -> ArtifactBuildStore {
        ArtifactBuildStore::open(
            directory(&self.0.join("cache"), CapabilityRights::ReadWrite),
            ArtifactBuildLimits {
                metadata_bytes: ByteLimit::new(64 * 1024).unwrap(),
                member_bytes: ByteLimit::new(256).unwrap(),
                group_bytes: ByteLimit::new(4096).unwrap(),
                retained_bytes: ByteLimit::new(256 * 1024).unwrap(),
                work_bytes: ByteLimit::new(32 * 1024 * 1024).unwrap(),
                members: ItemLimit::ONE,
                observations: ItemLimit::new(8).unwrap(),
                index_entries: ItemLimit::new(8).unwrap(),
            },
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(root.parent(), Some(temporary.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-source-budget-retry-")
        );
        fs::remove_dir_all(&root).unwrap();
        assert!(
            !root.exists(),
            "Runtime/store capabilities must permit complete fixture cleanup"
        );
    }
}

fn directory(path: &Path, rights: CapabilityRights) -> DirectoryCapability {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let file = fs::File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(rights, TrustMode::TrustedLocal),
    )
    .unwrap()
}

fn definition<const CASE: usize>() -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    let implementation = *IMPLEMENTATION.get_or_init(|| {
        let file = fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let length = file.metadata().unwrap().len();
        let file = FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0).unwrap();
        ArtifactContentDigest::from_content(file.digest(length).unwrap())
    });
    ArtifactBuildDefinition {
        id: Domain::<CASE>::BUILD_ID.into(),
        revision: FormatVersion::new(Domain::<CASE>::BUILD_REVISION).unwrap(),
        implementation,
    }
}

enum FrameCommand {
    Change(AssetSourceChange),
    TouchState(AssetId),
    TouchSlot(Handle<BuiltSourceAsset<Selection>>),
    ReplaceGenerations(AssetId, u64),
}

#[derive(Clone, Default, Resource)]
struct Input(Arc<Mutex<Vec<FrameCommand>>>);

fn publish_commands(
    input: Res<Input>,
    mut changes: ResMut<AssetSourceChanges>,
    mut states: ResMut<AssetStates>,
    mut assets: ResMut<Assets<BuiltSourceAsset<Selection>>>,
    mut generations: ResMut<AssetLoadGenerations>,
) {
    for command in std::mem::take(&mut *input.0.lock().unwrap()) {
        match command {
            FrameCommand::Change(change) => {
                changes.push(change).unwrap();
            }
            FrameCommand::TouchState(id) => {
                states.set_loading(id);
            }
            FrameCommand::TouchSlot(handle) => {
                let _value = assets.get_mut(handle).unwrap();
            }
            FrameCommand::ReplaceGenerations(id, number) => {
                *generations = AssetLoadGenerations::default();
                for _ in 0..number {
                    generations.begin_request(id);
                }
            }
        }
    }
}

#[derive(Debug)]
struct ObservedTerminal {
    paths: Vec<AssetSourcePath>,
    budget: Option<(&'static str, usize, usize)>,
    success: bool,
}

#[derive(Default, Resource)]
struct Terminals(Vec<ObservedTerminal>);

fn observe_terminals<const CASE: usize>(
    executor: Res<SourceBuildExecutor<Domain<CASE>>>,
    mut terminals: ResMut<Terminals>,
) {
    for ready in &executor.ready {
        if let TaskTerminal::Completed(output) = &ready.terminal {
            let budget = match &output.result {
                Err(ArtifactBuildError::Budget {
                    kind,
                    required,
                    limit,
                }) => Some((*kind, *required, *limit)),
                _ => None,
            };
            terminals.0.push(ObservedTerminal {
                paths: output.observations.paths().cloned().collect(),
                budget,
                success: output.result.is_ok(),
            });
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Stats {
    spawned: u64,
    adopted: u64,
    failed: u64,
    rejected: u64,
    stale: u64,
    pending: usize,
}

struct Live<const CASE: usize> {
    runtime: RuntimeInstance,
    store: Arc<ArtifactBuildStore>,
    input: Input,
    handle: Handle<BuiltSourceAsset<Selection>>,
}

impl<const CASE: usize> Live<CASE> {
    fn stats(&self) -> Stats {
        let stats = self
            .runtime
            .world()
            .resource::<SourceAssetBuildStats<Domain<CASE>>>();
        Stats {
            spawned: stats.spawned,
            adopted: stats.adopted,
            failed: stats.failed,
            rejected: stats.rejected,
            stale: stats.stale,
            pending: stats.pending,
        }
    }

    fn current(&self) -> Option<&BuiltSourceAsset<Selection>> {
        self.runtime
            .world()
            .resource::<Assets<BuiltSourceAsset<Selection>>>()
            .get(self.handle)
    }

    fn state(&self) -> &LoadState {
        self.runtime
            .world()
            .resource::<AssetStates>()
            .state(self.handle.id())
            .unwrap()
            .load_state()
    }

    fn command(&self, command: FrameCommand) {
        self.input.0.lock().unwrap().push(command);
    }

    fn change(&self, name: &str, kind: AssetSourceChangeKind) {
        self.command(FrameCommand::Change(AssetSourceChange::new(
            canonical(name),
            kind,
        )));
    }

    fn advance(&mut self) {
        self.runtime.drive(Duration::ZERO).unwrap();
        thread::yield_now();
    }

    fn until(&mut self, reason: &str, mut finished: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        while !finished(self) {
            assert!(
                Instant::now() < deadline,
                "{reason}: {:?}; calls {:?}",
                self.stats(),
                probe::<CASE>().calls()
            );
            self.advance();
        }
    }

    fn paths(&self, kind: SourceSubscriptionKind) -> Vec<AssetSourcePath> {
        self.runtime
            .world()
            .resource::<AssetDependencyGraph>()
            .source_subscription_paths(source_id(), kind)
            .to_vec()
    }

    fn assert_paths(&self, published: &[&str], retry: &[&str]) {
        for (kind, expected) in [
            (SourceSubscriptionKind::Published, published),
            (SourceSubscriptionKind::Retry, retry),
        ] {
            let mut expected = expected
                .iter()
                .map(|name| canonical(name))
                .collect::<Vec<_>>();
            expected.sort_unstable();
            assert_eq!(self.paths(kind), expected, "{kind:?}");
        }
    }

    fn assert_idle(&mut self) {
        let before = self.stats();
        let calls = probe::<CASE>().calls();
        assert_eq!(before.pending, 0);
        for _ in 0..8 {
            self.advance();
        }
        assert!(self.input.0.lock().unwrap().is_empty());
        assert!(
            self.runtime
                .world()
                .resource::<AssetSourceChanges>()
                .is_empty()
        );
        assert_eq!(
            self.stats(),
            before,
            "a global quota failure must not schedule retries"
        );
        assert_eq!(probe::<CASE>().calls(), calls);
    }

    fn fail_refresh(&mut self) {
        let before = self.stats();
        self.change(PRIMARY, AssetSourceChangeKind::Modified);
        self.until("source failure did not settle", |live| {
            live.stats().failed > before.failed && live.stats().pending == 0
        });
        assert_eq!(self.stats().failed, before.failed + 1);
        assert_eq!(self.stats().spawned, before.spawned + 1);
        assert_eq!(self.stats().rejected, before.rejected);
    }

    fn establish_missing_retry(&mut self, fixture: &Fixture) -> BuiltSourceAsset<Selection> {
        self.until("initial source did not load", |live| {
            live.stats().adopted == 1
        });
        assert_eq!(self.state(), &LoadState::Loaded);
        let original = self.current().unwrap().clone();
        assert_eq!(original.value().byte, b'A');
        fixture.write(PRIMARY, b"c.txt");
        self.fail_refresh();
        assert_eq!(probe::<CASE>().calls(), (2, 1));
        assert!(matches!(self.state(), LoadState::Failed { message }
            if message == "asset.build.required-input-missing"));
        self.assert_paths(&[PRIMARY, METADATA, "a.txt"], &[PRIMARY, METADATA, "c.txt"]);
        original
    }

    fn assert_partial_budget(&self, kind: &str, original: &BuiltSourceAsset<Selection>) {
        let terminals = &self.runtime.world().resource::<Terminals>().0;
        let terminal = terminals.last().unwrap();
        let (actual, required, limit) = terminal.budget.expect("actual worker budget refusal");
        assert_eq!(actual, kind);
        assert!(required > limit);
        let mut expected = vec![canonical(PRIMARY), canonical(METADATA)];
        expected.sort_unstable();
        assert_eq!(
            terminal.paths, expected,
            "refusal must follow actual selection reads"
        );
        assert!(matches!(self.state(), LoadState::Failed { message }
            if message == "asset.build.budget"));
        assert_eq!(
            self.current().unwrap().group().digest(),
            original.group().digest()
        );
        assert_eq!(self.current().unwrap().value().byte, b'A');
        assert_eq!(self.stats().adopted, 1);
        self.assert_paths(&[PRIMARY, METADATA, "a.txt"], &[PRIMARY, METADATA, "c.txt"]);
    }

    fn recover_from_c_creation(&mut self, fixture: &Fixture) {
        let before = self.stats();
        fixture.write("c.txt", b"C");
        self.change("c.txt", AssetSourceChangeKind::Modified);
        self.until("only c.txt creation must recover the source", |live| {
            live.stats().adopted == before.adopted + 1 && live.stats().pending == 0
        });
        assert_eq!(self.stats().spawned, before.spawned + 1);
        assert_eq!(self.stats().failed, before.failed);
        assert_eq!(self.stats().rejected, before.rejected);
        assert_eq!(self.stats().stale, before.stale);
        assert_eq!(self.state(), &LoadState::Loaded);
        let value = self.current().unwrap().value();
        assert_eq!(value.source, source_id());
        assert_eq!(value.byte, b'C');
        self.assert_paths(&[PRIMARY, METADATA, "c.txt"], &[]);
        self.assert_idle();
    }

    fn hold_c_decode(&mut self, fixture: &Fixture) -> HeldDecode {
        let before = self.stats();
        let calls = probe::<CASE>().calls();
        let gate = HeldDecode::install::<CASE>();
        fixture.write("c.txt", b"C");
        self.change("c.txt", AssetSourceChangeKind::Modified);
        self.until("successful C decode did not reach its gate", |_| {
            gate.reached()
        });
        assert_eq!(probe::<CASE>().calls(), (calls.0 + 1, calls.1 + 1));
        assert_eq!(self.stats().spawned, before.spawned + 1);
        assert_eq!(self.stats().pending, 1);
        assert_eq!(self.current().unwrap().value().byte, b'A');
        self.assert_paths(&[PRIMARY, METADATA, "a.txt"], &[PRIMARY, METADATA, "c.txt"]);
        gate
    }

    fn workers_idle(&self) -> bool {
        let stats = self
            .runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io);
        stats.queued == 0 && stats.running == 0
    }
}

fn with_live<const CASE: usize>(fixture: &Fixture, exercise: impl FnOnce(&mut Live<CASE>)) {
    let store = Arc::new(fixture.store());
    let input = Input::default();
    let mut app = App::new();
    app.add_plugins((
        TaskPlugin::new(
            TaskPoolConfig::threaded(ItemLimit::ONE, ItemLimit::ONE, ItemLimit::ONE).unwrap(),
        ),
        AssetPlugin,
        AssetSourcePlugin,
    ))
    .unwrap();
    app.add_plugin(
        SourceAssetBuildPlugin::<Domain<CASE>>::new(
            Arc::clone(&store),
            directory(&fixture.0.join("source"), CapabilityRights::ReadOnly),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            definition::<CASE>(),
        )
        .with_pending_limit(ItemLimit::new(2).unwrap()),
    )
    .unwrap();
    app.insert_resource(input.clone()).unwrap();
    app.init_resource::<Terminals>().unwrap();
    app.add_systems(CoreStage::First, publish_commands).unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        observe_terminals::<CASE>
            .after(AssetTaskUpdateSet::SpawnJobs)
            .before(AssetTaskUpdateSet::ApplyResults),
    )
    .unwrap();
    let record = AssetRecord::new(
        source_id(),
        canonical(PRIMARY),
        AssetSourceKind::Other(Domain::<CASE>::SOURCE_KIND.into()),
    );
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<BuiltSourceAsset<Selection>>(&record)
        .unwrap();
    let runtime = match RuntimeInstance::start(app) {
        Ok(runtime) => runtime,
        Err(mut failure) => {
            let error = format!("{failure:?}");
            failure.drive_retirement();
            drop(failure);
            panic!("source regression Runtime startup failed: {error}");
        }
    };
    let mut live = Live {
        runtime,
        store,
        input,
        handle,
    };
    live.change(PRIMARY, AssetSourceChangeKind::Modified);
    let result = catch_unwind(AssertUnwindSafe(|| exercise(&mut live)));
    let closed = live.runtime.finish().map_err(|owner| {
        let error = owner.to_string();
        drop(owner);
        error
    });
    match result {
        Ok(()) => {
            closed.expect("source regression Runtime must retire physically");
        }
        Err(failure) => resume_unwind(failure),
    }
}

#[test]
fn lookup_work_refusal_after_selection_preserves_missing_dependency_retry() {
    let fixture = Fixture::new();
    with_live::<0>(&fixture, |live| {
        let original = live.establish_missing_retry(&fixture);
        let limits = Domain::<0>::limits();
        let fixed_work = limits.work_bytes().get()
            + limits.retained_bytes().get()
            + AssetMetaFileLimits::default().encoded_bytes().get() * 8;
        let available = fixed_work + 64 * 1024;
        let pressure = live
            .store
            .reserve_work(live.store.limits().work_bytes.get() - available)
            .unwrap();
        let calls = probe::<0>().calls();
        live.fail_refresh();
        assert_eq!(
            probe::<0>().calls(),
            calls,
            "lookup refusal must precede D::build"
        );
        live.assert_partial_budget("work", &original);
        live.assert_idle();
        drop(pressure);
        live.assert_idle();
        live.recover_from_c_creation(&fixture);
        assert_eq!(probe::<0>().calls(), (calls.0 + 1, calls.1 + 1));
    });
}

#[test]
fn retained_trace_refusal_before_dependency_tracking_preserves_existing_retry() {
    let fixture = Fixture::new();
    with_live::<1>(&fixture, |live| {
        let original = live.establish_missing_retry(&fixture);
        let pressure = RetainedPressure::<1>::install(&live.store);
        let calls = probe::<1>().calls();
        live.fail_refresh();
        assert!(probe::<1>().retained_pressure.lock().unwrap().is_some());
        assert_eq!(probe::<1>().calls(), (calls.0 + 1, calls.1));
        live.assert_partial_budget("retained", &original);
        live.assert_idle();
        drop(pressure);
        live.assert_idle();
        live.recover_from_c_creation(&fixture);
        assert_eq!(probe::<1>().calls(), (calls.0 + 2, calls.1 + 1));
    });
}

#[test]
fn principal_removal_clears_both_sets_and_retires_a_pending_old_success() {
    let fixture = Fixture::new();
    with_live::<2>(&fixture, |live| {
        let original = live.establish_missing_retry(&fixture);
        let gate = live.hold_c_decode(&fixture);
        let before = live.stats();
        fixture.remove_primary();
        live.change(PRIMARY, AssetSourceChangeKind::Removed);
        live.until("principal removal did not settle", |live| {
            live.state() == &LoadState::Removed && live.stats().pending == 0
        });
        assert!(live.current().is_none());
        live.assert_paths(&[], &[]);
        assert_eq!(live.stats().spawned, before.spawned);
        assert_eq!(live.stats().adopted, before.adopted);
        assert!(live.stats().stale > before.stale);

        drop(gate);
        live.until("removed source worker did not retire", Live::workers_idle);
        live.assert_idle();
        assert_eq!(live.state(), &LoadState::Removed);
        assert!(live.current().is_none());
        assert_eq!(live.stats().spawned, before.spawned);
        assert_eq!(live.stats().adopted, before.adopted);
        assert_eq!(live.stats().failed, before.failed);
        assert_eq!(live.stats().rejected, before.rejected);
        live.assert_paths(&[], &[]);
        let graph = live.runtime.world().resource::<AssetDependencyGraph>();
        for path in [PRIMARY, METADATA, "a.txt", "c.txt"] {
            assert!(
                graph
                    .raw_dependents_for_path(&canonical(path))
                    .next()
                    .is_none()
            );
        }
        live.change("a.txt", AssetSourceChangeKind::Modified);
        live.change("c.txt", AssetSourceChangeKind::Modified);
        live.assert_idle();
        assert_eq!(original.value().byte, b'A');
    });
}

fn reject_same_generation_mutation<const CASE: usize>(touch_slot: bool) {
    let fixture = Fixture::new();
    with_live::<CASE>(&fixture, |live| {
        let original = live.establish_missing_retry(&fixture);
        let gate = live.hold_c_decode(&fixture);
        let before = live.stats();
        let generation = live
            .runtime
            .world()
            .resource::<AssetLoadGenerations>()
            .current(live.handle.id());
        let state = live
            .runtime
            .world()
            .resource::<AssetStates>()
            .state_revision(live.handle.id());
        let slot = live
            .runtime
            .world()
            .resource::<Assets<BuiltSourceAsset<Selection>>>()
            .slot_revision(live.handle);
        live.command(if touch_slot {
            FrameCommand::TouchSlot(live.handle)
        } else {
            FrameCommand::TouchState(live.handle.id())
        });
        live.advance();
        assert_eq!(
            live.runtime
                .world()
                .resource::<AssetLoadGenerations>()
                .current(live.handle.id()),
            generation
        );
        let changed_state = live
            .runtime
            .world()
            .resource::<AssetStates>()
            .state_revision(live.handle.id());
        let changed_slot = live
            .runtime
            .world()
            .resource::<Assets<BuiltSourceAsset<Selection>>>()
            .slot_revision(live.handle);
        assert_eq!(changed_state != state, !touch_slot);
        assert_eq!(changed_slot != slot, touch_slot);
        let events = live
            .runtime
            .world()
            .resource::<AssetEvents>()
            .iter()
            .count();

        drop(gate);
        live.until("same-generation result was not rejected", |live| {
            live.stats().stale > before.stale && live.stats().pending == 0 && live.workers_idle()
        });
        let terminals = &live.runtime.world().resource::<Terminals>().0;
        assert!(
            terminals.last().unwrap().success,
            "reject an actual successful worker result"
        );
        assert_eq!(live.stats().adopted, before.adopted);
        assert_eq!(live.stats().failed, before.failed);
        assert_eq!(live.stats().rejected, before.rejected);
        assert_eq!(
            live.current().unwrap().group().digest(),
            original.group().digest()
        );
        assert_eq!(live.current().unwrap().value().byte, b'A');
        assert_eq!(live.state(), &LoadState::Loading);
        assert_eq!(
            live.runtime
                .world()
                .resource::<AssetStates>()
                .state_revision(live.handle.id()),
            changed_state
        );
        assert!(
            live.runtime
                .world()
                .resource::<Assets<BuiltSourceAsset<Selection>>>()
                .slot_revision(live.handle)
                == changed_slot
        );
        assert_eq!(
            live.runtime
                .world()
                .resource::<AssetEvents>()
                .iter()
                .count(),
            events
        );
        live.assert_paths(&[PRIMARY, METADATA, "a.txt"], &[PRIMARY, METADATA, "c.txt"]);
        live.assert_idle();
    });
}

#[test]
fn same_generation_state_write_rejects_pending_success_without_changing_subscriptions() {
    reject_same_generation_mutation::<3>(false);
}

#[test]
fn same_generation_slot_write_rejects_pending_success_without_changing_subscriptions() {
    reject_same_generation_mutation::<4>(true);
}

#[test]
fn replacing_generation_owner_rejects_old_work_even_when_its_number_matches() {
    let fixture = Fixture::new();
    with_live::<5>(&fixture, |live| {
        let original = live.establish_missing_retry(&fixture);
        let gate = live.hold_c_decode(&fixture);
        let before = live.stats();
        let generation = live
            .runtime
            .world()
            .resource::<AssetLoadGenerations>()
            .current(live.handle.id());
        let state = live
            .runtime
            .world()
            .resource::<AssetStates>()
            .state_revision(live.handle.id());
        let slot = live
            .runtime
            .world()
            .resource::<Assets<BuiltSourceAsset<Selection>>>()
            .slot_revision(live.handle);
        live.command(FrameCommand::ReplaceGenerations(
            live.handle.id(),
            generation.raw(),
        ));
        live.advance();
        assert_eq!(
            live.runtime
                .world()
                .resource::<AssetLoadGenerations>()
                .current(live.handle.id()),
            generation
        );
        drop(gate);
        live.until("old generation owner work was not retired", |live| {
            live.stats().stale > before.stale && live.stats().pending == 0 && live.workers_idle()
        });
        assert_eq!(live.stats().adopted, before.adopted);
        assert_eq!(live.stats().failed, before.failed);
        assert_eq!(
            live.runtime
                .world()
                .resource::<AssetStates>()
                .state_revision(live.handle.id()),
            state
        );
        assert!(
            live.runtime
                .world()
                .resource::<Assets<BuiltSourceAsset<Selection>>>()
                .slot_revision(live.handle)
                == slot
        );
        assert_eq!(
            live.current().unwrap().group().digest(),
            original.group().digest()
        );
        live.assert_paths(&[PRIMARY, METADATA, "a.txt"], &[PRIMARY, METADATA, "c.txt"]);
        live.change(PRIMARY, AssetSourceChangeKind::Modified);
        live.until(
            "new generation owner could not adopt its own request",
            |live| live.stats().adopted == before.adopted + 1 && live.stats().pending == 0,
        );
        assert_eq!(live.current().unwrap().value().byte, b'C');
        assert_eq!(live.state(), &LoadState::Loaded);
        live.assert_paths(&[PRIMARY, METADATA, "c.txt"], &[]);
    });
}
