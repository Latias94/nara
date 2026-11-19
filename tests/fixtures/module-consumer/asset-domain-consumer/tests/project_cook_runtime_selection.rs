//! Project Cook packs the Runtime closure of a typed badge and retains its source inputs.

#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::CurrentScene,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetPathScope, AssetProductRef, AssetRef,
        AssetReferenceDemand, AssetSourceKind, AssetSourcePath, Assets, ImportedProductId,
        SourceAssetBuild, SourceAssetImporter, StableAssetId,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildReason, ArtifactBuildStatus, ArtifactBuildStore, ArtifactContentDigest,
        ArtifactSourceRequest, build_source,
    },
    core::FormatVersion,
    ecs::Component,
    fs::{ContentDigest, DirectoryCapability, FsError, RelativePath},
    identity::EntityLookup,
    image::{ImageAsset, ImageExtent, ImageImportSettings, ImageRuntimeCodec},
    project_host::{
        HeadlessSession, HeadlessSessionClose, ProjectContentMemberKind, ProjectDomainAssets,
        ProjectSettingsCandidate, ingest_project_manifest, resolve_product_recipe,
    },
    project_tooling::{
        host_fs::{readable_directory, readable_file, writable_directory},
        project_cook::{ProjectCookError, ProjectCookRequest, prepare_project_cooked_content},
    },
    project_workflow::{
        ProjectCheckRequest, check_project,
        cooked_content::{CookedContentLimits, PreparedRuntimeContent},
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentLimits, RuntimeContentTarget,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
};
use serde::Serialize;

#[path = "support/project_cook_badge.rs"]
mod badge;

use badge::{BadgeArt, BadgeDomain, BadgeSettings, BadgeSource, ImageDelivery};

const SCENE: &str = "startup.scene.json";
const BADGE: &str = "badge.json";
const LATER_SCENE: &str = "later.scene.json";
const LATER_BADGE: &str = "later.badge.json";
const PNG: &str = "badge.png";
const PNG_SOURCE: &str = "assets/badge.png";
const SCENE_ID: &str = "25910000-0000-4000-8000-000000000001";
const BADGE_ID: &str = "25910000-0000-4000-8000-000000000002";
const IMAGE_ID: &str = "25910000-0000-4000-8000-000000000003";
const UNUSED_ID: &str = "25910000-0000-4000-8000-000000000004";
const LATER_SCENE_ID: &str = "25910000-0000-4000-8000-000000000006";
const LATER_BADGE_ID: &str = "25910000-0000-4000-8000-000000000007";
const PLACEMENT: &str = "consumer.cook-badge.Placement";
const PNG_BEFORE: &[u8] = include_bytes!("../../../images/valid-rgba-1x1.png");
const PNG_AFTER: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x01, 0x01, 0x05, 0x00, 0xfa, 0xff,
    0x00, 0xe8, 0x40, 0x18, 0xff, 0x05, 0x94, 0x02, 0x40, 0xad, 0x45, 0xa8, 0x9e, 0x00, 0x00, 0x00,
    0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn project_cook_embeds_build_only_png_and_rebuilds_after_source_drift() {
    exercise_project_cook(ImageDelivery::Embedded);
}

#[test]
fn project_cook_retains_linked_image_and_rebuilds_after_source_drift() {
    exercise_project_cook(ImageDelivery::Linked);
}

#[test]
fn project_cook_revisits_earlier_image_once_for_later_scene() {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(
        &project,
        ImageDelivery::Embedded,
        Some(ImageDelivery::LinkedAtBuild),
        &recipe,
    );
    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let build_group = |path| {
        let request = build_request(&root, path, implementation);
        build_source::<BadgeDomain>(&store, &request, |_| Ok(()))
            .unwrap()
            .into_group()
    };
    let embedded_group = build_group(BADGE);
    let linked_group = build_group(LATER_BADGE);
    let imports_before = badge::embedded_imports();
    let mut contributions = Vec::new();
    let image = product(IMAGE_ID);
    let roots = [product(SCENE_ID), product(LATER_SCENE_ID)];
    let prepared = prepare_project_cooked_content(
        ProjectCookRequest {
            root: &root,
            candidate: &candidate,
            recipe: recipe.clone(),
            target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
            store: &store,
            implementation,
            limits: CookedContentLimits::default(),
        },
        |context| {
            let snapshot = context.snapshot();
            let group = match snapshot.selected_scene_path().as_str() {
                SCENE => {
                    let [available] = snapshot.images() else {
                        panic!("the startup source snapshot must contain exactly one Image");
                    };
                    assert_eq!(available.image().product(), Some(image));
                    &embedded_group
                }
                LATER_SCENE => {
                    assert!(snapshot.images().is_empty());
                    &linked_group
                }
                path => panic!("unexpected Scene contribution: {path}"),
            };
            contributions.push(snapshot.selected_scene_product().unwrap());
            context.add_group::<BadgeDomain>(group)
        },
    )
    .expect("Project Cook must recover the earlier stock Image for the later Badge");
    assert_eq!(contributions, roots);
    assert_eq!(badge::embedded_imports() - imports_before, 2);

    let (content, _) = prepared.into_parts();
    let base = reopen_pack(content, &scratch.0.join("later.pack"), &recipe);
    let catalog = base.catalog();
    let [startup, later] = roots;
    let embedded = product(BADGE_ID);
    let linked = product(LATER_BADGE_ID);
    assert_eq!(catalog.startup_scene(), startup);
    assert_eq!(catalog.scenes(), roots);
    assert_eq!(catalog.members().len(), 5);
    for (member, dependency) in [
        (startup, Some(embedded)),
        (later, Some(linked)),
        (embedded, None),
        (linked, Some(image)),
        (image, None),
    ] {
        assert_eq!(
            catalog.member_for_product(member).unwrap().dependencies(),
            dependency.map(required).as_slice()
        );
    }
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), 5);
    assert_eq!(complete.products(), 5);
}

fn exercise_project_cook(image_delivery: ImageDelivery) {
    let scratch = Scratch::new();
    let project = scratch.0.join("project");
    let recipe = recipe();
    let (root, candidate) = write_project(&project, image_delivery, None, &recipe);

    let checked = check_project(ProjectCheckRequest::new(root.share(), recipe.clone())).unwrap();
    assert!(checked.members().iter().any(|member| {
        member.path().as_str() == PNG_SOURCE
            && member.kind() == ProjectContentMemberKind::AssetSource
            && member.digest() == ContentDigest::of_bytes(PNG_BEFORE)
    }));
    assert!(checked.members().iter().any(|member| {
        member.path().as_str() == "assets/badge.png.meta"
            && member.kind() == ProjectContentMemberKind::AssetMetadata
    }));

    let store = open_store(&scratch.0);
    let implementation = implementation_digest();
    let request = build_request(&root, BADGE, implementation);
    let build = || build_source::<BadgeDomain>(&store, &request, |_| Ok(())).unwrap();
    let built = build();
    assert_eq!(built.status(), ArtifactBuildStatus::Built);
    assert!(built.publication_receipt().is_some());
    let hit = build();
    assert_eq!(hit.status(), ArtifactBuildStatus::Hit);
    assert_eq!(hit.reason(), &ArtifactBuildReason::Reused);
    assert_eq!(hit.group().digest(), built.group().digest());
    let group = hit.into_group();
    drop(built);

    let image = (image_delivery == ImageDelivery::Linked).then(|| product(IMAGE_ID));
    let edge = image.map(|image| (image, AssetReferenceDemand::Immediate));
    let [member] = group.members() else {
        panic!("the badge build must produce one member");
    };
    assert_eq!(member.products, [product(BADGE_ID)]);
    assert_eq!(member.dependencies.as_slice(), edge.as_slice());
    assert_png_observation(group.observations(), PNG_BEFORE);

    let cook_request = || ProjectCookRequest {
        root: &root,
        candidate: &candidate,
        recipe: recipe.clone(),
        target: RuntimeContentTarget::new("consumer-headless", "badge").unwrap(),
        store: &store,
        implementation,
        limits: CookedContentLimits::default(),
    };
    let cook_retained = || {
        prepare_project_cooked_content(cook_request(), |context| {
            context.add_group::<BadgeDomain>(&group)
        })
    };
    let first = cook_retained().expect("Project Cook must deliver the badge's Runtime closure");
    let (content, selection) = first.into_parts();
    assert_png_observation(selection.observations(), PNG_BEFORE);
    selection.verify(&project).unwrap();
    observe_player(
        content,
        &scratch.0.join("first.pack"),
        &recipe,
        image,
        PNG_BEFORE,
        [24, 120, 220, 255],
    );

    fs::write(project.join(PNG_SOURCE), PNG_AFTER).unwrap();
    let drift = selection.verify(&project).unwrap_err();
    assert!(
        matches!(drift, ProjectCookError::Filesystem(FsError::DigestMismatch { expected, actual })
            if expected == ContentDigest::of_bytes(PNG_BEFORE)
                && actual == ContentDigest::of_bytes(PNG_AFTER)),
        "{drift:?}"
    );
    assert!(
        cook_retained().is_err(),
        "a later Cook accepted the stale badge group"
    );

    let rebuilt = build();
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        rebuilt.reason(),
        &ArtifactBuildReason::RawInputChanged(AssetSourcePath::new(PNG_SOURCE).unwrap()),
    );
    assert_ne!(rebuilt.group().digest(), group.digest());
    assert_png_observation(rebuilt.group().observations(), PNG_AFTER);

    let second = prepare_project_cooked_content(cook_request(), |context| {
        context.add_source_assets::<BadgeDomain>()
    })
    .expect("Project Cook must see the rebuilt badge after PNG drift");
    let (content, selection) = second.into_parts();
    assert_png_observation(selection.observations(), PNG_AFTER);
    selection.verify(&project).unwrap();
    observe_player(
        content,
        &scratch.0.join("second.pack"),
        &recipe,
        image,
        PNG_AFTER,
        [232, 64, 24, 255],
    );
}

fn assert_png_observation(observations: &[ArtifactBuildObservation], png: &[u8]) {
    let expected = ArtifactBuildObservation::Raw {
        path: AssetSourcePath::new(PNG_SOURCE).unwrap(),
        digest: Some(ArtifactContentDigest::of_bytes(png)),
    };
    assert!(
        observations.contains(&expected),
        "missing PNG observation: {expected:?}"
    );
}

fn reopen_pack(
    content: PreparedRuntimeContent,
    pack: &Path,
    recipe: &ProductRecipe,
) -> RuntimeContentBase {
    content
        .base()
        .write_pack(&mut File::create_new(pack).unwrap())
        .unwrap();
    let expectation = content.expectation().clone();
    drop(content);
    RuntimeContentBase::open_pack(
        readable_file(pack).unwrap(),
        &expectation,
        recipe.clone(),
        RuntimeContentLimits::default(),
    )
    .unwrap()
}

fn required(target: AssetProductRef) -> RuntimeContentDependency {
    RuntimeContentDependency::new(target, RuntimeContentDependencyRole::Required)
}

fn observe_player(
    content: PreparedRuntimeContent,
    pack: &Path,
    recipe: &ProductRecipe,
    image: Option<AssetProductRef>,
    expected_png: &[u8],
    expected_pixel: [u8; 4],
) {
    let catalog_digest = content.expectation().catalog_digest();
    let base = reopen_pack(content, pack, recipe);
    let catalog = base.catalog();
    let scene = product(SCENE_ID);
    let badge = product(BADGE_ID);
    let image_edge = image.map(required);
    let members = 2 + usize::from(image.is_some());
    assert_eq!(catalog.startup_scene(), scene);
    assert_eq!(catalog.scenes(), [scene]);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(catalog.members().len(), members);
    assert_eq!(
        catalog.member_for_product(scene).unwrap().dependencies(),
        [required(badge)]
    );
    assert_eq!(
        catalog.member_for_product(badge).unwrap().dependencies(),
        image_edge.as_slice()
    );
    assert_eq!(
        catalog.member_for_product(product(IMAGE_ID)).is_some(),
        image.is_some()
    );
    assert!(catalog.member_for_product(product(UNUSED_ID)).is_none());
    let complete = base.validate_deep().unwrap();
    assert_eq!(complete.members(), members);
    assert_eq!(complete.products(), members);
    assert_eq!(complete.catalog_digest(), catalog_digest);

    let mut player = HeadlessSession::from_runtime_content(base);
    player.start().expect("the cooked Player must start");
    assert!(player.frame(Duration::ZERO).unwrap().is_none());
    player
        .observe(|world| {
            let current = CurrentScene::get(world).unwrap();
            let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &placement_id())
            else {
                panic!("the authored badge placement must exist in the Player");
            };
            let placed = world.get::<Placement>(entity).unwrap();
            assert_eq!(placed.badge.as_product(), Some(badge));
            let badges = world
                .resource::<ProjectDomainAssets>()
                .collection::<BadgeDomain>()
                .unwrap();
            assert_eq!(badges.iter().len(), 1);
            assert_eq!(badges.source_records().len(), 0);
            let decoded = badges.get(&placed.badge).unwrap();
            assert_eq!(decoded.product, badge);
            let images = world.resource::<Assets<ImageAsset>>();
            match (&decoded.art, image) {
                (BadgeArt::Embedded(png), None) => {
                    assert_eq!(png.as_ref(), expected_png);
                    assert!(images.is_empty());
                }
                (BadgeArt::Linked(target), Some(expected)) => {
                    assert_eq!(*target, expected);
                    assert_eq!(images.len(), 1);
                    let (_, published) = images.iter().next().unwrap();
                    assert_eq!(published.product(), Some(*target));
                    assert!(published.source().is_none());
                    assert_eq!(published.extent(), ImageExtent::new(1, 1));
                    assert_eq!(published.pixels(), expected_pixel);
                }
                (art, expected) => panic!("unexpected decoded badge {art:?} for {expected:?}"),
            }
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match player.close() {
            HeadlessSessionClose::Complete => break,
            HeadlessSessionClose::Failed => {
                panic!("Player close failed: {:?}", player.diagnostics());
            }
            HeadlessSessionClose::Pending => {
                assert!(Instant::now() < deadline, "Player close timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.cook-badge.Placement",
    version = 1,
    alias = "Badge Placement",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct Placement {
    #[nara(id = "badge", alias = "Badge", capabilities(scene, asset_ref))]
    badge: AssetRef,
}

nara::persistent_components! { mod components = [Placement]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.cook-badge.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.cook-badge"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.cook-badge.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct BadgePlugin;

impl Plugin for BadgePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.cook-badge"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<BadgePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_asset_domain::<BadgeDomain>()
        .unwrap()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.cook-badge.image",
            1,
        ))
        .unwrap()
        .add_runtime_asset_codec::<BadgeDomain, BadgeDomain>(RuntimeCodecDefinitionId::new(
            "consumer.cook-badge.codec",
            1,
        ))
        .unwrap()
}

fn write_project(
    project: &Path,
    image_delivery: ImageDelivery,
    later_image_delivery: Option<ImageDelivery>,
    recipe: &ProductRecipe,
) -> (DirectoryCapability, ProjectSettingsCandidate) {
    for directory in ["assets", "scenes", "prefabs"] {
        fs::create_dir_all(project.join(directory)).unwrap();
    }
    let mut manifest = r#"schema_version = 1
[project]
source_universe = '25910000-0000-4000-8000-000000000005'
native_document_identity = 'project_location'
name = "Badge Cook consumer"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
[tasks.io]
workers = 1
pending_capacity = 16
[tasks.compute]
workers = 1
pending_capacity = 16
[tasks.async_compute]
workers = 1
pending_capacity = 16
"#
    .to_owned();
    write_badge_scene(
        project,
        (SCENE, SCENE_ID),
        (BADGE, BADGE_ID),
        image_delivery,
    );
    if let Some(image_delivery) = later_image_delivery {
        manifest.push_str(&format!("\n[content]\nscenes = [\"{LATER_SCENE}\"]\n"));
        write_badge_scene(
            project,
            (LATER_SCENE, LATER_SCENE_ID),
            (LATER_BADGE, LATER_BADGE_ID),
            image_delivery,
        );
    }
    fs::write(project.join("nara.toml"), manifest).unwrap();

    let assets = project.join("assets");
    for (name, id) in [(PNG, IMAGE_ID), ("unused.png", UNUSED_ID)] {
        fs::write(assets.join(name), PNG_BEFORE).unwrap();
        write_meta(
            &assets,
            name,
            id,
            AssetSourceKind::Image,
            ImageImportSettings::default(),
        );
    }

    let root = readable_directory(project).unwrap();
    let manifest = root
        .open_file_exact(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    let bundle = plan
        .schema_validation()
        .authoring_schema_bundle()
        .to_json_string()
        .unwrap();
    fs::write(project.join("authoring.bundle.json"), bundle).unwrap();
    (root, candidate)
}

fn write_badge_scene(
    project: &Path,
    (scene, scene_id): (&str, &str),
    (badge, badge_id): (&str, &str),
    image_delivery: ImageDelivery,
) {
    let placement = Placement {
        badge: AssetRef::Product(product(badge_id)),
    };
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(placement_id())
        .with_component(
            ComponentTypeId::new(PLACEMENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                placement.__encode_persistent_component().unwrap(),
            ),
        )]);
    let scenes = project.join("scenes");
    fs::write(scenes.join(scene), document.to_json_string().unwrap()).unwrap();
    write_meta(&scenes, scene, scene_id, AssetSourceKind::Scene, ());

    let assets = project.join("assets");
    let source = BadgeSource {
        png: AssetPath::new(PNG).unwrap(),
        image: product(IMAGE_ID),
    };
    fs::write(assets.join(badge), serde_json::to_vec(&source).unwrap()).unwrap();
    write_meta(
        &assets,
        badge,
        badge_id,
        AssetSourceKind::Other(BadgeDomain::SOURCE_KIND.into()),
        BadgeSettings { image_delivery },
    );
}

fn open_store(parent: &Path) -> ArtifactBuildStore {
    let cache = parent.join("cache");
    fs::create_dir(&cache).unwrap();
    ArtifactBuildStore::open(
        writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap()
}

fn build_request(
    root: &DirectoryCapability,
    badge: &str,
    implementation: ArtifactContentDigest,
) -> ArtifactSourceRequest {
    ArtifactSourceRequest::new(
        root.open_directory_exact(&RelativePath::new("assets").unwrap())
            .unwrap(),
        AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
        AssetPath::new(badge).unwrap(),
        ArtifactBuildDefinition {
            id: BadgeDomain::BUILD_ID.to_owned(),
            revision: FormatVersion::new(BadgeDomain::BUILD_REVISION).unwrap(),
            implementation,
        },
    )
}

fn write_meta<T: Serialize>(dir: &Path, name: &str, id: &str, kind: AssetSourceKind, settings: T) {
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(product(id).source(), AssetPath::new(name).unwrap(), kind),
        settings,
    );
    fs::write(
        dir.join(format!("{name}.meta")),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
}

fn product(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(id).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn placement_id() -> SceneEntityId {
    SceneEntityId::new("badge-placement").unwrap()
}

fn implementation_digest() -> ArtifactContentDigest {
    static DIGEST: OnceLock<ArtifactContentDigest> = OnceLock::new();
    *DIGEST.get_or_init(|| {
        let path = std::env::current_exe().unwrap();
        let length = fs::metadata(&path).unwrap().len();
        let digest = readable_file(&path).unwrap().digest(length).unwrap();
        ArtifactContentDigest::from_content(digest)
    })
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "nara-cook-badge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize()
            && path == self.0
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}
