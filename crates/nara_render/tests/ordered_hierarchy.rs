#![cfg(all(feature = "hud-runtime", feature = "wgpu", not(target_family = "wasm")))]

use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

use nara_app::{App, RuntimeInstance};
use nara_asset::{
    AssetPlugin, add_asset,
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
};
use nara_core::Mat3;
use nara_ecs::{Entity, bevy_ecs::hierarchy::ChildOf};
use nara_input::{InputPlugin, InputPointerId, InputSeatId, InputWindowId};
use nara_render::{
    ClearColor, Color, Extent2d, ExtractedView, ExtractedViews, RenderImage2d, RenderImagePacket,
    RenderTarget, RenderViewId, RenderWindowPacket, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_image_render_frame_packet, build_render_frame_packet,
    gpu::{GpuContext, RenderInput, RenderTargetView, Renderer},
    hud::HudFrame,
    runtime_integration::build_window_overlay_frame_packet,
};
use nara_ui::{
    UiColor, UiCornerRadii, UiNode, UiOverflow, UiPlugin, UiRoot, UiSize, UiStyle,
    UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiText, UiVisual,
    install_ui_surface, latest_presented_ui_frame, presentation::publish_presented_frame,
    text::bundled_regular_font_face, ui_candidate_frame,
};
use nara_window::{Window, WindowId, WindowResolution};

const SIZE: u32 = 64;
const ROW_BYTES: u32 = SIZE * 4;
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[test]
#[ignore = "requires a native WGPU adapter"]
fn hud_preserves_sibling_order_clip_and_alpha_over_the_scene() {
    let mut app = App::new();
    let font = bundled_regular_font_face().unwrap();
    let font_id = font.id();
    app.add_plugins((AssetPlugin, InputPlugin, UiPlugin::with_font(font)))
        .unwrap();
    app.insert_resource(ClearColor(Color::rgb(0.0, 0.0, 1.0)))
        .unwrap();
    let world = app.world_mut().unwrap();

    let surface = UiSurfaceId::new(7).unwrap();
    let secondary = UiSurfaceId::new(1).unwrap();
    let transform =
        UiSurfaceContentTransform::direct(UiSize::new(SIZE as f32, SIZE as f32).unwrap(), 1.0)
            .unwrap();
    for (surface_id, window_id) in [
        (surface, InputWindowId::PRIMARY),
        (secondary, InputWindowId::new(2)),
    ] {
        let endpoint =
            UiSurfaceEndpoint::new(window_id, InputSeatId::PRIMARY, InputPointerId::PRIMARY);
        install_ui_surface(
            world,
            UiSurfaceSlot::new(surface_id, endpoint, transform).unwrap(),
        )
        .unwrap();
    }

    let image = add_asset(
        world,
        ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![0, 255, 0, 128],
        )
        .unwrap(),
    )
    .unwrap();
    let root = world.spawn(UiRoot::default()).id();
    let group = world
        .spawn((
            ChildOf(root),
            UiNode::new(UiStyle::absolute(8.0, 8.0, 24.0, 24.0).with_overflow(UiOverflow::Clip)),
        ))
        .id();
    world.spawn((
        ChildOf(group),
        UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0)),
        UiVisual::solid(UiColor::rgba(1.0, 0.0, 0.0, 0.5)),
    ));
    world.spawn((
        ChildOf(group),
        UiNode::new(UiStyle::absolute(8.0, 8.0, 32.0, 32.0)),
        UiVisual::image_asset(image, UiColor::rgba(1.0, 1.0, 1.0, 1.0))
            .with_radii(UiCornerRadii::all(6.0).unwrap()),
    ));
    let label = world
        .spawn((
            ChildOf(root),
            UiNode::new(UiStyle::absolute(6.0, 35.0, 54.0, 24.0)),
            UiText::new("Relay", 16.0).unwrap(),
        ))
        .id();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::from_millis(16)).unwrap();
    let candidate = ui_candidate_frame(runtime.world(), surface)
        .unwrap()
        .unwrap();
    let secondary_hud = HudFrame::capture_surface(runtime.world(), secondary).unwrap();
    assert_eq!(secondary_hud.frame().environment().surface_id(), secondary);
    assert_eq!(secondary_hud.fonts()[0].id(), font_id);
    let frame = capture_window(&runtime, 1);

    let context = GpuContext::request(wgpu::Instance::default(), None).unwrap();
    let device = context.device().clone();
    let queue = context.queue().clone();
    let mut renderer = Renderer::new(context);
    let size = wgpu::Extent3d {
        width: SIZE,
        height: SIZE,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let view = RenderTargetView {
        view: &target_view,
        format: TARGET_FORMAT,
        extent: Extent2d::new(SIZE, SIZE).unwrap(),
    };

    let report = renderer.render_to_view(&frame, view).unwrap();
    let hud = report.hud.unwrap().unwrap();
    assert!(hud.glyph_count > 0);
    assert!(Arc::ptr_eq(&hud.frame, &candidate));
    assert!(latest_presented_ui_frame(runtime.world(), surface).is_none());
    assert!(report.work.issued_texture_write_calls >= 3);

    let pixels = readback(&device, &queue, &target, size);
    let pixel = |x: u32, y: u32| &pixels[(y * ROW_BYTES + x * 4) as usize..][..4];
    assert_eq!(pixel(1, 1), [0, 0, 255, 255]);
    assert_eq!(pixel(33, 24), [0, 0, 255, 255]);
    assert_near(pixel(12, 12), [128, 0, 128, 255]);
    assert_near(pixel(16, 16), [128, 0, 128, 255]);
    assert_near(pixel(24, 24), [64, 128, 64, 255]);
    assert!((35..59).any(|y| (6..60).any(|x| pixel(x, y)[0] > 128)));

    let warm = renderer.render_to_view(&frame, view).unwrap();
    assert_eq!(warm.work.created_buffer_calls, 0);
    assert_eq!(warm.work.created_texture_calls, 0);
    assert_eq!(warm.work.issued_texture_write_calls, 0);

    let publication = runtime
        .with_driver_scope(|scope| {
            scope.with_world_mut(|world| publish_presented_frame(world, &hud.frame, 1))
        })
        .unwrap();
    publication.output.unwrap();
    let presented = latest_presented_ui_frame(runtime.world(), surface)
        .unwrap()
        .1;
    assert!(Arc::ptr_eq(&presented, &candidate));

    let missing_image =
        HudFrame::new(Arc::clone(&candidate), []).with_fonts(secondary_hud.fonts().to_vec());
    let rejected = renderer
        .render_to_view(&frame.clone().with_hud(missing_image), view)
        .unwrap();
    assert!(matches!(
        rejected.hud,
        Err(nara_render::hud::HudDrawError::Paint(_))
    ));
    let scene_only = readback(&device, &queue, &target, size);
    assert!(
        scene_only
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255])
    );

    runtime
        .with_driver_scope(|scope| {
            scope.with_world_mut(|world| {
                world
                    .entity_mut(label)
                    .insert(UiText::new("Ready", 16.0).unwrap());
            })
        })
        .unwrap();
    runtime.drive(Duration::from_millis(16)).unwrap();
    let changed_frame = capture_window(&runtime, 2);
    let changed = renderer.render_to_view(&changed_frame, view).unwrap();
    assert_eq!(changed.work.created_texture_calls, 0);
    assert_eq!(changed.work.issued_texture_write_calls, 1);
    assert!(!Arc::ptr_eq(
        &changed.hud.unwrap().unwrap().frame,
        &candidate
    ));
    let updated_pixels = readback(&device, &queue, &target, size);
    assert_ne!(updated_pixels, pixels);
    let mut fresh = Renderer::new(renderer.context().clone());
    fresh.render_to_view(&changed_frame, view).unwrap();
    assert_eq!(updated_pixels, readback(&device, &queue, &target, size));

    let direct_image = capture_image(&runtime, view.extent);
    let (_, bare) = renderer.render_to_image(&direct_image).unwrap();
    assert!(bare.hud.unwrap().is_none());
    let selected = Arc::clone(secondary_hud.frame());
    let (output, direct) = renderer
        .render_to_image(&direct_image.with_hud(secondary_hud))
        .unwrap();
    let direct = direct.hud.unwrap().unwrap();
    assert!(Arc::ptr_eq(&direct.frame, &selected));
    assert_eq!(direct.glyph_count, hud.glyph_count);
    assert!(latest_presented_ui_frame(runtime.world(), secondary).is_none());
    let srgb = readback(&device, &queue, output.native_texture().unwrap(), size);
    let overlap = (24 * ROW_BYTES + 24 * 4) as usize;
    assert_near(&srgb[overlap..][..4], [137, 188, 137, 255]);

    let adapted = capture_secondary_as_image(&runtime, view.extent);
    let (_, report) = renderer.render_to_image(&adapted).unwrap();
    assert_eq!(
        report
            .hud
            .unwrap()
            .unwrap()
            .frame
            .environment()
            .surface_id(),
        secondary
    );

    let input =
        capture_image(&runtime, view.extent).with_hud(fallback_hud(candidate.environment()));
    let (_, report) = renderer.render_to_image(&input).unwrap();
    assert_eq!(report.hud.unwrap().unwrap().glyph_count, 2);
}

fn fallback_hud(environment: nara_ui::kernel::UiEnvironment) -> HudFrame {
    use nara_ui::{FontFace, kernel as ui, text::bundled_font_face};

    let fonts = vec![
        bundled_font_face().unwrap(),
        FontFace::from_bytes(
            include_bytes!("../../nara_ui/tests/text/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap(),
    ];
    let snapshot = ui::UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            ui::UiRoot::new(
                ui::UiRootKey::runtime("fallback", "hud").unwrap(),
                ui::UiLayer::Base,
                ui::UiStyle::fill(),
            )
            .with_child(
                ui::UiNode::new(ui::UiStyle::absolute(0.0, 0.0, 64.0, 48.0))
                    .with_key(ui::UiLocalKey::runtime("fallback", "label").unwrap())
                    .with_text(ui::UiText::new("A\u{2229}", 24.0).unwrap()),
            ),
        )
        .build()
        .unwrap();
    let frame = ui::UiCompiler::with_fonts(fonts.clone(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, ui::UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    HudFrame::new(frame, []).with_fonts(fonts)
}

fn capture_window(runtime: &RuntimeInstance, index: u64) -> RenderInput {
    let packet = build_window_overlay_frame_packet(
        runtime.generation(),
        index,
        RenderWindowPacket {
            id: WindowId::PRIMARY,
            resolution: WindowResolution::new(SIZE, SIZE),
            present_mode: Default::default(),
        },
        None,
    )
    .unwrap();
    RenderInput::capture(runtime.world(), packet)
}

fn capture_image(runtime: &RuntimeInstance, extent: Extent2d) -> RenderInput {
    let image = RenderImage2d::allocate_handle().unwrap();
    let target = RenderImagePacket::new(image, extent).unwrap();
    let viewport = ViewportRect::from_extent(extent).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::Image(image),
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::rgb(0.0, 0.0, 1.0)),
    });
    let packet = build_image_render_frame_packet(runtime.generation().into(), 3, &views, target)
        .unwrap()
        .unwrap();
    RenderInput::capture(runtime.world(), packet)
}

fn capture_secondary_as_image(runtime: &RuntimeInstance, extent: Extent2d) -> RenderInput {
    let mut window = Window::new(
        "secondary",
        WindowResolution::new(extent.width, extent.height),
    );
    window.id = WindowId::new(2);
    let viewport = ViewportRect::from_extent(extent).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    let target = RenderImagePacket::new(RenderImage2d::allocate_handle().unwrap(), extent).unwrap();
    let frame =
        build_render_frame_packet(runtime.generation(), 4, Some(window.id), [&window], &views)
            .unwrap()
            .unwrap()
            .with_image_target(target)
            .unwrap();
    RenderInput::capture(runtime.world(), frame)
}

fn assert_near(actual: &[u8], expected: [u8; 4]) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
        "expected {expected:?}, got {actual:?}"
    );
}

fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    size: wgpu::Extent3d,
) -> Vec<u8> {
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE * ROW_BYTES),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_BYTES),
                rows_per_image: Some(SIZE),
            },
        },
        size,
    );
    let submission_index = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        sender.send(result).unwrap()
    });
    let timeout = Some(Duration::from_secs(10));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission_index),
            timeout,
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    readback.get_mapped_range(..).unwrap().to_vec()
}
