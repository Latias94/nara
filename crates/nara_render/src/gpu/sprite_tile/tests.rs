use std::{collections::BTreeMap, sync::Arc};

use nara_core::{Color, Mat3, Vec2};

use crate::material::{
    AlphaMode2d, Material2dDeclaration, Material2dGeneration, MaterialParameters,
    SamplerDescriptor, Vec4,
};
use crate::sprite_render::SpriteTileChunk;
use crate::sprite_render::batching::{
    PreparedMaterial2d, SpriteBatchKind, SpriteInstance, SpriteMaterialKey,
};
use crate::tilemap::{DirtyTileChunk, TileCoord};
use crate::{RenderTarget, RenderViewId, ViewMapping2d};

use super::*;

#[test]
fn interleaved_batches_keep_painter_order_and_share_a_chunk_buffer_range() {
    let views = [
        view(
            RenderTarget::PrimaryWindow,
            0,
            0,
            ViewLoadOp::Clear(Color::BLACK),
        ),
        view(RenderTarget::PrimaryWindow, 1, 4, ViewLoadOp::Load),
    ];
    let chunk = tile_chunk(&views[0], 0, 1);
    let batches = [
        batch(&views[0], 0, Color::WHITE, Some(chunk)),
        batch(&views[1], 1, Color::WHITE, None),
        batch(&views[0], 0, Color::BLACK, None),
        batch(&views[0], 0, Color::BLACK, Some(chunk)),
    ];
    let geometry = FrameGeometry::collect(&views, &batches).unwrap();
    let first = &geometry.views[0].batches;
    assert!(std::ptr::eq(first[0].batch, &batches[0]));
    assert!(std::ptr::eq(first[1].batch, &batches[2]));
    assert!(std::ptr::eq(first[2].batch, &batches[3]));
    assert_eq!(first[0].instances, 0..1);
    assert_eq!(first[1].instances, 1..2);
    assert_eq!(first[2].instances, 1..2);
    assert_eq!(geometry.views[1].batches[0].instances, 0..1);
    assert_eq!(geometry.chunks[&chunk].len(), 2);
    assert_eq!(geometry.dynamic.len(), 2);

    let mut foreign_chunk = batches[0].clone();
    foreign_chunk.view_index = 1;
    foreign_chunk.view_order = views[1].order;
    assert!(matches!(
        FrameGeometry::collect(&views, &[foreign_chunk]),
        Err(WgpuRenderError::InvalidTarget(_))
    ));
}

#[test]
fn material_uniform_upload_shares_exact_parameters_but_preserves_each_view() {
    let views = [
        view(
            RenderTarget::PrimaryWindow,
            0,
            0,
            ViewLoadOp::Clear(Color::BLACK),
        ),
        view(RenderTarget::PrimaryWindow, 1, 4, ViewLoadOp::Load),
    ];
    let declaration = Material2dDeclaration::from_parameters::<Tint>(
        "test.gpu-tint",
        1,
        AlphaMode2d::Blend,
        vec![],
    )
    .unwrap();
    let generation = Material2dGeneration::from_shader_source(
        declaration,
        &tint_shader("material_parameters.rgba"),
    )
    .unwrap();
    let binding = generation
        .bind(
            &Tint {
                rgba: Vec4::new(1.0, 0.25, 0.5, 0.75),
            },
            BTreeMap::new(),
        )
        .unwrap();
    let material = Arc::new(PreparedMaterial2d {
        binding,
        textures: Vec::new(),
    });
    let mut batches = [
        batch(&views[0], 0, Color::WHITE, None),
        batch(&views[1], 1, Color::WHITE, None),
    ];
    for batch in &mut batches {
        batch.material_extension = Some(Arc::clone(&material));
    }
    let mut geometry = FrameGeometry::collect(&views, &batches).unwrap();
    let bytes = pack_uniforms(&mut geometry, &views, None, 256).unwrap();
    let [first, second] = &geometry.views[..] else {
        panic!("two test views")
    };
    assert_eq!(first.uniform_offset, Some(0));
    assert_eq!(first.batches[0].uniform_offset, Some(256));
    assert_eq!(second.uniform_offset, Some(512));
    assert_eq!(second.batches[0].uniform_offset, Some(256));
    let uniform_bytes = material.binding.uniform_bytes();
    assert_eq!(&bytes[256..256 + uniform_bytes.len()], uniform_bytes);
    assert_eq!(
        &bytes[512..],
        bytemuck::bytes_of(&WgpuViewUniform::capture(&views[1], None)),
    );
}

#[derive(MaterialParameters)]
#[material(crate = crate::material)]
struct Tint {
    #[material(id = "color")]
    rgba: Vec4,
}

fn tint_shader(color: &str) -> String {
    format!(
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {{ return {color}; }}"
    )
}

fn view(target: RenderTarget, order: i32, x: u32, load: ViewLoadOp) -> ExtractedView {
    let viewport = ViewportRect::new(x, 0, 4, 4).unwrap();
    ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: nara_ecs::Entity::PLACEHOLDER,
        target,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
        order,
        load,
    }
}

fn tile_chunk(view: &ExtractedView, x: i32, revision: u64) -> SpriteTileChunk {
    SpriteTileChunk {
        view: view.id,
        entity: nara_ecs::Entity::PLACEHOLDER,
        change: DirtyTileChunk {
            coord: TileCoord::new(x, 0).chunk(),
            revision,
        },
    }
}

fn batch(
    view: &ExtractedView,
    index: usize,
    color: Color,
    tile_chunk: Option<SpriteTileChunk>,
) -> SpriteBatch {
    SpriteBatch {
        view_index: index,
        view_order: view.order,
        target: view.target,
        kind: if tile_chunk.is_some() {
            SpriteBatchKind::Tilemap
        } else {
            SpriteBatchKind::Sprite
        },
        layer: 0,
        sort_key: 0,
        material: SpriteMaterialKey {
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Blend,
        },
        image: None,
        material_extension: None,
        instances: vec![SpriteInstance::axis_aligned(Vec2::ZERO, Vec2::ONE, color)],
        tile_chunk,
    }
}

#[cfg(not(target_family = "wasm"))]
mod native;
