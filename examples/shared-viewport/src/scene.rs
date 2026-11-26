use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, ensure};
use nara_app::{App, CoreStage, RealTime, RuntimeInstance, VirtualTime};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::{Component, Entity, Query, Res, World};
use nara_render::gpu::{
    RenderInput, Renderer, TextureCacheStats, WgpuImageOutput, WgpuRenderError,
};
use nara_render::image::ImagePreparePlugin;
use nara_render::sprite::Sprite;
use nara_render::sprite_render::{SpriteRenderPlugin, SpriteRenderStats};
use nara_render::tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
use nara_render::{
    Camera2d, Extent2d, ExtractedViews, RenderImage2d, RenderImagePacket, RenderPlugin,
    RenderTarget, ViewportRect, build_image_render_frame_packet,
};
use nara_scene::spatial::{SpatialPlugin, Transform2d};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SceneFacts {
    pub(crate) frames: u64,
    pub(crate) extraction: SpriteRenderStats,
    pub(crate) textures: TextureCacheStats,
    pub(crate) created_textures: u64,
    pub(crate) created_buffers: u64,
}

pub(crate) struct Scene {
    runtime: RuntimeInstance,
    camera: Entity,
    renderer: Renderer,
    image: Handle<RenderImage2d>,
    last_frame: Option<Instant>,
    facts: SceneFacts,
}

impl Scene {
    pub(crate) fn new(renderer: Renderer) -> Result<Self> {
        let mut app = App::new();
        app.add_plugins((
            SpatialPlugin,
            RenderPlugin::default(),
            ImagePreparePlugin,
            SpriteRenderPlugin,
        ))?;
        app.add_systems(CoreStage::Update, animate_sprites)?;
        let world = app.world_mut()?;
        let image = RenderImage2d::allocate_handle()?;
        let camera = world
            .spawn((
                Camera2d {
                    target: RenderTarget::Image(image),
                    viewport_height: 336.0,
                    clear_color: Some(Color::rgba(0.022, 0.035, 0.075, 0.72)),
                    ..Camera2d::default()
                },
                Transform2d::IDENTITY,
            ))
            .id();
        populate_scene(world);

        Ok(Self {
            image,
            runtime: RuntimeInstance::start(app)
                .map_err(|error| anyhow::anyhow!("start the Nara scene: {error}"))?,
            camera,
            renderer,
            last_frame: None,
            facts: SceneFacts::default(),
        })
    }

    pub(crate) fn render(&mut self, extent: Extent2d) -> Result<Option<WgpuImageOutput>> {
        let now = Instant::now();
        let delta = self.last_frame.replace(now).map_or(Duration::ZERO, |last| {
            now.duration_since(last).min(Duration::from_millis(50))
        });
        let target = RenderImagePacket::new(self.image, extent)?;
        let viewport = ViewportRect::from_extent(extent);
        let camera = self.camera;
        self.runtime
            .with_driver_scope(|scope| {
                scope.with_world_mut(|world| {
                    let mut camera = world.get_mut::<Camera2d>(camera)?;
                    if camera.viewport != viewport {
                        camera.viewport = viewport;
                    }
                    Some(())
                })
            })?
            .output
            .context("viewport camera was removed from the scene")?;
        self.runtime
            .drive(delta)
            .map_err(|error| anyhow::anyhow!("drive Nara extraction: {:?}", error.fault()))?;
        let world = self.runtime.world();
        let packet = build_image_render_frame_packet(
            self.runtime.generation().into(),
            world.resource::<RealTime>().frame,
            world.resource::<ExtractedViews>(),
            target,
        )?
        .context("Nara extracted no viewport")?;
        let input = RenderInput::capture(world, packet);
        let extraction = *world.resource::<SpriteRenderStats>();
        ensure!(
            extraction.extracted_sprites > 0
                && extraction.extracted_tile_cells > 0
                && extraction.queued_items > 0,
            "Nara must extract sprites and tile cells: {extraction:?}"
        );

        let (output, report) = match self.renderer.render_to_image(&input) {
            Err(WgpuRenderError::TargetsBusy) => return Ok(None),
            rendered => rendered.context("submit the Nara stock passes")?,
        };
        self.facts.frames += 1;
        self.facts.extraction = extraction;
        self.facts.textures = self.renderer.texture_cache_stats();
        self.facts.created_textures += report.work.created_texture_calls;
        self.facts.created_buffers += report.work.created_buffer_calls;
        Ok(Some(output))
    }

    pub(crate) fn facts(&self) -> SceneFacts {
        self.facts
    }
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

fn populate_scene(world: &mut World) {
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
    world.spawn((tilemap, Transform2d::IDENTITY));
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
        world.spawn((
            Sprite::from_color(Vec2::splat(size), color).with_layer(1),
            Transform2d::IDENTITY,
            Orbit { radius, phase },
        ));
    }
}
