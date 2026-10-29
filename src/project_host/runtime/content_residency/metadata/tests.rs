use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{ImportedProductId, StableAssetId};
use nara_fs::ContentDigest;
use nara_identity::SceneEntityId;
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_scene::{
    ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneDocument,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

use super::*;
use crate::{
    ProductRecipe,
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentBudgetKind, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};

struct Fixture(PathBuf);

impl Fixture {
    fn open() -> (Self, RuntimeContentBase, Arc<ProcessedSceneContent>) {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_residency_metadata_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        let product = AssetProductRef::new(
            StableAssetId::parse_str("83100000-0000-4000-8000-000000000001").unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        let plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), ProductRecipe::new())
                .unwrap();
        let source = SourceDocumentKey::stable_asset(product.source(), SourceDocumentKind::Scene);
        let expanded = ExpandedSceneCandidate::plain(
            source.clone(),
            Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("small-scene").unwrap(),
            )])),
        );
        let scene = RuntimeSceneCandidate::cook(
            &expanded,
            plan.schema_validation().bindings(),
            &ProjectAssetDatabase::default(),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(fixture.0.join("scene.runtime.json"), &scene).unwrap();
        let target = RuntimeContentTarget::new("test-host", "residency-metadata").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: product,
                scenes: vec![product],
                addressable_roots: vec![],
                members: vec![
                    RuntimeContentMember::new(
                        "scene.runtime.json",
                        ContentDigest::of_bytes(&scene),
                        RuntimeContentMemberKind::Scene { source },
                        vec![product],
                        vec![],
                    )
                    .unwrap(),
                ],
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(fixture.0.join(RUNTIME_CONTENT_CATALOG_FILE), &catalog).unwrap();
        let base = RuntimeContentBase::open(
            &open_explicit_product_project_root(&fixture.0).unwrap(),
            &RuntimeContentExpectation::new(ContentDigest::of_bytes(&catalog), target),
            ProductRecipe::new(),
            RuntimeContentLimits::default(),
        )
        .unwrap();
        let content = base.prepare_scene_content(product).unwrap();
        (fixture, base, content)
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
                .starts_with("nara_residency_metadata_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn active_retained_bytes(base: &RuntimeContentBase) -> usize {
    let cap = RuntimeContentLimits::default().retained_bytes().get();
    match base.reserve_retained(cap) {
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            limit,
        }) => {
            assert_eq!(limit, cap);
            required - cap
        }
        _ => panic!("the probe must exceed the original base's retained-byte limit"),
    }
}

#[test]
fn unrelated_catalogue_snapshots_refuse_before_residency_clones_and_release_their_charge() {
    let (_fixture, base, content) = Fixture::open();
    assert!(content.images.is_empty());
    assert!(content.tilesets.is_empty());
    let input = ResidencyInput::Processed(&content);
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    let small_bytes = metadata_bytes(&world, input, None).unwrap();
    let before = active_retained_bytes(&base);
    let prepared = PreparedResidency::prepare(&world, input, None).unwrap();
    assert_eq!(active_retained_bytes(&base) - before, small_bytes);
    drop(prepared);
    assert_eq!(active_retained_bytes(&base), before);

    let empty_catalogue = world
        .resource::<AssetServer>()
        .checked_retained_bytes()
        .unwrap();
    {
        let mut server = world.resource_mut::<AssetServer>();
        for _ in 0..4096 {
            server.reserve_anonymous::<ImageAsset>().unwrap();
        }
        server
            .reserve::<ImageAsset>("unrelated/a-long-path-to-an-unloaded-image.png")
            .unwrap();
    }
    let large_catalogue = world
        .resource::<AssetServer>()
        .checked_retained_bytes()
        .unwrap();
    let large_bytes = metadata_bytes(&world, input, None).unwrap();
    assert_eq!(
        large_bytes - small_bytes,
        3 * (large_catalogue - empty_catalogue),
    );

    // Leave exactly the tiny Scene's former allowance in this same original base. Its content
    // is already decoded; the unrelated live catalogue must now refuse before cloning it.
    let remaining = RuntimeContentLimits::default().retained_bytes().get() - before;
    let held = base.reserve_retained(remaining - small_bytes).unwrap();
    let error = PreparedResidency::prepare(&world, input, None)
        .err()
        .unwrap();
    assert!(error.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.scene.residency-budget-exceeded"
    }));
    assert_eq!(
        world.resource::<AssetServer>().checked_retained_bytes(),
        Some(large_catalogue),
    );
    let unused = base.reserve_retained(small_bytes).unwrap();
    drop(unused);
    drop(held);
    assert_eq!(active_retained_bytes(&base), before);
    assert!(!world.contains_resource::<ProjectSceneResidency>());
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<Assets<ImageAsset>>());
}
