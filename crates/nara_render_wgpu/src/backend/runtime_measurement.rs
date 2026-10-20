//! Decision-local offscreen costs, not surface-present latency or a product frame-rate promise.

use super::*;
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Instant,
};

use nara_app::{App, CoreStage, RuntimeCandidateRetirementState, RuntimeInstance};
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, ResMut, Resource};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{InputPlugin, InputPointerId, InputSeatId, InputWindowId};
use nara_render::{
    Camera2d, ExtractedViews, WorldCanvas, build_render_frame_packet_with_world_canvas,
};
use nara_sprite_render::{__private::SpriteBatches, SpriteRenderStats};
use nara_tilemap::{TileCell, TileCoord, TileIndex, Tilemap};
use nara_transform::Transform2d;
use nara_ui::{UiNode, UiRoot, UiRuntimeKey, UiSurfaceEndpoint, UiSurfaceSlot, UiText, UiVisual};
use nara_ui_kernel::{
    UiColor, UiLayer, UiSafeAreaInsets, UiSize, UiStyle, UiSurfaceContentTransform, UiSurfaceId,
};
use nara_ui_render::{UiPreparedImages, UiPresentationPlanner};
use nara_window::{Window, WindowResolution};

const WARMUP: i32 = 2;
const SAMPLES: i32 = 16;
const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const ROW_BYTES: u32 = 3_840;

#[derive(Resource)]
struct Workload {
    changing: bool,
    sequence: u32,
    tilemap: Entity,
    label: Entity,
    center: TileCoord,
}

fn change_one_cell_and_label(
    mut work: ResMut<Workload>,
    mut maps: Query<&mut Tilemap>,
    mut labels: Query<&mut UiText>,
) {
    if !work.changing {
        return;
    }
    work.sequence += 1;
    let color = if work.sequence % 2 == 0 {
        Color::rgb(1.0, 0.0, 0.0)
    } else {
        Color::rgb(0.0, 1.0, 0.0)
    };
    maps.get_mut(work.tilemap).unwrap().set_cell(
        work.center,
        TileCell::new(TileIndex::new(0)).with_color(color),
    );
    let mut label = labels.get_mut(work.label).unwrap();
    *label = label
        .clone()
        .with_content(format!("Score {:06}", work.sequence));
}

fn runtime(population: usize, changing: bool) -> RuntimeInstance {
    let side = match population {
        64 => 8,
        4_096 => 64,
        65_536 => 256,
        _ => unreachable!(),
    };
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        InputPlugin,
        nara_ui::UiPlugin::default(),
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin {
            world_canvas: Some(WorldCanvas::new(320, 180).unwrap()),
        },
        nara_image::ImagePreparePlugin,
    ))
    .unwrap();
    app.add_plugins(nara_sprite_render::SpriteRenderPlugin)
        .unwrap();
    let world = app.world_mut().unwrap();
    world.spawn(Window::new(
        "runtime cost",
        WindowResolution::new(WIDTH, HEIGHT),
    ));
    // One fixed view makes the larger populations mostly offscreen. Current extraction queues
    // all cells; this deliberately measures that cost instead of claiming every tile is visible.
    let camera = world
        .spawn((
            Camera2d {
                viewport_height: 32.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let center = TileCoord::new(side / 2, side / 2);
    let mut tiles = Tilemap::new(Vec2::ONE);
    for y in 0..side {
        for x in 0..side {
            tiles.set_cell(
                TileCoord::new(x, y),
                TileCell::new(TileIndex::new(0)).with_color(Color::rgb(0.0, 1.0, 0.0)),
            );
        }
    }
    let tilemap = world
        .spawn((
            tiles,
            Transform2d::from_translation(Vec2::splat(-(side as f32) * 0.5)),
        ))
        .id();
    let root = world
        .spawn((
            UiRoot::new(UiLayer::Base),
            UiNode::new(UiStyle::fill()),
            UiRuntimeKey::new("cost", "root").unwrap(),
        ))
        .id();
    let marker = world
        .spawn((
            UiNode::new(UiStyle::absolute(0.0, 0.0, 8.0, 8.0)),
            UiVisual::solid(UiColor::rgba(0.0, 0.0, 1.0, 1.0)),
            UiRuntimeKey::new("cost", "marker").unwrap(),
        ))
        .id();
    let mut children = vec![marker];
    for row in 0..8 {
        children.push(
            world
                .spawn((
                    UiNode::new(UiStyle::absolute(
                        12.0,
                        12.0 + row as f32 * 20.0,
                        180.0,
                        18.0,
                    )),
                    UiText::new(
                        if row == 0 {
                            "Score 000000".to_owned()
                        } else {
                            format!("HUD label {row}")
                        },
                        16.0,
                    )
                    .unwrap(),
                    UiRuntimeKey::new("cost", format!("label-{row}")).unwrap(),
                ))
                .id(),
        );
    }
    let label = children[1];
    let operations = [camera, tilemap, root]
        .into_iter()
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        })
        .chain(
            children
                .into_iter()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent {
                        parent: root,
                    }),
                }),
        )
        .collect::<Vec<_>>();
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .unwrap();
    nara_ui::install_ui_surface(
        world,
        UiSurfaceSlot::new(
            UiSurfaceId::new(1).unwrap(),
            UiSurfaceEndpoint::new(
                InputWindowId::new(WindowId::PRIMARY.get()),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(
                UiSize::new(WIDTH as f32, HEIGHT as f32).unwrap(),
                UiSize::new(WIDTH as f32, HEIGHT as f32).unwrap(),
                UiSafeAreaInsets::default(),
                1.0,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    app.insert_resource(Workload {
        changing,
        sequence: 0,
        tilemap,
        label,
        center,
    })
    .unwrap();
    app.add_systems(CoreStage::Update, change_one_cell_and_label)
        .unwrap();
    RuntimeInstance::start(app).unwrap()
}

#[test]
#[ignore = "optimized decision-local runtime cost measurement; requires a WGPU adapter"]
fn tilemap_and_changing_hud_costs() {
    assert!(!cfg!(debug_assertions), "measure with --release");
    let path = std::env::var_os("NARA_RUNTIME_COST_OUTPUT")
        .expect("set a distinct absolute JSONL output path");
    let mut output = BufWriter::new(File::create(path).unwrap());
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("measurement requires a WGPU adapter");
    let info = adapter.get_info();
    println!("runtime_cost_adapter: {info:?}");
    writeln!(
        output,
        "{{\"measurement\":\"runtime-cost-environment\",\"backend\":\"{:?}\",\"vendor\":{},\"device\":{}}}",
        info.backend, info.vendor, info.device
    )
    .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_cost_destination"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    for population in [64, 4_096, 65_536] {
        for changing in [false, true] {
            let scenario = if changing {
                "one_cell_and_hud_label"
            } else {
                "static"
            };
            let mut runtime = runtime(population, changing);
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            backend.device_epoch = 1;
            backend.configure_telemetry(WgpuBackendTelemetryConfig::new(8, 32).unwrap());
            backend.telemetry.begin_device_epoch(1);
            let mut planner = UiPresentationPlanner::default();
            let window = Window::new("runtime cost", WindowResolution::new(WIDTH, HEIGHT));
            // Device/fixture construction and output are excluded. Every sample drains the prior
            // submission, so this is serial latency attribution, not pipelined throughput.
            for sample in -WARMUP..SAMPLES {
                let begin = Instant::now();
                runtime.drive(Duration::ZERO).unwrap();
                let drive_ns = begin.elapsed().as_nanos();
                let frame = nara_ui::latest_ui_frame(runtime.world(), UiSurfaceId::new(1).unwrap())
                    .unwrap()
                    .unwrap();
                let views = runtime.world().resource::<ExtractedViews>();
                let packet = build_render_frame_packet_with_world_canvas(
                    runtime.generation(),
                    (sample + WARMUP + 1) as u64,
                    Some(WindowId::PRIMARY),
                    [&window],
                    views,
                    views.world_canvas(),
                )
                .unwrap()
                .unwrap();
                let begin = Instant::now();
                let ui = planner
                    .plan(
                        Arc::clone(&frame),
                        RenderTarget::Window(WindowId::PRIMARY),
                        &UiPreparedImages::new(),
                    )
                    .unwrap();
                let plan_ns = begin.elapsed().as_nanos();
                let payload = WgpuFramePayload::for_editor_viewport(
                    runtime.world().resource::<SpriteBatches>().clone(),
                    vec![Arc::clone(&ui)],
                );
                backend.begin_frame_transaction(packet.frame_index(), Some(packet.frame_index()));
                backend
                    .admit_packet(packet.source_generation(), packet.frame_index())
                    .unwrap();
                backend.quad_textures.begin_frame(packet.frame_index());
                let begin = Instant::now();
                let mut prepared = backend
                    .prepare_target_draw(format, &packet, &payload, packet.frame_index())
                    .unwrap();
                let prepare_ns = begin.elapsed().as_nanos();
                let instance_bytes = prepared.instance_buffer_bytes();
                backend
                    .telemetry
                    .observe_gpu_resources(backend.logical_texture_bytes(), instance_bytes);
                let submit_started = Instant::now();
                let mut encoder = device.create_command_encoder(&Default::default());
                crate::encode_target_draw(&mut encoder, &target_view, &prepared);
                queue.submit([encoder.finish()]);
                prepared.commit_instance_reservations();
                prepared.retain_images_until_submission_complete(&queue);
                let token = backend
                    .telemetry
                    .begin_frame_submission(
                        packet.frame_index(),
                        packet.frame_index(),
                        1,
                        submit_started,
                    )
                    .unwrap();
                queue.on_submitted_work_done(move || token.complete());
                backend.telemetry.finish_prepared_gpu_resources();
                let submit_ns = submit_started.elapsed().as_nanos();
                let begin = Instant::now();
                backend
                    .wait_for_telemetry_completions(Duration::from_secs(5))
                    .unwrap();
                let drain_ns = begin.elapsed().as_nanos();
                let completions = backend.drain_frame_completion_samples();
                assert_eq!(completions.len(), 1);
                assert_eq!(completions[0].app_frame_index(), packet.frame_index());
                let resources = backend.gpu_resource_stats();
                assert_eq!(resources.pending_retirements(), 0);
                assert_eq!(resources.lost_retirements(), 0);
                let stats = *runtime.world().resource::<SpriteRenderStats>();
                assert_eq!(stats.extracted_tile_cells as usize, population);
                assert_eq!(stats.queued_items as usize, population);
                assert_eq!(
                    runtime
                        .world()
                        .resource::<SpriteBatches>()
                        .total_instances(),
                    population
                );
                assert_eq!(
                    stats.missing_tilesets + stats.invalid_tile_regions + stats.unprepared_textures,
                    0
                );
                let expected_label = format!(
                    "Score {:06}",
                    runtime.world().resource::<Workload>().sequence
                );
                assert!(
                    frame
                        .semantics()
                        .iter()
                        .any(|node| node.name() == expected_label)
                );
                assert!(
                    ui.glyph_pages()
                        .iter()
                        .any(|page| page.pixels().iter().any(|&pixel| pixel != 0))
                );
                assert_eq!(prepared.ui_packets.len(), 1);
                assert_eq!(
                    backend.frame_transaction_stats().surface_acquire_attempts(),
                    0
                );
                for (phase, elapsed_ns) in [
                    ("managed_drive", drive_ns),
                    ("ui_packet_plan", plan_ns),
                    ("wgpu_prepare", prepare_ns),
                    ("encode_and_submit", submit_ns),
                    ("gpu_drain", drain_ns),
                    (
                        "encode_start_to_completion_callback",
                        u128::from(completions[0].duration_ns()),
                    ),
                ] {
                    row(
                        &mut output,
                        scenario,
                        population,
                        sample,
                        phase,
                        elapsed_ns,
                        &backend,
                        instance_bytes,
                        prepared.draw_calls,
                        ui.primitives().len(),
                        ui.glyph_pages()
                            .iter()
                            .map(|page| page.pixels().len())
                            .sum(),
                    );
                }
                if sample == SAMPLES - 1 {
                    assert_pixels(&device, &queue, &target, &packet, &runtime);
                }
            }
            let resources = backend.gpu_resource_stats();
            assert_eq!(resources.pending_retirements(), 0);
            let begin = Instant::now();
            let mut retirement = runtime.begin_retirement();
            let deadline = Instant::now() + Duration::from_secs(5);
            while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
                assert!(Instant::now() < deadline, "the finite runtime must retire");
                retirement.drive_retirement();
                std::thread::yield_now();
            }
            let retirement_ns = begin.elapsed().as_nanos();
            drop(planner);
            backend
                .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
                .unwrap();
            assert_eq!(backend.logical_texture_bytes(), 0);
            assert_eq!(backend.gpu_resource_stats().current_total_bytes(), 0);
            assert_eq!(backend.gpu_resource_stats().pending_retirements(), 0);
            assert_eq!(backend.gpu_resource_stats().lost_retirements(), 0);
            row(
                &mut output,
                scenario,
                population,
                SAMPLES,
                "runtime_retirement",
                retirement_ns,
                &backend,
                0,
                0,
                0,
                0,
            );
        }
    }
    output.flush().unwrap();
}

fn row(
    output: &mut impl Write,
    scenario: &str,
    population: usize,
    sample: i32,
    phase: &str,
    elapsed_ns: u128,
    backend: &WgpuRenderBackend,
    instance_bytes: u64,
    draw_calls: u32,
    ui_primitives: usize,
    glyph_snapshot_bytes: usize,
) {
    let stats = backend.gpu_resource_stats();
    let textures = backend.texture_stats();
    // Camera bounds are +/-28.44445 by +/-16. Unit cells with positive intersection span
    // 58 columns and32 rows for the large maps; the8x8 map is entirely inside those bounds.
    let visible_cells = if population == 64 { 64 } else { 58 * 32 };
    writeln!(output, "{{\"measurement\":\"runtime-cost-v1\",\"scenario\":\"{scenario}\",\"population\":{population},\"sample\":{sample},\"phase\":\"{phase}\",\"elapsed_ns\":{elapsed_ns},\"optimized\":true,\"camera_world_height\":32,\"camera_world_width\":56.88889,\"tile_world_size\":1,\"tilemap_entities\":1,\"queued_world_cells\":{population},\"geometrically_visible_cells\":{visible_cells},\"instance_payload_bytes\":{instance_bytes},\"canvas_bytes\":{},\"image_glyph_fallback_bytes\":{},\"glyph_snapshot_bytes\":{glyph_snapshot_bytes},\"tracked_current_bytes\":{},\"tracked_peak_bytes\":{},\"pending_retirements\":{},\"lost_retirements\":{},\"draw_calls\":{draw_calls},\"ui_primitives\":{ui_primitives}}}", backend.world_canvas.logical_bytes(), textures.texture_bytes, stats.current_total_bytes(), stats.peak_total_bytes(), stats.pending_retirements(), stats.lost_retirements()).unwrap();
}

fn assert_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    packet: &nara_render::RenderFramePacket,
    runtime: &RuntimeInstance,
) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("runtime_cost_readback"),
        size: u64::from(ROW_BYTES * HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_BYTES),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let pixels = buffer.get_mapped_range(..).unwrap();
    assert_eq!(
        &pixels[..4],
        &[0, 0, 255, 255],
        "native UI marker overlays the fixed world"
    );
    let view = &packet.views()[0];
    let cell = runtime.world().resource::<Workload>();
    let tiles = runtime.world().get::<Tilemap>(cell.tilemap).unwrap();
    let center = view.world_to_viewport(Vec2::splat(0.5));
    let pixel = [
        center.x.floor() as u32 * 3 + 1,
        center.y.floor() as u32 * 3 + 1,
    ];
    let offset = (pixel[1] * ROW_BYTES + pixel[0] * 4) as usize;
    let red = tiles.get_cell(cell.center).unwrap().color.r > 0.5;
    assert_eq!(
        &pixels[offset..offset + 4],
        if red {
            &[255, 0, 0, 255]
        } else {
            &[0, 255, 0, 255]
        },
        "the changed cell reaches shared world raster/composite output"
    );
    drop(pixels);
    buffer.unmap();
}
