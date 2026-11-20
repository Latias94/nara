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
    ecs::{Entity, World},
    identity::EntityLookup,
    image::{ImageAsset, ImageDomain, ImageRuntimeCodec},
    project_host::{HeadlessSession, HeadlessSessionClose, resolve_product_recipe},
    project_tooling::{
        host_fs::{readable_file, writable_directory},
        project_cook::{
            PreparedProjectCook, ProjectCookError, ProjectCookRequest,
            prepare_project_cooked_content,
        },
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

    assert_image_publication_refusal(
        cook(cpu.clone()),
        "runtime.content.sprite-image-publication-unavailable",
        scene,
        TEXTURED,
        (SPRITE, "material.image"),
        image,
    );

    let base = reopen_cooked_pack(&project, cook(native.clone()).unwrap(), native);
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

#[cfg(feature = "runtime-ui")]
#[test]
#[allow(
    clippy::result_large_err,
    reason = "The public Cook error is preserved for typed assertions."
)]
fn cooked_ui_visual_requires_native_image_publication() {
    use nara::{
        asset::{AssetServer, AssetSourcePath},
        reflect::ComponentEncodeContext,
        ui::{UiColor, UiLayer, UiNode, UiRoot, UiStyle, UiVisual},
    };

    const WIDGET: &str = "menu-icon";
    const VISUAL: &str = "nara.ui.UiVisual";

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let manifest = project.path().join("nara.toml");
    let settings = fs::read_to_string(&manifest).unwrap();
    fs::write(
        manifest,
        settings.replace(r#"["runtime-2d"]"#, r#"["runtime-2d", "runtime-ui"]"#),
    )
    .unwrap();
    let source = StableAssetId::parse_str("01d5ce17-6ddc-4187-9134-3b7f11710dbf").unwrap();
    project.write_scene_meta(source);
    let scene = AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap();
    let image = AssetProductRef::new(
        player_image_meta().identity().stable_id(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();

    let (candidate, plan, root) = project.candidate_plan_and_root();
    let registry = plan.schema_validation().snapshot();
    let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
    let encode = ComponentEncodeContext::new().with_asset_path_scope(&scope);
    let mut world = World::new();
    let mut server = AssetServer::new();
    let handle = server
        .reserve::<ImageAsset>("assets/textures/player.png")
        .unwrap();
    world.insert_resource(server);
    let entity = world
        .spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill())))
        .id();
    let mut authored_scene = |visual: UiVisual| {
        world.entity_mut(entity).insert(visual);
        let mut record = SceneEntityRecord::new(scene_id(WIDGET));
        for component in ["nara.ui.UiRoot", "nara.ui.UiNode", VISUAL] {
            let component = ComponentTypeId::new(component);
            let value = registry
                .encode_component_with_context(&component, &world, entity, &encode)
                .unwrap()
                .unwrap();
            record = record.with_component(
                component,
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
            );
        }
        SceneDocument::from_ordered_roots([record])
    };
    let white = UiColor::rgba(1.0, 1.0, 1.0, 1.0);
    project.write_scene_source(&authored_scene(UiVisual::image_asset(handle, white)));

    let cache = project.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let store = ArtifactBuildStore::open(
        writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap();
    let target = RuntimeContentTarget::new("test-headless", "ui-visual-publication").unwrap();
    let codec = RuntimeCodecDefinitionId::new("nara.test.ui-visual-publication.image", 1);
    let cpu = ProductRecipe::new()
        .add_runtime_asset_codec::<ImageDomain, ImageRuntimeCodec>(codec)
        .unwrap();
    let native = ProductRecipe::new()
        .add_runtime_image_codec::<ImageRuntimeCodec>(codec)
        .unwrap();
    let cook = |recipe| {
        prepare_project_cooked_content(
            ProjectCookRequest {
                root: &root,
                candidate: &candidate,
                recipe,
                target: target.clone(),
                store: &store,
                implementation: ArtifactContentDigest::of_bytes(b"ui-visual-publication"),
                limits: CookedContentLimits::default(),
            },
            |_| Ok(()),
        )
    };
    check_project(ProjectCheckRequest::new(root.share(), cpu.clone())).unwrap();
    assert_image_publication_refusal(
        cook(cpu.clone()),
        "runtime.content.ui-visual-image-publication-unavailable",
        scene,
        WIDGET,
        (VISUAL, "background.image"),
        image,
    );

    let base = reopen_cooked_pack(&project, cook(native.clone()).unwrap(), native);
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
    assert_headless_world(base, |world| {
        let visual = world
            .get::<UiVisual>(headless_entity(world, WIDGET))
            .unwrap();
        let handle = visual.image_handle().unwrap();
        let images = world.resource::<Assets<ImageAsset>>();
        let published = images.get(handle).unwrap();
        assert_eq!(published.product(), Some(image));
        assert_eq!(published.pixels(), PIXEL);
        assert!(published.source().is_none());
    });

    let solid = UiVisual::solid(UiColor::rgba(0.25, 0.5, 0.75, 1.0));
    let database = ProjectAssetDatabase::default();
    let resolver = database.resolver(&AssetPathScope::ROOT);
    let compilation = plan
        .schema_validation()
        .bindings()
        .try_compile_scene_with_loader(
            &authored_scene(solid.clone()),
            Default::default(),
            Some(resolver),
            |_, _| Ok::<_, Infallible>(None),
        )
        .unwrap();
    assert!(
        !compilation.diagnostics.has_errors(),
        "{:?}",
        compilation.diagnostics
    );
    let solid_scene = compilation
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
    builder.add_scene(scene, &solid_scene, resolver).unwrap();
    builder.add_image(decoded.product()).unwrap();
    let control = builder.finish().unwrap();
    let base = control.base();
    assert!(base.member(scene).unwrap().dependencies().is_empty());
    let decoded = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image)
        .unwrap();
    assert_eq!(decoded.product().product(), Some(image));
    assert_eq!(decoded.product().pixels(), PIXEL);
    assert_headless_world(base.clone(), |world| {
        assert_eq!(
            world.get::<UiVisual>(headless_entity(world, WIDGET)),
            Some(&solid)
        );
        assert!(world.resource::<Assets<ImageAsset>>().is_empty());
    });
}

fn assert_image_publication_refusal(
    result: Result<PreparedProjectCook, ProjectCookError>,
    code: &str,
    scene: AssetProductRef,
    entity: &str,
    (component, field): (&str, &str),
    image: AssetProductRef,
) {
    let error = match result {
        Err(ProjectCookError::Content(CookedContentError::Runtime(
            RuntimeContentError::Asset(error),
        ))) => error,
        Err(error) => panic!("unexpected Cook refusal: {error:?}"),
        Ok(_) => panic!("Cook accepted {component} bound to a CPU-only Image"),
    };
    let diagnostic = error.diagnostic();
    assert_eq!(diagnostic.code().as_str(), code);
    let scene = scene.to_string();
    let image = image.to_string();
    for (key, expected) in [
        ("scene-product", scene.as_str()),
        ("entity", entity),
        ("component", component),
        ("field", field),
        ("target-product", image.as_str()),
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
}

fn reopen_cooked_pack(
    project: &TestProject,
    cooked: PreparedProjectCook,
    recipe: ProductRecipe,
) -> RuntimeContentBase {
    let (content, selection) = cooked.into_parts();
    selection.verify(project.path()).unwrap();
    let pack = project.path().join("content.pack");
    content
        .base()
        .write_pack(&mut File::create_new(&pack).unwrap())
        .unwrap();
    let expectation = content.expectation().clone();
    drop(content);
    RuntimeContentBase::open_pack(
        readable_file(&pack).unwrap(),
        &expectation,
        recipe,
        RuntimeContentLimits::default(),
    )
    .unwrap()
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
    assert_headless_world(base, |world| {
        let images = world.resource::<Assets<ImageAsset>>();
        let textured = expected.iter().filter(|(_, image)| image.is_some()).count();
        assert_eq!(images.len(), textured);
        for (id, product) in expected {
            let sprite = world.get::<Sprite>(headless_entity(world, id)).unwrap();
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
    });
}

fn headless_entity(world: &World, id: &str) -> Entity {
    let current = CurrentScene::get(world).unwrap();
    let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &scene_id(id)) else {
        panic!("Scene entity {id} was not materialized");
    };
    entity
}

fn assert_headless_world(base: RuntimeContentBase, observe: impl FnOnce(&World)) {
    let mut player = HeadlessSession::from_runtime_content(base);
    player.start().unwrap();
    assert!(player.frame(Duration::ZERO).unwrap().is_none());
    player.observe(observe).unwrap();
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
