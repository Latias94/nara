use std::time::Duration;

use nara_asset::{AssetEvents, AssetPath, AssetStates, Assets, Handle};

use crate::gpu::{GpuContext, RenderInput, RenderTargetView, Renderer, WgpuNativeWorkStats};
use crate::image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot};
use crate::material::{Material2dLibrary, MaterialTextureDeclaration};
use crate::sprite_render::batching::SpriteBatches;
use crate::{
    ClearColor, Extent2d, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderWindowPacket, build_image_render_frame_packet,
    runtime_integration::build_window_overlay_frame_packet,
};

use super::*;

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
#[ignore = "requires a native WGPU adapter"]
fn native_sprite_tile_pass_draws_ordered_views_and_reuses_static_gpu_resources() {
    let mut harness = NativeSceneTest::new();
    let first = view(
        harness.logical_target(),
        0,
        0,
        ViewLoadOp::Clear(Color::TRANSPARENT),
    );
    let mut second = view(harness.logical_target(), 1, 4, ViewLoadOp::Load);
    let half_red = Color::rgba(1.0, 0.0, 0.0, 0.5);
    let mut first_draw = batch(&first, 0, half_red, None);
    let mut texture_draw = batch(&second, 1, Color::WHITE, None);
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let handle = nara_asset::allocate_anonymous_handle().unwrap();
    texture_draw.image = Some(publish_pixel(
        &mut images,
        &mut states,
        handle,
        ImageColorSpace::Srgb,
        [0, 0, 128, 0],
    ));
    texture_draw.material.alpha_mode = AlphaMode2d::Opaque;

    let start = harness.input(
        &[first, second],
        vec![first_draw.clone(), texture_draw.clone()],
    );
    let (pixels, cold) = harness.draw(&start);
    assert_pixel(pixels[0][1], [128, 0, 0, 128]);
    assert_pixel(pixels[1][1], [0, 0, 55, 255]);
    assert_eq!(cold.created_buffer_calls, 2);
    assert_eq!(cold.created_texture_calls, 2);

    first_draw.instances[0].color = Color::rgba(0.0, 1.0, 0.0, 0.5);
    let animated = harness.input(
        &[first, second],
        vec![first_draw.clone(), texture_draw.clone()],
    );
    let (pixels, warm) = harness.draw(&animated);
    assert_pixel(pixels[0][1], [0, 128, 0, 128]);
    assert_pixel(pixels[1][1], [0, 0, 55, 255]);
    assert_no_native_creates(warm);
    assert_eq!(warm.issued_buffer_write_calls, 1);
    assert_eq!(warm.issued_texture_write_calls, 0);

    second.load = ViewLoadOp::Clear(Color::rgba(0.0, 0.0, 1.0, 0.5));
    let clear_second = harness.input(&[first, second], vec![first_draw]);
    let (pixels, _) = harness.draw(&clear_second);
    for pixel in pixels.into_iter().flatten() {
        assert_pixel(pixel, [0, 0, 128, 128]);
    }

    let runtime = nara_app::RuntimeInstance::start(nara_app::App::new()).unwrap();
    let overlay = build_window_overlay_frame_packet(
        runtime.generation(),
        1,
        RenderWindowPacket {
            id: nara_window::WindowId::PRIMARY,
            resolution: nara_window::WindowResolution::new(8, 4),
            present_mode: nara_window::PresentMode::default(),
        },
        None,
    )
    .unwrap();
    let mut world = nara_ecs::World::new();
    world.insert_resource(ClearColor(Color::rgba(0.0, 1.0, 0.0, 0.5)));
    let (pixels, clear_only) = harness.draw(&RenderInput::capture(&world, overlay));
    for pixel in pixels.into_iter().flatten() {
        assert_pixel(pixel, [0, 128, 0, 128]);
    }
    assert_eq!(clear_only, WgpuNativeWorkStats::default());
    runtime.finish().unwrap();
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn native_dirty_tiles_and_custom_materials_follow_frozen_geometry_and_resource_generations() {
    let mut harness = NativeSceneTest::new();
    let first = view(
        harness.logical_target(),
        0,
        0,
        ViewLoadOp::Clear(Color::BLACK),
    );
    let mut left = batch(&first, 0, Color::WHITE, Some(tile_chunk(&first, 0, 1)));
    let mut right = batch(&first, 0, Color::WHITE, Some(tile_chunk(&first, 32, 1)));
    left.instances[0] =
        SpriteInstance::axis_aligned(Vec2::new(-0.5, 0.0), Vec2::new(0.5, 1.0), Color::WHITE);
    right.instances[0] =
        SpriteInstance::axis_aligned(Vec2::new(0.5, 0.0), Vec2::new(0.5, 1.0), Color::WHITE);

    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    let red = publish_pixel(
        &mut images,
        &mut states,
        image,
        ImageColorSpace::Linear,
        RED,
    );
    let mut library = Material2dLibrary::default();
    let id = "test.tile-material";
    library
        .declare(
            Material2dDeclaration::from_parameters::<Tint>(
                id,
                1,
                AlphaMode2d::Blend,
                vec![MaterialTextureDeclaration::new("palette", "palette_image", 1).unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let rgba = Tint { rgba: Vec4::ONE };
    let older = publish_material(
        &mut library,
        id,
        "select(vec4<f32>(0.0), textureLoad(palette_image, vec2<i32>(0, 0), 0) * material_parameters.rgba * tint, nara_view.viewport.z == 4.0)",
    );
    let prepared = |generation: &Material2dGeneration, snapshot| {
        Arc::new(PreparedMaterial2d {
            binding: generation
                .bind(&rgba, BTreeMap::from([("palette".into(), image)]))
                .unwrap(),
            textures: vec![(1, snapshot)],
        })
    };
    let old_material = prepared(&older, red);
    left.material_extension = Some(Arc::clone(&old_material));
    right.material_extension = Some(old_material);
    let old_frame = harness.input(&[first], vec![left.clone(), right.clone()]);
    let (pixels, cold) = harness.draw(&old_frame);
    assert_eq!(pixels[0], [RED; 4]);
    assert_eq!(pixels[1], [BLACK; 4]);
    assert_eq!(cold.created_buffer_calls, 4);
    let (_, warm) = harness.draw(&old_frame);
    assert_no_native_creates(warm);
    assert_eq!(warm.issued_buffer_write_calls, 1);

    left.tile_chunk = Some(tile_chunk(&first, 0, 2));
    left.instances[0].color = Color::BLACK;
    let green = publish_pixel(
        &mut images,
        &mut states,
        image,
        ImageColorSpace::Linear,
        GREEN,
    );
    let newer = publish_material(
        &mut library,
        id,
        "vec4<f32>(textureLoad(palette_image, vec2<i32>(0, 0), 0).rbg * material_parameters.rgba.rgb * tint.rgb, 1.0)",
    );
    assert_ne!(older.sequence(), newer.sequence());
    assert_ne!(older.shader().digest(), newer.shader().digest());
    let new_material = prepared(&newer, green);
    left.material_extension = Some(Arc::clone(&new_material));
    right.material_extension = Some(new_material);
    let new_frame = harness.input(&[first], vec![left, right]);
    let (pixels, changed) = harness.draw(&new_frame);
    assert_eq!(pixels[0], [BLACK, BLACK, BLUE, BLUE]);
    assert_eq!(changed.created_buffer_calls, 1);
    assert_eq!(changed.created_texture_calls, 1);
    assert_eq!(changed.issued_buffer_write_calls, 2);
    assert_eq!(changed.issued_texture_write_calls, 1);

    let (pixels, loaded_old) = harness.draw(&old_frame);
    assert_eq!(pixels[0], [RED; 4]);
    assert_eq!(loaded_old.created_texture_calls, 0);
    assert_eq!(loaded_old.issued_texture_write_calls, 0);
}

fn assert_no_native_creates(work: WgpuNativeWorkStats) {
    assert_eq!(work.created_buffer_calls, 0);
    assert_eq!(work.created_texture_calls, 0);
}

fn assert_pixel(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
        "expected {expected:?}, got {actual:?}"
    );
}

fn publish_material(
    library: &mut Material2dLibrary,
    id: &str,
    result: &str,
) -> Material2dGeneration {
    let body: Arc<str> = tint_shader(result).into();
    let path = AssetPath::new("test.wgsl").unwrap();
    let prepared = library
        .prepare_update(id, &[path], |_| Ok(Arc::clone(&body)))
        .unwrap();
    library
        .publish(&prepared, |_| Ok(Arc::clone(&body)))
        .unwrap()
}

fn publish_pixel(
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    handle: Handle<ImageAsset>,
    color_space: ImageColorSpace,
    color: [u8; 4],
) -> PreparedImageSnapshot {
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        color_space,
        color.to_vec(),
    )
    .unwrap();
    images
        .commit_loaded(handle, image, states, &mut AssetEvents::default())
        .unwrap();
    PreparedImageSnapshot::capture(handle, images, states).unwrap()
}

struct NativeSceneTest {
    renderer: Renderer,
    target: wgpu::Texture,
    view: wgpu::TextureView,
    source: RenderFrameSourceGeneration,
    packet_target: RenderImagePacket,
    frame: u64,
}

impl NativeSceneTest {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    fn new() -> Self {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let context = GpuContext::request(instance, None).unwrap();
        let target = context.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("sprite_tile_readback"),
            size: wgpu::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let packet_target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(8, 4).unwrap(),
        )
        .unwrap();
        Self {
            renderer: Renderer::new(context),
            target,
            view,
            source: RenderFrameSourceGeneration::try_acquire().unwrap(),
            packet_target,
            frame: 0,
        }
    }

    fn logical_target(&self) -> RenderTarget {
        RenderTarget::Image(self.packet_target.image())
    }

    fn input(&mut self, views: &[ExtractedView], batches: Vec<SpriteBatch>) -> RenderInput {
        self.frame += 1;
        let mut views_resource = ExtractedViews::default();
        for view in views {
            views_resource.push(*view);
        }
        let packet = build_image_render_frame_packet(
            self.source,
            self.frame,
            &views_resource,
            self.packet_target,
        )
        .unwrap()
        .unwrap();
        let mut world = nara_ecs::World::new();
        let mut batches_resource = SpriteBatches::default();
        batches_resource.replace(batches);
        world.insert_resource(batches_resource);
        RenderInput::capture(&world, packet)
    }

    fn draw(&mut self, input: &RenderInput) -> ([[[u8; 4]; 4]; 2], WgpuNativeWorkStats) {
        let report = self
            .renderer
            .render_to_view(
                input,
                RenderTargetView {
                    view: &self.view,
                    format: Self::FORMAT,
                    extent: self.packet_target.extent(),
                },
            )
            .unwrap();
        let context = self.renderer.context();
        let device = context.device();
        let stride = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_tile_readback"),
            size: u64::from(stride) * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: None,
                },
            },
            self.target.size(),
        );
        let submission = context.queue().submit([encoder.finish()]);
        let (ready, completion) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                ready.send(result).unwrap();
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        completion
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let row = &mapped[2 * stride as usize..2 * stride as usize + 32];
        let pixels = [
            bytemuck::cast_slice(&row[..16]).try_into().unwrap(),
            bytemuck::cast_slice(&row[16..]).try_into().unwrap(),
        ];
        drop(mapped);
        buffer.unmap();
        (pixels, report.work)
    }
}
