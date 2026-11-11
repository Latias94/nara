#![cfg(feature = "stock-sprite")]

use super::*;
use crate::{
    WgpuImageOutput, WgpuPolicyDefinition, WgpuSceneColor, WgpuStockCapture, WgpuStockPolicy,
    WgpuWorldColor,
};
use nara_app::{App, RuntimeInstance};
use nara_asset::{AssetEvents, AssetStates, Assets};
use nara_core::{Color as LinearColor, Vec2};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};
use nara_material::{
    AlphaMode2d, Material2dDefinition, Material2dDefinitions, Material2dInput, SamplerDescriptor,
};
use nara_render::{
    Camera2d, ExtractedViews, RenderFrame, RenderFramePacket, RenderImage2d, RenderTarget,
    ViewportRect, WorldCanvas, build_render_frame_packet_with_world_canvas,
};
use nara_sprite::Sprite;
use nara_sprite_render::__private::SpriteBatches;
use nara_transform::Transform2d;
use nara_window::{Window, WindowResolution};

const CAMERA_CLEAR: LinearColor = LinearColor::rgba(0.8, 0.4, 0.2, 0.25);
const BAR_CLEAR: LinearColor = LinearColor::rgba(0.125, 0.75, 0.375, 0.5);
const MATERIAL_FRAGMENT: &str = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return material_parameters; }";
static MATERIALS: [Material2dDefinition; 2] = [
    Material2dDefinition {
        id: "test.stock-alpha.opaque",
        fragment_shader: MATERIAL_FRAGMENT,
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Opaque,
    },
    Material2dDefinition {
        id: "test.stock-alpha.blend",
        fragment_shader: MATERIAL_FRAGMENT,
        input: Material2dInput::SPRITE_FRAGMENT_V1,
        alpha_mode: AlphaMode2d::Blend,
    },
];

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_stock_camera_and_canvas_clears_write_associated_pixels() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    for canvas in [false, true] {
        for alpha in [0.0, 0.25, 1.0] {
            let clear = LinearColor::rgba(0.8, 0.4, 0.2, alpha);
            let (packet, empty_sprites) = captured_stock(canvas, false, clear);
            assert!(empty_sprites.sprite_batches().unwrap().is_empty());
            for capture in [WgpuStockCapture::new(), empty_sprites] {
                let producer = host.create_producer().unwrap();
                let output = submit_stock(&mut host, &producer, packet.clone(), capture);
                assert_stock_descriptor(&output, canvas);
                let actual = image_pixels(&mut host, &output);
                let extent = output.extent();
                for y in 0..extent.height {
                    for x in 0..extent.width {
                        let color = if canvas && !(16..48).contains(&y) {
                            associated(BAR_CLEAR)
                        } else {
                            associated(clear)
                        };
                        assert_pixel(
                            actual[(y * extent.width + x) as usize],
                            color,
                            (canvas, x, y),
                        );
                    }
                }
                drain(&host.retire_producer(&producer).unwrap());
            }
        }
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_stock_blend_and_opaque_preserve_accumulated_alpha_and_retained_outputs() {
    let mut host = WgpuRenderHost::offscreen();
    host.begin_frame(1).unwrap();
    for canvas in [false, true] {
        let producer = host.create_producer().unwrap();
        let (packet, capture) = captured_stock(canvas, true, CAMERA_CLEAR);
        assert_eq!(capture.sprite_batches().unwrap().total_instances(), 16);
        let output = submit_stock(&mut host, &producer, packet, capture);
        assert_stock_descriptor(&output, canvas);
        let descriptor = output.descriptor();
        let actual = image_pixels(&mut host, &output);
        let extent = output.extent();
        for y in 0..extent.height {
            for x in 0..extent.width {
                let expected = if canvas && !(16..48).contains(&y) {
                    associated(BAR_CLEAR)
                } else {
                    let (world_x, world_y) = if canvas {
                        (x / 2, (y - 16) / 2)
                    } else {
                        (x, y)
                    };
                    matrix_pixel(world_x, world_y)
                };
                assert_pixel(
                    actual[(y * extent.width + x) as usize],
                    expected,
                    (canvas, x, y),
                );
            }
        }

        let (next_packet, _) = captured_stock(!canvas, false, LinearColor::BLACK);
        let next = submit_stock(&mut host, &producer, next_packet, WgpuStockCapture::new());
        assert_ne!(next.extent(), output.extent());
        assert_ne!(next.receipt(), output.receipt());
        assert_eq!(output.descriptor(), descriptor);
        assert_eq!(image_pixels(&mut host, &output), actual);
        assert_stock_descriptor(&output, canvas);
        drain(&host.retire_producer(&producer).unwrap());
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_declared_program_clears_remain_raw_with_a_premultiplied_descriptor() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let source = source();
    host.begin_frame(1).unwrap();
    for (frame, alpha) in [(1, 0.0), (2, 64.0 / 255.0)] {
        let raw =
            crate::clear_color_to_wgpu(LinearColor::rgba(1.0, 128.0 / 255.0, 64.0 / 255.0, alpha));
        let (program, target) = clear(source, frame, raw);
        let outputs = host.submit_program(&producer, &program).unwrap();
        let output = outputs.texture(&target).unwrap();
        assert_eq!(output.descriptor().alpha, TextureAlpha::Premultiplied);
        let expected = [255, 128, 64, (alpha * 255.0).round() as u8];
        let actual = pixels(&mut host, &output);
        assert!(
            actual.iter().all(|pixel| *pixel == expected),
            "raw clear alpha={alpha}: expected {expected:?}, got {actual:?}"
        );
        drain(outputs.submitted_prefix());
    }
}

fn captured_stock(
    canvas: bool,
    draw_sprites: bool,
    clear: LinearColor,
) -> (RenderFramePacket, WgpuStockCapture) {
    let world_canvas = canvas.then(|| {
        WorldCanvas::new(32, 16)
            .unwrap()
            .with_bar_color(BAR_CLEAR)
            .unwrap()
    });
    let window = Window::new(
        "stock accumulated alpha",
        if canvas {
            WindowResolution::new(64, 64)
        } else {
            WindowResolution::new(32, 16)
        },
    );
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin { world_canvas },
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let textures = [128, 0].map(|alpha| {
        let handle = nara_asset::allocate_anonymous_handle::<ImageAsset>().unwrap();
        images
            .commit_loaded(
                handle,
                ImageAsset::new(
                    ImageExtent::new(1, 1),
                    ImageFormat::Rgba8,
                    ImageColorSpace::Linear,
                    vec![128, 64, 192, alpha],
                )
                .unwrap(),
                &mut states,
                &mut AssetEvents::default(),
                None,
                None,
            )
            .unwrap();
        handle
    });
    let materials = Material2dDefinitions::new(&MATERIALS).unwrap();
    app.insert_resource(images).unwrap();
    app.insert_resource(states).unwrap();
    app.insert_resource(materials.clone()).unwrap();
    let world = app.world_mut().unwrap();
    let mut entities = Vec::new();
    for (order, color) in [(0, LinearColor::rgba(0.75, 0.125, 0.5, 0.625)), (1, clear)] {
        entities.push(
            world
                .spawn((
                    Camera2d {
                        target: RenderTarget::Window(window.id),
                        viewport: Some(ViewportRect::new(0, 0, 32, 16).unwrap()),
                        viewport_height: 16.0,
                        clear_color: Some(color),
                        order,
                    },
                    Transform2d::default(),
                ))
                .id(),
        );
    }
    if draw_sprites {
        let tint = LinearColor::rgba(0.5, 0.75, 0.25, 0.5);
        let textured = |index| {
            Sprite::from_texture(textures[index], Vec2::splat(6.0))
                .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
                .with_tint(tint)
        };
        let sprites = [
            textured(0),
            Sprite::from_color(Vec2::splat(6.0), LinearColor::rgba(0.6, 0.3, 0.9, 0.25))
                .with_alpha_mode(AlphaMode2d::Opaque),
            Sprite::from_color(Vec2::splat(6.0), LinearColor::rgba(0.6, 0.3, 0.9, 0.0))
                .with_alpha_mode(AlphaMode2d::Opaque),
            textured(0).with_alpha_mode(AlphaMode2d::Opaque),
            textured(1).with_alpha_mode(AlphaMode2d::Opaque),
            textured(0),
            textured(1),
            textured(0),
        ];
        for (index, sprite) in sprites.into_iter().enumerate() {
            let mut entity = world.spawn((
                sprite,
                Transform2d::from_translation(Vec2::new(
                    -12.0 + (index % 4) as f32 * 8.0,
                    4.0 - (index / 4) as f32 * 8.0,
                )),
            ));
            if index >= 5 {
                let (id, parameters) = match index {
                    5 => ("test.stock-alpha.opaque", [0.25, 0.625, 0.875, 0.375]),
                    6 => ("test.stock-alpha.opaque", [0.25, 0.625, 0.875, 0.0]),
                    _ => ("test.stock-alpha.blend", [0.75, 0.125, 0.5, 0.5]),
                };
                entity.insert(materials.instantiate(id, parameters, textures[0]).unwrap());
            }
            entities.push(entity.id());
        }
    }
    HierarchyWriter::new(world)
        .apply_batch(
            &entities
                .into_iter()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let packet = build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        runtime.world().resource::<RenderFrame>().index,
        Some(window.id),
        [&window],
        runtime.world().resource::<ExtractedViews>(),
        world_canvas,
    )
    .unwrap()
    .unwrap();
    let capture =
        WgpuStockCapture::new().with_sprites(runtime.world().resource::<SpriteBatches>().clone());
    runtime.finish().unwrap();
    (packet, capture)
}

fn submit_stock(
    host: &mut WgpuRenderHost,
    producer: &WgpuRenderProducer,
    packet: RenderFramePacket,
    capture: WgpuStockCapture,
) -> WgpuImageOutput {
    let definition = WgpuPolicyDefinition::new(WgpuStockPolicy::new());
    host.prepare_policy(producer, &definition).unwrap();
    let installation = definition.install().unwrap();
    let target = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        packet.target().extent(),
    )
    .unwrap();
    let reservation = host
        .reserve_image_target(producer, packet.source_generation(), target)
        .unwrap();
    host.activate_image_target(producer, reservation).unwrap();
    let input = installation
        .capture(packet.with_image_target(target).unwrap(), Arc::new(capture))
        .unwrap();
    host.submit(producer, &input).unwrap()
}

fn assert_stock_descriptor(output: &WgpuImageOutput, canvas: bool) {
    let descriptor = output.descriptor();
    assert_eq!(descriptor.alpha, TextureAlpha::Premultiplied);
    assert_eq!(descriptor.transfer, TextureTransfer::Linear);
    assert_eq!(descriptor.native.format, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(descriptor.native.size.width, output.extent().width);
    assert_eq!(descriptor.native.size.height, output.extent().height);
    let typed_descriptor = if canvas {
        output.texture::<WgpuSceneColor>().unwrap().descriptor()
    } else {
        output.texture::<WgpuWorldColor>().unwrap().descriptor()
    };
    assert_eq!(descriptor, typed_descriptor);
}

fn image_pixels(host: &mut WgpuRenderHost, output: &WgpuImageOutput) -> Vec<[u8; 4]> {
    let extent = output.extent();
    let row_stride = (extent.width * 4).div_ceil(256) * 256;
    let ((device, readback), prefix) = host
        .submit_image_use(std::slice::from_ref(output), |device, _, encoder, views| {
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("stock-alpha-readback"),
                size: u64::from(row_stride) * u64::from(extent.height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                views[0].texture().as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_stride),
                        rows_per_image: Some(extent.height),
                    },
                },
                views[0].texture().size(),
            );
            Ok((device.clone(), readback))
        })
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    drain(&prefix);
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let result = {
        let bytes = readback.get_mapped_range(..).unwrap();
        bytes
            .chunks_exact(row_stride as usize)
            .flat_map(|row| {
                row[..(extent.width * 4) as usize]
                    .chunks_exact(4)
                    .map(|pixel| pixel.try_into().unwrap())
            })
            .collect()
    };
    readback.unmap();
    result
}

fn associated(color: LinearColor) -> [f64; 4] {
    let alpha = f64::from(color.a);
    [
        f64::from(color.r) * alpha,
        f64::from(color.g) * alpha,
        f64::from(color.b) * alpha,
        alpha,
    ]
}

fn matrix_pixel(x: u32, y: u32) -> [f64; 4] {
    let background = associated(CAMERA_CLEAR);
    if !(1..7).contains(&(x % 8)) || !(1..7).contains(&(y % 8)) {
        return background;
    }
    let textured = [
        0.5 * 128.0 / 255.0,
        0.75 * 64.0 / 255.0,
        0.25 * 192.0 / 255.0,
    ];
    let source = match y / 8 * 4 + x / 8 {
        0 => [textured[0], textured[1], textured[2], 0.5 * 128.0 / 255.0],
        1 | 2 => return [0.6, 0.3, 0.9, 1.0],
        3 | 4 => return [textured[0], textured[1], textured[2], 1.0],
        5 | 6 => return [0.25, 0.625, 0.875, 1.0],
        7 => [0.75, 0.125, 0.5, 0.5],
        _ => unreachable!(),
    };
    let inverse_alpha = 1.0 - source[3];
    [
        source[0] * source[3] + background[0] * inverse_alpha,
        source[1] * source[3] + background[1] * inverse_alpha,
        source[2] * source[3] + background[2] * inverse_alpha,
        source[3] + background[3] * inverse_alpha,
    ]
}

fn assert_pixel(actual: [u8; 4], linear: [f64; 4], location: (bool, u32, u32)) {
    let expected = linear.map(|channel| (channel * 255.0).round() as u8);
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
        "(canvas, x, y) = {location:?}: expected {expected:?}, got {actual:?}"
    );
}
