//! Decode and lifetime checks against the actual shared store reservations.

use std::{
    cell::RefCell,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetProductRef,
    AssetRuntimeError, AssetRuntimeLimits, ImportedProductId, SourceAssetImporter, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion, ItemLimit};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};

use super::*;
use crate::asset_build::{
    ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildOutput, ArtifactBuildSeed,
    ArtifactContentDigest, ArtifactGroupCandidate,
};

const CODEC_WORK: usize = 2048;
const PRODUCT_PROMISE: usize = 4096;
const NORMAL: u8 = 0;
const MISSING_PRODUCT: u8 = 1;
const EXTRA_PRODUCT: u8 = 2;
const DUPLICATE_PRODUCT: u8 = 3;
const FOREIGN_SOURCE: u8 = 4;
const DOMAIN_REJECTS: u8 = 5;
const CODEC_REJECTS: u8 = 6;
const EXCESS_RETENTION: u8 = 7;
const SHORT_RETENTION: u8 = 8;
const MIXED_DEMAND: u8 = 9;
const DECODE_PANICS: u8 = 10;
const QUERY_PANICS: u8 = 11;
const RETENTION_PANICS: u8 = 12;
const DROP_PANICS: u8 = 13;
const QUERY_AND_DROP_PANIC: u8 = 14;
const IGNORE_PRODUCT_FAILURE: u8 = 15;
const IGNORE_DEPENDENCY_FAILURE: u8 = 16;
const RETENTION_REJECTS: u8 = 17;

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static DROP_OBSERVER: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
}

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara-live-source-value-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn store(&self, limits: ArtifactBuildLimits) -> ArtifactBuildStore {
        let directory = DirectoryCapability::from_host_handle(
            directory_handle(&self.0),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap();
        ArtifactBuildStore::open(directory, limits).unwrap()
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(windows)]
fn directory_handle(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)
        .unwrap()
}

#[cfg(target_os = "linux")]
fn directory_handle(path: &Path) -> File {
    File::open(path).unwrap()
}

fn limits() -> ArtifactBuildLimits {
    ArtifactBuildLimits {
        metadata_bytes: ByteLimit::new(64 * 1024).unwrap(),
        member_bytes: ByteLimit::new(1024).unwrap(),
        group_bytes: ByteLimit::new(4096).unwrap(),
        retained_bytes: ByteLimit::new(32 * 1024).unwrap(),
        work_bytes: ByteLimit::new(8 * 1024 * 1024).unwrap(),
        members: ItemLimit::new(16).unwrap(),
        observations: ItemLimit::new(16).unwrap(),
        index_entries: ItemLimit::new(16).unwrap(),
    }
}

fn product(index: usize) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("00000000-0000-4000-8000-{index:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn auxiliary() -> AssetProductRef {
    AssetProductRef::new(
        product(1).source(),
        ImportedProductId::parse_subproduct("00000000-0000-4000-8000-00000000000a").unwrap(),
    )
    .unwrap()
}

fn diagnostic() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("test.live-source-codec-rejected").unwrap(),
        SafeSummary::new("The fixture Runtime value was rejected").unwrap(),
    )
}

struct FixtureProduct {
    mode: u8,
    products: Vec<AssetProductRef>,
    dependencies: Vec<(AssetProductRef, AssetReferenceDemand)>,
    payload: Box<[u8]>,
}

impl Drop for FixtureProduct {
    fn drop(&mut self) {
        let observer = DROP_OBSERVER.with(|observer| observer.borrow_mut().take());
        if let Some(observer) = observer {
            observer();
        }
        if matches!(self.mode, DROP_PANICS | QUERY_AND_DROP_PANIC) {
            panic!("fixture destructor panic");
        }
    }
}

struct FixtureDomain;
struct FixtureCodec;

impl AssetDomain for FixtureDomain {
    type Product = FixtureProduct;

    fn contains_product(value: &FixtureProduct, id: ImportedProductId) -> bool {
        value.mode != DOMAIN_REJECTS
            && value
                .products
                .iter()
                .any(|reference| reference.product() == id)
    }
}

impl SourceAssetImporter for FixtureDomain {
    type Settings = ();

    const SOURCE_KIND: &'static str = "live-source-fixture";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(1024).unwrap(),
            ByteLimit::new(CODEC_WORK).unwrap(),
            ByteLimit::new(PRODUCT_PROMISE).unwrap(),
        )
    }

    fn prepare<E>(_: &mut AssetImportSource<'_, E>) -> Result<FixtureProduct, AssetImportError<E>> {
        Err(AssetImportError::domain(diagnostic()))
    }
}

impl SourceAssetBuild for FixtureDomain {
    type Codec = FixtureCodec;

    const BUILD_ID: &'static str = "test.live-source-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(_: ()) -> Result<(), Diagnostic> {
        Ok(())
    }

    fn visit_products<E>(
        value: &FixtureProduct,
        mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        if matches!(value.mode, QUERY_PANICS | QUERY_AND_DROP_PANIC) {
            panic!("fixture membership query panic");
        }
        for reference in &value.products {
            if value.mode == IGNORE_PRODUCT_FAILURE {
                let _ = visitor(*reference);
            } else {
                visitor(*reference)?;
            }
        }
        Ok(())
    }
}

impl RuntimeAssetCodec<FixtureProduct> for FixtureCodec {
    const RUNTIME_KIND: &'static str = "live_source_fixture";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(1024).unwrap(),
            ByteLimit::new(CODEC_WORK).unwrap(),
            ByteLimit::new(PRODUCT_PROMISE).unwrap(),
        )
    }

    fn encode_runtime(value: &FixtureProduct) -> Result<Box<[u8]>, AssetRuntimeError> {
        Ok(vec![value.mode].into_boxed_slice())
    }

    fn decode_runtime(bytes: &[u8]) -> Result<FixtureProduct, AssetRuntimeError> {
        let [mode] = bytes else {
            return Err(AssetRuntimeError::new(diagnostic()));
        };
        if *mode == DECODE_PANICS {
            panic!("fixture decoder panic");
        }
        let mut products = vec![product(1)];
        match *mode {
            MISSING_PRODUCT => products.clear(),
            EXTRA_PRODUCT | IGNORE_PRODUCT_FAILURE => products.push(auxiliary()),
            DUPLICATE_PRODUCT => products.push(product(1)),
            FOREIGN_SOURCE => products.push(product(2)),
            _ => {}
        }
        let dependencies = if matches!(*mode, MIXED_DEMAND | IGNORE_DEPENDENCY_FAILURE) {
            vec![
                (product(2), AssetReferenceDemand::Deferred),
                (product(2), AssetReferenceDemand::Immediate),
                (product(2), AssetReferenceDemand::Deferred),
            ]
        } else {
            Vec::new()
        };
        Ok(FixtureProduct {
            mode: *mode,
            products,
            dependencies,
            payload: vec![42; 512].into_boxed_slice(),
        })
    }

    fn runtime_retained_bytes(value: &FixtureProduct) -> Result<usize, AssetRuntimeError> {
        match value.mode {
            EXCESS_RETENTION => Ok(PRODUCT_PROMISE + 1),
            SHORT_RETENTION => Ok(size_of::<FixtureProduct>() - 1),
            RETENTION_PANICS => panic!("fixture retention query panic"),
            RETENTION_REJECTS => Err(AssetRuntimeError::new(diagnostic())),
            _ => Ok(size_of::<FixtureProduct>()
                + value.products.capacity() * size_of::<AssetProductRef>()
                + value.dependencies.capacity()
                    * size_of::<(AssetProductRef, AssetReferenceDemand)>()
                + value.payload.len()),
        }
    }

    fn visit_runtime_dependencies<E>(
        value: &FixtureProduct,
        mut visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        for (reference, demand) in &value.dependencies {
            if value.mode == IGNORE_DEPENDENCY_FAILURE {
                let _ = visitor(*reference, *demand);
            } else {
                visitor(*reference, *demand)?;
            }
        }
        Ok(())
    }

    fn contains_runtime_product(value: &FixtureProduct, reference: AssetProductRef) -> bool {
        value.mode != CODEC_REJECTS && value.products.contains(&reference)
    }
}

fn candidate(
    mode: u8,
    products: Vec<AssetProductRef>,
    dependencies: Vec<(AssetProductRef, AssetReferenceDemand)>,
) -> ArtifactGroupCandidate {
    let format = FormatKind::new(FixtureCodec::RUNTIME_KIND).unwrap();
    ArtifactGroupCandidate {
        seed: ArtifactBuildSeed {
            source: product(1),
            location: None,
            primary: Some(ArtifactContentDigest::of_bytes(&[mode])),
            definitions: vec![ArtifactBuildDefinition {
                id: FixtureDomain::BUILD_ID.to_owned(),
                revision: FormatVersion::ONE,
                implementation: ArtifactContentDigest::of_bytes(b"compiled live fixture"),
            }],
            settings: Vec::new(),
            target: Vec::new(),
            format: format.clone(),
            format_version: FormatVersion::ONE,
        },
        observations: Vec::new(),
        outputs: vec![ArtifactBuildOutput {
            name: "runtime".to_owned(),
            format,
            format_version: FormatVersion::ONE,
            products,
            dependencies,
            bytes: Arc::from([mode]),
        }],
    }
}

fn commit(store: &ArtifactBuildStore, input: ArtifactGroupCandidate) -> VerifiedArtifactGroup {
    store.commit(input, |_| Ok(())).unwrap().into_group()
}

fn group(store: &ArtifactBuildStore, mode: u8) -> VerifiedArtifactGroup {
    commit(store, candidate(mode, vec![product(1)], Vec::new()))
}

fn assert_retained_released(store: &ArtifactBuildStore, encoded: &VerifiedArtifactGroup) {
    let _all_free = store
        .reserve_retained(store.limits().retained_bytes.get() - encoded.retained_bytes())
        .unwrap();
}

#[test]
fn member_name_format_version_and_complete_group_are_checked_before_decode() {
    for mismatch in 0..5 {
        let root = TestRoot::new();
        let store = root.store(limits());
        let mut input = candidate(DECODE_PANICS, vec![product(1)], Vec::new());
        match mismatch {
            0 => input.outputs[0].name = "another".to_owned(),
            1 => input.outputs[0].format = FormatKind::new("another_format").unwrap(),
            2 => input.outputs[0].format_version = FormatVersion::new(2).unwrap(),
            3 => input.seed.format = FormatKind::new("another_format").unwrap(),
            _ => input.outputs.push(ArtifactBuildOutput {
                name: "auxiliary".to_owned(),
                format: input.seed.format.clone(),
                format_version: FormatVersion::ONE,
                products: vec![auxiliary()],
                dependencies: Vec::new(),
                bytes: Arc::from([DECODE_PANICS]),
            }),
        }
        let encoded = commit(&store, input);
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded.clone()),
            Err(ArtifactBuildError::InvalidCandidate(_))
        ));
        assert_retained_released(&store, &encoded);
    }
}

#[test]
fn escaped_non_clone_product_keeps_both_original_charges_through_destruction() {
    let root = TestRoot::new();
    let store = Arc::new(root.store(limits()));
    let encoded = group(&store, NORMAL);
    let encoded_retained = encoded.retained_bytes();
    let original = decode_source_build::<FixtureDomain>(&store, encoded.clone()).unwrap();
    let retained = original.decoded_retained_bytes();
    assert_eq!(
        retained,
        FixtureCodec::runtime_retained_bytes(original.value()).unwrap()
            + owner_overhead::<FixtureProduct>().unwrap()
    );
    assert!(retained < PRODUCT_PROMISE);
    let escaped = original.clone();
    assert!(std::ptr::eq(original.value(), escaped.value()));
    assert_eq!(escaped.group().digest(), encoded.digest());
    drop(original);
    drop(encoded);
    let _remaining = store
        .reserve_retained(store.limits().retained_bytes.get() - encoded_retained - retained)
        .unwrap();
    assert!(matches!(
        store.reserve_retained(1),
        Err(ArtifactBuildError::Budget {
            kind: "retained",
            ..
        })
    ));
    let observed = Arc::new(AtomicBool::new(false));
    let drop_observed = Arc::clone(&observed);
    let drop_store = Arc::clone(&store);
    DROP_OBSERVER.with(|observer| {
        *observer.borrow_mut() = Some(Box::new(move || {
            drop_observed.store(
                matches!(
                    drop_store.reserve_retained(1),
                    Err(ArtifactBuildError::Budget { .. })
                ),
                Ordering::Relaxed,
            );
        }));
    });
    drop(escaped);
    assert!(observed.load(Ordering::Relaxed));
    let _released = store.reserve_retained(encoded_retained + retained).unwrap();
}

#[test]
fn encoded_and_decoded_values_survive_store_shutdown() {
    let root = TestRoot::new();
    let store = root.store(limits());
    let encoded = group(&store, NORMAL);
    let digest = encoded.digest();
    let value = decode_source_build::<FixtureDomain>(&store, encoded).unwrap();
    drop(store);
    let escaped = value.clone();
    drop(value);
    assert_eq!(escaped.value().payload.as_ref(), [42; 512]);
    assert_eq!(escaped.group().digest(), digest);
    assert_eq!(
        escaped.group().member_bytes("runtime"),
        Some([NORMAL].as_slice())
    );
}

#[test]
fn exact_product_inventory_and_both_membership_queries_are_required() {
    for mode in [
        MISSING_PRODUCT,
        EXTRA_PRODUCT,
        DUPLICATE_PRODUCT,
        DOMAIN_REJECTS,
        CODEC_REJECTS,
        IGNORE_PRODUCT_FAILURE,
    ] {
        let root = TestRoot::new();
        let store = root.store(limits());
        let encoded = group(&store, mode);
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded.clone()),
            Err(ArtifactBuildError::InvalidCandidate(_))
        ));
        assert_retained_released(&store, &encoded);
    }
    let root = TestRoot::new();
    let store = root.store(limits());
    for (mode, declared) in [
        (NORMAL, vec![product(1), auxiliary()]),
        (FOREIGN_SOURCE, vec![product(1), product(2)]),
    ] {
        let encoded = commit(&store, candidate(mode, declared, Vec::new()));
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded),
            Err(ArtifactBuildError::InvalidCandidate(_))
        ));
    }
}

#[test]
fn exact_dependency_inventory_uses_immediate_precedence() {
    let root = TestRoot::new();
    let store = root.store(limits());
    let input = candidate(
        MIXED_DEMAND,
        vec![product(1)],
        vec![(product(2), AssetReferenceDemand::Immediate)],
    );
    let encoded = commit(&store, input);
    let value = decode_source_build::<FixtureDomain>(&store, encoded).unwrap();
    assert_eq!(value.value().dependencies.len(), 3);
    drop(value);
    for (mode, dependencies) in [
        (MIXED_DEMAND, Vec::new()),
        (IGNORE_DEPENDENCY_FAILURE, Vec::new()),
        (NORMAL, vec![(product(2), AssetReferenceDemand::Immediate)]),
        (
            MIXED_DEMAND,
            vec![(product(2), AssetReferenceDemand::Deferred)],
        ),
    ] {
        let root = TestRoot::new();
        let store = root.store(limits());
        let encoded = commit(&store, candidate(mode, vec![product(1)], dependencies));
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded.clone()),
            Err(ArtifactBuildError::InvalidCandidate(_))
        ));
        assert_retained_released(&store, &encoded);
    }
}

#[test]
fn retention_query_must_cover_the_inline_value_and_fit_the_original_promise() {
    for mode in [EXCESS_RETENTION, SHORT_RETENTION] {
        let root = TestRoot::new();
        let store = root.store(limits());
        let encoded = group(&store, mode);
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded.clone()),
            Err(ArtifactBuildError::Budget {
                kind: "decoded-product-retained",
                ..
            })
        ));
        assert_retained_released(&store, &encoded);
    }
    let root = TestRoot::new();
    let store = root.store(limits());
    let encoded = group(&store, RETENTION_REJECTS);
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded.clone()),
        Err(ArtifactBuildError::Codec(_))
    ));
    assert_retained_released(&store, &encoded);
}

#[test]
fn codec_and_inventory_scratch_are_reserved_before_decode() {
    let root = TestRoot::new();
    let store = root.store(limits());
    let encoded = group(&store, DECODE_PANICS);
    let held = store
        .reserve_work(store.limits().work_bytes.get() - CODEC_WORK)
        .unwrap();
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded.clone()),
        Err(ArtifactBuildError::Budget { kind: "work", .. })
    ));
    drop(held);
    let promised = PRODUCT_PROMISE + owner_overhead::<FixtureProduct>().unwrap();
    let _held = store
        .reserve_retained(
            store.limits().retained_bytes.get() - encoded.retained_bytes() - promised + 1,
        )
        .unwrap();
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded),
        Err(ArtifactBuildError::Budget {
            kind: "retained",
            ..
        })
    ));
}

#[test]
fn repeated_inventory_occurrences_have_independent_bounds() {
    let root = TestRoot::new();
    let mut bound = limits();
    bound.observations = ItemLimit::new(2).unwrap();
    let store = root.store(bound);
    let encoded = commit(
        &store,
        candidate(
            MIXED_DEMAND,
            vec![product(1)],
            vec![(product(2), AssetReferenceDemand::Immediate)],
        ),
    );
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded),
        Err(ArtifactBuildError::Budget {
            kind: "decoded-dependency-occurrences",
            ..
        })
    ));
    let root = TestRoot::new();
    bound = limits();
    bound.members = ItemLimit::new(1).unwrap();
    let store = root.store(bound);
    let encoded = group(&store, DUPLICATE_PRODUCT);
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded),
        Err(ArtifactBuildError::Budget {
            kind: "decoded-product-occurrences",
            ..
        })
    ));
}

#[test]
fn decode_query_and_rejection_destructor_panics_return_only_the_typed_failure() {
    for mode in [
        DECODE_PANICS,
        QUERY_PANICS,
        RETENTION_PANICS,
        QUERY_AND_DROP_PANIC,
    ] {
        let root = TestRoot::new();
        let store = root.store(limits());
        let encoded = group(&store, mode);
        assert!(matches!(
            decode_source_build::<FixtureDomain>(&store, encoded.clone()),
            Err(ArtifactBuildError::Panicked)
        ));
        assert_retained_released(&store, &encoded);
    }
    let root = TestRoot::new();
    let store = root.store(limits());
    let encoded = commit(
        &store,
        candidate(
            DROP_PANICS,
            vec![product(1)],
            vec![(product(2), AssetReferenceDemand::Deferred)],
        ),
    );
    assert!(matches!(
        decode_source_build::<FixtureDomain>(&store, encoded.clone()),
        Err(ArtifactBuildError::Panicked)
    ));
    assert_retained_released(&store, &encoded);
}

#[test]
fn last_owner_contains_destructor_panics_and_releases_its_charge() {
    let root = TestRoot::new();
    let store = root.store(limits());
    let encoded = group(&store, DROP_PANICS);
    let original = decode_source_build::<FixtureDomain>(&store, encoded.clone()).unwrap();
    let escaped = original.clone();
    drop(original);
    assert!(catch_unwind(AssertUnwindSafe(|| drop(escaped))).is_ok());
    assert_retained_released(&store, &encoded);
}
