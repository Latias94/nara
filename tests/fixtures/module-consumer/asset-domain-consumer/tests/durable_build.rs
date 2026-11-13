//! Independent typed source builds, durable reuse and consumption of their actual Runtime bytes.

use std::{
    cell::Cell,
    convert::Infallible,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use nara::{
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildResult, ArtifactBuildStatus, ArtifactBuildStore, ArtifactContentDigest,
        ArtifactPublicationPoint, ArtifactSourceRequest, VerifiedArtifactGroup, build_source,
    },
    core::{ByteLimit, FormatVersion},
    fs::{CapabilityRights, DirectoryCapability, FileCapability, HostCapabilityOptions, TrustMode},
};
use nara_asset::{
    AssetBuildSource, AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource,
    AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetPathScope,
    AssetProductRef, AssetSourceKind, AssetSourcePath, ImportedProductId, RuntimeAssetCodec,
    SourceAssetBuild, SourceAssetImporter, StableAssetId,
};
use nara_asset_domain_consumer::{
    TextBundle, TextBundleDomain, TextBundleIdentityDecision, TextBundleRebindAssignment,
    TextBundleSettings,
};
use nara_diagnostic::Diagnostic;
use serde_json::Value;

#[path = "support/durable_build_runtime.rs"]
mod runtime;

const SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;
const CHILD_ROOT: &str = "NARA_DURABLE_BUILD_CHILD_ROOT";

fn id(number: usize) -> ImportedProductId {
    ImportedProductId::parse_subproduct(&format!("00000000-0000-4000-8000-{number:012x}")).unwrap()
}

fn reference(product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(SOURCE).unwrap(), product).unwrap()
}

fn settings() -> TextBundleSettings {
    TextBundleSettings::initial(RAW, 64, [id(1), id(2)]).unwrap()
}

fn metadata(settings: &TextBundleSettings) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(SOURCE).unwrap(),
            AssetPath::new("bundle.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        settings,
    )
    .to_json_string()
    .unwrap()
}

fn definition() -> ArtifactBuildDefinition {
    static COMPILED_TOOL: OnceLock<ArtifactContentDigest> = OnceLock::new();
    let implementation = *COMPILED_TOOL.get_or_init(|| {
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
            .share_mode(7)
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

fn request_at(root: &Path) -> ArtifactSourceRequest {
    ArtifactSourceRequest::new(
        directory(&root.join("source"), CapabilityRights::ReadOnly),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new("bundle.json").unwrap(),
        definition(),
    )
}

fn store_at(root: &Path, name: &str) -> ArtifactBuildStore {
    ArtifactBuildStore::open(
        directory(&root.join(name), CapabilityRights::ReadWrite),
        ArtifactBuildLimits::default(),
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
            "nara-durable-build-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        for name in ["source", "cache-a", "cache-b", "delivery"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let fixture = Self { root };
        fixture.write_source("bundle.json", RAW);
        fixture.write_source("suffix.txt", b" world");
        fixture.write_settings(&settings());
        fixture
    }

    fn write_source(&self, name: &str, bytes: &[u8]) {
        fs::write(self.root.join("source").join(name), bytes).unwrap();
    }

    fn write_settings(&self, settings: &TextBundleSettings) {
        self.write_source("bundle.json.meta", metadata(settings).as_bytes());
    }

    fn request(&self) -> ArtifactSourceRequest {
        request_at(&self.root)
    }

    fn store(&self, name: &str) -> ArtifactBuildStore {
        store_at(&self.root, name)
    }

    fn remove_tree(&self, name: &str) {
        let root = self.root.canonicalize().unwrap();
        let target = root.join(name).canonicalize().unwrap();
        assert!(target.starts_with(&root) && target != root);
        fs::remove_dir_all(target).unwrap();
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
                .starts_with("nara-durable-build-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn build(store: &ArtifactBuildStore, request: &ArtifactSourceRequest) -> ArtifactBuildResult {
    build_source::<TextBundleDomain>(store, request, |_| Ok(())).unwrap()
}

fn decoded(group: &VerifiedArtifactGroup) -> TextBundle {
    TextBundleDomain::decode_runtime(group.member_bytes("runtime").unwrap()).unwrap()
}

fn assert_text(group: &VerifiedArtifactGroup, first: &str, second: &str) {
    let product = decoded(group);
    assert_eq!(product.get(&reference(id(1))).unwrap().text, first);
    assert_eq!(product.get(&reference(id(2))).unwrap().text, second);
}

fn raw_observation(group: &VerifiedArtifactGroup, name: &str) -> Option<ArtifactContentDigest> {
    let path = format!("assets/{name}");
    group
        .observations()
        .iter()
        .find_map(|observation| match observation {
            ArtifactBuildObservation::Raw {
                path: observed,
                digest,
            } if observed.as_str() == path => Some(*digest),
            _ => None,
        })
        .expect("the actual input presence must be observed")
}

thread_local! {
    static BUILD_CALLS: Cell<usize> = const { Cell::new(0) };
}

/// Measures execution while delegating the entire build and codec to the real consumer.
struct CountedBundle;

impl AssetDomain for CountedBundle {
    type Product = TextBundle;

    fn contains_product(product: &TextBundle, id: ImportedProductId) -> bool {
        TextBundleDomain::contains_product(product, id)
    }
}

impl SourceAssetImporter for CountedBundle {
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

impl SourceAssetBuild for CountedBundle {
    type Codec = TextBundleDomain;
    const BUILD_ID: &'static str = TextBundleDomain::BUILD_ID;
    const BUILD_REVISION: u32 = TextBundleDomain::BUILD_REVISION;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        TextBundleDomain::normalize_settings(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<TextBundle, AssetImportError<E>> {
        BUILD_CALLS.with(|calls| calls.set(calls.get() + 1));
        TextBundleDomain::build(source)
    }

    fn visit_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_products(product, visitor)
    }
}

struct IgnoredReadFailureBundle;

impl AssetDomain for IgnoredReadFailureBundle {
    type Product = TextBundle;

    fn contains_product(product: &TextBundle, id: ImportedProductId) -> bool {
        TextBundleDomain::contains_product(product, id)
    }
}

impl SourceAssetImporter for IgnoredReadFailureBundle {
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

impl SourceAssetBuild for IgnoredReadFailureBundle {
    type Codec = TextBundleDomain;
    const BUILD_ID: &'static str = "consumer.text-bundle.ignored-read-failure";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        TextBundleDomain::normalize_settings(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<TextBundle, AssetImportError<E>> {
        let _ = source.read_optional_dependency(
            &AssetPath::new("probe.txt").unwrap(),
            ByteLimit::new(1).unwrap(),
        );
        TextBundleDomain::build(source)
    }

    fn visit_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_products(product, visitor)
    }
}

#[test]
fn an_ignored_tracked_read_failure_cannot_publish_a_successful_closed_recipe() {
    let fixture = Fixture::new();
    fixture.write_source("probe.txt", b"ab");
    let request = ArtifactSourceRequest::new(
        directory(&fixture.root.join("source"), CapabilityRights::ReadOnly),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new("bundle.json").unwrap(),
        ArtifactBuildDefinition {
            id: IgnoredReadFailureBundle::BUILD_ID.to_owned(),
            revision: FormatVersion::new(IgnoredReadFailureBundle::BUILD_REVISION).unwrap(),
            implementation: ArtifactContentDigest::of_bytes(b"ignored-read-failure-v1"),
        },
    );
    let store = fixture.store("cache-a");
    let mut publication_calls = 0;
    let refused = build_source::<IgnoredReadFailureBundle>(&store, &request, |_| {
        publication_calls += 1;
        Ok(())
    });
    assert!(matches!(
        refused,
        Err(ArtifactBuildError::Budget {
            kind: "source-bytes",
            required: 2,
            limit: 1
        }),
    ));
    assert_eq!(publication_calls, 0);
    fixture.write_source("probe.txt", b"a");
    let accepted = build_source::<IgnoredReadFailureBundle>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(accepted.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        raw_observation(accepted.group(), "probe.txt"),
        Some(ArtifactContentDigest::of_bytes(b"a")),
    );
    assert_text(accepted.group(), "Hello world", "Goodbye world");
}

struct DefaultBundle;

impl AssetDomain for DefaultBundle {
    type Product = TextBundle;

    fn contains_product(product: &TextBundle, id: ImportedProductId) -> bool {
        TextBundleDomain::contains_product(product, id)
    }
}

impl SourceAssetImporter for DefaultBundle {
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

impl SourceAssetBuild for DefaultBundle {
    type Codec = TextBundleDomain;
    const BUILD_ID: &'static str = "consumer.text-bundle.default-source-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        TextBundleDomain::normalize_settings(settings)
    }

    fn visit_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_products(product, visitor)
    }
}

/// A tracked selector controls whether raw metadata spelling is part of the build's inputs.
struct SelectedMetadataBundle;

impl AssetDomain for SelectedMetadataBundle {
    type Product = TextBundle;

    fn contains_product(product: &TextBundle, id: ImportedProductId) -> bool {
        TextBundleDomain::contains_product(product, id)
    }
}

impl SourceAssetImporter for SelectedMetadataBundle {
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

impl SourceAssetBuild for SelectedMetadataBundle {
    type Codec = TextBundleDomain;
    const BUILD_ID: &'static str = "consumer.text-bundle.selected-metadata-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(settings: Self::Settings) -> Result<Self::Settings, Diagnostic> {
        TextBundleDomain::normalize_settings(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<TextBundle, AssetImportError<E>> {
        let selector = source.read_dependency(
            &AssetPath::new("selector.txt").unwrap(),
            ByteLimit::new(16).unwrap(),
        )?;
        if selector.as_ref() == b"raw" {
            let raw = source.read_dependency(
                &AssetPath::new("bundle.json.meta").unwrap(),
                ByteLimit::new(16 * 1024).unwrap(),
            )?;
            let observed = AssetMetaIdentityCandidate::decode_json_bytes(&raw).unwrap();
            assert_eq!(observed.identity(), source.metadata().identity());
        }
        TextBundleDomain::build(source)
    }

    fn visit_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_products(product, visitor)
    }
}

#[test]
fn two_clean_stores_have_identical_recipes_bytes_groups_and_actual_product_mappings() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let first = build(&fixture.store("cache-a"), &request);
    let second = build(&fixture.store("cache-b"), &request);
    assert_eq!(first.status(), ArtifactBuildStatus::Built);
    assert_eq!(second.status(), ArtifactBuildStatus::Built);
    let (first, second) = (first.group(), second.group());
    assert_eq!(first.recipe_digest(), second.recipe_digest());
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.seed(), second.seed());
    assert_eq!(first.observations(), second.observations());
    assert_eq!(first.members(), second.members());
    assert_eq!(
        first.member_bytes("runtime"),
        second.member_bytes("runtime")
    );
    assert!(first.retained_bytes() >= first.member_bytes("runtime").unwrap().len());
    assert!(first.member_bytes("unused").is_none());
    assert_eq!(first.members().len(), 1);
    let member = &first.members()[0];
    assert_eq!(member.name, "runtime");
    assert_eq!(member.format.as_str(), TextBundleDomain::RUNTIME_KIND);
    assert_eq!(
        member.format_version.get(),
        TextBundleDomain::RUNTIME_VERSION
    );
    assert_eq!(
        member.digest,
        ArtifactContentDigest::of_bytes(first.member_bytes("runtime").unwrap())
    );
    let mut expected = vec![
        reference(ImportedProductId::PRIMARY),
        reference(id(1)),
        reference(id(2)),
    ];
    expected.sort_unstable();
    assert_eq!(member.products, expected);
    assert_text(first, "Hello world", "Goodbye world");
}

#[test]
fn reopened_store_hits_without_executing_the_source_build() {
    let fixture = Fixture::new();
    let request = fixture.request();
    BUILD_CALLS.with(|calls| calls.set(0));
    let first =
        build_source::<CountedBundle>(&fixture.store("cache-a"), &request, |_| Ok(())).unwrap();
    assert_eq!(first.status(), ArtifactBuildStatus::Built);
    let receipt = first
        .publication_receipt()
        .expect("a built result retains publication evidence");
    assert_eq!(receipt.group().digest(), first.group().digest());
    assert!(receipt.index_receipt().verified_content_digest().is_some());
    if let Some(sync) = receipt.directory_sync() {
        assert_eq!(sync.tier(), receipt.directory_sync_tier());
    }
    BUILD_CALLS.with(|calls| assert_eq!(calls.get(), 1));
    let reopened = fixture.store("cache-a");
    let hit = build_source::<CountedBundle>(&reopened, &request, |_| Ok(())).unwrap();
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    assert!(hit.publication_receipt().is_none());
    BUILD_CALLS.with(|calls| assert_eq!(calls.get(), 1));
    assert_eq!(first.group().digest(), hit.group().digest());
    assert_eq!(
        first.group().member_bytes("runtime"),
        hit.group().member_bytes("runtime")
    );
}

#[test]
fn padded_primary_larger_than_the_output_member_limit_still_has_a_warm_hit() {
    let fixture = Fixture::new();
    let mut raw = RAW.to_vec();
    raw.resize(12 * 1024, b' ');
    fixture.write_source("bundle.json", &raw);
    fixture.write_settings(&TextBundleSettings::initial(&raw, 64, [id(1), id(2)]).unwrap());
    let limits = ArtifactBuildLimits {
        member_bytes: ByteLimit::new(2 * 1024).unwrap(),
        ..ArtifactBuildLimits::default()
    };
    let request = fixture.request();
    let store = ArtifactBuildStore::open(
        directory(&fixture.root.join("cache-a"), CapabilityRights::ReadWrite),
        limits,
    )
    .unwrap();
    let cold = build(&store, &request);
    assert_eq!(cold.status(), ArtifactBuildStatus::Built);
    assert!(raw.len() > limits.member_bytes.get());
    assert!(cold.group().member_bytes("runtime").unwrap().len() <= limits.member_bytes.get());
    assert_eq!(
        raw_observation(cold.group(), "bundle.json"),
        Some(ArtifactContentDigest::of_bytes(&raw))
    );
    drop(store);
    let reopened = ArtifactBuildStore::open(
        directory(&fixture.root.join("cache-a"), CapabilityRights::ReadWrite),
        limits,
    )
    .unwrap();
    let warm = build(&reopened, &request);
    assert_eq!(warm.status(), ArtifactBuildStatus::Hit);
    assert_eq!(cold.group().digest(), warm.group().digest());
    assert_eq!(
        cold.group().member_bytes("runtime"),
        warm.group().member_bytes("runtime")
    );
    assert_text(warm.group(), "Hello world", "Goodbye world");
}

#[test]
fn selector_change_removes_raw_metadata_observation_before_equivalent_formatting() {
    let fixture = Fixture::new();
    fixture.write_source("selector.txt", b"raw");
    let mut selected = definition();
    selected.id = SelectedMetadataBundle::BUILD_ID.to_owned();
    selected.revision = FormatVersion::new(SelectedMetadataBundle::BUILD_REVISION).unwrap();
    let request = ArtifactSourceRequest::new(
        directory(&fixture.root.join("source"), CapabilityRights::ReadOnly),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new("bundle.json").unwrap(),
        selected,
    );
    let store = fixture.store("cache-a");
    let original = build_source::<SelectedMetadataBundle>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(original.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        raw_observation(original.group(), "bundle.json.meta"),
        Some(ArtifactContentDigest::of_bytes(
            metadata(&settings()).as_bytes()
        )),
    );
    // This path sorts after metadata: replay records metadata before finding the selector drift.
    fixture.write_source("selector.txt", b"normalized");
    let changed = build_source::<SelectedMetadataBundle>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(changed.status(), ArtifactBuildStatus::Built);
    assert!(!changed.group().observations().iter().any(|observation| matches!(
        observation,
        ArtifactBuildObservation::Raw { path, .. } if path.as_str() == "assets/bundle.json.meta"
    )));
    assert_eq!(
        raw_observation(changed.group(), "bundle.json"),
        Some(ArtifactContentDigest::of_bytes(RAW))
    );
    assert_eq!(
        raw_observation(changed.group(), "selector.txt"),
        Some(ArtifactContentDigest::of_bytes(b"normalized"))
    );
    assert_ne!(
        original.group().recipe_digest(),
        changed.group().recipe_digest()
    );
    assert_eq!(
        original.group().member_bytes("runtime"),
        changed.group().member_bytes("runtime")
    );

    let value: Value = serde_json::from_str(&metadata(&settings())).unwrap();
    let compact = serde_json::to_vec(&value).unwrap();
    assert_ne!(compact.as_slice(), metadata(&settings()).as_bytes());
    fixture.write_source("bundle.json.meta", &compact);
    let formatted = build_source::<SelectedMetadataBundle>(&store, &request, |_| Ok(())).unwrap();
    assert_eq!(formatted.status(), ArtifactBuildStatus::Hit);
    assert_eq!(formatted.group().digest(), changed.group().digest());
    assert_eq!(
        formatted.group().observations(),
        changed.group().observations()
    );
}

#[test]
fn source_facade_preserves_uncertain_receipt_when_after_index_control_panics() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let store = fixture.store("cache-a");
    let mut reached_index = false;
    let result = build_source::<TextBundleDomain>(&store, &request, |point| {
        if point == ArtifactPublicationPoint::AfterIndex {
            reached_index = true;
            panic!("intentional post-index source facade regression");
        }
        Ok(())
    });
    assert!(reached_index);
    let receipt = match result {
        Err(ArtifactBuildError::Uncertain { receipt, cause }) => {
            assert!(matches!(*cause, ArtifactBuildError::Panicked));
            receipt
        }
        _ => panic!("a post-index panic must retain an uncertain publication receipt"),
    };
    assert!(receipt.index_receipt().verified_content_digest().is_some());
    assert_text(receipt.group(), "Hello world", "Goodbye world");
    drop(store);
    let hit = build(&fixture.store("cache-a"), &request);
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    assert_eq!(hit.group().digest(), receipt.group().digest());
    assert_eq!(
        hit.group().member_bytes("runtime"),
        receipt.group().member_bytes("runtime")
    );
}

#[test]
fn metadata_whitespace_and_semantic_set_order_share_the_canonical_recipe() {
    let fixture = Fixture::new();
    let original = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"},{"name":"Retired one","text":"Old"},{"name":"Retired two","text":"Old"}]}"#;
    let settings = TextBundleSettings::initial(original, 64, [id(1), id(2), id(3), id(4)]).unwrap();
    let settings = settings
        .rebind(
            RAW,
            &[
                TextBundleRebindAssignment {
                    row: 0,
                    identity: TextBundleIdentityDecision::Continue(id(1)),
                },
                TextBundleRebindAssignment {
                    row: 1,
                    identity: TextBundleIdentityDecision::Continue(id(2)),
                },
            ],
        )
        .unwrap();
    fixture.write_settings(&settings);
    let first = build(&fixture.store("cache-a"), &fixture.request());
    let mut reordered: Value = serde_json::from_str(&metadata(&settings)).unwrap();
    let continuity = &mut reordered["payload"]["settings"]["continuity"];
    continuity["assignments"].as_array_mut().unwrap().reverse();
    continuity["retired"].as_array_mut().unwrap().reverse();
    let permuted = serde_json::from_value(reordered["payload"]["settings"].clone()).unwrap();
    assert_eq!(
        TextBundleDomain::normalize_settings(permuted).unwrap(),
        settings
    );
    let reordered = format!(" \n{}\n ", serde_json::to_string(&reordered).unwrap());
    fixture.write_source("bundle.json.meta", reordered.as_bytes());
    let second = build(&fixture.store("cache-b"), &fixture.request());
    let hit = build(&fixture.store("cache-a"), &fixture.request());
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    assert_eq!(
        first.group().recipe_digest(),
        second.group().recipe_digest()
    );
    assert_eq!(first.group().digest(), second.group().digest());
    assert_eq!(
        first.group().seed().settings,
        second.group().seed().settings
    );
    assert_eq!(first.group().digest(), hit.group().digest());
    let normalized = TextBundleDomain::normalize_settings(settings.clone()).unwrap();
    assert_eq!(normalized, settings);
    assert_eq!(normalized.max_characters, 64);
    assert_eq!(
        normalized.continuity.source_hash(),
        settings.continuity.source_hash()
    );
    assert_eq!(normalized.continuity.retired(), &[id(3), id(4)]);
}

#[test]
fn changed_required_suffix_changes_the_recipe_and_actual_runtime_body() {
    let fixture = Fixture::new();
    let store = fixture.store("cache-a");
    let request = fixture.request();
    let original = build(&store, &request);
    fixture.write_source("suffix.txt", b" there");
    let changed = build(&store, &request);
    assert_eq!(changed.status(), ArtifactBuildStatus::Built);
    assert_ne!(
        original.group().recipe_digest(),
        changed.group().recipe_digest()
    );
    assert_ne!(
        raw_observation(original.group(), "suffix.txt"),
        raw_observation(changed.group(), "suffix.txt")
    );
    assert_ne!(
        original.group().members()[0].digest,
        changed.group().members()[0].digest
    );
    assert_eq!(
        original.group().members()[0].products,
        changed.group().members()[0].products
    );
    assert_text(changed.group(), "Hello there", "Goodbye there");
    assert_text(original.group(), "Hello world", "Goodbye world");
}

#[test]
fn admitted_settings_change_the_recipe_even_when_encoded_output_is_equal() {
    let fixture = Fixture::new();
    let store = fixture.store("cache-a");
    let request = fixture.request();
    let first = build(&store, &request);
    let mut settings = settings();
    settings.max_characters = 128;
    fixture.write_settings(&settings);
    let second = build(&store, &request);
    assert_eq!(second.status(), ArtifactBuildStatus::Built);
    assert_ne!(
        first.group().recipe_digest(),
        second.group().recipe_digest()
    );
    assert_ne!(
        first.group().seed().settings,
        second.group().seed().settings
    );
    assert_eq!(
        first.group().members()[0].digest,
        second.group().members()[0].digest
    );
    assert_eq!(
        first.group().member_bytes("runtime"),
        second.group().member_bytes("runtime")
    );
}

#[test]
fn warmed_cache_cannot_bypass_too_small_settings_or_replace_them_with_defaults() {
    let fixture = Fixture::new();
    fixture.write_source("build-suffix.txt", b"!");
    let store = fixture.store("cache-a");
    let request = fixture.request();
    let original = build(&store, &request);
    for (maximum, code) in [
        (0, "consumer.text-bundle.settings-invalid"),
        (10, "consumer.text-bundle.too-long"),
        (13, "consumer.text-bundle.too-long"),
    ] {
        let mut settings = settings();
        settings.max_characters = maximum;
        fixture.write_settings(&settings);
        match build_source::<TextBundleDomain>(&store, &request, |_| Ok(())) {
            Err(ArtifactBuildError::Domain(diagnostic)) => {
                assert_eq!(diagnostic.code().as_str(), code)
            }
            _ => panic!("current invalid settings must reject instead of returning a warm hit"),
        }
    }
    fixture.write_settings(&settings());
    let restored = build(&store, &request);
    assert_eq!(restored.status(), ArtifactBuildStatus::Hit);
    assert_eq!(original.group().digest(), restored.group().digest());
    assert_text(original.group(), "Hello world!", "Goodbye world!");
}

#[test]
fn optional_absence_presence_and_deletion_are_real_build_observations() {
    let fixture = Fixture::new();
    let store = fixture.store("cache-a");
    let request = fixture.request();
    let absent = build(&store, &request);
    assert_eq!(raw_observation(absent.group(), "build-suffix.txt"), None);
    fixture.write_source("build-suffix.txt", b"!");
    let present = build(&store, &request);
    assert_eq!(present.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        raw_observation(present.group(), "build-suffix.txt"),
        Some(ArtifactContentDigest::of_bytes(b"!"))
    );
    assert_ne!(
        absent.group().recipe_digest(),
        present.group().recipe_digest()
    );
    assert_ne!(
        absent.group().observations(),
        present.group().observations()
    );
    assert_text(present.group(), "Hello world!", "Goodbye world!");
    fs::remove_file(fixture.root.join("source/build-suffix.txt")).unwrap();
    let removed = build(&store, &request);
    assert_eq!(raw_observation(removed.group(), "build-suffix.txt"), None);
    assert_eq!(
        absent.group().recipe_digest(),
        removed.group().recipe_digest()
    );
    assert_eq!(absent.group().digest(), removed.group().digest());
    assert_text(removed.group(), "Hello world", "Goodbye world");
}

#[test]
fn cache_clear_preserves_authored_continuity_and_retained_original_group_bytes() {
    let fixture = Fixture::new();
    let request = fixture.request();
    let original = build(&fixture.store("cache-a"), &request).into_group();
    let original_bytes = original.member_bytes("runtime").unwrap().to_vec();
    let original_metadata = fs::read(fixture.root.join("source/bundle.json.meta")).unwrap();
    fixture.remove_tree("cache-a");
    fs::create_dir(fixture.root.join("cache-a")).unwrap();
    let store = fixture.store("cache-a");
    let rebuilt = build(&store, &request);
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(original.recipe_digest(), rebuilt.group().recipe_digest());
    assert_eq!(original.digest(), rebuilt.group().digest());
    assert_eq!(
        original.members()[0].products,
        rebuilt.group().members()[0].products
    );
    assert_eq!(
        fs::read(fixture.root.join("source/bundle.json.meta")).unwrap(),
        original_metadata
    );
    fixture.write_source("suffix.txt", b" successor");
    let successor = build(&store, &request).into_group();
    assert_ne!(original.digest(), successor.digest());
    assert_eq!(
        original.members()[0].products,
        successor.members()[0].products
    );
    drop(rebuilt);
    drop(store);
    drop(request);
    fixture.remove_tree("cache-a");
    fixture.remove_tree("source");
    assert_eq!(original.member_bytes("runtime").unwrap(), original_bytes);
    assert_text(&original, "Hello world", "Goodbye world");
    assert_text(&successor, "Hello successor", "Goodbye successor");
}

#[test]
fn real_scene_and_bundle_group_runs_managed_travel_from_a_retained_pack_after_cache_removal() {
    let fixture = Fixture::new();
    let source_store = fixture.store("cache-a");
    let request = fixture.request();
    let bundle = build(&source_store, &request).into_group();
    let delivery = runtime::Delivery::new(id(1), id(2));
    let group_store = fixture.store("cache-b");
    let original = group_store
        .commit(delivery.candidate(&bundle), |_| Ok(()))
        .unwrap()
        .into_group();
    assert_eq!(original.members().len(), 3);
    let old_base = delivery.pack(&fixture, &original);
    assert!(old_base.validate_deep().is_ok());

    fixture.write_source("suffix.txt", b" successor");
    let new_bundle = build(&source_store, &request).into_group();
    let successor = group_store
        .commit(delivery.candidate(&new_bundle), |_| Ok(()))
        .unwrap()
        .into_group();
    assert_ne!(original.recipe_digest(), successor.recipe_digest());
    assert_ne!(original.digest(), successor.digest());
    assert_eq!(original.members().len(), successor.members().len());
    let new_base = delivery.pack(&fixture, &successor);
    assert!(new_base.validate_deep().is_ok());
    drop(group_store);
    drop(source_store);
    drop(request);
    fixture.remove_tree("cache-a");
    fixture.remove_tree("cache-b");
    fixture.remove_tree("source");
    assert_text(&bundle, "Hello world", "Goodbye world");
    assert_text(&new_bundle, "Hello successor", "Goodbye successor");
    delivery.run_managed(old_base, "Hello world", "Goodbye world");
    delivery.run_managed(new_base, "Hello successor", "Goodbye successor");
}

#[test]
fn every_real_three_blob_publication_boundary_preserves_the_previous_complete_group() {
    for stop in [
        ArtifactPublicationPoint::BeforeBlob { index: 0 },
        ArtifactPublicationPoint::AfterBlob { index: 0 },
        ArtifactPublicationPoint::BeforeBlob { index: 1 },
        ArtifactPublicationPoint::AfterBlob { index: 1 },
        ArtifactPublicationPoint::BeforeBlob { index: 2 },
        ArtifactPublicationPoint::AfterBlob { index: 2 },
        ArtifactPublicationPoint::BeforeManifest,
        ArtifactPublicationPoint::BeforeIndex,
        ArtifactPublicationPoint::AfterIndex,
    ] {
        let fixture = Fixture::new();
        let bundle = build(&fixture.store("cache-a"), &fixture.request()).into_group();
        let previous_delivery = runtime::Delivery::new(id(1), id(2));
        let next_delivery = runtime::Delivery::new(id(2), id(1));
        let store = fixture.store("cache-b");
        let previous = store
            .commit(previous_delivery.candidate(&bundle), |_| Ok(()))
            .unwrap()
            .into_group();
        let candidate = next_delivery.candidate(&bundle);
        let seed = candidate.seed.clone();
        assert_ne!(&seed, previous.seed());
        let mut visited = Vec::new();
        let interrupted = store.commit(candidate, |point| {
            visited.push(point);
            if point == stop {
                Err(ArtifactBuildError::Aborted)
            } else {
                Ok(())
            }
        });
        assert_eq!(visited.last(), Some(&stop));
        let uncertain = match interrupted {
            Err(ArtifactBuildError::Aborted) if stop != ArtifactPublicationPoint::AfterIndex => {
                None
            }
            Err(ArtifactBuildError::Uncertain { receipt, cause })
                if stop == ArtifactPublicationPoint::AfterIndex =>
            {
                assert!(matches!(*cause, ArtifactBuildError::Aborted));
                Some(receipt.into_group())
            }
            other => panic!("unexpected interruption at {stop:?}: {other:?}"),
        };
        drop(store);
        let reopened = fixture.store("cache-b");
        let retained_previous = reopened.lookup_seed(previous.seed()).unwrap().unwrap();
        assert_eq!(retained_previous.digest(), previous.digest());
        assert_eq!(retained_previous.members().len(), 3);
        for member in previous.members() {
            assert_eq!(
                retained_previous.member_bytes(&member.name),
                previous.member_bytes(&member.name)
            );
        }
        let current = reopened.lookup_seed(&seed).unwrap();
        if let Some(uncertain) = uncertain {
            let current = current.expect("AfterIndex is uncertain, not definitely unpublished");
            assert_eq!(current.digest(), uncertain.digest());
            assert_eq!(current.members().len(), 3);
            let base = next_delivery.pack(&fixture, &current);
            assert!(base.validate_deep().is_ok());
        } else {
            assert!(
                current.is_none(),
                "no reusable half group may appear at {stop:?}"
            );
            let recovered = reopened
                .commit(next_delivery.candidate(&bundle), |_| Ok(()))
                .unwrap()
                .into_group();
            assert_eq!(recovered.members().len(), 3);
            assert_eq!(
                reopened.lookup_seed(&seed).unwrap().unwrap().digest(),
                recovered.digest()
            );
        }
    }
}

#[test]
fn fresh_process_reopens_a_complete_group_without_source_execution() {
    let fixture = Fixture::new();
    let built = build(&fixture.store("cache-a"), &fixture.request());
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "durable_build_child_hit", "--nocapture"])
        .env(CHILD_ROOT, &fixture.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("verified durable build hit"));
    let hit = build(&fixture.store("cache-a"), &fixture.request());
    assert_eq!(built.group().digest(), hit.group().digest());
}

#[test]
fn durable_build_child_hit() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let root = PathBuf::from(root).canonicalize().unwrap();
    assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nara-durable-build-")
    );
    BUILD_CALLS.with(|calls| assert_eq!(calls.get(), 0));
    let hit =
        build_source::<CountedBundle>(&store_at(&root, "cache-a"), &request_at(&root), |_| Ok(()))
            .unwrap();
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    BUILD_CALLS.with(|calls| assert_eq!(calls.get(), 0));
    assert_text(hit.group(), "Hello world", "Goodbye world");
    println!("verified durable build hit");
}

#[test]
fn build_reader_keeps_optional_absence_and_exact_failures_separate_from_required_absence() {
    #[derive(Debug, PartialEq, Eq)]
    struct ReadFailure(u32);
    let encoded = metadata(&settings());
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    let path = AssetPath::new("suffix.txt").unwrap();
    let limit = ByteLimit::new(1024).unwrap();
    let mut absent = |_: &AssetPath, _: ByteLimit| Ok::<_, ReadFailure>(None);
    let mut source = AssetBuildSource::new(&metadata, Arc::from(RAW), &mut absent);
    assert_eq!(source.bytes(), RAW);
    assert_eq!(source.metadata().identity(), metadata.identity());
    assert_eq!(source.read_optional_dependency(&path, limit).unwrap(), None);
    for error in [
        source.read_dependency(&path, limit).unwrap_err(),
        source.prepare_import::<TextBundleDomain>().unwrap_err(),
    ] {
        match error {
            AssetImportError::Domain(diagnostic) => assert_eq!(
                diagnostic.code().as_str(),
                "asset.build.required-input-missing"
            ),
            AssetImportError::Source(_) => panic!("observed absence must be a domain rejection"),
        }
    }
    let mut failing = |_: &AssetPath, _: ByteLimit| Err::<Option<Arc<[u8]>>, _>(ReadFailure(73));
    let mut source = AssetBuildSource::new(&metadata, Arc::from(RAW), &mut failing);
    for error in [
        source.read_optional_dependency(&path, limit).unwrap_err(),
        source.read_dependency(&path, limit).unwrap_err(),
        source.prepare_import::<TextBundleDomain>().unwrap_err(),
    ] {
        assert!(matches!(error, AssetImportError::Source(ReadFailure(73))));
    }
}

#[test]
fn default_build_executes_the_unchanged_importer_and_only_its_required_inputs() {
    let encoded = metadata(&settings());
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.as_str().to_owned(), limit.get()));
        Ok::<_, Infallible>(Some(Arc::<[u8]>::from(b" world".as_slice())))
    };
    let product = DefaultBundle::build(&mut AssetBuildSource::new(
        &metadata,
        Arc::from(RAW),
        &mut reader,
    ))
    .unwrap();
    assert_eq!(requests, [("suffix.txt".to_owned(), 1024)]);
    assert_eq!(product.get(&reference(id(1))).unwrap().text, "Hello world");
    assert_eq!(
        product.get(&reference(id(2))).unwrap().text,
        "Goodbye world"
    );
}

#[test]
fn build_only_suffix_honors_utf8_byte_and_character_bounds_and_typed_read_failures() {
    let encoded = metadata(&settings());
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    for suffix in [vec![0xff], vec![b'x'; 1025], vec![b'x'; 64]] {
        let mut reader = |path: &AssetPath, limit: ByteLimit| {
            assert_eq!(limit.get(), 1024);
            Ok::<_, Infallible>(Some(Arc::<[u8]>::from(match path.as_str() {
                "suffix.txt" => b" world".as_slice(),
                "build-suffix.txt" => &suffix,
                _ => panic!("unexpected build input"),
            })))
        };
        let result = TextBundleDomain::build(&mut AssetBuildSource::new(
            &metadata,
            Arc::from(RAW),
            &mut reader,
        ));
        let expected = if suffix.len() == 64 {
            "consumer.text-bundle.too-long"
        } else {
            "consumer.text-bundle.suffix-invalid"
        };
        match result {
            Err(AssetImportError::Domain(diagnostic)) => {
                assert_eq!(diagnostic.code().as_str(), expected)
            }
            _ => panic!("invalid build-only text must reject"),
        }
    }
    let mut reader = |path: &AssetPath, _: ByteLimit| {
        if path.as_str() == "suffix.txt" {
            Ok(Some(Arc::<[u8]>::from(b" world".as_slice())))
        } else {
            Err(97_u32)
        }
    };
    let result = TextBundleDomain::build(&mut AssetBuildSource::new(
        &metadata,
        Arc::from(RAW),
        &mut reader,
    ));
    assert!(matches!(result, Err(AssetImportError::Source(97))));
}

#[test]
fn actual_product_visitor_propagates_failures_and_ordinary_source_never_reads_build_suffix() {
    let encoded = metadata(&settings());
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    let mut reads = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        reads.push((path.as_str().to_owned(), limit.get()));
        Ok::<_, Infallible>(Arc::<[u8]>::from(b" world".as_slice()))
    };
    let product = TextBundleDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        Arc::from(RAW),
        &mut reader,
    ))
    .unwrap();
    assert_eq!(reads, [("suffix.txt".to_owned(), 1024)]);
    let mut visited = Vec::new();
    TextBundleDomain::visit_products(&product, |reference| {
        visited.push(reference);
        Ok::<_, Infallible>(())
    })
    .unwrap();
    assert_eq!(
        visited,
        [
            reference(ImportedProductId::PRIMARY),
            reference(id(1)),
            reference(id(2))
        ]
    );
    let mut calls = 0;
    let error = TextBundleDomain::visit_products(&product, |_| {
        calls += 1;
        Err(91)
    });
    assert_eq!(error, Err(91));
    assert_eq!(calls, 1);
}
