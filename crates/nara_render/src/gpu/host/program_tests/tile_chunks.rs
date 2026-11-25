use super::{drain, pixels};
use crate::gpu::{
    WgpuPolicyDefinition, WgpuRenderHost, WgpuStockCapture, WgpuStockPolicy, WgpuWorldColor,
    quad::quad_instance_byte_len,
};
use crate::image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};
use crate::material::{AlphaMode2d, SamplerDescriptor};
use crate::sprite_render::batching::SpriteBatches;
use crate::tilemap::{TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSet, Tilemap};
use crate::{
    Camera2d, Extent2d, ExtractedViews, RenderFrame, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, ViewportRect, build_image_render_frame_packet,
};
use nara_app::{App, CoreStage, RealTime, RuntimeInstance};
use nara_asset::{AssetEvents, AssetStates, Assets, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::{Query, Res, ResMut};
use nara_scene::spatial::Transform2d;
use std::{sync::Arc, time::Duration};

const BLACK: [u8; 4] = [0, 0, 0, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_stock_tile_chunks_upload_only_changed_instances() {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_scene::spatial::SpatialPlugin,
        crate::RenderComponentsPlugin,
        crate::RenderPlugin::default(),
        crate::image::ImagePreparePlugin,
        crate::sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let image = nara_asset::allocate_anonymous_handle().unwrap();
    publish_atlas(&mut images, &mut states, image, [RED, GREEN, BLUE, BLUE]);
    app.insert_resource(images).unwrap();
    app.insert_resource(states).unwrap();

    let mut tilesets = Assets::default();
    let tileset = nara_asset::allocate_anonymous_handle().unwrap();
    tilesets.insert(
        tileset,
        atlas(image, TileAtlasLayout::grid(Vec2::ONE, 2, 2)),
    );
    app.insert_resource(tilesets).unwrap();

    let image_target = RenderImage2d::allocate_handle().unwrap();
    let target = RenderImagePacket::new(image_target, Extent2d::new(64, 64).unwrap()).unwrap();
    let world = app.world_mut().unwrap();
    world.spawn((
        Camera2d {
            target: RenderTarget::Image(image_target),
            viewport: Some(ViewportRect::new(0, 0, 64, 64).unwrap()),
            viewport_height: 64.0,
            clear_color: Some(Color::BLACK),
            ..Default::default()
        },
        Transform2d::default(),
    ));
    let lower_cells = [TileCoord::new(0, 31), TileCoord::new(1, 31)];
    let upper_cells = [TileCoord::new(0, 32), TileCoord::new(1, 32)];
    let mut tilemap = Tilemap::new(Vec2::splat(4.0)).with_tileset(tileset);
    for (lower, upper) in lower_cells.into_iter().zip(upper_cells) {
        tilemap.set_cell(lower, TileCell::new(TileIndex::new(0)));
        tilemap.set_cell(upper, TileCell::new(TileIndex::new(1)));
    }
    let tilemap = world
        .spawn((
            tilemap,
            Transform2d::from_translation(Vec2::new(-16.0, -128.0)),
        ))
        .id();
    app.add_systems(
        CoreStage::Update,
        move |time: Res<RealTime>,
              mut maps: Query<&mut Tilemap>,
              mut images: ResMut<Assets<ImageAsset>>,
              mut states: ResMut<AssetStates>,
              mut tilesets: ResMut<Assets<TileSet>>| {
            let mut map = maps.get_mut(tilemap).unwrap();
            match time.frame {
                3 => {
                    map.set_cell(lower_cells[0], TileCell::new(TileIndex::new(1)));
                }
                4 => {
                    for coord in lower_cells {
                        map.remove_cell(coord);
                    }
                }
                5 => publish_atlas(&mut images, &mut states, image, [RED, YELLOW, BLUE, BLUE]),
                6 => {
                    *tilesets.get_mut(tileset).unwrap() =
                        atlas(image, TileAtlasLayout::grid(Vec2::new(2.0, 1.0), 1, 2));
                }
                _ => {}
            }
            map.clear_dirty_chunks();
        },
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let source = RenderFrameSourceGeneration::from(runtime.generation());
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let definition = WgpuPolicyDefinition::new(WgpuStockPolicy::new());
    host.prepare_policy(&producer, &definition).unwrap();
    let installation = definition.install().unwrap();
    let reservation = host
        .reserve_image_target(&producer, source, target)
        .unwrap();
    host.activate_image_target(&producer, reservation).unwrap();

    for (index, (colors, draws, chunk_initializations, uploaded_instances, texture_uploads)) in [
        ([RED, GREEN, RED, GREEN], 4, 2, 4, 1),
        ([RED, GREEN, RED, GREEN], 4, 0, 0, 0),
        ([GREEN, GREEN, RED, GREEN], 4, 1, 2, 0),
        ([BLACK, GREEN, BLACK, GREEN], 1, 0, 0, 0),
        ([BLACK, YELLOW, BLACK, YELLOW], 1, 0, 0, 1),
        ([BLACK, BLUE, BLACK, BLUE], 1, 1, 2, 0),
    ]
    .into_iter()
    .enumerate()
    {
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        let packet = build_image_render_frame_packet(
            source,
            world.resource::<RenderFrame>().index,
            world.resource::<ExtractedViews>(),
            target,
        )
        .unwrap()
        .unwrap();
        let batches = world.resource::<SpriteBatches>();
        assert_eq!(batches.len(), draws);
        let input = installation
            .capture(
                packet,
                Arc::new(WgpuStockCapture::new().with_sprites(batches.clone())),
            )
            .unwrap();
        let frame = index as u64 + 1;
        host.begin_frame(frame).unwrap();
        let output = host.submit(&producer, &input).unwrap();
        let work = host.frame_transaction_stats().native_work();
        assert_eq!(work.created_buffer_calls, chunk_initializations);
        assert_eq!(
            work.created_buffer_bytes,
            quad_instance_byte_len(uploaded_instances)
        );
        assert_eq!(
            work.initialized_buffer_calls, chunk_initializations,
            "frame {frame}"
        );
        assert_eq!(
            work.initialized_buffer_payload_bytes,
            quad_instance_byte_len(uploaded_instances)
        );
        assert_eq!(work.issued_buffer_write_calls, 0);
        assert_eq!(work.issued_texture_write_calls, texture_uploads);

        let readback = pixels(&mut host, &output.texture::<WgpuWorldColor>().unwrap());
        for ((x, y), expected) in [(18, 34), (18, 30), (22, 34), (22, 30)]
            .into_iter()
            .zip(colors)
        {
            assert_eq!(
                readback[y * 64 + x],
                expected,
                "frame {frame}, pixel ({x}, {y})"
            );
        }
    }
    drain(&host.retire_producer(&producer).unwrap());
    runtime.finish().unwrap();
}

fn atlas(image: Handle<ImageAsset>, layout: TileAtlasLayout) -> TileSet {
    TileSet::from_image(image, layout)
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_alpha_mode(AlphaMode2d::Opaque)
}

fn publish_atlas(
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    handle: Handle<ImageAsset>,
    pixels: [[u8; 4]; 4],
) {
    let image = ImageAsset::new(
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels.into_iter().flatten().collect(),
    )
    .unwrap();
    images
        .commit_loaded(
            handle,
            image,
            states,
            &mut AssetEvents::default(),
            None,
            None,
        )
        .unwrap();
}
