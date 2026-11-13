//! Independent live typed builds driven exclusively by semantic source-change observations.

use std::{
    collections::BTreeSet,
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    app::{App, CoreStage, RuntimeInstance},
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, BuiltSourceAsset, SourceAssetBuildPlugin, SourceAssetBuildStats,
    },
    core::{FormatVersion, ItemLimit},
    ecs::{Res, ResMut, Resource},
    fs::{CapabilityRights, DirectoryCapability, FileCapability, HostCapabilityOptions, TrustMode},
    tasks::{TaskPlugin, TaskPoolConfig},
};
use nara_asset::{
    AssetBuildSource, AssetDependencyGraph, AssetDomain, AssetImportError, AssetImportLimits,
    AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
    AssetPathScope, AssetPlugin, AssetProductRef, AssetRecord, AssetReferenceDemand,
    AssetRuntimeError, AssetRuntimeLimits, AssetServer, AssetSourceChange, AssetSourceChangeKind,
    AssetSourceChanges, AssetSourceKind, AssetSourcePath, AssetSourcePlugin, AssetStates, Assets,
    Handle, ImportedProductId, LoadState, ProjectAssetDatabase, RuntimeAssetCodec,
    SourceAssetBuild, SourceAssetImporter, SourceSubscriptionKind, StableAssetId,
};
use nara_asset_domain_consumer::{
    TextBundle, TextBundleDomain, TextBundleIdentityDecision, TextBundleRebindAssignment,
    TextBundleSettings, bundle::MAX_BUNDLE_SUFFIX_BYTES,
};
use nara_diagnostic::Diagnostic;

#[path = "support/durable_build_runtime.rs"]
mod runtime;

const SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const RAW: &[u8] = br#"{
    "suffix":"a.txt",
    "entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]
}"#;
const SELECT_B: &[u8] = br#"{
    "suffix":"b.txt",
    "entries":[{"name":"Closing","text":"Farewell"},{"name":"Greeting","text":"Welcome"}]
}"#;
const SELECT_C: &[u8] = br#"{
    "suffix":"c.txt",
    "entries":[{"name":"Closing","text":"Farewell"},{"name":"Greeting","text":"Welcome"}]
}"#;
const SELECT_MISSING: &[u8] = br#"{
    "suffix":"retry.txt",
    "entries":[{"name":"Closing","text":"Farewell"},{"name":"Greeting","text":"Welcome"}]
}"#;
const DEADLINE: Duration = Duration::from_secs(15);

fn id(number: usize) -> ImportedProductId {
    ImportedProductId::parse_subproduct(&format!("00000000-0000-4000-8000-{number:012x}")).unwrap()
}

fn reference(product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(SOURCE).unwrap(), product).unwrap()
}

fn canonical(name: &str) -> AssetSourcePath {
    AssetSourcePath::new(format!("assets/{name}")).unwrap()
}

fn definition() -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    let implementation = *IMPLEMENTATION.get_or_init(|| {
        let file = fs::File::open(std::env::current_exe().unwrap()).unwrap();
        let length = file.metadata().unwrap().len();
        let file = FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0).unwrap();
        ArtifactContentDigest::from_content(file.digest(length).unwrap())
    });
    ArtifactBuildDefinition {
        id: TextBundleDomain::BUILD_ID.to_owned(),
        revision: FormatVersion::new(TextBundleDomain::BUILD_REVISION).unwrap(),
        implementation,
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

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nara-dependent-resource-refresh-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        for name in ["source", "cache-a", "cache-b", "delivery"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let fixture = Self { root };
        let settings = TextBundleSettings::initial(RAW, 64, [id(1), id(2)]).unwrap();
        fixture.write_bundle(RAW, &settings);
        fixture
    }

    fn write_source(&self, name: &str, bytes: &[u8]) {
        let path = self.root.join("source").join(name);
        assert_eq!(path.parent().unwrap(), self.root.join("source"));
        fs::write(path, bytes).unwrap();
    }

    fn write_bundle(&self, raw: &[u8], settings: &TextBundleSettings) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(SOURCE).unwrap(),
                AssetPath::new("bundle.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        )
        .to_json_string()
        .unwrap();
        self.write_source("bundle.json", raw);
        self.write_source("bundle.json.meta", metadata.as_bytes());
    }

    fn settings(&self) -> TextBundleSettings {
        let bytes = fs::read(self.root.join("source/bundle.json.meta")).unwrap();
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&bytes).unwrap();
        assert_eq!(metadata.identity().stable_id(), reference(id(1)).source());
        metadata
            .admit::<TextBundleSettings>()
            .unwrap()
            .settings()
            .clone()
    }

    fn rebind(&self, raw: &[u8]) {
        let previous = self.settings();
        let settings = previous
            .rebind(
                raw,
                &[
                    TextBundleRebindAssignment {
                        row: 1,
                        identity: TextBundleIdentityDecision::Continue(id(1)),
                    },
                    TextBundleRebindAssignment {
                        row: 0,
                        identity: TextBundleIdentityDecision::Continue(id(2)),
                    },
                ],
            )
            .unwrap();
        settings.validate_successor(&previous).unwrap();
        assert!(settings.continuity.retired().is_empty());
        self.write_bundle(raw, &settings);
    }

    fn remove_source(&self, name: &str) {
        let root = self.root.join("source").canonicalize().unwrap();
        let target = root.join(name).canonicalize().unwrap();
        assert_eq!(target.parent(), Some(root.as_path()));
        fs::remove_file(target).unwrap();
    }

    fn store(&self, name: &str) -> ArtifactBuildStore {
        ArtifactBuildStore::open(
            directory(&self.root.join(name), CapabilityRights::ReadWrite),
            ArtifactBuildLimits::default(),
        )
        .unwrap()
    }

    fn remove_tree(&self, name: &str) {
        let root = self.root.canonicalize().unwrap();
        let target = root.join(name).canonicalize().unwrap();
        assert!(target.starts_with(&root) && target != root);
        assert_eq!(target.parent(), Some(root.as_path()));
        fs::remove_dir_all(target).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(root.parent(), Some(temporary.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-dependent-resource-refresh-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

/// Per-trajectory atomics observe worker execution, even when the test runner is parallel.
#[derive(Default)]
struct Probe {
    builds: AtomicUsize,
    decodes: AtomicUsize,
    build_gate: Mutex<Option<Arc<Gate>>>,
    decode_gate: Mutex<Option<Arc<Gate>>>,
}

impl Probe {
    fn calls(&self) -> (usize, usize) {
        (
            self.builds.load(Ordering::SeqCst),
            self.decodes.load(Ordering::SeqCst),
        )
    }
}

fn probe<const CASE: usize>() -> &'static Probe {
    static PROBES: [OnceLock<Probe>; 6] = [const { OnceLock::new() }; 6];
    PROBES[CASE].get_or_init(Probe::default)
}

#[derive(Default)]
struct Gate {
    reached: AtomicBool,
    succeeded: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}

impl Gate {
    fn block(&self, succeeded: bool) {
        self.succeeded.store(succeeded, Ordering::SeqCst);
        self.reached.store(true, Ordering::SeqCst);
        let released = self.released.lock().unwrap();
        let (released, _) = self
            .wake
            .wait_timeout_while(released, Duration::from_secs(30), |released| !*released)
            .unwrap();
        assert!(
            *released,
            "the consumer must release its finite worker gate"
        );
    }
}

/// Releases a worker during assertion unwinding, before managed Runtime cleanup starts.
struct HeldGate(Arc<Gate>);

impl HeldGate {
    fn install(slot: &Mutex<Option<Arc<Gate>>>) -> Self {
        let gate = Arc::new(Gate::default());
        let mut slot = slot.lock().unwrap();
        assert!(slot.is_none());
        *slot = Some(Arc::clone(&gate));
        Self(gate)
    }

    fn reached(&self) -> bool {
        self.0.reached.load(Ordering::SeqCst)
    }

    fn release(self) {
        drop(self);
    }
}

impl Drop for HeldGate {
    fn drop(&mut self) {
        *self.0.released.lock().unwrap() = true;
        self.0.wake.notify_all();
    }
}

struct CountedBundle<const CASE: usize>;
struct CountedCodec<const CASE: usize>;

impl<const CASE: usize> AssetDomain for CountedBundle<CASE> {
    type Product = TextBundle;

    fn contains_product(product: &TextBundle, id: ImportedProductId) -> bool {
        TextBundleDomain::contains_product(product, id)
    }
}

impl<const CASE: usize> SourceAssetImporter for CountedBundle<CASE> {
    type Settings = TextBundleSettings;
    const SOURCE_KIND: &'static str = TextBundleDomain::SOURCE_KIND;

    fn limits() -> AssetImportLimits {
        TextBundleDomain::limits()
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<TextBundle, AssetImportError<E>> {
        TextBundleDomain::prepare(source)
    }
}

impl<const CASE: usize> SourceAssetBuild for CountedBundle<CASE> {
    type Codec = CountedCodec<CASE>;
    const BUILD_ID: &'static str = TextBundleDomain::BUILD_ID;
    const BUILD_REVISION: u32 = TextBundleDomain::BUILD_REVISION;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        TextBundleDomain::normalize_settings(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<TextBundle, AssetImportError<E>> {
        let probe = probe::<CASE>();
        let gate = probe.build_gate.lock().unwrap().take();
        probe.builds.fetch_add(1, Ordering::SeqCst);
        let result = TextBundleDomain::build(source);
        if let Some(gate) = gate {
            gate.block(result.is_ok());
        }
        result
    }

    fn visit_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_products(product, visitor)
    }
}

impl<const CASE: usize> RuntimeAssetCodec<TextBundle> for CountedCodec<CASE> {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TextBundleDomain::runtime_limits()
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(product)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        let probe = probe::<CASE>();
        let gate = probe.decode_gate.lock().unwrap().take();
        probe.decodes.fetch_add(1, Ordering::SeqCst);
        let result = TextBundleDomain::decode_runtime(bytes);
        if let Some(gate) = gate {
            gate.block(result.is_ok());
        }
        result
    }

    fn runtime_retained_bytes(product: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(product)
    }

    fn visit_runtime_dependencies<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_runtime_dependencies(product, visitor)
    }

    fn contains_runtime_product(product: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(product, reference)
    }
}

#[derive(Clone, Default, Resource)]
struct SourceInput(Arc<Mutex<Vec<AssetSourceChange>>>);

fn publish_source_changes(input: Res<SourceInput>, mut changes: ResMut<AssetSourceChanges>) {
    for change in std::mem::take(&mut *input.0.lock().unwrap()) {
        changes.push(change).unwrap();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stats {
    spawned: u64,
    built: u64,
    reused: u64,
    adopted: u64,
    failed: u64,
    rejected: u64,
    stale: u64,
    pending: usize,
}

struct Live<const CASE: usize> {
    runtime: RuntimeInstance,
    input: SourceInput,
    handle: Handle<BuiltSourceAsset<TextBundle>>,
}

impl<const CASE: usize> Live<CASE> {
    fn stats(&self) -> Stats {
        let stats = self
            .runtime
            .world()
            .resource::<SourceAssetBuildStats<CountedBundle<CASE>>>();
        Stats {
            spawned: stats.spawned,
            built: stats.built,
            reused: stats.reused,
            adopted: stats.adopted,
            failed: stats.failed,
            rejected: stats.rejected,
            stale: stats.stale,
            pending: stats.pending,
        }
    }

    fn current(&self) -> Option<&BuiltSourceAsset<TextBundle>> {
        self.runtime
            .world()
            .resource::<Assets<BuiltSourceAsset<TextBundle>>>()
            .get(self.handle)
    }

    fn graph(&self) -> &AssetDependencyGraph {
        self.runtime.world().resource::<AssetDependencyGraph>()
    }

    fn change(&self, name: &str, kind: AssetSourceChangeKind) {
        self.input
            .0
            .lock()
            .unwrap()
            .push(AssetSourceChange::new(canonical(name), kind));
    }

    fn advance(&mut self) {
        self.runtime.drive(Duration::ZERO).unwrap();
        thread::yield_now();
    }

    fn until(&mut self, reason: &str, mut complete: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        while !complete(self) {
            assert!(
                Instant::now() < deadline,
                "{reason}: {:?}; calls {:?}",
                self.stats(),
                probe::<CASE>().calls(),
            );
            self.advance();
        }
    }

    fn wait_adopted(&mut self, adopted: u64, first: &str, second: &str) {
        self.until("typed source adoption did not finish", |live| {
            live.stats().adopted >= adopted && live.stats().pending == 0
        });
        assert_eq!(self.stats().adopted, adopted);
        assert_eq!(self.stats().rejected, 0);
        assert_eq!(
            self.runtime
                .world()
                .resource::<AssetStates>()
                .state(self.handle.id())
                .unwrap()
                .load_state(),
            &LoadState::Loaded,
        );
        assert_text(self.current().unwrap(), first, second);
    }

    fn refresh(&mut self, name: &str, kind: AssetSourceChangeKind, first: &str, second: &str) {
        let before = self.stats();
        let decodes = probe::<CASE>().calls().1;
        self.change(name, kind);
        self.wait_adopted(before.adopted + 1, first, second);
        assert_eq!(self.stats().spawned, before.spawned + 1);
        assert_eq!(probe::<CASE>().calls().1, decodes + 1);
    }

    fn fail_change(&mut self, name: &str, kind: AssetSourceChangeKind) {
        let failed = self.stats().failed;
        self.change(name, kind);
        self.until("executed source failure did not settle", |live| {
            live.stats().failed > failed && live.stats().pending == 0
        });
        assert_eq!(self.stats().failed, failed + 1);
        assert_eq!(self.stats().rejected, 0);
    }

    fn assert_no_refresh(&mut self, name: &str, kind: AssetSourceChangeKind) {
        assert_eq!(self.stats().pending, 0);
        let stats = self.stats();
        let calls = probe::<CASE>().calls();
        self.change(name, kind);
        for _ in 0..4 {
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
            stats,
            "an unrelated observation scheduled work"
        );
        assert_eq!(
            probe::<CASE>().calls(),
            calls,
            "unrelated input built or decoded",
        );
    }

    fn assert_paths(&self, published: &[&str], retry: &[&str]) {
        let source = reference(id(1)).source();
        for (kind, expected) in [
            (SourceSubscriptionKind::Published, published),
            (SourceSubscriptionKind::Retry, retry),
        ] {
            let mut expected = expected
                .iter()
                .map(|name| canonical(name))
                .collect::<Vec<_>>();
            expected.sort_unstable();
            assert_eq!(
                self.graph().source_subscription_paths(source, kind),
                expected.as_slice(),
            );
        }
        let expected = published
            .iter()
            .chain(retry)
            .map(|name| canonical(name))
            .collect::<BTreeSet<_>>();
        for path in expected {
            assert_eq!(
                self.graph()
                    .raw_dependents_for_path(&path)
                    .collect::<Vec<_>>(),
                vec![source],
            );
        }
    }

    fn assert_unsubscribed(&self, name: &str) {
        assert!(
            self.graph()
                .raw_dependents_for_path(&canonical(name))
                .next()
                .is_none()
        );
    }
}

fn with_live<const CASE: usize, T>(
    fixture: &Fixture,
    exercise: impl FnOnce(&mut Live<CASE>) -> T,
) -> T {
    let input = SourceInput::default();
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
        SourceAssetBuildPlugin::<CountedBundle<CASE>>::new(
            Arc::new(fixture.store("cache-a")),
            directory(&fixture.root.join("source"), CapabilityRights::ReadOnly),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            definition(),
        )
        .with_pending_limit(ItemLimit::new(2).unwrap()),
    )
    .unwrap();
    app.insert_resource(input.clone()).unwrap();
    app.add_systems(CoreStage::First, publish_source_changes)
        .unwrap();
    let record = AssetRecord::new(
        StableAssetId::parse_str(SOURCE).unwrap(),
        canonical("bundle.json"),
        AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
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
        .reserve_record::<BuiltSourceAsset<TextBundle>>(&record)
        .unwrap();
    let runtime = match RuntimeInstance::start(app) {
        Ok(runtime) => runtime,
        Err(mut failure) => {
            let error = format!("{failure:?}");
            failure.drive_retirement();
            drop(failure);
            panic!("independent source consumer startup failed: {error}");
        }
    };
    let mut live = Live {
        runtime,
        input,
        handle,
    };
    assert!(live.current().is_none());
    live.assert_paths(&[], &[]);
    live.change("bundle.json", AssetSourceChangeKind::Modified);
    let result = catch_unwind(AssertUnwindSafe(|| exercise(&mut live)));
    let closed = live.runtime.finish().map_err(|owner| {
        let error = owner.to_string();
        drop(owner);
        error
    });
    match result {
        Ok(value) => {
            closed.expect("the source consumer's real Runtime must close physically");
            value
        }
        Err(failure) => resume_unwind(failure),
    }
}

fn assert_text(owner: &BuiltSourceAsset<TextBundle>, first: &str, second: &str) {
    let product = owner.value();
    assert_eq!(product.source(), reference(id(1)).source());
    assert_eq!(
        product.entries().keys().copied().collect::<Vec<_>>(),
        vec![id(1), id(2)],
    );
    assert_eq!(product.get(&reference(id(1))).unwrap().text, first);
    assert_eq!(product.get(&reference(id(2))).unwrap().text, second);
    let retained = TextBundleDomain::runtime_retained_bytes(product).unwrap();
    assert!(retained <= TextBundleDomain::runtime_limits().retained_bytes().get());
    assert!(owner.decoded_retained_bytes() > retained);
    assert!(owner.decoded_retained_bytes() <= ArtifactBuildLimits::default().retained_bytes.get());
    let group = owner.group();
    assert_eq!(group.members().len(), 1);
    assert_eq!(group.members()[0].name, "runtime");
    assert_eq!(
        group.members()[0]
            .products
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        [
            reference(ImportedProductId::PRIMARY),
            reference(id(1)),
            reference(id(2)),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(
        &*TextBundleDomain::encode_runtime(product).unwrap(),
        group.member_bytes("runtime").unwrap(),
    );
}

fn assert_presence(owner: &BuiltSourceAsset<TextBundle>, name: &str, expected: Option<&[u8]>) {
    let digest = owner
        .group()
        .observations()
        .iter()
        .find_map(|observation| match observation {
            ArtifactBuildObservation::Raw { path, digest } if *path == canonical(name) => {
                Some(*digest)
            }
            _ => None,
        });
    assert_eq!(digest, Some(expected.map(ArtifactContentDigest::of_bytes)));
}

#[test]
fn live_dependencies_rebind_and_warm_reuse_without_changing_escaped_runtime_bases() {
    let fixture = Fixture::new();
    fixture.write_source("a.txt", b" A");
    fixture.write_source("b.txt", b" B");
    let delivery = runtime::Delivery::new(id(1), id(2));
    let delivery_store = fixture.store("cache-b");
    let (original, original_base, successor, successor_base) =
        with_live::<0, _>(&fixture, |live| {
            live.wait_adopted(1, "Hello A", "Goodbye A");
            assert_eq!(probe::<0>().calls(), (1, 1));
            assert_eq!(live.stats().built, 1);
            assert_eq!(live.stats().reused, 0);
            live.assert_paths(
                &[
                    "bundle.json",
                    "bundle.json.meta",
                    "a.txt",
                    "build-suffix.txt",
                ],
                &[],
            );
            assert_presence(live.current().unwrap(), "a.txt", Some(b" A"));
            assert_presence(live.current().unwrap(), "build-suffix.txt", None);
            let original = live.current().unwrap().clone();
            let group = delivery_store
                .commit(delivery.candidate(original.group()), |_| Ok(()))
                .unwrap()
                .into_group();
            let original_base = delivery.pack(&fixture, &group);
            assert!(original_base.validate_deep().is_ok());

            fixture.write_source("a.txt", b" changed");
            live.refresh(
                "a.txt",
                AssetSourceChangeKind::Modified,
                "Hello changed",
                "Goodbye changed",
            );
            assert_presence(live.current().unwrap(), "a.txt", Some(b" changed"));
            fixture.write_source("build-suffix.txt", b" +");
            live.refresh(
                "build-suffix.txt",
                AssetSourceChangeKind::Modified,
                "Hello changed +",
                "Goodbye changed +",
            );
            assert_presence(live.current().unwrap(), "build-suffix.txt", Some(b" +"));
            fixture.write_source("build-suffix.txt", b" ++");
            live.refresh(
                "build-suffix.txt",
                AssetSourceChangeKind::Modified,
                "Hello changed ++",
                "Goodbye changed ++",
            );
            fixture.remove_source("build-suffix.txt");
            live.refresh(
                "build-suffix.txt",
                AssetSourceChangeKind::Removed,
                "Hello changed",
                "Goodbye changed",
            );
            assert_presence(live.current().unwrap(), "build-suffix.txt", None);

            let before_failure = live.current().unwrap().clone();
            let version = live
                .runtime
                .world()
                .resource::<AssetStates>()
                .state(live.handle.id())
                .unwrap()
                .usable_version();
            assert!(version.is_some());
            let calls = probe::<0>().calls();
            fixture.remove_source("a.txt");
            live.fail_change("a.txt", AssetSourceChangeKind::Removed);
            assert_eq!(
                live.current().unwrap().group().digest(),
                before_failure.group().digest()
            );
            assert_text(live.current().unwrap(), "Hello changed", "Goodbye changed");
            assert_eq!(probe::<0>().calls(), (calls.0 + 1, calls.1));
            assert_eq!(
                live.runtime
                    .world()
                    .resource::<AssetStates>()
                    .state(live.handle.id())
                    .unwrap()
                    .usable_version(),
                version,
            );
            live.assert_paths(
                &[
                    "bundle.json",
                    "bundle.json.meta",
                    "a.txt",
                    "build-suffix.txt",
                ],
                &["bundle.json", "bundle.json.meta", "a.txt"],
            );
            fixture.write_source("a.txt", b" recovered");
            live.refresh(
                "a.txt",
                AssetSourceChangeKind::Modified,
                "Hello recovered",
                "Goodbye recovered",
            );

            fixture.rebind(SELECT_B);
            live.refresh(
                "bundle.json",
                AssetSourceChangeKind::Modified,
                "Welcome B",
                "Farewell B",
            );
            assert_eq!(
                live.current()
                    .unwrap()
                    .value()
                    .get(&reference(id(1)))
                    .unwrap()
                    .name,
                "Greeting",
            );
            assert_eq!(
                live.current()
                    .unwrap()
                    .value()
                    .get(&reference(id(2)))
                    .unwrap()
                    .name,
                "Closing",
            );
            live.assert_paths(
                &[
                    "bundle.json",
                    "bundle.json.meta",
                    "b.txt",
                    "build-suffix.txt",
                ],
                &[],
            );
            live.assert_unsubscribed("a.txt");
            fixture.remove_source("a.txt");
            live.assert_no_refresh("a.txt", AssetSourceChangeKind::Removed);
            fixture.write_source("unrelated.txt", b"not consumed by this domain");
            live.assert_no_refresh("unrelated.txt", AssetSourceChangeKind::Modified);
            fixture.write_source("b.txt", b" rebound");
            live.refresh(
                "b.txt",
                AssetSourceChangeKind::Modified,
                "Welcome rebound",
                "Farewell rebound",
            );

            let last_good = live.current().unwrap().clone();
            fixture.rebind(SELECT_C);
            let calls = probe::<0>().calls();
            live.fail_change("bundle.json", AssetSourceChangeKind::Modified);
            assert_text(
                live.current().unwrap(),
                "Welcome rebound",
                "Farewell rebound",
            );
            assert_eq!(
                live.current().unwrap().group().digest(),
                last_good.group().digest()
            );
            assert_eq!(probe::<0>().calls(), (calls.0 + 1, calls.1));
            live.assert_paths(
                &[
                    "bundle.json",
                    "bundle.json.meta",
                    "b.txt",
                    "build-suffix.txt",
                ],
                &["bundle.json", "bundle.json.meta", "c.txt"],
            );
            fixture.write_source("c.txt", b" C");
            live.refresh(
                "c.txt",
                AssetSourceChangeKind::Modified,
                "Welcome C",
                "Farewell C",
            );
            live.assert_paths(
                &[
                    "bundle.json",
                    "bundle.json.meta",
                    "c.txt",
                    "build-suffix.txt",
                ],
                &[],
            );
            live.assert_unsubscribed("b.txt");
            fixture.remove_source("b.txt");
            live.assert_no_refresh("b.txt", AssetSourceChangeKind::Removed);
            let successor = live.current().unwrap().clone();
            assert_ne!(original.group().digest(), successor.group().digest());
            assert_eq!(
                original.group().members()[0].products,
                successor.group().members()[0].products
            );
            let group = delivery_store
                .commit(delivery.candidate(successor.group()), |_| Ok(()))
                .unwrap()
                .into_group();
            let successor_base = delivery.pack(&fixture, &group);
            assert!(successor_base.validate_deep().is_ok());
            (original, original_base, successor, successor_base)
        });

    let before_warm = probe::<0>().calls();
    with_live::<0, _>(&fixture, |live| {
        live.wait_adopted(1, "Welcome C", "Farewell C");
        assert_eq!(probe::<0>().calls(), (before_warm.0, before_warm.1 + 1));
        assert_eq!(live.stats().built, 0);
        assert_eq!(live.stats().reused, 1);
        assert_eq!(
            live.current().unwrap().group().digest(),
            successor.group().digest()
        );
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "c.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("a.txt");
        live.assert_unsubscribed("b.txt");
        fixture.write_source("a.txt", b"obsolete");
        live.assert_no_refresh("a.txt", AssetSourceChangeKind::Modified);
        fixture.write_source("c.txt", b" warm");
        live.refresh(
            "c.txt",
            AssetSourceChangeKind::Modified,
            "Welcome warm",
            "Farewell warm",
        );
        assert_eq!(probe::<0>().calls(), (before_warm.0 + 1, before_warm.1 + 2));
        fixture.write_source("build-suffix.txt", b" reused subscription");
        live.refresh(
            "build-suffix.txt",
            AssetSourceChangeKind::Modified,
            "Welcome warm reused subscription",
            "Farewell warm reused subscription",
        );
        fixture.write_source("unrelated.txt", b"still unrelated after restart");
        live.assert_no_refresh("unrelated.txt", AssetSourceChangeKind::Modified);
    });

    drop(delivery_store);
    fixture.remove_tree("cache-a");
    fixture.remove_tree("cache-b");
    fixture.remove_tree("source");
    assert_text(&original, "Hello A", "Goodbye A");
    assert_text(&successor, "Welcome C", "Farewell C");
    assert!(original_base.validate_deep().is_ok());
    assert!(successor_base.validate_deep().is_ok());
    delivery.run_managed(original_base, "Hello A", "Goodbye A");
    delivery.run_managed(successor_base, "Welcome C", "Farewell C");
}

#[test]
fn missing_required_input_recovers_first_load_from_its_creation_observation() {
    let fixture = Fixture::new();
    with_live::<1, _>(&fixture, |live| {
        live.until("missing required input did not fail", |live| {
            live.stats().failed == 1 && live.stats().pending == 0
        });
        assert!(live.current().is_none());
        assert_eq!(probe::<1>().calls(), (1, 0));
        assert_eq!(live.stats().adopted, 0);
        live.assert_paths(&[], &["bundle.json", "bundle.json.meta", "a.txt"]);
        fixture.write_source("unrelated.txt", b"must not rescue first load");
        live.assert_no_refresh("unrelated.txt", AssetSourceChangeKind::Modified);
        fixture.write_source("a.txt", b" created");
        live.refresh(
            "a.txt",
            AssetSourceChangeKind::Modified,
            "Hello created",
            "Goodbye created",
        );
        assert_eq!(probe::<1>().calls(), (2, 1));
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &[],
        );
    });
}

#[test]
fn creation_during_a_blocked_failed_build_cannot_strand_first_load() {
    let fixture = Fixture::new();
    with_live::<2, _>(&fixture, |live| {
        let gate = HeldGate::install(&probe::<2>().build_gate);
        live.until(
            "the real missing-input build did not reach its gate",
            |_| gate.reached(),
        );
        assert!(!gate.0.succeeded.load(Ordering::SeqCst));
        assert!(live.current().is_none());
        assert_eq!(probe::<2>().calls(), (1, 0));
        live.assert_paths(&[], &[]);
        live.assert_unsubscribed("a.txt");

        fixture.write_source("a.txt", b" arrived during failure");
        live.change("a.txt", AssetSourceChangeKind::Modified);
        live.advance();
        assert!(live.input.0.lock().unwrap().is_empty());
        assert!(
            live.runtime
                .world()
                .resource::<AssetSourceChanges>()
                .is_empty()
        );
        assert_eq!(live.stats().spawned, 1);
        assert_eq!(probe::<2>().calls(), (1, 0));
        live.assert_paths(&[], &[]);
        gate.release();

        live.wait_adopted(
            1,
            "Hello arrived during failure",
            "Goodbye arrived during failure",
        );
        assert_eq!(probe::<2>().calls(), (2, 1));
        assert_eq!(live.stats().spawned, 2);
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        fixture.write_source("a.txt", b" next");
        live.refresh(
            "a.txt",
            AssetSourceChangeKind::Modified,
            "Hello next",
            "Goodbye next",
        );

        let before = live.stats();
        let calls = probe::<2>().calls();
        let oversized = vec![b'x'; MAX_BUNDLE_SUFFIX_BYTES + 1];
        fixture.write_source("b.txt", &oversized);
        let gate = HeldGate::install(&probe::<2>().build_gate);
        fixture.rebind(SELECT_B);
        live.change("bundle.json", AssetSourceChangeKind::Modified);
        live.until(
            "the real source-byte refusal did not reach its gate",
            |_| gate.reached(),
        );
        assert!(!gate.0.succeeded.load(Ordering::SeqCst));
        assert_eq!(probe::<2>().calls(), (calls.0 + 1, calls.1));
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("b.txt");

        fixture.write_source("b.txt", b" repaired");
        live.change("b.txt", AssetSourceChangeKind::Modified);
        live.advance();
        assert!(live.input.0.lock().unwrap().is_empty());
        assert!(
            live.runtime
                .world()
                .resource::<AssetSourceChanges>()
                .is_empty()
        );
        assert_eq!(live.stats().spawned, before.spawned + 1);
        assert_text(live.current().unwrap(), "Hello next", "Goodbye next");
        gate.release();
        live.wait_adopted(before.adopted + 1, "Welcome repaired", "Farewell repaired");
        assert_eq!(probe::<2>().calls(), (calls.0 + 2, calls.1 + 1));
        assert_eq!(live.stats().spawned, before.spawned + 2);
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "b.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("a.txt");

        let last_good = live.current().unwrap().clone();
        fixture.write_source("c.txt", &oversized);
        fixture.rebind(SELECT_C);
        let calls = probe::<2>().calls();
        live.fail_change("bundle.json", AssetSourceChangeKind::Modified);
        assert_eq!(probe::<2>().calls(), (calls.0 + 1, calls.1));
        assert_eq!(
            live.current().unwrap().group().digest(),
            last_good.group().digest()
        );
        assert_text(
            live.current().unwrap(),
            "Welcome repaired",
            "Farewell repaired",
        );
        assert!(matches!(
            live.runtime
                .world()
                .resource::<AssetStates>()
                .state(live.handle.id())
                .unwrap()
                .load_state(),
            LoadState::Failed { .. },
        ));
        assert!(
            live.runtime
                .world()
                .resource::<AssetSourceChanges>()
                .is_empty()
        );
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "b.txt",
                "build-suffix.txt",
            ],
            &["bundle.json", "bundle.json.meta", "c.txt"],
        );
        fixture.write_source("unrelated.txt", b"cannot repair a persistent byte refusal");
        live.assert_no_refresh("unrelated.txt", AssetSourceChangeKind::Modified);
        fixture.write_source("c.txt", b" within budget");
        live.refresh(
            "c.txt",
            AssetSourceChangeKind::Modified,
            "Welcome within budget",
            "Farewell within budget",
        );
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "c.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("b.txt");
    });
}

#[test]
fn current_metadata_adoption_acknowledges_the_original_dirty_observation() {
    let fixture = Fixture::new();
    fixture.write_source("a.txt", b" original");
    with_live::<5, _>(&fixture, |live| {
        live.wait_adopted(1, "Hello original", "Goodbye original");
        let original = live.current().unwrap().clone();
        let mut metadata = fs::read(fixture.root.join("source/bundle.json.meta")).unwrap();
        metadata.push(b'\n');
        fixture.write_source("bundle.json.meta", &metadata);
        let gate = HeldGate::install(&probe::<5>().decode_gate);
        live.change("bundle.json", AssetSourceChangeKind::MetaModified);
        live.until("metadata candidate did not reach its decode gate", |_| {
            gate.reached()
        });
        assert!(
            live.runtime
                .world()
                .resource::<AssetStates>()
                .metadata_change_pending(live.handle.id())
        );
        assert_eq!(probe::<5>().calls(), (1, 2));
        gate.release();
        live.wait_adopted(2, "Hello original", "Goodbye original");
        assert!(
            !live
                .runtime
                .world()
                .resource::<AssetStates>()
                .metadata_change_pending(live.handle.id())
        );
        assert_eq!(
            live.current().unwrap().group().digest(),
            original.group().digest()
        );
        assert_eq!(probe::<5>().calls(), (1, 2));
        assert_eq!(live.stats().reused, 1);
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &[],
        );
    });
}

#[test]
fn a_new_input_changed_during_successful_decode_is_revalidated_before_adoption() {
    let fixture = Fixture::new();
    fixture.write_source("a.txt", b" original");
    fixture.write_source("b.txt", b" old candidate");
    with_live::<4, _>(&fixture, |live| {
        live.wait_adopted(1, "Hello original", "Goodbye original");
        let original = live.current().unwrap().clone();
        let gate = HeldGate::install(&probe::<4>().decode_gate);
        fixture.rebind(SELECT_B);
        live.change("bundle.json", AssetSourceChangeKind::Modified);
        live.until(
            "new input candidate did not reach its successful decode gate",
            |_| gate.reached(),
        );
        assert!(gate.0.succeeded.load(Ordering::SeqCst));
        assert_eq!(probe::<4>().calls(), (2, 2));
        live.assert_unsubscribed("b.txt");
        let generation = live
            .runtime
            .world()
            .resource::<nara_asset::AssetLoadGenerations>()
            .current(live.handle.id());
        fixture.write_source("b.txt", b" current");
        live.change("b.txt", AssetSourceChangeKind::Modified);
        live.advance();
        assert!(live.input.0.lock().unwrap().is_empty());
        assert_eq!(
            live.runtime
                .world()
                .resource::<nara_asset::AssetLoadGenerations>()
                .current(live.handle.id()),
            generation
        );
        assert_eq!(
            live.current().unwrap().group().digest(),
            original.group().digest()
        );
        assert_eq!(live.stats().adopted, 1);
        gate.release();
        live.wait_adopted(2, "Welcome current", "Farewell current");
        assert_eq!(probe::<4>().calls(), (3, 3));
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "b.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("a.txt");
        assert_text(&original, "Hello original", "Goodbye original");
        live.assert_no_refresh("a.txt", AssetSourceChangeKind::Modified);
    });
}

#[test]
fn superseded_success_cannot_change_the_value_or_either_subscription_set() {
    let fixture = Fixture::new();
    fixture.write_source("a.txt", b" original");
    fixture.write_source("b.txt", b" superseded");
    fixture.write_source("c.txt", b" current");
    with_live::<3, _>(&fixture, |live| {
        live.wait_adopted(1, "Hello original", "Goodbye original");
        let original = live.current().unwrap().clone();
        fixture.rebind(SELECT_MISSING);
        live.fail_change("bundle.json", AssetSourceChangeKind::Modified);
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &["bundle.json", "bundle.json.meta", "retry.txt"],
        );
        let before = live.stats();
        let old = HeldGate::install(&probe::<3>().decode_gate);
        fixture.rebind(SELECT_B);
        live.change("bundle.json", AssetSourceChangeKind::Modified);
        live.until("old successful decode did not reach its gate", |_| {
            old.reached()
        });
        assert!(old.0.succeeded.load(Ordering::SeqCst));
        assert_eq!(probe::<3>().calls(), (3, 2));
        assert_text(
            live.current().unwrap(),
            "Hello original",
            "Goodbye original",
        );
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &["bundle.json", "bundle.json.meta", "retry.txt"],
        );

        let current = HeldGate::install(&probe::<3>().decode_gate);
        fixture.rebind(SELECT_C);
        live.change("bundle.json", AssetSourceChangeKind::Modified);
        live.advance();
        assert!(live.input.0.lock().unwrap().is_empty());
        old.release();
        live.until("successor decode did not reach its gate", |_| {
            current.reached()
        });
        live.until("superseded completion was not retired", |live| {
            live.stats().stale > before.stale
        });
        assert!(current.0.succeeded.load(Ordering::SeqCst));
        assert_eq!(live.stats().adopted, before.adopted);
        assert_eq!(live.stats().failed, before.failed);
        assert_eq!(
            live.current().unwrap().group().digest(),
            original.group().digest()
        );
        assert_text(
            live.current().unwrap(),
            "Hello original",
            "Goodbye original",
        );
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "a.txt",
                "build-suffix.txt",
            ],
            &["bundle.json", "bundle.json.meta", "retry.txt"],
        );
        live.assert_unsubscribed("b.txt");
        live.assert_unsubscribed("c.txt");
        current.release();
        live.wait_adopted(2, "Welcome current", "Farewell current");
        live.assert_paths(
            &[
                "bundle.json",
                "bundle.json.meta",
                "c.txt",
                "build-suffix.txt",
            ],
            &[],
        );
        live.assert_unsubscribed("a.txt");
        live.assert_unsubscribed("b.txt");
        live.assert_unsubscribed("retry.txt");
        assert_eq!(probe::<3>().calls(), (4, 3));
        fixture.remove_source("b.txt");
        live.assert_no_refresh("b.txt", AssetSourceChangeKind::Removed);
        fixture.remove_source("a.txt");
        live.assert_no_refresh("a.txt", AssetSourceChangeKind::Removed);
        fixture.write_source("c.txt", b" after stale");
        live.refresh(
            "c.txt",
            AssetSourceChangeKind::Modified,
            "Welcome after stale",
            "Farewell after stale",
        );
        assert_text(&original, "Hello original", "Goodbye original");
    });
}
