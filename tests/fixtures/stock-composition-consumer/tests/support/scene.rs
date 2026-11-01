//! Ordinary source-bound Scene materialization and public stock collection.

use std::{fs, path::Path, sync::Arc, time::Duration};

use nara::{
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, RuntimeInstance,
        StartupStage,
    },
    ecs::{Resource, World},
    hierarchy::HierarchyPlugin,
    image::ImagePreparePlugin,
    reflect::{
        ComponentRegistryPlugin, ComponentRegistrySnapshot, ComponentTypeId, ComponentValue,
        component_registry,
    },
    render::{RenderComponentsPlugin, RenderPlugin, WorldCanvas},
    scene::{
        ExpandedSceneCandidate, SceneComponentRecord, SceneComponentsPlugin, SceneDocument,
        SceneEntityId, SceneEntityRecord, SceneOccurrenceKey, SourceDocumentKey,
        SourceDocumentKind, SpawnedSceneInstance, spawn_scene,
    },
    sprite::SpritePlugin,
    sprite_render::SpriteRenderPlugin,
    transform::TransformPlugin,
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_render_wgpu::{WgpuCapturePlugin, WgpuStockCapturePlugin, WgpuStockSpritePlugin};
use nara_stock_composition_consumer::{
    CompositionConfiguration, CompositionPlugin, RetainedInputs,
};

pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 24;
pub const WORLD_WIDTH: u32 = 8;
pub const WORLD_HEIGHT: u32 = 6;
/// Straight linear tint. The stock Sprite draw premultiplies it over transparent black.
pub const TRANSLUCENT_TINT: [f32; 4] = [0.8, 0.4, 0.2, 0.5];
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Resource)]
pub struct SceneReceipt(pub SpawnedSceneInstance);

#[derive(Resource)]
struct TransparentSource(bool);

/// Builds a normal ECS Runtime with CPU window facts, without a platform EventLoop or surface.
/// No SpriteBatch, extracted View, stock lowering or render packet is constructed here.
pub fn capture_runtime(canvas: bool) -> Result<RuntimeInstance> {
    capture_runtime_with(canvas, false, CompositionConfiguration::default())
}

fn capture_runtime_with(
    canvas: bool,
    transparent: bool,
    configuration: CompositionConfiguration,
) -> Result<RuntimeInstance> {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        SceneComponentsPlugin,
        SpritePlugin,
    ))?;
    app.add_plugins((
        WindowPlugin {
            primary_window: Some(Window::new(
                "Stock composition CPU source",
                WindowResolution::new(WIDTH, HEIGHT),
            )),
        },
        RenderComponentsPlugin,
        RenderPlugin {
            world_canvas: canvas.then(|| WorldCanvas::new(WORLD_WIDTH, WORLD_HEIGHT).unwrap()),
        },
        ImagePreparePlugin,
        SpriteRenderPlugin,
    ))?;
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockSpritePlugin,
        CompositionPlugin(configuration),
    ))?;
    app.insert_resource(TransparentSource(transparent))?;
    app.add_systems(StartupStage::Scene, spawn_document)?;
    Ok(RuntimeInstance::start(app)?)
}

fn spawn_document(world: &mut World) {
    let transparent = world.resource::<TransparentSource>().0;
    let registry = component_registry(world)
        .expect("the actual admitted Runtime registry")
        .clone();
    let candidate = ExpandedSceneCandidate::plain(
        SourceDocumentKey::named(
            "5e42bc45-cbbc-4e44-a4c4-cc12dc635047".parse().unwrap(),
            if transparent {
                "stock-composition/translucent-sprite"
            } else {
                "stock-composition/ordinary-sprites"
            },
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        Arc::new(document_with_alpha(&registry, transparent)),
    );
    let report = spawn_scene(
        world,
        &registry,
        &candidate,
        &SceneOccurrenceKey::new("consumer.stock-composition", "source").unwrap(),
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    world.insert_resource(SceneReceipt(
        report.instance.expect("ordinary Scene receipt"),
    ));
}

pub fn capture(canvas: bool) -> nara_render_wgpu::WgpuRenderInput {
    finish_capture(
        capture_runtime(canvas).expect("start the ordinary source-bound Scene Runtime"),
        canvas,
    )
}

/// Captures a real translucent Scene through normal extraction, with effects disabled so its
/// exact original input can publish the stock source before the separate native format matrix.
pub fn capture_translucent() -> nara_render_wgpu::WgpuRenderInput {
    let configuration = CompositionConfiguration {
        enabled: false,
        ..Default::default()
    };
    finish_capture(
        capture_runtime_with(false, true, configuration)
            .expect("start the ordinary translucent Scene Runtime"),
        false,
    )
}

fn finish_capture(mut runtime: RuntimeInstance, canvas: bool) -> nara_render_wgpu::WgpuRenderInput {
    runtime
        .drive(Duration::ZERO)
        .expect("extract, prepare, queue and collect ordinary Sprites");
    let input = runtime
        .world()
        .resource::<RetainedInputs>()
        .0
        .first()
        .expect("public collector seals the actual first Scene input")
        .clone();
    assert_eq!(
        runtime
            .world()
            .resource::<SceneReceipt>()
            .0
            .entity_ids()
            .len(),
        2
    );
    assert_eq!(input.packet().world_canvas().is_some(), canvas);
    assert_eq!(input.packet().views().len(), 1);
    assert_eq!(
        input.packet().source_generation().runtime_generation(),
        Some(runtime.generation())
    );
    assert!(
        input
            .captured::<nara_stock_composition_consumer::CompositionPolicy>()
            .is_some()
    );
    runtime
        .finish()
        .unwrap_or_else(|retirement| panic!("ordinary source retirement: {retirement:?}"));
    input
}

/// A red 4x2 Sprite centered in an 8x6 blue Camera view. All edges align to raster cells.
pub fn document(registry: &ComponentRegistrySnapshot) -> SceneDocument {
    document_with_alpha(registry, false)
}

fn document_with_alpha(registry: &ComponentRegistrySnapshot, transparent: bool) -> SceneDocument {
    let mut sprite = initial(registry, "nara.sprite.Sprite");
    let ComponentValue::Map(fields) = &mut sprite.value else {
        panic!("Sprite map")
    };
    fields.insert("size".into(), vec2(4.0, 2.0));
    let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
        panic!("Sprite material")
    };
    let tint = if transparent {
        TRANSLUCENT_TINT.map(f64::from)
    } else {
        [1.0, 0.0, 0.0, 1.0]
    };
    material.insert("tint".into(), color(tint));
    let camera_schema = registry
        .schema(&ComponentTypeId::new("nara.render.Camera2d"))
        .expect("ordinary Camera schema");
    let camera = SceneComponentRecord::new(
        camera_schema.version(),
        ComponentValue::map([
            ("viewport_height", ComponentValue::f64(6.0).unwrap()),
            (
                "clear_color",
                color(if transparent {
                    [0.0; 4]
                } else {
                    [0.0, 0.0, 1.0, 1.0]
                }),
            ),
        ]),
    );
    let transform = initial(registry, "nara.transform.Transform2d");
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("sprite").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform.clone(),
            )
            .with_component(ComponentTypeId::new("nara.sprite.Sprite"), sprite),
        SceneEntityRecord::new(SceneEntityId::new("camera").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                transform,
            )
            .with_component(ComponentTypeId::new("nara.render.Camera2d"), camera),
    ])
}

fn initial(registry: &ComponentRegistrySnapshot, name: &str) -> SceneComponentRecord {
    let schema = registry
        .schema(&ComponentTypeId::new(name))
        .expect("ordinary component schema");
    SceneComponentRecord::new(
        schema.version(),
        schema.authoring_initial_value().unwrap().clone(),
    )
}

fn vec2(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

fn color([r, g, b, a]: [f64; 4]) -> ComponentValue {
    ComponentValue::map([
        ("r", ComponentValue::f64(r).unwrap()),
        ("g", ComponentValue::f64(g).unwrap()),
        ("b", ComponentValue::f64(b).unwrap()),
        ("a", ComponentValue::f64(a).unwrap()),
    ])
}

/// Adds an ordinary full-resolution UI entity to the product's primary UI surface.
#[derive(Default)]
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.stock-composition.hud"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::ui::UI_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> std::result::Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, |world: &mut World| {
            use nara::ui::{UiColor, UiNode, UiRoot, UiRuntimeKey, UiStyle, UiVisual};
            let root = world
                .spawn((
                    UiRoot::default(),
                    UiNode::new(UiStyle::fill()),
                    UiRuntimeKey::new("consumer.stock-composition", "hud-root").unwrap(),
                ))
                .id();
            let stripe = world
                .spawn((
                    UiNode::new(UiStyle::absolute(1.0, 1.0, 1.0, (HEIGHT - 2) as f32)),
                    UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)),
                    UiRuntimeKey::new("consumer.stock-composition", "hud").unwrap(),
                ))
                .id();
            use nara::hierarchy::{
                HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
            };
            HierarchyWriter::new(world)
                .apply_batch(&[
                    HierarchyOperation::Place {
                        entity: root,
                        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                    },
                    HierarchyOperation::Place {
                        entity: stripe,
                        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                            parent: root,
                        }),
                    },
                ])
                .expect("the ordinary authored HUD has a registered root and child");
        })?;
        Ok(())
    }
}

pub struct SourceFixture(pub tempfile::TempDir);

impl SourceFixture {
    pub fn new(recipe: nara::ProductRecipe) -> Result<Self> {
        use nara::{project::ProjectManifest, project_host::resolve_runtime_configuration};
        let root = tempfile::tempdir()?;
        for path in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.path().join(path))?;
        }
        fs::write(root.path().join("nara.toml"), MANIFEST)?;
        let manifest = ProjectManifest::parse_toml_bytes(MANIFEST.as_bytes());
        if manifest.has_errors() {
            return Err(std::io::Error::other("source fixture manifest was rejected").into());
        }
        let configuration = manifest
            .manifest
            .expect("the fixture manifest was validated")
            .resolve_profile(None)?
            .runtime_configuration()?;
        let plan = resolve_runtime_configuration(&configuration, recipe)?;
        fs::write(
            root.path().join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()?,
        )?;
        // The source registry is the same production composition selected by this recipe.
        let mut runtime = capture_runtime(false)?;
        let registry =
            component_registry(runtime.world()).expect("the actual admitted source registry");
        fs::write(
            root.path().join("scenes/startup.scene.json"),
            document(registry).to_json_string()?,
        )?;
        runtime.drive(Duration::ZERO)?;
        runtime
            .finish()
            .unwrap_or_else(|retirement| panic!("source writer retirement: {retirement:?}"));
        Ok(Self(root))
    }

    pub fn root(&self) -> &Path {
        self.0.path()
    }
}

const MANIFEST: &str = r#"schema_version = 1
[project]
source_universe = "5e42bc45-cbbc-4e44-a4c4-cc12dc635047"
native_document_identity = "project_location"
name = "Independent stock composition"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[tasks.io]
workers = 1
pending_capacity = 16
[tasks.compute]
workers = 1
pending_capacity = 16
[tasks.async_compute]
workers = 1
pending_capacity = 16
[window]
enabled = true
# The odd width puts the centered Sprite's edges on pixel centers at unit DPI.
width = 33
height = 24
[capabilities]
requested = ["runtime-2d", "presentation-2d", "runtime-ui", "presentation-ui", "desktop-winit", "render-wgpu"]
[ui.primary_surface]
logical_width = 33
logical_height = 24
"#;
