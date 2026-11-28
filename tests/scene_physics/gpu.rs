use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    asset::{
        cooked::{NpakArchive, NpakCompression, NpakEntryKind, NpakInput, encode_npak},
        image::SamplerDescriptor,
    },
    prelude::*,
    render::{
        Extent2d, ExtractedViews, RenderFrameSourceGeneration, RenderImagePacket, ViewportRect,
        build_image_render_frame_packet, image::ImagePreparePlugin,
        sprite_render::SpriteRenderPlugin,
    },
    render_wgpu::{GpuContext, RenderInput, Renderer},
    scene::{
        authoring::{SceneDocument, SceneEntity, SpriteSource},
        cook::cook_flattened_scene,
    },
};

#[test]
#[ignore = "requires a native WGPU adapter with allocator reporting"]
fn repeated_scene_switches_release_unused_native_textures() {
    let first = "0193f100-2222-7000-8000-000000000041".parse().unwrap();
    let second = "0193f100-2222-7000-8000-000000000042".parse().unwrap();
    let textures = [
        "0193f100-2222-7000-8000-000000000043".parse().unwrap(),
        "0193f100-2222-7000-8000-000000000044".parse().unwrap(),
    ];
    let tileset = "0193f100-2222-7000-8000-000000000045".parse().unwrap();
    let mut entries = Vec::new();
    for ((scene_id, texture), rgba) in [first, second]
        .into_iter()
        .zip(textures)
        .zip([[255, 0, 0, 255], [0, 255, 0, 255]])
    {
        let mut scene = SceneDocument::new("681a78dc-b6ab-4f3a-8eb6-4742d2ced237".parse().unwrap());
        let mut entity = SceneEntity::new(
            "4f99b200-2a5e-4872-8d3d-000000000001".parse().unwrap(),
            "Sector",
        );
        if scene_id == first {
            entity
                .set(&SpriteSource {
                    image: Some(texture),
                    size: Vec2::splat(8.0),
                    ..Default::default()
                })
                .unwrap();
        } else {
            entity
                .set(&TilemapSource {
                    tileset,
                    tile_size: Vec2::splat(8.0),
                    width: 1,
                    height: 1,
                    origin_x: 0,
                    origin_y: 0,
                    layer: 0,
                    cells: vec![1].into(),
                })
                .unwrap();
            entries.push((
                tileset,
                NpakEntryKind::TileSet,
                TileSetAsset {
                    image: texture,
                    layout: TileAtlasLayout::grid(Vec2::ONE, 1, 1),
                    sampler: SamplerDescriptor::NEAREST_CLAMP,
                    tiles: vec![TileDefinition::default()],
                }
                .encode_cooked()
                .unwrap(),
            ));
        }
        scene.entities.push(entity);
        let cooked = cook_flattened_scene(&scene, &Default::default()).unwrap();
        let image = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            rgba.to_vec(),
        )
        .unwrap();
        entries.push((scene_id, NpakEntryKind::Scene, cooked.bytes));
        entries.push((
            texture,
            NpakEntryKind::Image,
            image.encode_cooked().unwrap(),
        ));
    }
    let inputs = entries
        .iter()
        .map(|(id, kind, bytes)| NpakInput::new(*id, *kind, bytes, NpakCompression::None))
        .collect::<Vec<_>>();
    let archive = NpakArchive::from_bytes(encode_npak(&inputs).unwrap()).unwrap();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless()).unwrap();
    app.add_plugins((
        RenderPlugin::default(),
        ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app.set_asset_source(RuntimeAssetSource::from_npak_archive(Arc::new(archive)))
        .unwrap();
    let extent = Extent2d::new(16, 16).unwrap();
    let target = RenderImage2d::allocate_handle().unwrap();
    let image = RenderImagePacket::new(target, extent).unwrap();
    app.world_mut().unwrap().spawn((
        Camera2d {
            target: RenderTarget::Image(target),
            viewport: Some(ViewportRect::from_extent(extent).unwrap()),
            viewport_height: extent.height as f32,
            ..Default::default()
        },
        Transform2d::default(),
    ));

    let context = GpuContext::request(Default::default(), None).unwrap();
    let adapter = context.adapter().get_info();
    let mut renderer = Renderer::new(context);
    let stream = RenderFrameSourceGeneration::try_acquire().unwrap();
    for (switch, scene) in [first, second].into_iter().cycle().take(17).enumerate() {
        switch_scene_sync(app.world_mut().unwrap(), scene).unwrap();
        if scene == second {
            load_asset_sync::<ImageAsset>(app.world_mut().unwrap(), textures[1]).unwrap();
        }
        let mut texture_creates = 0;
        for frame in 0..6 {
            app.update_with_delta(Duration::ZERO).unwrap();
            let world = app.world();
            let packet = build_image_render_frame_packet(
                stream,
                (switch * 6 + frame) as u64,
                world.resource::<ExtractedViews>(),
                image,
            )
            .unwrap()
            .unwrap();
            let input = RenderInput::capture(world, packet);
            assert_eq!(input.sprite_batches().len(), 1);
            let (output, report) = renderer.render_to_image(&input).unwrap();
            drop(output);
            texture_creates += report.work.created_texture_calls;
            if frame > 0 {
                assert_eq!(report.work.created_texture_calls, 0);
                assert_eq!(report.work.created_buffer_calls, 0);
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while !renderer.poll_gpu().unwrap().is_queue_empty() {
                assert!(
                    Instant::now() < deadline,
                    "render submission did not finish"
                );
                std::thread::yield_now();
            }
        }
        assert!(
            texture_creates > 0,
            "the entering scene never uploaded its texture"
        );
        let report = renderer
            .context()
            .device()
            .generate_allocator_report()
            .expect("adapter did not report GPU allocations");
        let sampled_images = report
            .allocations
            .iter()
            .filter(|allocation| allocation.name == "nara_sampled_scene_image")
            .count();
        assert_eq!(
            sampled_images, 1,
            "a retired scene texture remained allocated"
        );
        let tile_chunks = report
            .allocations
            .iter()
            .filter(|allocation| allocation.name == "nara_tile_chunk_instances")
            .count();
        assert_eq!(
            tile_chunks,
            usize::from(scene == second),
            "unexpected retained tile buffers after returning to scene {scene}: {report:?}"
        );
    }
    assert_eq!(app.world().resource::<Assets<SceneAsset>>().len(), 2);
    assert_eq!(app.world().resource::<Assets<ImageAsset>>().len(), 2);
    app.finish().unwrap();
    println!(
        "{} ({:?}): 17 switches with one resident sampled image and zero steady-state allocations",
        adapter.name, adapter.backend
    );
}
