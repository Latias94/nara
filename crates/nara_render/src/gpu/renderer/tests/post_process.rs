#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
use crate::gpu::Bloom2d;
use crate::gpu::{ScreenFade2d, Vignette2d};

use super::*;

#[cfg(feature = "sprite-render")]
#[test]
#[ignore = "requires a native WGPU adapter"]
fn post_process_pixels_and_steady_state_allocations() {
    use nara_core::Vec2;

    use crate::material::{AlphaMode2d, SamplerDescriptor};
    use crate::sprite_render::{
        SpriteBatch, SpriteBatchKind, SpriteBatches, SpriteInstance, SpriteMaterialKey,
    };

    let extent = Extent2d::new(17, 17).unwrap();
    let packet = window_packet(extent, None, Color::rgba(0.125, 0.125, 0.125, 0.5));
    let mut sprites = SpriteBatches::default();
    sprites.replace(vec![SpriteBatch {
        view_index: 0,
        view_order: 0,
        target: packet.logical_target(),
        kind: SpriteBatchKind::Sprite,
        layer: 0,
        sort_key: 0,
        material: SpriteMaterialKey {
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Blend,
        },
        image: None,
        material_extension: None,
        instances: vec![SpriteInstance::axis_aligned(
            Vec2::ZERO,
            Vec2::splat(1.0 / 17.0),
            Color::rgb(4.0, 0.125, 0.125),
        )],
        tile_chunk: None,
    }]);
    let mut world = World::new();
    world.insert_resource(sprites);
    let absent = RenderInput::capture(&world, packet.clone());
    let bloom = Bloom2d {
        threshold: 1.0,
        intensity: 4.0,
        radius: 8.0,
    };
    world.insert_resource(PostProcess2d {
        bloom: Some(Bloom2d {
            intensity: 0.0,
            ..bloom
        }),
        vignette: Some(Vignette2d {
            intensity: 0.0,
            smoothness: 0.5,
        }),
        screen_fade: Some(ScreenFade2d {
            color: Color::WHITE,
            alpha: 0.0,
        }),
    });
    let zero = RenderInput::capture(&world, packet.clone());
    world.insert_resource(PostProcess2d {
        bloom: Some(bloom),
        ..Default::default()
    });
    let active = RenderInput::capture(&world, packet.clone());
    world.resource_mut::<PostProcess2d>().bloom = Some(Bloom2d {
        radius: 0.0,
        ..bloom
    });
    let local = RenderInput::capture(&world, packet);
    assert_eq!(absent.post_process(), PostProcess2d::default());
    assert!(!zero.post_process().is_active());
    assert_eq!(active.post_process().bloom, Some(bloom));

    let mut renderer = gpu();
    let target = ReadbackTarget::new(&renderer, extent, wgpu::TextureFormat::Rgba8Unorm);
    let (baseline, cold_direct) = target.draw(&mut renderer, &absent);
    let (zero_pixels, warm_direct) = target.draw(&mut renderer, &zero);
    assert_eq!(zero_pixels, baseline);
    assert_no_native_creates(warm_direct.work);
    assert_eq!(
        warm_direct.work.issued_buffer_write_calls,
        cold_direct.work.issued_buffer_write_calls,
    );
    assert_eq!(renderer.texture_cache_stats(), TextureCacheStats::default());
    assert_eq!(baseline[8 * 17 + 8][0], 255);

    let (blurred, cold) = target.draw(&mut renderer, &active);
    assert_eq!(cold.work.created_texture_calls, 3);
    let (unblurred, warm) = target.draw(&mut renderer, &local);
    assert_no_native_creates(warm.work);
    for index in [8 * 17 + 4, 4 * 17 + 8] {
        assert_eq!(unblurred[index], baseline[index]);
        assert!(blurred[index][0] > baseline[index][0] + 8);
        assert_eq!(blurred[index][3], baseline[index][3]);
    }
    for _ in 0..3 {
        let (pixels, report) = target.draw(&mut renderer, &active);
        assert_eq!(pixels, blurred);
        assert_no_native_creates(report.work);
    }
    let cache = renderer.texture_cache_stats();
    let (disabled_again, report) = target.draw(&mut renderer, &zero);
    assert_eq!(disabled_again, baseline);
    assert_eq!(renderer.texture_cache_stats(), cache);
    assert_no_native_creates(report.work);
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn post_process_preserves_canvas_and_leased_image_targets() {
    use crate::gpu::texture_cache::MAX_ACTIVE_TEXTURES;

    let mut renderer = gpu();
    let context = renderer.context().clone();
    let (window, image, extent) = frame(true);
    let target = ReadbackTarget::new(&renderer, extent, wgpu::TextureFormat::Bgra8UnormSrgb);
    let mut world = World::new();
    world.insert_resource(PostProcess2d {
        vignette: Some(Vignette2d {
            intensity: 1.0,
            smoothness: 0.25,
        }),
        screen_fade: Some(ScreenFade2d {
            color: Color::rgba(0.0, 1.0, 0.0, 0.5),
            alpha: 0.5,
        }),
        ..Default::default()
    });
    let window = RenderInput::capture(&world, window.packet().clone());
    let effects = RenderInput::capture(&world, image.packet().clone());
    let (pixels, cold) = target.draw(&mut renderer, &window);
    assert_eq!(cold.work.created_texture_calls, 1);
    assert_eq!(pixels[0], [0, 0, 255, 255]);
    let interior = pixels[2 * 8 + 2];
    for index in [2 * 8 + 3, 3 * 8 + 2, 3 * 8 + 3] {
        assert_eq!(pixels[index], interior);
    }
    assert!(interior[0].abs_diff(120) <= 1);
    assert!(interior[1].abs_diff(137) <= 1);
    assert_eq!(interior[2..], [0, 112]);
    let edge = pixels[2 * 8];
    assert!(edge[0] < interior[0]);
    assert_eq!(edge[1..], interior[1..]);

    let (output, report) = renderer.render_to_image(&effects).unwrap();
    assert_eq!(report.work.created_texture_calls, 1);
    assert_eq!(report.work.created_buffer_calls, 0);
    assert_eq!(renderer.texture_cache_stats().idle, 0);
    let read_output = |output: &WgpuImageOutput| {
        read(
            context.device(),
            context.queue(),
            output.native_texture().unwrap(),
            extent,
        )
    };
    assert_eq!(read_output(&output), pixels);
    assert_eq!(renderer.texture_cache_stats().idle, 1);

    let plain = frame(false).1;
    let mut held = Vec::new();
    for _ in 0..MAX_ACTIVE_TEXTURES - 2 {
        held.push(renderer.render_to_image(&plain).unwrap().0);
    }
    wait(context.device(), None);
    let cache = renderer.texture_cache_stats();
    assert!(matches!(
        renderer.render_to_image(&effects),
        Err(WgpuRenderError::TargetsBusy)
    ));
    assert_eq!(renderer.texture_cache_stats(), cache);
    drop(held.pop());
    let (reused, report) = renderer.render_to_image(&effects).unwrap();
    assert_no_native_creates(report.work);
    assert_ne!(reused.native_texture(), output.native_texture());
    assert_eq!(read_output(&reused), pixels);
    assert_eq!(read_output(&output), pixels);

    context.device().destroy();
    assert!(matches!(
        renderer.check_errors(),
        Err(WgpuRenderError::DeviceLost)
    ));
    assert!(output.native_texture().is_none());
    assert!(reused.native_texture().is_none());
}

#[cfg(feature = "hud-runtime")]
#[test]
#[ignore = "requires a native WGPU adapter"]
fn post_process_leaves_hud_pixels_crisp() {
    let extent = Extent2d::new(64, 32).unwrap();
    let packet = window_packet(extent, None, Color::BLACK);
    let mut world = World::new();
    let hud = glyph_hud(extent);
    let direct = RenderInput::capture(&world, packet.clone()).with_hud(hud.clone());
    world.insert_resource(PostProcess2d {
        bloom: Some(Bloom2d {
            threshold: 0.2,
            intensity: 3.0,
            radius: 12.0,
        }),
        ..Default::default()
    });
    let active = RenderInput::capture(&world, packet).with_hud(hud);
    let mut renderer = gpu();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = ReadbackTarget::new(&renderer, extent, format);
    let overlay = editor_overlay(renderer.context().device(), format);
    renderer
        .set_editor_overlay(Some(Box::new(overlay)))
        .unwrap();

    let (baseline, report) = target.draw(&mut renderer, &direct);
    let glyph_count = report.hud.unwrap().unwrap().glyph_count;
    assert!(glyph_count > 0);
    assert_eq!(baseline[4 * 64 + 4], [0, 255, 0, 255]);
    assert_eq!(baseline[4 * 64 + 5], [255, 255, 0, 255]);
    assert!(baseline[8 * 64..].iter().any(|pixel| pixel[0] > 200));

    let (with_effects, report) = target.draw(&mut renderer, &active);
    assert_eq!(report.hud.unwrap().unwrap().glyph_count, glyph_count);
    assert_eq!(with_effects, baseline);
    let (warm, report) = target.draw(&mut renderer, &active);
    assert_eq!(warm, baseline);
    assert_no_native_creates(report.work);
}

#[cfg(feature = "hud-runtime")]
fn glyph_hud(extent: Extent2d) -> crate::hud::HudFrame {
    use nara_ui::kernel as ui;

    let snapshot = ui::UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            ui::UiRoot::new(
                ui::UiRootKey::runtime("post-process", "hud").unwrap(),
                ui::UiLayer::Base,
                ui::UiStyle::fill(),
            )
            .with_child(
                ui::UiNode::new(ui::UiStyle::absolute(4.0, 4.0, 2.0, 1.0))
                    .with_key(ui::UiLocalKey::runtime("post-process", "solid").unwrap())
                    .with_visual(ui::UiVisual::solid(ui::UiColor::rgba(1.0, 1.0, 0.0, 1.0))),
            )
            .with_child(
                ui::UiNode::new(ui::UiStyle::absolute(8.0, 8.0, 52.0, 24.0))
                    .with_key(ui::UiLocalKey::runtime("post-process", "text").unwrap())
                    .with_text(
                        ui::UiText::new("HUD", 16.0)
                            .unwrap()
                            .with_brush(ui::UiBrush::Solid(ui::UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                    ),
            ),
        )
        .build()
        .unwrap();
    let transform = ui::UiSurfaceContentTransform::direct(
        ui::UiSize::new(extent.width as f32, extent.height as f32).unwrap(),
        1.0,
    )
    .unwrap();
    let environment = ui::UiEnvironment::for_surface(
        ui::UiSurfaceId::new(1).unwrap(),
        transform,
        ui::UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let font = nara_ui::text::bundled_font_face().unwrap();
    let frame = ui::UiCompiler::new(font.clone(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, ui::UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    crate::hud::HudFrame::new(frame, []).with_font(font)
}

struct ReadbackTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    extent: Extent2d,
}

impl ReadbackTarget {
    fn new(renderer: &Renderer, extent: Extent2d, format: wgpu::TextureFormat) -> Self {
        let texture = renderer
            .context()
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("post_process_readback"),
                size: size(extent),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        let view = texture.create_view(&Default::default());
        Self {
            texture,
            view,
            extent,
        }
    }

    fn draw(&self, renderer: &mut Renderer, frame: &RenderInput) -> (Vec<[u8; 4]>, RenderReport) {
        let format = self.texture.format();
        let report = renderer
            .render_to_view(
                frame,
                RenderTargetView {
                    view: &self.view,
                    format,
                    extent: self.extent,
                },
            )
            .unwrap();
        let context = renderer.context();
        let mut pixels = read(
            context.device(),
            context.queue(),
            &self.texture,
            self.extent,
        );
        if format == wgpu::TextureFormat::Bgra8UnormSrgb {
            for pixel in &mut pixels {
                pixel.swap(0, 2);
            }
        }
        (pixels, report)
    }
}

fn assert_no_native_creates(work: WgpuNativeWorkStats) {
    assert_eq!(work.created_texture_calls, 0);
    assert_eq!(work.created_buffer_calls, 0);
}
