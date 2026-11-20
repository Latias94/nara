#![cfg(all(feature = "project-tooling", any(windows, target_os = "linux")))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    convert::Infallible,
    fs::{self, File},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    advanced_prelude::CurrentScene,
    asset::{
        AssetPathScope, AssetProductRef, AssetRef, Assets, ImportedProductId, ProjectAssetDatabase,
        StableAssetId,
    },
    asset_build::{ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest},
    diagnostic::DiagnosticValueRef,
    identity::EntityLookup,
    image::{ImageAsset, ImageDomain, ImageRuntimeCodec},
    project_host::{HeadlessSession, HeadlessSessionClose, resolve_product_recipe},
    project_tooling::{
        host_fs::{readable_file, writable_directory},
        project_cook::{ProjectCookError, ProjectCookRequest, prepare_project_cooked_content},
    },
    project_workflow::{
        ProjectCheckRequest, check_project,
        cooked_content::{
            CookedContentBuilder, CookedContentError, CookedContentLimits, CookedContentRequest,
        },
    },
    reflect::{ComponentFieldPath, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    runtime_content::{
        RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentLimits,
        RuntimeContentTarget,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityRecord, SourceDocumentKey,
        SourceDocumentKind,
    },
    sprite::Sprite,
};
use project_content_fixture::{
    TestProject, identity_transform_record, player_image_meta, scene_id, sprite_value,
};

const SPRITE: &str = "nara.sprite.Sprite";
const TEXTURED: &str = "textured";
const NULL_IMAGE: &str = "null-image";
const OMITTED_IMAGE: &str = "omitted-image";
const PIXEL: [u8; 4] = [24, 120, 220, 255];

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public Cook error is preserved for typed assertions."
)]
fn cooked_sprite_requires_native_image_publication() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let source = StableAssetId::parse_str("01d5ce17-6ddc-4187-9134-3b7f11710dbe").unwrap();
    project.write_scene_meta(source);
    let scene = AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap();
    let image = AssetProductRef::new(
        player_image_meta().identity().stable_id(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    let reference = AssetRef::path("textures/player.png").unwrap();
    project.write_scene_source(&SceneDocument::from_ordered_roots([sprite_entity(
        TEXTURED,
        sprite_value(&reference),
    )]));
    let (candidate, _, root) = project.candidate_plan_and_root();
    let cache = project.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let store = ArtifactBuildStore::open(
        writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap();
    let target = RuntimeContentTarget::new("test-headless", "sprite-publication").unwrap();
    let codec = RuntimeCodecDefinitionId::new("nara.test.sprite-publication.image", 1);
    let cpu = ProductRecipe::new()
        .add_runtime_asset_codec::<ImageDomain, ImageRuntimeCodec>(codec)
        .unwrap();
    let native = ProductRecipe::new()
        .add_runtime_image_codec::<ImageRuntimeCodec>(codec)
        .unwrap();

    check_project(ProjectCheckRequest::new(root.share(), cpu.clone())).unwrap();
    let cook = |recipe| {
        prepare_project_cooked_content(
            ProjectCookRequest {
                root: &root,
                candidate: &candidate,
                recipe,
                target: target.clone(),
                store: &store,
                implementation: ArtifactContentDigest::of_bytes(b"sprite-publication"),
                limits: CookedContentLimits::default(),
            },
            |_| Ok(()),
        )
    };

    let error = match cook(cpu.clone()) {
        Err(ProjectCookError::Content(CookedContentError::Runtime(
            RuntimeContentError::Asset(error),
        ))) => error,
        Err(error) => panic!("unexpected Cook refusal: {error:?}"),
        Ok(_) => panic!("Cook accepted a Sprite bound to a CPU-only Image"),
    };
    let diagnostic = error.diagnostic();
    assert_eq!(
        diagnostic.code().as_str(),
        "runtime.content.sprite-image-publication-unavailable"
    );
    let scene_id_text = scene.to_string();
    let image_id_text = image.to_string();
    for (key, expected) in [
        ("scene-product", scene_id_text.as_str()),
        ("entity", TEXTURED),
        ("component", SPRITE),
        ("field", "material.image"),
        ("target-product", image_id_text.as_str()),
    ] {
        let actual = diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .map(|field| field.value());
        assert_eq!(
            actual,
            Some(DiagnosticValueRef::Identifier(expected)),
            "{key}"
        );
    }

    let (content, selection) = cook(native.clone()).unwrap().into_parts();
    selection.verify(project.path()).unwrap();
    let pack = project.path().join("content.pack");
    content
        .base()
        .write_pack(&mut File::create_new(&pack).unwrap())
        .unwrap();
    let expectation = content.expectation().clone();
    drop(content);
    let base = RuntimeContentBase::open_pack(
        readable_file(&pack).unwrap(),
        &expectation,
        native,
        RuntimeContentLimits::default(),
    )
    .unwrap();
    assert_eq!(
        base.member(scene).unwrap().dependencies(),
        [RuntimeContentDependency::new(
            image,
            RuntimeContentDependencyRole::Required,
        )]
    );
    assert_eq!(base.validate_deep().unwrap().members(), 2);
    let decoded = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_headless_sprites(base, &[(TEXTURED, Some(image))]);

    let plan = resolve_product_recipe(&candidate, cpu.clone()).unwrap();
    let bindings = plan.schema_validation().bindings();
    let null = bindings
        .schema(&ComponentTypeId::new(SPRITE))
        .unwrap()
        .authoring_initial_value()
        .unwrap()
        .clone();
    let mut omitted = null.clone();
    let removed = omitted
        .remove_path(&ComponentFieldPath::from_fields(["material", "image"]))
        .unwrap();
    assert_eq!(removed, ComponentValue::Null);
    let untextured = SceneDocument::from_ordered_roots([
        sprite_entity(NULL_IMAGE, null),
        sprite_entity(OMITTED_IMAGE, omitted),
    ]);
    let database = ProjectAssetDatabase::default();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    let compilation = bindings
        .try_compile_scene_with_loader(&untextured, Default::default(), Some(resolver), |_, _| {
            Ok::<_, Infallible>(None)
        })
        .unwrap();
    assert!(
        !compilation.diagnostics.has_errors(),
        "{:?}",
        compilation.diagnostics
    );
    let untextured = compilation
        .candidate
        .unwrap()
        .bind_source(SourceDocumentKey::stable_asset(
            source,
            SourceDocumentKind::Scene,
        ));
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            configuration: candidate.runtime_configuration().to_configuration(),
            target,
            startup_scene: scene,
            scenes: vec![scene],
            addressable_roots: vec![image],
        },
        cpu,
        CookedContentLimits::default(),
    )
    .unwrap();
    builder.add_scene(scene, &untextured, resolver).unwrap();
    builder.add_image(decoded.product()).unwrap();
    let control = builder.finish().unwrap();
    assert!(
        control
            .base()
            .member(scene)
            .unwrap()
            .dependencies()
            .is_empty()
    );
    let decoded = control
        .base()
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_eq!(decoded.product().product(), Some(image));
    assert_eq!(decoded.product().pixels(), PIXEL);
    assert_headless_sprites(
        control.base().clone(),
        &[(NULL_IMAGE, None), (OMITTED_IMAGE, None)],
    );
}

fn sprite_entity(id: &str, value: ComponentValue) -> SceneEntityRecord {
    SceneEntityRecord::new(scene_id(id))
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new(SPRITE),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
        )
}

fn assert_headless_sprites(base: RuntimeContentBase, expected: &[(&str, Option<AssetProductRef>)]) {
    let mut player = HeadlessSession::from_runtime_content(base);
    player.start().unwrap();
    assert!(player.frame(Duration::ZERO).unwrap().is_none());
    player
        .observe(|world| {
            let current = CurrentScene::get(world).unwrap();
            let images = world.resource::<Assets<ImageAsset>>();
            let textured = expected.iter().filter(|(_, image)| image.is_some()).count();
            assert_eq!(images.len(), textured);
            for (id, product) in expected {
                let EntityLookup::Resolved(entity) =
                    current.receipt().resolve(world, &scene_id(id))
                else {
                    panic!("Sprite entity {id} was not materialized");
                };
                let sprite = world.get::<Sprite>(entity).unwrap();
                let image = sprite
                    .material
                    .image
                    .map(|handle| images.get(handle).unwrap());
                assert_eq!(image.and_then(ImageAsset::product), *product);
                if let Some(image) = image {
                    assert_eq!(image.pixels(), PIXEL);
                    assert!(image.source().is_none());
                }
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
