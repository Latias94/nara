use super::*;
use nara_render::ExtractedViews;
use nara_sprite_render::__private::ExtractedSprites;

#[test]
fn per_view_culling_matches_unclipped_runtime_geometry_pixels() {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    let mut roots = Vec::new();
    for (position, rotation) in [(Vec2::ZERO, 0.0), (Vec2::new(24.0, 7.0), 0.37)] {
        roots.push(
            world
                .spawn((
                    Camera2d {
                        target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                        viewport: Some(ViewportRect::new(0, 0, 64, 64).unwrap()),
                        viewport_height: 64.0,
                        ..Default::default()
                    },
                    Transform2d {
                        translation: position,
                        rotation,
                        ..Default::default()
                    },
                ))
                .id(),
        );
    }
    // Nonuniform parent scale plus child rotation produces a sheared global quad.
    let parent = world
        .spawn(Transform2d {
            scale: Vec2::new(-1.7, 0.8),
            rotation: 0.23,
            ..Default::default()
        })
        .id();
    roots.push(parent);
    let sheared = world
        .spawn((
            Sprite::from_color(Vec2::new(17.0, 9.0), Color::rgba(0.0, 0.0, 1.0, 0.5)).with_layer(2),
            Transform2d {
                rotation: 0.71,
                ..Default::default()
            },
        ))
        .id();
    for x in [-100.0, -33.0, -31.0, -3.0, 31.0, 33.0, 100.0] {
        roots.push(
            world
                .spawn((
                    Sprite::from_color(Vec2::new(5.0, 23.0), Color::rgba(1.0, 0.0, 0.0, 0.5)),
                    Transform2d {
                        translation: Vec2::new(x, 3.0),
                        rotation: 0.4,
                        scale: Vec2::new(-1.0, 1.0),
                    },
                ))
                .id(),
        );
        let mut map = Tilemap::new(Vec2::new(4.0, 5.0)).with_layer(1);
        map.set_cell(
            TileCoord::new(0, 0),
            TileCell::new(TileIndex::new(0)).with_color(Color::rgba(0.0, 1.0, 0.0, 0.5)),
        );
        roots.push(
            world
                .spawn((map, Transform2d::from_translation(Vec2::new(x, -8.0))))
                .id(),
        );
    }
    let operations = roots
        .into_iter()
        .map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        })
        .chain([HierarchyOperation::Place {
            entity: sheared,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
        }])
        .collect::<Vec<_>>();
    HierarchyWriter::new(world)
        .apply_batch(&operations)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let batches = runtime.world().resource::<SpriteBatches>().clone();
    let views = runtime.world().resource::<ExtractedViews>();
    let mut extracted = runtime
        .world()
        .resource::<ExtractedSprites>()
        .as_slice()
        .to_vec();
    extracted.sort_by(|a, b| {
        (a.layer, a.sort_key)
            .cmp(&(b.layer, b.sort_key))
            .then_with(|| b.world_sort_y.total_cmp(&a.world_sort_y))
            .then_with(|| a.source_order.cmp(&b.source_order))
    });
    let comparisons = views
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, view)| {
            let clipped = collect_sprite_quad_batches(&batches, index);
            // This oracle projects every extracted quad without the queue visibility predicate.
            let reference = extracted
                .iter()
                .map(|sprite| {
                    let mut batch = WgpuQuadBatch {
                        material: crate::quad::WgpuQuadMaterialKey {
                            texture: crate::quad::WgpuQuadTextureKey::Fallback,
                            sampler: sprite.material.sampler,
                            alpha_mode: sprite.material.alpha_mode,
                        },
                        texture: crate::quad::WgpuQuadTexture::Fallback,
                        scissor: None,
                        instances: Vec::new(),
                        counts_as_sprites: true,
                        material_extension: None,
                    };
                    batch.instances.push(crate::quad::WgpuQuadInstance {
                        center: view.mapping.world_to_clip(sprite.world_center).to_array(),
                        x_axis: view
                            .mapping
                            .world_vector_to_clip(sprite.world_x_axis)
                            .to_array(),
                        y_axis: view
                            .mapping
                            .world_vector_to_clip(sprite.world_y_axis)
                            .to_array(),
                        color: [
                            sprite.color.r,
                            sprite.color.g,
                            sprite.color.b,
                            sprite.color.a,
                        ],
                        uv_min: sprite.texture_region.min.to_array(),
                        uv_size: sprite.texture_region.size.to_array(),
                        texture_mode: 0,
                        shape_mode: crate::quad::WGPU_QUAD_SHAPE_PLAIN,
                        rect_size: [0.0; 2],
                        corner_radius: 0.0,
                        border_width: 0.0,
                    });
                    batch
                })
                .collect::<Vec<_>>();
            assert!(
                clipped
                    .iter()
                    .map(|batch| batch.instances.len())
                    .sum::<usize>()
                    < extracted.len()
            );
            (clipped, reference)
        })
        .collect::<Vec<_>>();
    // The captured geometry remains valid after the Runtime and its ECS storage retire.
    runtime.finish().unwrap();
    for (clipped, reference) in comparisons {
        let expected = render_batches_to_rgba(&reference).unwrap();
        assert!(expected.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let actual = render_batches_to_rgba(&clipped).unwrap();
        assert_eq!(
            actual, expected,
            "culling must preserve every rasterized pixel"
        );
    }
}
