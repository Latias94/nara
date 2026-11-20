use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara_app::{App, CoreStage, RealTime, RuntimeInstance};
use nara_core::{Color, Vec2};
use nara_ecs::{
    Component, Entity, Query, ResMut, Resource, World, bevy_ecs::entity_disabling::Disabled,
    schedule::IntoScheduleConfigs,
};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_material::{
    AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dLibrary, MaterialParameters,
};
use nara_render::{Camera2d, ViewportRect, WorldCanvas};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuRenderInput, WgpuStockCapturePlugin,
    WgpuStockPolicyPlugin, WgpuStockSpritePlugin, take_captured_policy_batch,
};
use nara_tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
use nara_transform::Transform2d;
use nara_window::{Window, WindowResolution};

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub const CANVAS_WIDTH: u32 = 320;
pub const CANVAS_HEIGHT: u32 = 180;
pub const MAP_X_OFFSET: f64 = 0.13;
pub const MAP_Y_OFFSET: f64 = -0.27;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Static,
    Cell,
    Camera,
    WarmedCell,
}

impl Scenario {
    pub fn name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Cell => "one-cell",
            Self::Camera => "moving-camera",
            Self::WarmedCell => "warmed-cell-control",
        }
    }
}

#[derive(Resource)]
pub struct State {
    pub side: i32,
    pub views: usize,
    pub scenario: Scenario,
    pub sequence: u32,
    pub map: Entity,
    pub camera: Entity,
}

#[derive(MaterialParameters)]
struct Parameters {
    #[material(id = "red")]
    x: f32,
    #[material(id = "green")]
    y: f32,
    #[material(id = "blue")]
    z: f32,
    #[material(id = "alpha")]
    w: f32,
}

fn material_binding() -> Result<Material2dBinding> {
    let mut library = Material2dLibrary::default();
    let id = "consumer.geometry.tint";
    library.declare(Material2dDeclaration::from_parameters::<Parameters>(
        id,
        1,
        AlphaMode2d::Blend,
        vec![],
    )?)?;
    let path = nara_asset::AssetPath::new("geometry.wgsl")?;
    let read = |_: &nara_asset::AssetPath| {
        Ok(std::sync::Arc::<str>::from(
            "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint * vec4<f32>(material_parameters.x, material_parameters.y, material_parameters.z, material_parameters.w); }",
        ))
    };
    let prepared = library.prepare_update(id, &[path], read)?;
    library.publish(&prepared, read)?;
    Ok(library.current(id).unwrap().bind(
        &Parameters {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            w: 1.0,
        },
        Default::default(),
    )?)
}

impl State {
    pub fn camera_x(&self) -> f64 {
        if self.scenario == Scenario::Camera {
            f64::from(self.sequence % 8) * 0.125
        } else {
            0.0
        }
    }

    pub fn center_red(&self) -> bool {
        match self.scenario {
            Scenario::Cell => self.sequence.is_multiple_of(2),
            Scenario::WarmedCell => self.sequence < 4,
            Scenario::Static | Scenario::Camera => true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Snapshot {
    pub side: i32,
    pub views: usize,
    pub camera_x: f64,
    pub center_red: bool,
}

pub fn snapshot(runtime: &RuntimeInstance) -> Snapshot {
    let state = runtime.world().resource::<State>();
    Snapshot {
        side: state.side,
        views: state.views,
        camera_x: state.camera_x(),
        center_red: state.center_red(),
    }
}

pub fn expected(snapshot: Snapshot) -> Vec<[u8; 4]> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    let view_width = f64::from(CANVAS_WIDTH) / snapshot.views as f64;
    let map_min = [
        -f64::from(snapshot.side) * 0.5 + MAP_X_OFFSET,
        -f64::from(snapshot.side) * 0.5 + MAP_Y_OFFSET,
    ];
    for y in 0..HEIGHT {
        let canvas_y = f64::from(y / 3) + 0.5;
        let world_y = (f64::from(CANVAS_HEIGHT) * 0.5 - canvas_y) * 32.0 / f64::from(CANVAS_HEIGHT);
        for x in 0..WIDTH {
            let canvas_x = f64::from(x / 3) + 0.5;
            let view = (canvas_x / view_width).floor() as usize;
            let camera_x = if view == 0 { snapshot.camera_x } else { 3.0 };
            let world_x = (canvas_x - (view as f64 + 0.5) * view_width) * 32.0
                / f64::from(CANVAS_HEIGHT)
                + camera_x;
            let cell = [
                (world_x - map_min[0]).floor() as i32,
                (world_y - map_min[1]).floor() as i32,
            ];
            let mut color = if cell
                .iter()
                .all(|&coordinate| coordinate >= 0 && coordinate < snapshot.side)
            {
                if snapshot.center_red && cell == [snapshot.side / 2; 2] {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            } else {
                [0, 0, 0, 255]
            };
            if (-2.455..-1.745).contains(&world_x) && (1.135..2.065).contains(&world_y) {
                color = [0, 0, 255, 255];
            }
            pixels.push(color);
        }
    }
    pixels
}

#[derive(Default, Resource)]
pub struct Captured(pub Option<WgpuRenderInput>);

#[derive(Resource)]
struct Lifetime(Arc<AtomicBool>);

impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn install_capture(app: &mut App) -> Result<()> {
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_window::WindowPlugin {
            primary_window: Some(Window::new(
                "Geometry consumer",
                WindowResolution::new(WIDTH, HEIGHT),
            )),
        },
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin {
            world_canvas: Some(WorldCanvas::new(CANVAS_WIDTH, CANVAS_HEIGHT).unwrap()),
        },
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))?;
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockSpritePlugin,
        WgpuStockPolicyPlugin,
    ))?;
    app.world_mut()?
        .insert_resource(nara_render::ClearColor(Color::BLACK));
    app.init_resource::<Captured>()?;
    app.add_systems(
        CoreStage::Render,
        capture.in_set(WgpuPolicyCaptureSet::Publish),
    )?;
    Ok(())
}

pub fn build(
    population: usize,
    views: usize,
    scenario: Scenario,
    bound: bool,
    dropped: Arc<AtomicBool>,
) -> Result<RuntimeInstance> {
    let side = match population {
        64 => 8,
        4096 => 64,
        65536 => 256,
        _ => return Err("unsupported finite population".into()),
    };
    assert!(matches!(views, 1 | 2));
    let mut app = App::new();
    install_capture(&mut app)?;
    let world = app.world_mut()?;
    let mut placements = Vec::new();
    let mut camera = None;
    for view in 0..views {
        let width = CANVAS_WIDTH / views as u32;
        let entity = world
            .spawn((
                Transform2d::from_translation(Vec2::new(if view == 0 { 0.0 } else { 3.0 }, 0.0)),
                Camera2d {
                    viewport: ViewportRect::new(view as u32 * width, 0, width, CANVAS_HEIGHT),
                    viewport_height: 32.0,
                    clear_color: None,
                    ..Camera2d::default()
                },
            ))
            .id();
        camera.get_or_insert(entity);
        placements.push(HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        });
    }
    let mut tiles = Tilemap::new(Vec2::ONE);
    for y in 0..side {
        for x in 0..side {
            tiles.set_cell(
                TileCoord::new(x, y),
                TileCell::new(TileIndex::new(0)).with_color(Color::rgb(0.0, 1.0, 0.0)),
            );
        }
    }
    tiles.set_cell(
        TileCoord::new(side / 2, side / 2),
        TileCell::new(TileIndex::new(0)).with_color(Color::rgb(1.0, 0.0, 0.0)),
    );
    tiles.clear_dirty_chunks();
    let map = world
        .spawn((
            tiles,
            Transform2d::from_translation(Vec2::new(
                -(side as f32) * 0.5 + MAP_X_OFFSET as f32,
                -(side as f32) * 0.5 + MAP_Y_OFFSET as f32,
            )),
        ))
        .id();
    if bound {
        world.entity_mut(map).insert(material_binding()?);
    }
    placements.push(HierarchyOperation::Place {
        entity: map,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
    let sprite = world
        .spawn((
            Transform2d::from_translation(Vec2::new(-2.1, 1.6)),
            nara_sprite::Sprite::from_color(Vec2::new(0.71, 0.93), Color::rgb(0.0, 0.0, 1.0))
                .with_layer(1),
        ))
        .id();
    placements.push(HierarchyOperation::Place {
        entity: sprite,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    });
    HierarchyWriter::new(world).apply_batch(&placements)?;
    world.insert_resource(State {
        side,
        views,
        scenario,
        sequence: 0,
        map,
        camera: camera.unwrap(),
    });
    world.insert_resource(Lifetime(dropped));
    app.add_systems(CoreStage::Update, change)?;
    Ok(RuntimeInstance::start(app)?)
}

#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct GameplayDisabled;

pub const CAMERA_SPRITE_X: f32 = 103.0;

pub fn build_camera_inheritance() -> Result<(RuntimeInstance, Entity, Entity)> {
    let mut app = App::new();
    app.world_mut()?
        .register_disabling_component::<GameplayDisabled>();
    install_capture(&mut app)?;

    let world = app.world_mut()?;
    let parent_pose = Transform2d::from_translation(Vec2::new(100.0, 0.0));
    let parent = world.spawn(parent_pose).id();
    let camera = Camera2d {
        viewport_height: CANVAS_HEIGHT as f32,
        ..Default::default()
    };
    let child = world
        .spawn((camera, Transform2d::from_translation(Vec2::new(3.0, 0.0))))
        .id();
    world.spawn((camera, Transform2d::IDENTITY, Disabled));
    world.spawn((camera, Transform2d::IDENTITY, GameplayDisabled));
    let sprite = world
        .spawn((
            Transform2d::from_translation(Vec2::new(CAMERA_SPRITE_X, 0.0)),
            nara_sprite::Sprite::from_color(Vec2::splat(8.0), Color::rgb(1.0, 0.0, 0.0)),
        ))
        .id();
    HierarchyWriter::new(world).apply_batch(&[
        HierarchyOperation::Place {
            entity: parent,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
        HierarchyOperation::Place {
            entity: child,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
        },
        HierarchyOperation::Place {
            entity: sprite,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
    ])?;
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        let frame = world.resource::<RealTime>().frame;
        let mut parent = world.entity_mut(parent);
        match frame {
            2 => {
                parent.insert(Disabled);
            }
            3 => {
                parent.remove::<Disabled>();
            }
            4 => {
                parent.insert(GameplayDisabled);
            }
            5 => {
                parent.remove::<GameplayDisabled>();
            }
            6 => {
                parent.remove::<Transform2d>();
            }
            7 => {
                parent.insert(parent_pose);
            }
            _ => {}
        }
    })?;
    Ok((RuntimeInstance::start(app)?, parent, child))
}

fn change(
    mut state: ResMut<State>,
    mut maps: Query<&mut Tilemap>,
    mut transforms: Query<&mut Transform2d>,
) {
    state.sequence += 1;
    match state.scenario {
        Scenario::Static => {}
        Scenario::Cell | Scenario::WarmedCell => {
            if state.scenario == Scenario::WarmedCell && state.sequence < 4 {
                return;
            }
            let mut map = maps.get_mut(state.map).unwrap();
            map.set_cell(
                TileCoord::new(state.side / 2, state.side / 2),
                TileCell::new(TileIndex::new(0)).with_color(if state.center_red() {
                    Color::rgb(1.0, 0.0, 0.0)
                } else {
                    Color::rgb(0.0, 1.0, 0.0)
                }),
            );
            map.clear_dirty_chunks();
        }
        Scenario::Camera => {
            transforms.get_mut(state.camera).unwrap().translation.x = state.camera_x() as f32
        }
    }
}

fn capture(world: &mut World) {
    let batch = take_captured_policy_batch(world).expect("one original sealed batch");
    assert!(take_captured_policy_batch(world).is_none());
    let input = batch
        .into_render_input()
        .unwrap()
        .expect("the actual logical Window View");
    world.resource_mut::<Captured>().0 = Some(input);
}

pub fn drive(runtime: &mut RuntimeInstance) -> WgpuRenderInput {
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .world()
        .resource::<Captured>()
        .0
        .clone()
        .expect("original sealed stock input")
}
