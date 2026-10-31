use std::sync::Arc;

use nara_asset::{
    AssetPathScope, AssetProductRef, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_scene::{
    ExpandedSceneCandidate, RuntimeSceneLimits, SceneDocument, SourceDocumentKey,
    SourceDocumentKind,
};

use super::*;
use crate::{
    ProductRecipe,
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentLimits, CookedContentRequest,
    },
    runtime_content::{RuntimeContentLimits, RuntimeContentTarget},
};

fn content() -> PreparedRuntimeContent {
    let product = AssetProductRef::new(
        StableAssetId::parse_str("1f0f5110-0000-4000-8000-000000000001").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    let request = CookedContentRequest {
        target: RuntimeContentTarget::new("x86_64-pc-windows-msvc", "test-content").unwrap(),
        configuration,
        startup_scene: product,
        scenes: vec![product],
        addressable_roots: Vec::new(),
    };
    let mut builder = CookedContentBuilder::new(
        request,
        ProductRecipe::new(),
        CookedContentLimits {
            runtime: RuntimeContentLimits::default()
                .with_scene(
                    RuntimeSceneLimits::default()
                        .with_working_bytes(ByteLimit::new(1024 * 1024).unwrap()),
                )
                .with_retained_bytes(ByteLimit::new(32 * 1024 * 1024).unwrap())
                .with_work_bytes(ByteLimit::new(32 * 1024 * 1024).unwrap()),
            ..CookedContentLimits::default()
        },
    )
    .unwrap();
    let expanded = ExpandedSceneCandidate::plain(
        SourceDocumentKey::stable_asset(product.source(), SourceDocumentKind::Scene),
        Arc::new(SceneDocument::default()),
    );
    builder
        .add_scene(
            product,
            &expanded,
            ProjectAssetDatabase::default().resolver(&AssetPathScope::ROOT),
        )
        .unwrap();
    builder.finish().unwrap()
}

#[test]
fn generated_rust_input_preserves_independent_facts_in_the_canonical_golden() {
    let expected = RuntimeContentExpectation::new(
        ContentDigest::from_parts(123, [7; 32]),
        RuntimeContentTarget::new("x86_64-pc-windows-msvc", "desktop-release").unwrap(),
    );
    let mut generated = String::new();
    write_bootstrap(&mut generated, &expected).unwrap();
    assert_eq!(generated, include_str!("bootstrap.golden.rs"));
}

#[test]
fn bootstrap_byte_and_overlapping_content_retention_limits_are_exact() {
    let content = content();
    let limits = PlayerPackageLimits::default();
    let baseline =
        GeneratedPlayerBootstrap::new(&content, "src/content_expectation.rs", limits).unwrap();
    let bytes = baseline.bytes().len();
    let retained = content.retained_bytes() + baseline.retained_bytes();
    let exact = PlayerPackageLimits {
        bootstrap_bytes: ByteLimit::new(bytes).unwrap(),
        retained_bytes: ByteLimit::new(retained).unwrap(),
        work_bytes: ByteLimit::new(retained).unwrap(),
        ..limits
    };
    let admitted = GeneratedPlayerBootstrap::new(&content, baseline.path(), exact).unwrap();
    assert_eq!(admitted.digest(), baseline.digest());
    assert_eq!(admitted.bytes(), baseline.bytes());
    for one_short in [
        PlayerPackageLimits {
            bootstrap_bytes: ByteLimit::new(bytes - 1).unwrap(),
            ..exact
        },
        PlayerPackageLimits {
            retained_bytes: ByteLimit::new(retained - 1).unwrap(),
            ..exact
        },
        PlayerPackageLimits {
            work_bytes: ByteLimit::new(retained - 1).unwrap(),
            ..exact
        },
    ] {
        assert!(matches!(
            GeneratedPlayerBootstrap::new(&content, baseline.path(), one_short),
            Err(PlayerPackageError::Limit { .. }),
        ));
        assert_eq!(content.base().validate_deep().unwrap().members(), 1);
    }
}

#[test]
fn generated_input_rejects_alternate_or_nonportable_path_spellings() {
    for path in [
        "",
        "../expected.rs",
        "src//expected.rs",
        "src/./expected.rs",
        "src\\expected.rs",
        "/expected.rs",
    ] {
        assert!(validate_input_path(path).is_err(), "accepted {path:?}");
    }
    validate_input_path("src/expected.rs").unwrap();
}

#[test]
fn changed_content_expectation_cannot_reuse_the_original_generated_input() {
    let first = content();
    let mut bootstrap =
        GeneratedPlayerBootstrap::new(&first, "src/expected.rs", PlayerPackageLimits::default())
            .unwrap();
    check_bootstrap(&first, &bootstrap).unwrap();
    bootstrap.expected = RuntimeContentExpectation::new(
        ContentDigest::of_bytes(b"different selected catalog"),
        first.expectation().target().clone(),
    );
    assert!(matches!(
        check_bootstrap(&first, &bootstrap),
        Err(PlayerPackageError::BootstrapMismatch)
    ));
}

#[test]
fn streaming_pack_extent_matches_actual_bytes_and_refuses_short_or_excess_writes() {
    let content = content();
    let base = content.base();
    let mut bytes = Vec::new();
    base.write_pack(&mut bytes).unwrap();
    assert_eq!(base.pack_encoded_bytes().unwrap(), bytes.len() as u64);
    let mut stream = DigestWriter::new(io::sink(), base.pack_encoded_bytes().unwrap());
    base.write_pack(&mut stream).unwrap();
    assert_eq!(stream.finish().unwrap(), ContentDigest::of_bytes(&bytes));
    let mut excess = DigestWriter::new(io::sink(), bytes.len() as u64 - 1);
    assert!(base.write_pack(&mut excess).is_err());
    let mut short = DigestWriter::new(io::sink(), bytes.len() as u64 + 1);
    base.write_pack(&mut short).unwrap();
    assert!(matches!(
        short.finish(),
        Err(PlayerPackageError::StagedBytes)
    ));
}

#[test]
fn callback_panic_and_cancellation_keep_the_exact_control_point() {
    let point = PlayerPackagePoint::AfterPackWrite;
    let panic = checkpoint(&mut |_| panic!("injected Host fence panic"), point);
    assert!(matches!(
        panic,
        Err(PlayerPackageError::Control {
            point: PlayerPackagePoint::AfterPackWrite,
            failure: PlayerPackageControlFailure::Panicked,
        })
    ));
    let cancelled = checkpoint(&mut |_| Err(PlayerPackageControlFailure::Cancelled), point);
    assert!(matches!(
        cancelled,
        Err(PlayerPackageError::Control {
            point: PlayerPackagePoint::AfterPackWrite,
            failure: PlayerPackageControlFailure::Cancelled,
        })
    ));
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn staged_inventory_rejects_extra_missing_and_non_file_members() {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Root(PathBuf);
    impl Drop for Root {
        fn drop(&mut self) {
            let resolved = self.0.canonicalize().unwrap();
            assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(
                resolved
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("nara-player-inventory-")
            );
            fs::remove_dir_all(resolved).unwrap();
        }
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = Root(std::env::temp_dir().join(format!(
        "nara-player-inventory-{}-{unique}",
        std::process::id()
    )));
    fs::create_dir(&root.0).unwrap();
    let staging = crate::project_tooling::host_fs::writable_directory(&root.0).unwrap();
    for name in [PLAYER_PACK_FILE, PLAYER_PACKAGE_MANIFEST_FILE, "game.exe"] {
        write_member(&staging, name, b"inventory owner test", false).unwrap();
    }
    validate_staged_members(&staging, "game.exe").unwrap();
    write_member(&staging, "undeclared.bin", b"extra disk payload", false).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
    fs::remove_file(root.0.join("undeclared.bin")).unwrap();
    fs::remove_file(root.0.join(PLAYER_PACK_FILE)).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
    fs::create_dir(root.0.join(PLAYER_PACK_FILE)).unwrap();
    assert!(matches!(
        validate_staged_members(&staging, "game.exe"),
        Err(PlayerPackageError::StagedMembers)
    ));
}
