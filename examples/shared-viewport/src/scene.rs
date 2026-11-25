use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, ensure};
use nara_app::{App, CoreStage, RuntimeInstance, VirtualTime};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Component, Query, Res, Resource, Single, World};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyPlugin,
    HierarchyWriter,
};
use nara_reflect::ComponentRegistryPlugin;
use nara_render::gpu::{
    TextureCacheStats, WgpuCapturePlugin, WgpuFrameCaptureError, WgpuImageOutput, WgpuRenderHost,
    WgpuRenderInput, WgpuRenderProducer, WgpuStockCapturePlugin, WgpuStockPolicyPlugin,
    WgpuStockSpritePlugin, take_captured_policy_batch,
};
use nara_render::image::ImagePreparePlugin;
use nara_render::sprite::Sprite;
use nara_render::sprite_render::{SpriteRenderPlugin, SpriteRenderStats};
use nara_render::tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
use nara_render::{
    Camera2d, Extent2d, RenderComponentsPlugin, RenderImage2d, RenderImagePacket, RenderPlugin,
};
use nara_transform::{Transform2d, TransformPlugin};
use nara_window::{Window, WindowResolution};
use parking_lot::Mutex;

#[derive(Clone, Default, Resource)]
struct CaptureBridge(Arc<Mutex<FrameExchange>>);

#[derive(Default)]
struct FrameExchange {
    resolution: WindowResolution,
    captured: Option<Result<Option<WgpuRenderInput>, WgpuFrameCaptureError>>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SceneFacts {
    pub(crate) frames: u64,
    pub(crate) extraction: SpriteRenderStats,
    pub(crate) textures: TextureCacheStats,
    pub(crate) created_buffers: u64,
    pub(crate) texture_copies: u64,
}

pub(crate) struct Scene {
    runtime: RuntimeInstance,
    bridge: CaptureBridge,
    producer: WgpuRenderProducer,
    host: WgpuRenderHost,
    image: Handle<RenderImage2d>,
    last_frame: Option<Instant>,
    facts: SceneFacts,
}

impl Scene {
    pub(crate) fn new(mut host: WgpuRenderHost) -> Result<Self> {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            HierarchyPlugin,
            TransformPlugin,
            RenderComponentsPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
            SpriteRenderPlugin,
        ))?;
        app.add_plugins((
            WgpuCapturePlugin,
            WgpuStockCapturePlugin,
            WgpuStockPolicyPlugin,
            WgpuStockSpritePlugin,
        ))?;
        let bridge = CaptureBridge::default();
        app.insert_resource(bridge.clone())?;
        app.add_systems(CoreStage::First, set_capture_resolution)?;
        app.add_systems(CoreStage::Update, animate_sprites)?;
        app.add_systems(CoreStage::Last, take_stock_capture)?;
        populate_scene(app.world_mut()?)?;

        Ok(Self {
            producer: host.create_producer()?,
            image: RenderImage2d::allocate_handle()?,
            runtime: RuntimeInstance::start(app)
                .map_err(|error| anyhow::anyhow!("start the Nara scene: {error}"))?,
            bridge,
            host,
            last_frame: None,
            facts: SceneFacts::default(),
        })
    }

    pub(crate) fn render(&mut self, extent: Extent2d, scale: f32) -> Result<WgpuImageOutput> {
        let now = Instant::now();
        let delta = self.last_frame.replace(now).map_or(Duration::ZERO, |last| {
            now.duration_since(last).min(Duration::from_millis(50))
        });
        self.bridge.0.lock().resolution =
            WindowResolution::new(extent.width, extent.height).with_scale_factor(f64::from(scale));
        self.runtime
            .drive(delta)
            .map_err(|error| anyhow::anyhow!("drive Nara extraction: {:?}", error.fault()))?;
        let input = self
            .bridge
            .0
            .lock()
            .captured
            .take()
            .context("Nara did not seal a stock capture")??
            .context("Nara captured no viewport")?;
        let target = RenderImagePacket::new(self.image, extent)?;
        let input = input.with_image_target(target)?;
        let packet = input.packet();
        let extraction = *self.runtime.world().resource::<SpriteRenderStats>();
        ensure!(
            extraction.extracted_sprites > 0
                && extraction.extracted_tile_cells > 0
                && extraction.queued_items > 0,
            "Nara must extract sprites and tile cells: {extraction:?}"
        );

        self.host.begin_frame(packet.frame_index())?;
        let source = packet.source_generation();
        if !self
            .host
            .image_target_matches(&self.producer, source, target)
        {
            let reservation = self
                .host
                .reserve_image_target(&self.producer, source, target)?;
            self.host
                .activate_image_target(&self.producer, reservation)?;
        }
        let output = self
            .host
            .submit(&self.producer, &input)
            .context("submit the Nara stock passes")?;
        let work = self.host.frame_transaction_stats().native_work();
        self.facts.frames += 1;
        self.facts.extraction = extraction;
        self.facts.textures = self.host.transient_texture_stats();
        self.facts.created_buffers += work.created_buffer_calls;
        self.facts.texture_copies += work.encoded_texture_copy_commands;
        Ok(output)
    }

    pub(crate) fn facts(&self) -> SceneFacts {
        self.facts
    }
}

fn set_capture_resolution(bridge: Res<CaptureBridge>, mut window: Single<&mut Window>) {
    let resolution = bridge.0.lock().resolution;
    if window.resolution != resolution {
        window.resolution = resolution;
    }
}

fn take_stock_capture(world: &mut World) {
    let captured = take_captured_policy_batch(world).map(|batch| batch.into_render_input());
    world.resource::<CaptureBridge>().0.lock().captured = captured;
}

#[derive(Component)]
struct Orbit {
    radius: Vec2,
    phase: f32,
}

fn animate_sprites(time: Res<VirtualTime>, mut sprites: Query<(&Orbit, &mut Transform2d)>) {
    for (orbit, mut transform) in &mut sprites {
        let angle = time.elapsed_seconds() as f32 * 1.5 + orbit.phase;
        transform.translation = Vec2::new(angle.cos(), angle.sin()) * orbit.radius;
        transform.rotation = angle * 0.5;
    }
}

fn populate_scene(world: &mut World) -> Result<()> {
    world.spawn(Window::new("Viewport capture", WindowResolution::new(1, 1)));
    let camera = world
        .spawn((
            Camera2d {
                viewport_height: 336.0,
                clear_color: Some(Color::rgba(0.022, 0.035, 0.075, 0.72)),
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
        ))
        .id();

    let mut tilemap = Tilemap::new(Vec2::splat(16.0));
    for row in -8_i32..8 {
        for column in -15_i32..15 {
            let shade = (row + column).rem_euclid(4) as f32;
            let is_edge = row == -8 || row == 7 || column == -15 || column == 14;
            let color = if is_edge {
                Color::rgb(0.04, 0.25, 0.31)
            } else {
                Color::rgb(0.02, 0.075 + shade * 0.012, 0.13 + shade * 0.015)
            };
            tilemap.set_cell(
                TileCoord::new(column, row),
                TileCell::new(TileIndex::new(0)).with_color(color),
            );
        }
    }
    let ground = world.spawn((tilemap, Transform2d::IDENTITY)).id();
    let mut roots = vec![camera, ground];
    for (phase, radius, size, color) in [
        (
            0.0,
            Vec2::new(168.0, 145.0),
            48.0,
            Color::rgba(0.08, 0.85, 1.0, 0.65),
        ),
        (
            2.2,
            Vec2::new(136.0, 94.0),
            34.0,
            Color::rgb(1.0, 0.48, 0.12),
        ),
        (
            4.4,
            Vec2::new(112.0, 132.0),
            40.0,
            Color::rgba(0.92, 0.22, 0.62, 0.70),
        ),
    ] {
        roots.push(
            world
                .spawn((
                    Sprite::from_color(Vec2::splat(size), color).with_layer(1),
                    Transform2d::IDENTITY,
                    Orbit { radius, phase },
                ))
                .id(),
        );
    }
    let placement = HierarchyPlacement::last(HierarchyPlacementOwner::Root);
    let operations: Vec<_> = roots
        .into_iter()
        .map(|entity| HierarchyOperation::Place { entity, placement })
        .collect();
    HierarchyWriter::new(world).apply_batch(&operations)?;
    Ok(())
}
