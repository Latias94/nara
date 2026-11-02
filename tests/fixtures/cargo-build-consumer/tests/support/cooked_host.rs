//! Actual compiled Host: tracked imports, concrete cook, assembly, captured Player, publication.

#![forbid(unsafe_code)]

use std::{
    cell::Cell,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use aurora_game as game;
use engine::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetPathScope,
        AssetRef, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase, RuntimeAssetCodec,
        SourceAssetBuild, SourceAssetImporter,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildStore, ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup,
        build_image_import, build_source, cook_image_group,
    },
    core::{ByteLimit, FormatVersion, Vec2},
    fs::{ContentDigest, RelativeComponent, RelativePath},
    image::{
        ImageColorSpace, ImageCookLimits, ImageCookSettings, ImageCookTarget, ImageExtent,
        ImageImportSettings, ImageOversizePolicy, ImageRuntimeCodec,
    },
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapAsset, ActionMapBuilder,
        ActionMapFileCandidate, ActionMapLimits,
    },
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::resolve_runtime_configuration,
    project_tooling::{
        cargo_build::{
            CapturedCargoExecutable, CargoBuildFailure, CargoBuildLimits, CargoBuildPoint,
            CargoBuildRequest, CargoBuildTools, build_executable,
        },
        host_fs,
        player_package::{
            GeneratedPlayerBootstrap, PlayerPackageControlFailure, PlayerPackageError,
            PlayerPackageLimits, PlayerPackageOutcome, PlayerPackagePoint, PreparedPlayerPackage,
            build_player_package,
        },
    },
    project_workflow::{
        cargo::CargoBuildSelection,
        cooked_content::{
            CookedContentBuilder, CookedContentLimits, CookedContentRequest, PreparedRuntimeContent,
        },
    },
    runtime_content::{RuntimeContentDependencyRole, RuntimeContentTarget},
    scene::{
        SceneCompilationOptions, SceneDocumentCandidate, SourceDocumentKey, SourceDocumentKind,
    },
    tilemap::{TileAtlasLayout, TileSetDescriptor, TileSetSource, TileSetSourceFileLimits},
};
use nara_asset_domain_consumer::{TextBundleDomain, TextBundleSettings};

const GENERATED: &str = "player/src/generated.rs";
const CARGO_PROFILE: &str = "consumer-checked";
const CONTENT_PROFILE: &str = "cpu4-shipping";
const PLAYER_PACKAGE: &str = "aurora-native-game";
const PLAYER_BIN: &str = "aurora-portable-player";
const BUNDLE_RAW: &[u8] =
    br#"{"suffix":"suffix.txt","entries":[{"name":"Greeting","text":"Hello"}]}"#;
const OBSERVED: &[&str] = &[
    "stripe.png",
    "stripe.png.meta",
    "bundle.json",
    "bundle.json.meta",
    "suffix.txt",
    "tileset.ron",
    "tileset.ron.meta",
    "player.actions.json",
    "player.actions.json.meta",
    "1.scene.json",
    "1.scene.json.meta",
    "2.scene.json",
    "2.scene.json.meta",
    "3.scene.json",
    "3.scene.json.meta",
];
const PLAYER_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "host/Cargo.toml",
    "player/Cargo.toml",
    "player/src/main.rs",
    GENERATED,
    "game/Cargo.toml",
    "game/src/lib.rs",
    "game/src/probe.rs",
];

fn main() {
    assert!(!cfg!(feature = "unwanted-default"));
    assert!(cfg!(feature = "cook-entry"));
    let mut args = std::env::args_os().skip(1);
    let workspace = PathBuf::from(args.next().unwrap()).canonicalize().unwrap();
    let output = PathBuf::from(args.next().unwrap()).canonicalize().unwrap();
    let cargo = PathBuf::from(args.next().unwrap());
    let rustc = PathBuf::from(args.next().unwrap());
    let triple = args.next().unwrap().into_string().unwrap();
    let target = PathBuf::from(args.next().unwrap()).canonicalize().unwrap();
    assert!(args.next().is_none());
    let source = workspace.join("source");
    prepare_source(&source);
    let observations = Observations::capture(&source);
    let cache = workspace.join("source-cache");
    fs::create_dir(&cache).unwrap();
    let store = ArtifactBuildStore::open(
        host_fs::writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap();
    let image_request = ArtifactSourceRequest::new(
        host_fs::readable_directory(&source).unwrap(),
        scope(),
        AssetPath::new("stripe.png").unwrap(),
        definition("consumer.cooked-player.png-import"),
    );
    let imported = build_image_import(&store, &image_request, |_| observations.artifact_fence())
        .unwrap()
        .into_group();
    assert_eq!(
        imported.seed().location.as_ref().unwrap().as_str(),
        "assets/stripe.png"
    );
    let full = cook(&store, &imported, 8, &observations);
    let image = cook(&store, &imported, 4, &observations);
    let decoded =
        ImageRuntimeCodec::decode_runtime(image.member_bytes("runtime").unwrap()).unwrap();
    assert_eq!(decoded.extent(), ImageExtent::new(4, 1));
    assert_eq!(decoded.pixels(), game::cooked_pixels());
    assert_ne!(image.members()[0].digest, full.members()[0].digest);
    assert!(
        matches!(image.observations(), [ArtifactBuildObservation::Processed { product, group, .. }]
        if *product == game::product(game::IMAGE) && *group == imported.digest())
    );
    drop(decoded);
    let bundle_request = ArtifactSourceRequest::new(
        host_fs::readable_directory(&source).unwrap(),
        scope(),
        AssetPath::new("bundle.json").unwrap(),
        definition(TextBundleDomain::BUILD_ID),
    );
    let bundle = build_source::<TextBundleDomain>(&store, &bundle_request, |_| {
        observations.artifact_fence()
    })
    .unwrap()
    .into_group();
    assert!(
        bundle
            .observations()
            .iter()
            .any(|observation| matches!(observation,
        ArtifactBuildObservation::Raw { path, .. } if path.as_str() == "assets/suffix.txt"))
    );
    assert_eq!(bundle.members().len(), 1);
    assert_eq!(bundle.members()[0].products.len(), 2);
    let repeat_cache = workspace.join("source-cache-cold-repeat");
    fs::create_dir(&repeat_cache).unwrap();
    let repeat_store = ArtifactBuildStore::open(
        host_fs::writable_directory(&repeat_cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap();
    let repeat_import = build_image_import(&repeat_store, &image_request, |_| {
        observations.artifact_fence()
    })
    .unwrap()
    .into_group();
    let repeat_image = cook(&repeat_store, &repeat_import, 4, &observations);
    let repeat_bundle = build_source::<TextBundleDomain>(&repeat_store, &bundle_request, |_| {
        observations.artifact_fence()
    })
    .unwrap()
    .into_group();
    assert_eq!(imported.digest(), repeat_import.digest());
    assert_eq!(image.digest(), repeat_image.digest());
    assert_eq!(bundle.digest(), repeat_bundle.digest());
    let repeat_selected = Cell::new(repeat_image.digest());
    let repeat_inputs = ContentInputs {
        observations: Observations::capture(&source),
        image: repeat_image,
        bundle: repeat_bundle,
        selected_image: repeat_selected,
    };
    let selected_image = Cell::new(image.digest());
    let inputs = ContentInputs {
        observations,
        image,
        bundle,
        selected_image,
    };
    let limits = PlayerPackageLimits::default();
    let a = assemble(&inputs, &triple, CONTENT_PROFILE);
    let repeat = assemble(&repeat_inputs, &triple, CONTENT_PROFILE);
    assert_eq!(a.base().catalog_digest(), repeat.base().catalog_digest());
    let mut a_pack = Vec::new();
    let mut repeat_pack = Vec::new();
    a.base().write_pack(&mut a_pack).unwrap();
    repeat.base().write_pack(&mut repeat_pack).unwrap();
    assert_eq!(a_pack, repeat_pack);
    drop((a_pack, repeat_pack, repeat));
    drop((repeat_inputs, repeat_import, repeat_store));
    let bootstrap = GeneratedPlayerBootstrap::new(&a, GENERATED, limits).unwrap();
    fs::write(workspace.join(GENERATED), bootstrap.bytes()).unwrap();
    let selection = player_selection(&triple, ["ship"]);
    let request = CargoBuildRequest::new(
        &workspace,
        RelativePath::new("player/Cargo.toml").unwrap(),
        selection.clone(),
        CargoBuildTools::new(&cargo, &rustc).unwrap(),
        PLAYER_INPUTS
            .iter()
            .map(|path| RelativePath::new(path).unwrap()),
        CargoBuildLimits::default(),
    )
    .unwrap()
    .with_target_directory(&target)
    .unwrap();
    let captured = build_executable(&request, |_| {
        inputs.fence().map_err(|_| CargoBuildFailure::Stale)
    })
    .unwrap();
    assert_eq!(captured.provenance().selection(), &selection);
    assert_eq!(
        captured.provenance().resolved_features(),
        ["player-entry", "quality", "ship"]
    );
    assert!(
        captured
            .provenance()
            .declared_inputs()
            .iter()
            .any(|input| input.path() == GENERATED && input.digest() == bootstrap.digest())
    );
    let bounded_a = qualify_binding_limits(&inputs, &triple, &selection, &captured, limits);
    assert!(matches!(
        PreparedPlayerPackage::bind(
            a,
            bootstrap,
            &player_selection(&triple, ["ship", "quality"]),
            captured.clone(),
            limits,
        ),
        Err(PlayerPackageError::BuildSelectionMismatch)
    ));

    // Same source identities and exact startup root, but a distinct content profile is still B.
    let b = assemble(&inputs, &triple, "cpu4-alternate-content");
    let bootstrap_b = GeneratedPlayerBootstrap::new(&b, GENERATED, limits).unwrap();
    assert!(matches!(
        PreparedPlayerPackage::bind(b, bootstrap_b, &selection, captured.clone(), limits),
        Err(PlayerPackageError::BuildInputMismatch)
    ));
    let a = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap_a = GeneratedPlayerBootstrap::new(&a, GENERATED, limits).unwrap();
    let b = assemble(&inputs, &triple, "cpu4-alternate-content");
    assert!(matches!(
        PreparedPlayerPackage::bind(b, bootstrap_a, &selection, captured.clone(), limits),
        Err(PlayerPackageError::BootstrapMismatch)
    ));
    drop(a);

    // A different *actual* processed target output is selected without changing raw inputs.
    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    inputs.selected_image.set(full.digest());
    let mut reached = false;
    let rejected = build_player_package(content, bootstrap, &request, limits, |point| {
        assert_eq!(point, PlayerPackagePoint::BeforeBuild);
        reached = true;
        inputs.fence()
    })
    .unwrap_err();
    assert!(
        reached
            && matches!(
                rejected,
                PlayerPackageError::Control {
                    failure: PlayerPackageControlFailure::SelectionChanged,
                    ..
                }
            )
    );
    inputs.selected_image.set(inputs.image.digest());

    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let metadata_path = source.join("stripe.png.meta");
    let original_metadata = fs::read(&metadata_path).unwrap();
    let mut changed_metadata = original_metadata.clone();
    changed_metadata.push(b' ');
    fs::write(&metadata_path, changed_metadata).unwrap();
    assert!(matches!(
        build_player_package(content, bootstrap, &request, limits, |_| inputs.fence()),
        Err(PlayerPackageError::Control {
            point: PlayerPackagePoint::BeforeBuild,
            failure: PlayerPackageControlFailure::SelectionChanged
        })
    ));
    fs::write(&metadata_path, original_metadata).unwrap();

    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let original_bootstrap = bootstrap.bytes().to_vec();
    let mut changed = false;
    let stale = build_player_package(content, bootstrap, &request, limits, |point| {
        inputs.fence()?;
        if point == PlayerPackagePoint::Cargo(CargoBuildPoint::BeforeReturn) {
            let mut bytes = original_bootstrap.clone();
            bytes.extend_from_slice(b"// changed after compilation\n");
            fs::write(workspace.join(GENERATED), bytes).unwrap();
            changed = true;
        }
        Ok(())
    })
    .unwrap_err();
    assert!(changed);
    assert!(
        matches!(stale, PlayerPackageError::Build { source, .. } if matches!(source.as_ref(), CargoBuildFailure::Stale))
    );
    fs::write(workspace.join(GENERATED), &original_bootstrap).unwrap();

    // Current auxiliary input is rechecked at publication, not inferred from a valid old group.
    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let rejected =
        PreparedPlayerPackage::bind(content, bootstrap, &selection, captured.clone(), limits)
            .unwrap();
    let stale_parent = output.join("rejected-candidate");
    fs::create_dir(&stale_parent).unwrap();
    let parent = host_fs::writable_directory(&stale_parent).unwrap();
    let stale_publication = rejected.publish(
        &parent,
        &RelativeComponent::new("must-not-publish").unwrap(),
        |point| {
            if point == PlayerPackagePoint::BeforePublication {
                fs::write(source.join("suffix.txt"), b" stale auxiliary input").unwrap();
            }
            inputs.fence()
        },
    );
    assert!(
        matches!(
            stale_publication,
            PlayerPackageOutcome::Orphaned {
                error: Some(PlayerPackageError::Control {
                    point: PlayerPackagePoint::BeforePublication,
                    failure: PlayerPackageControlFailure::SelectionChanged
                }),
                ..
            }
        ),
        "{stale_publication:?}"
    );
    assert!(!stale_parent.join("must-not-publish").exists());
    drop(stale_publication);
    drop(parent);
    fs::write(source.join("suffix.txt"), b" cooked world").unwrap();
    inputs.fence().unwrap();

    // Trusted fault injection stays inside this Host's dedicated, checked output subtree. A
    // correct digest for each declared file cannot qualify an undeclared directory member.
    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let extra_member =
        PreparedPlayerPackage::bind(content, bootstrap, &selection, captured.clone(), limits)
            .unwrap();
    let extra_parent = output.join("extra-member-candidate");
    fs::create_dir(&extra_parent).unwrap();
    let owned_output = output.canonicalize().unwrap();
    let checked_parent = extra_parent.canonicalize().unwrap();
    assert!(checked_parent.starts_with(&owned_output) && checked_parent != owned_output);
    let parent = host_fs::writable_directory(&checked_parent).unwrap();
    let mut injected = None;
    let extra_outcome = extra_member.publish(
        &parent,
        &RelativeComponent::new("must-not-publish").unwrap(),
        |point| {
            inputs.fence()?;
            if point == PlayerPackagePoint::BeforePublication {
                assert!(injected.is_none());
                let mut staging = None;
                for entry in fs::read_dir(&checked_parent).unwrap() {
                    let entry = entry.unwrap();
                    if !entry
                        .file_name()
                        .to_str()
                        .unwrap()
                        .starts_with(".nara-player-")
                    {
                        continue;
                    }
                    let kind = entry.file_type().unwrap();
                    assert!(kind.is_dir() && !kind.is_symlink());
                    let checked_staging = entry.path().canonicalize().unwrap();
                    assert_eq!(checked_staging.parent(), Some(checked_parent.as_path()));
                    assert!(checked_staging.starts_with(&owned_output));
                    assert!(
                        staging.replace(checked_staging).is_none(),
                        "only this publication owns a staging directory"
                    );
                }
                let staging = staging.expect("the current publication's staging directory");
                let undeclared = staging.join("undeclared.bin");
                assert_eq!(undeclared.parent(), Some(staging.as_path()));
                assert!(!undeclared.exists());
                fs::write(&undeclared, b"undeclared delivery member").unwrap();
                injected = Some(undeclared);
            }
            Ok(())
        },
    );
    assert!(
        matches!(
            &extra_outcome,
            PlayerPackageOutcome::Orphaned {
                error: Some(PlayerPackageError::StagedMembers),
                ..
            }
        ),
        "{extra_outcome:?}"
    );
    assert!(!checked_parent.join("must-not-publish").exists());
    assert!(
        injected.unwrap().is_file(),
        "the nonempty rejected candidate keeps its orphan custody"
    );
    assert_eq!(ContentDigest::of_bytes(captured.bytes()), captured.digest());
    drop((extra_outcome, parent));

    // Captured A and its prepared package outlive every rejected budget/binding/fence attempt.
    // Publish those original bytes without another Cargo build, retaining the actual receipt.
    let bounded_parent = host_fs::writable_directory(&output).unwrap();
    let bounded_outcome = bounded_a.publish(
        &bounded_parent,
        &RelativeComponent::new("bounded-a-preserved").unwrap(),
        |_| inputs.fence(),
    );
    let PlayerPackageOutcome::Published {
        receipt,
        error: None,
    } = bounded_outcome
    else {
        panic!("{bounded_outcome:?}")
    };
    assert_eq!(receipt.files.executable, captured.digest());
    assert_eq!(ContentDigest::of_bytes(captured.bytes()), captured.digest());
    assert_eq!(
        receipt.files.executable,
        ContentDigest::of_bytes(
            &fs::read(
                output
                    .join("bounded-a-preserved")
                    .join(&receipt.files.executable_name)
            )
            .unwrap()
        )
    );
    drop((receipt, bounded_parent));

    // A postpublication callback cannot erase the already published directory or its receipt.
    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let already_published =
        PreparedPlayerPackage::bind(content, bootstrap, &selection, captured.clone(), limits)
            .unwrap();
    let receipt_parent = host_fs::writable_directory(&output).unwrap();
    let outcome = already_published.publish(
        &receipt_parent,
        &RelativeComponent::new("receipt-preserved").unwrap(),
        |point| {
            inputs.fence()?;
            if point == PlayerPackagePoint::AfterPublication {
                Err(PlayerPackageControlFailure::Cancelled)
            } else {
                Ok(())
            }
        },
    );
    let PlayerPackageOutcome::Published {
        receipt,
        error:
            Some(PlayerPackageError::Control {
                point: PlayerPackagePoint::AfterPublication,
                failure: PlayerPackageControlFailure::Cancelled,
            }),
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert!(output.join("receipt-preserved/content.pack").is_file());
    assert_eq!(
        receipt.files.executable_name,
        format!("{PLAYER_BIN}{}", std::env::consts::EXE_SUFFIX)
    );
    drop((receipt, receipt_parent));

    let content = assemble(&inputs, &triple, CONTENT_PROFILE);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    assert_eq!(bootstrap.bytes(), original_bootstrap);
    let mut points = Vec::new();
    let prepared = build_player_package(content, bootstrap, &request, limits, |point| {
        points.push(point);
        inputs.fence()
    })
    .unwrap();
    assert!(points.contains(&PlayerPackagePoint::BeforeBuild));
    assert!(points.contains(&PlayerPackagePoint::AfterBuild));
    assert_eq!(
        prepared.executable_name(),
        format!("{PLAYER_BIN}{}", std::env::consts::EXE_SUFFIX)
    );
    let envelope: serde_json::Value = serde_json::from_slice(prepared.manifest_bytes()).unwrap();
    assert_eq!(envelope["kind"], "player_package");
    assert_eq!(envelope["format_version"], 1);
    assert_eq!(envelope["engine_min_version"], "0.1.0");
    assert_eq!(
        envelope["generator"],
        serde_json::json!({"name": "nara", "version": "0.1.0"})
    );
    let manifest = &envelope["payload"];
    assert_eq!(
        manifest["build"]["selection"]["cargo_profile"],
        CARGO_PROFILE
    );
    assert_eq!(manifest["content_target"]["profile"], CONTENT_PROFILE);
    let output_authority = host_fs::writable_directory(&output).unwrap();
    let published = prepared.publish(
        &output_authority,
        &RelativeComponent::new("published").unwrap(),
        |point| {
            points.push(point);
            inputs.fence()
        },
    );
    let PlayerPackageOutcome::Published {
        receipt,
        error: None,
    } = published
    else {
        panic!("{published:?}")
    };
    assert!(points.contains(&PlayerPackagePoint::AfterPackWrite));
    assert!(points.contains(&PlayerPackagePoint::BeforePublication));
    assert_eq!(points.last(), Some(&PlayerPackagePoint::AfterPublication));
    assert_eq!(
        receipt.files.catalog,
        ContentDigest::of_bytes(&catalog_bytes(&output.join("published/content.pack")))
    );
    assert_eq!(
        receipt.files.pack,
        ContentDigest::of_bytes(&fs::read(output.join("published/content.pack")).unwrap())
    );
    assert_eq!(
        receipt.files.executable_name,
        format!("{PLAYER_BIN}{}", std::env::consts::EXE_SUFFIX)
    );
    drop((
        receipt,
        output_authority,
        captured,
        inputs,
        full,
        imported,
        store,
    ));
    println!(
        "cooked-host ok actual-import cpu-target compound-aux deterministic-pack binding-fences published"
    );
}

fn qualify_binding_limits(
    inputs: &ContentInputs,
    triple: &str,
    selection: &CargoBuildSelection,
    captured: &CapturedCargoExecutable,
    limits: PlayerPackageLimits,
) -> PreparedPlayerPackage {
    let content = assemble(inputs, triple, CONTENT_PROFILE);
    let catalog_digest = content.base().catalog_digest();
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    let bootstrap_retained = bootstrap.retained_bytes();
    let retained_a =
        PreparedPlayerPackage::bind(content, bootstrap, selection, captured.clone(), limits)
            .unwrap();
    let manifest_bytes = retained_a.manifest_bytes().len();
    let disk_bytes = usize::try_from(
        retained_a
            .pack_digest()
            .length()
            .checked_add(captured.digest().length())
            .unwrap()
            .checked_add(u64::try_from(manifest_bytes).unwrap())
            .unwrap(),
    )
    .unwrap();
    let retained_bytes = retained_a
        .retained_bytes()
        .checked_add(bootstrap_retained)
        .unwrap();
    // File digest verification alone needs a 64 KiB buffer. That allowance without the hasher
    // and bounded staging metadata must still reject; exact private scratch accounting belongs
    // to the owning module's tests. Retained/disk/manifest limits below are public exact facts.
    let insufficient_work_bytes = retained_bytes.checked_add(64 * 1024).unwrap();
    let exact = PlayerPackageLimits {
        manifest_bytes: ByteLimit::new(manifest_bytes).unwrap(),
        retained_bytes: ByteLimit::new(retained_bytes).unwrap(),
        disk_bytes: ByteLimit::new(disk_bytes).unwrap(),
        ..limits
    };
    let content = assemble(inputs, triple, CONTENT_PROFILE);
    assert_eq!(content.base().catalog_digest(), catalog_digest);
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    assert_eq!(bootstrap.retained_bytes(), bootstrap_retained);
    let admitted =
        PreparedPlayerPackage::bind(content, bootstrap, selection, captured.clone(), exact)
            .unwrap();
    assert_eq!(admitted.retained_bytes(), retained_a.retained_bytes());
    assert_eq!(admitted.pack_digest(), retained_a.pack_digest());
    assert_eq!(admitted.manifest_bytes(), retained_a.manifest_bytes());
    drop(admitted);

    let executable_digest = captured.digest();
    let provenance_digest = captured.provenance().digest();
    for (case, one_short) in [
        (
            "disk",
            PlayerPackageLimits {
                disk_bytes: ByteLimit::new(disk_bytes - 1).unwrap(),
                ..exact
            },
        ),
        (
            "manifest",
            PlayerPackageLimits {
                manifest_bytes: ByteLimit::new(manifest_bytes - 1).unwrap(),
                ..exact
            },
        ),
        (
            "retained",
            PlayerPackageLimits {
                retained_bytes: ByteLimit::new(retained_bytes - 1).unwrap(),
                ..exact
            },
        ),
        (
            "work",
            PlayerPackageLimits {
                work_bytes: ByteLimit::new(insufficient_work_bytes).unwrap(),
                ..exact
            },
        ),
    ] {
        let content = assemble(inputs, triple, CONTENT_PROFILE);
        assert_eq!(content.base().catalog_digest(), catalog_digest);
        let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
        assert_eq!(bootstrap.retained_bytes(), bootstrap_retained);
        // No publication authority enters this call. A rejected preparation cannot stage files.
        let rejected =
            PreparedPlayerPackage::bind(content, bootstrap, selection, captured.clone(), one_short)
                .unwrap_err();
        let correct_boundary = match case {
            "disk" => {
                matches!(rejected, PlayerPackageError::Limit { kind: "disk payload", required, limit }
                if required == disk_bytes && limit == disk_bytes - 1)
            }
            "manifest" => matches!(rejected, PlayerPackageError::Manifest),
            "retained" => {
                matches!(rejected, PlayerPackageError::Limit { kind: "manifest retention", required, limit }
                if required == retained_bytes && limit == retained_bytes - 1)
            }
            "work" => {
                matches!(rejected, PlayerPackageError::Limit { kind: "publication work", required, limit }
                if required > insufficient_work_bytes && limit == insufficient_work_bytes)
            }
            _ => unreachable!(),
        };
        assert!(correct_boundary, "{case}: {rejected:?}");
        assert_eq!(ContentDigest::of_bytes(captured.bytes()), executable_digest);
        assert_eq!(captured.provenance().digest(), provenance_digest);
        assert_eq!(retained_a.manifest_bytes().len(), manifest_bytes);
        inputs.fence().unwrap();
    }
    retained_a
}

fn player_selection<const N: usize>(triple: &str, features: [&str; N]) -> CargoBuildSelection {
    CargoBuildSelection::new(
        PLAYER_PACKAGE,
        PLAYER_BIN,
        CARGO_PROFILE,
        triple,
        features,
        false,
    )
    .unwrap()
}

fn scope() -> AssetPathScope {
    AssetPathScope::under(AssetSourcePath::new("assets").unwrap())
}

struct Observations {
    root: PathBuf,
    files: Vec<(&'static str, ContentDigest)>,
}
impl Observations {
    fn capture(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            files: OBSERVED
                .iter()
                .map(|path| {
                    (
                        *path,
                        ContentDigest::of_bytes(&fs::read(root.join(path)).unwrap()),
                    )
                })
                .collect(),
        }
    }
    fn current(&self) -> bool {
        self.files.iter().all(|(path, expected)| {
            fs::read(self.root.join(path))
                .is_ok_and(|bytes| ContentDigest::of_bytes(&bytes) == *expected)
        })
    }
    fn artifact_fence(&self) -> Result<(), ArtifactBuildError> {
        if self.current() {
            Ok(())
        } else {
            Err(ArtifactBuildError::Stale)
        }
    }
}

struct ContentInputs {
    observations: Observations,
    image: VerifiedArtifactGroup,
    bundle: VerifiedArtifactGroup,
    selected_image: Cell<ArtifactContentDigest>,
}
impl ContentInputs {
    fn fence(&self) -> Result<(), PlayerPackageControlFailure> {
        if self.observations.current() && self.selected_image.get() == self.image.digest() {
            Ok(())
        } else {
            Err(PlayerPackageControlFailure::SelectionChanged)
        }
    }
}

fn assemble(inputs: &ContentInputs, triple: &str, profile: &str) -> PreparedRuntimeContent {
    inputs.fence().unwrap();
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    configuration.input.action_map = Some(game::product(game::INPUT));
    let recipe = game::recipe();
    let plan =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone()).unwrap();
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            target: RuntimeContentTarget::new(triple, profile).unwrap(),
            configuration,
            startup_scene: game::product(1),
            scenes: vec![game::product(1), game::product(2), game::product(3)],
            addressable_roots: vec![],
        },
        recipe,
        CookedContentLimits::default(),
    )
    .unwrap();
    builder.add_group(&inputs.image).unwrap();
    builder.add_group(&inputs.bundle).unwrap();
    let root = &inputs.observations.root;
    let input = ActionMapFileCandidate::decode_json_bytes(
        &fs::read(root.join("player.actions.json")).unwrap(),
    )
    .unwrap();
    let (product, definition) = input.into_parts();
    builder
        .add_input(&ActionMapAsset::new(product, definition).unwrap())
        .unwrap();
    let tileset = TileSetSource::decode_ron_bytes(
        &fs::read(root.join("tileset.ron")).unwrap(),
        TileSetSourceFileLimits::default(),
    )
    .unwrap();
    let AssetRef::Product(image) = tileset.image else {
        panic!("explicit Image Product")
    };
    builder
        .add_tileset(
            &TileSetDescriptor::new(game::product(game::TILESET), image, tileset.atlas)
                .unwrap()
                .with_sampler(tileset.sampler)
                .with_alpha_mode(tileset.alpha_mode)
                .with_tint(tileset.tint)
                .unwrap(),
        )
        .unwrap();
    let mut database = ProjectAssetDatabase::default();
    for path in [
        "stripe.png",
        "bundle.json",
        "tileset.ron",
        "player.actions.json",
        "1.scene.json",
        "2.scene.json",
        "3.scene.json",
    ] {
        let identity = AssetMetaIdentityCandidate::decode_json_bytes(
            &fs::read(root.join(format!("{path}.meta"))).unwrap(),
        )
        .unwrap()
        .into_identity();
        database.insert(identity.into_record(&scope())).unwrap();
    }
    let scope = scope();
    for scene in 1..=3 {
        let document = SceneDocumentCandidate::decode_json_bytes(
            &fs::read(root.join(format!("{scene}.scene.json"))).unwrap(),
        )
        .unwrap()
        .into_composition_source()
        .unwrap();
        let compilation = plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                &document,
                SceneCompilationOptions::default(),
                Some(database.resolver(&scope)),
                |_, _| Ok::<_, std::convert::Infallible>(None),
            )
            .unwrap();
        assert!(
            !compilation.diagnostics.has_errors(),
            "{:?}",
            compilation.diagnostics
        );
        let compiled = compilation
            .candidate
            .expect("the saved game Scene compiles")
            .bind_source(SourceDocumentKey::stable_asset(
                game::product(scene).source(),
                SourceDocumentKind::Scene,
            ));
        builder
            .add_scene(game::product(scene), &compiled, database.resolver(&scope))
            .unwrap();
    }
    let prepared = builder.finish().unwrap();
    let catalog = prepared.base().catalog();
    assert_eq!(catalog.members().len(), 7);
    let first = catalog.member_for_product(game::product(1)).unwrap();
    assert!(
        first
            .dependencies()
            .iter()
            .any(|dependency| dependency.product() == game::product(3)
                && dependency.role() == RuntimeContentDependencyRole::Reference)
    );
    assert_eq!(
        catalog
            .member_for_product(game::product(game::BUNDLE))
            .unwrap()
            .path(),
        catalog.member_for_product(game::caption()).unwrap().path()
    );
    inputs.fence().unwrap();
    prepared
}

fn definition(id: &str) -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    ArtifactBuildDefinition {
        id: id.to_owned(),
        revision: FormatVersion::ONE,
        implementation: *IMPLEMENTATION.get_or_init(|| {
            let path = std::env::current_exe().unwrap();
            let file = host_fs::readable_file(&path).unwrap();
            ArtifactContentDigest::from_content(
                file.digest(fs::metadata(path).unwrap().len()).unwrap(),
            )
        }),
    }
}

fn cook(
    store: &ArtifactBuildStore,
    input: &VerifiedArtifactGroup,
    cap: u32,
    observations: &Observations,
) -> VerifiedArtifactGroup {
    cook_image_group(
        store,
        input,
        game::product(game::IMAGE),
        definition("consumer.cooked-player.cpu-cook"),
        ImageCookSettings {
            oversize: ImageOversizePolicy::FitNearest,
        },
        ImageCookTarget::new(NonZeroU32::new(cap).unwrap()),
        ImageCookLimits::new(
            ByteLimit::new(1024 * 1024).unwrap(),
            ByteLimit::new(2 * 1024 * 1024).unwrap(),
        ),
        |_| observations.artifact_fence(),
    )
    .unwrap()
    .into_group()
}

fn metadata(path: &str, product: engine::asset::AssetProductRef, kind: AssetSourceKind) -> String {
    AssetMeta::new(
        AssetMetaIdentity::new(product.source(), AssetPath::new(path).unwrap(), kind),
        (),
    )
    .to_json_string()
    .unwrap()
}

fn prepare_source(root: &Path) {
    assert!(root.is_dir());
    // The PNG is written by the outer independent consumer before this Host is built or launched.
    assert!(root.join("stripe.png").is_file());
    fs::write(
        root.join("stripe.png.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                game::product(game::IMAGE).source(),
                AssetPath::new("stripe.png").unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(ImageColorSpace::Srgb),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    fs::write(root.join("bundle.json"), BUNDLE_RAW).unwrap();
    fs::write(root.join("suffix.txt"), b" cooked world").unwrap();
    let settings =
        TextBundleSettings::initial(BUNDLE_RAW, 64, [game::caption().product()]).unwrap();
    fs::write(
        root.join("bundle.json.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                game::product(game::BUNDLE).source(),
                AssetPath::new("bundle.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let tileset = TileSetSource::new(
        AssetRef::Product(game::product(game::IMAGE)),
        TileAtlasLayout::grid(Vec2::ONE, 4, 1),
    );
    fs::write(
        root.join("tileset.ron"),
        tileset.encode_ron_bytes().unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("tileset.ron.meta"),
        metadata(
            "tileset.ron",
            game::product(game::TILESET),
            AssetSourceKind::Other("tileset".into()),
        ),
    )
    .unwrap();
    let mut input = ActionMapBuilder::new(ActionMapLimits::default());
    input
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    input.declare_context(game::probe::menu(), false).unwrap();
    input
        .bind(ActionBinding::key(
            ActionId::new(game::probe::MOVE).unwrap(),
            game::probe::KEY,
        ))
        .unwrap();
    input
        .bind(ActionBinding::mouse(
            ActionId::new(game::probe::FIRE).unwrap(),
            game::probe::MOUSE,
        ))
        .unwrap();
    fs::write(
        root.join("player.actions.json"),
        ActionMapFileCandidate::new(game::product(game::INPUT), input.compile().unwrap())
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("player.actions.json.meta"),
        metadata(
            "player.actions.json",
            game::product(game::INPUT),
            AssetSourceKind::Other("action_map".into()),
        ),
    )
    .unwrap();
    for scene in 1..=3 {
        let name = format!("{scene}.scene.json");
        fs::write(
            root.join(&name),
            game::document(scene).to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(format!("{name}.meta")),
            metadata(&name, game::product(scene), AssetSourceKind::Scene),
        )
        .unwrap();
    }
}

fn catalog_bytes(pack: &Path) -> Vec<u8> {
    let bytes = fs::read(pack).unwrap();
    assert_eq!(&bytes[..8], b"NARAPACK");
    let length = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap())).unwrap();
    bytes[20..20 + length].to_vec()
}
