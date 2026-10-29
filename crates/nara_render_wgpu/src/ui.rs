use std::{collections::BTreeSet, sync::Arc};

use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderTarget, ViewportRect};
use nara_ui_render::{
    UiPacketFill, UiPacketPrimitive, UiPacketRect, UiPacketUv, UiPaintPacket, UiPaintPacketIdentity,
};
use thiserror::Error;

use crate::quad::{
    WGPU_QUAD_SHAPE_PLAIN, WGPU_QUAD_SHAPE_ROUNDED_BORDER, WGPU_QUAD_SHAPE_ROUNDED_FILL,
    WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture, WgpuScissorRect,
};

pub(crate) fn ui_packet_targets_view(
    packet: &UiPaintPacket,
    target: RenderTarget,
    primary_window_id: Option<nara_window::WindowId>,
) -> bool {
    crate::frame::render_targets_match(packet.target(), target, primary_window_id)
}

#[cfg(test)]
pub(crate) fn collect_ui_quad_batches(
    packets: &[Arc<UiPaintPacket>],
    target: RenderTarget,
    primary_window_id: Option<nara_window::WindowId>,
    viewport: ViewportRect,
) -> Result<Vec<WgpuQuadBatch>, WgpuUiPacketError> {
    Ok(lower_ui_packets(packets, target, primary_window_id, viewport)?.batches)
}

pub(crate) fn lower_ui_packets(
    packets: &[Arc<UiPaintPacket>],
    target: RenderTarget,
    primary_window_id: Option<nara_window::WindowId>,
    viewport: ViewportRect,
) -> Result<WgpuUiLowering, WgpuUiPacketError> {
    let mut batches = Vec::<WgpuQuadBatch>::new();
    let mut packet_facts = Vec::new();
    for packet in packets
        .iter()
        .filter(|packet| ui_packet_targets_view(packet, target, primary_window_id))
    {
        let mut facts = WgpuLoweredUiPacketFacts::new(packet);
        for primitive in packet.primitives() {
            append_primitive(&mut batches, &mut facts, packet, primitive, viewport)?;
        }
        packet_facts.push(facts);
    }
    Ok(WgpuUiLowering {
        batches,
        packet_facts,
    })
}

#[derive(Debug)]
pub(crate) struct WgpuUiLowering {
    pub(crate) batches: Vec<WgpuQuadBatch>,
    pub(crate) packet_facts: Vec<WgpuLoweredUiPacketFacts>,
}

#[derive(Debug)]
pub(crate) struct WgpuLoweredUiPacketFacts {
    pub(crate) packet: UiPaintPacketIdentity,
    pub(crate) target: RenderTarget,
    pub(crate) primitive_count: u64,
    pub(crate) image_count: u64,
    pub(crate) glyph_primitive_count: u64,
    pub(crate) glyph_page_count: u64,
    pub(crate) submitted_quad_count: u64,
    pub(crate) submitted_glyph_quad_count: u64,
    pub(crate) resources: BTreeSet<WgpuQuadMaterialKey>,
}

impl WgpuLoweredUiPacketFacts {
    fn new(packet: &UiPaintPacket) -> Self {
        Self {
            packet: packet.identity(),
            target: packet.target(),
            primitive_count: crate::saturating_u64(packet.primitives().len()),
            image_count: crate::saturating_u64(packet.images().len()),
            glyph_primitive_count: crate::saturating_u64(
                packet
                    .primitives()
                    .iter()
                    .filter(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
                    .count(),
            ),
            glyph_page_count: crate::saturating_u64(packet.glyph_pages().len()),
            submitted_quad_count: 0,
            submitted_glyph_quad_count: 0,
            resources: BTreeSet::new(),
        }
    }

    fn record_quad(&mut self, material: WgpuQuadMaterialKey, texture: &WgpuQuadTexture) {
        self.submitted_quad_count = self.submitted_quad_count.saturating_add(1);
        if matches!(texture, WgpuQuadTexture::Glyph(_)) {
            self.submitted_glyph_quad_count = self.submitted_glyph_quad_count.saturating_add(1);
        }
        self.resources.insert(material);
    }
}

fn append_primitive(
    batches: &mut Vec<WgpuQuadBatch>,
    facts: &mut WgpuLoweredUiPacketFacts,
    packet: &UiPaintPacket,
    primitive: &UiPacketPrimitive,
    viewport: ViewportRect,
) -> Result<(), WgpuUiPacketError> {
    let Some(scissor) = primitive_scissor(primitive.clip(), viewport) else {
        return Ok(());
    };
    match primitive {
        UiPacketPrimitive::Rect {
            rect,
            fill,
            border,
            corner_radius,
            ..
        } => {
            if let Some(fill) = fill {
                let (texture, color, uv) = match fill {
                    UiPacketFill::Solid { color } => (
                        WgpuQuadTexture::Fallback,
                        color.components(),
                        UiPacketUv::FULL,
                    ),
                    UiPacketFill::Image {
                        image_index,
                        tint,
                        uv,
                    } => {
                        let image = packet.images().get(*image_index as usize).ok_or(
                            WgpuUiPacketError::MissingImageResource {
                                index: *image_index,
                            },
                        )?;
                        (
                            WgpuQuadTexture::Image(image.snapshot().clone()),
                            tint.components(),
                            *uv,
                        )
                    }
                };
                if let Some(instance) = ui_quad_instance(
                    *rect,
                    color,
                    uv,
                    0,
                    WgpuUiShape::fill(*corner_radius),
                    viewport,
                ) {
                    append_quad(batches, facts, texture, scissor, instance);
                }
            }
            if let Some(border) = border.filter(|border| border.width() > 0.0)
                && let Some(instance) = ui_quad_instance(
                    *rect,
                    border.color().components(),
                    UiPacketUv::FULL,
                    0,
                    WgpuUiShape::border(*corner_radius, border.width()),
                    viewport,
                )
            {
                append_quad(batches, facts, WgpuQuadTexture::Fallback, scissor, instance);
            }
        }
        UiPacketPrimitive::Glyph {
            rect,
            page_index,
            color,
            uv,
            ..
        } => {
            let page = packet
                .glyph_pages()
                .get(usize::from(*page_index))
                .ok_or(WgpuUiPacketError::MissingGlyphPage { index: *page_index })?;
            if let Some(instance) = ui_quad_instance(
                *rect,
                color.components(),
                *uv,
                1,
                WgpuUiShape::plain(),
                viewport,
            ) {
                append_quad(
                    batches,
                    facts,
                    WgpuQuadTexture::Glyph(page.clone()),
                    scissor,
                    instance,
                );
            }
        }
    }
    Ok(())
}

fn append_quad(
    batches: &mut Vec<WgpuQuadBatch>,
    facts: &mut WgpuLoweredUiPacketFacts,
    texture: WgpuQuadTexture,
    scissor: Option<WgpuScissorRect>,
    instance: WgpuQuadInstance,
) {
    let material = WgpuQuadMaterialKey {
        texture: texture.key(),
        sampler: SamplerDescriptor::default(),
        alpha_mode: AlphaMode2d::Blend,
    };
    facts.record_quad(material, &texture);
    if let Some(batch) = batches.last_mut()
        && batch.material == material
        && batch.texture == texture
        && batch.scissor == scissor
    {
        batch.instances.push(instance);
        return;
    }
    batches.push(WgpuQuadBatch {
        material,
        texture,
        scissor,
        instances: vec![instance],
        counts_as_sprites: false,
        material_extension: None,
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WgpuUiShape {
    mode: u32,
    corner_radius: f32,
    border_width: f32,
}

impl WgpuUiShape {
    const fn plain() -> Self {
        Self {
            mode: WGPU_QUAD_SHAPE_PLAIN,
            corner_radius: 0.0,
            border_width: 0.0,
        }
    }

    const fn fill(corner_radius: f32) -> Self {
        Self {
            mode: if corner_radius == 0.0 {
                WGPU_QUAD_SHAPE_PLAIN
            } else {
                WGPU_QUAD_SHAPE_ROUNDED_FILL
            },
            corner_radius,
            border_width: 0.0,
        }
    }

    const fn border(corner_radius: f32, border_width: f32) -> Self {
        Self {
            mode: WGPU_QUAD_SHAPE_ROUNDED_BORDER,
            corner_radius,
            border_width,
        }
    }
}

fn ui_quad_instance(
    rect: UiPacketRect,
    color: [f32; 4],
    uv: nara_ui_render::UiPacketUv,
    texture_mode: u32,
    shape: WgpuUiShape,
    viewport: ViewportRect,
) -> Option<WgpuQuadInstance> {
    if viewport.physical_width == 0 || viewport.physical_height == 0 {
        return None;
    }
    let center_x = (rect.min_x() as f32 + rect.max_x() as f32) * 0.5;
    let center_y = (rect.min_y() as f32 + rect.max_y() as f32) * 0.5;
    let half_width = rect.width() as f32 * 0.5;
    let half_height = rect.height() as f32 * 0.5;
    let center_physical = [
        center_x + viewport.physical_x as f32,
        center_y + viewport.physical_y as f32,
    ];
    let center = nara_render::viewport_physical_to_clip(viewport, center_physical.into())?;
    let x_axis = nara_render::viewport_physical_to_clip(
        viewport,
        [center_physical[0] + half_width, center_physical[1]].into(),
    )? - center;
    let y_axis = nara_render::viewport_physical_to_clip(
        viewport,
        [center_physical[0], center_physical[1] + half_height].into(),
    )? - center;
    let top_left = uv.top_left();
    let extent = uv.extent();
    Some(WgpuQuadInstance {
        center: center.to_array(),
        x_axis: x_axis.to_array(),
        y_axis: y_axis.to_array(),
        color,
        // Packet UVs use a top-left origin while the shared quad shader maps top vertices to v=1.
        uv_min: [top_left[0], top_left[1] + extent[1]],
        uv_size: [extent[0], -extent[1]],
        texture_mode,
        shape_mode: shape.mode,
        rect_size: [rect.width() as f32, rect.height() as f32],
        corner_radius: shape.corner_radius,
        border_width: shape.border_width,
    })
}

fn scissor_rect(rect: UiPacketRect, viewport: ViewportRect) -> Option<WgpuScissorRect> {
    let left = rect.min_x().max(0) as u32;
    let top = rect.min_y().max(0) as u32;
    let right = (rect.max_x().max(0) as u32).min(viewport.physical_width);
    let bottom = (rect.max_y().max(0) as u32).min(viewport.physical_height);
    if right <= left || bottom <= top {
        return None;
    }
    Some(WgpuScissorRect {
        x: viewport.physical_x.saturating_add(left),
        y: viewport.physical_y.saturating_add(top),
        width: right - left,
        height: bottom - top,
    })
}

fn primitive_scissor(
    clip: Option<UiPacketRect>,
    viewport: ViewportRect,
) -> Option<Option<WgpuScissorRect>> {
    match clip {
        Some(clip) => scissor_rect(clip, viewport).map(Some),
        None => Some(None),
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum WgpuUiPacketError {
    #[error("UI paint packet references missing image resource index {index}")]
    MissingImageResource { index: u32 },
    #[error("UI paint packet references missing glyph-atlas page {index}")]
    MissingGlyphPage { index: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    use nara_asset::{AssetEvents, AssetStates, Assets};

    use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot,
    };
    use nara_text::{FontFace, bundled_font_face};
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiBorder, UiBrush, UiColor, UiCompiler, UiCornerRadii,
        UiEnvironment, UiImageId, UiLayer, UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle,
        UiSurfaceId, UiText, UiTheme, UiVisual,
    };
    use nara_window::WindowId;

    use crate::readback_tests::{
        READBACK_BYTES_PER_PIXEL, READBACK_EXTENT, readback_pixel, readback_viewport,
        render_batches_to_rgba,
    };

    #[test]
    fn packet_clip_outside_the_viewport_culls_the_primitive() {
        let viewport = ViewportRect::new(0, 0, 100, 100).unwrap();
        let clip = UiPacketRect::new(120, 120, 140, 140).unwrap();

        assert_eq!(primitive_scissor(Some(clip), viewport), None);
        assert_eq!(primitive_scissor(None, viewport), Some(None));
    }

    #[test]
    fn physical_ui_rect_projects_into_the_target_viewport() {
        let instance = ui_quad_instance(
            UiPacketRect::new(0, 0, 50, 25).unwrap(),
            [1.0; 4],
            nara_ui_render::UiPacketUv::FULL,
            0,
            WgpuUiShape::plain(),
            ViewportRect::new(10, 20, 100, 50).unwrap(),
        )
        .unwrap();

        assert_eq!(instance.center, [-0.5, 0.5]);
        assert_eq!(instance.x_axis, [0.5, 0.0]);
        assert_eq!(instance.y_axis, [0.0, -0.5]);
    }

    #[test]
    fn packet_uv_is_flipped_once_at_the_wgpu_ui_boundary() {
        let packet = text_packet();
        let uv = packet
            .primitives()
            .iter()
            .find_map(|primitive| match primitive {
                UiPacketPrimitive::Glyph { uv, .. } => Some(*uv),
                UiPacketPrimitive::Rect { .. } => None,
            })
            .expect("text packet should contain glyph UVs");
        let top_left = uv.top_left();
        let extent = uv.extent();
        let instance = ui_quad_instance(
            UiPacketRect::new(0, 0, 50, 25).unwrap(),
            [1.0; 4],
            uv,
            0,
            WgpuUiShape::plain(),
            ViewportRect::new(0, 0, 100, 50).unwrap(),
        )
        .unwrap();

        assert_eq!(instance.uv_min, [top_left[0], top_left[1] + extent[1]]);
        assert_eq!(instance.uv_size, [extent[0], -extent[1]]);
    }

    #[test]
    fn scissor_is_clamped_and_offset_by_the_viewport() {
        let scissor = scissor_rect(
            UiPacketRect::new(-5, 10, 40, 70).unwrap(),
            ViewportRect::new(10, 20, 100, 50).unwrap(),
        )
        .unwrap();

        assert_eq!(scissor.x, 10);
        assert_eq!(scissor.y, 30);
        assert_eq!(scissor.width, 40);
        assert_eq!(scissor.height, 40);
    }

    #[test]
    fn glyph_packet_renders_nonblank_gpu_pixels() {
        let packet = text_packet();
        let viewport = readback_viewport();
        // The Host route is explicit while the extracted camera keeps the primary-window alias.
        // Both names resolve to the same presentation target and must share one GPU path.
        let target = RenderTarget::PrimaryWindow;
        let mut batches =
            collect_ui_quad_batches(&[packet], target, Some(WindowId::PRIMARY), viewport).unwrap();
        batches.retain(|batch| matches!(&batch.texture, WgpuQuadTexture::Glyph(_)));
        assert!(
            !batches.is_empty(),
            "the UI packet should contain glyph work"
        );

        let pixels = render_batches_to_rgba(&batches).unwrap();
        let nonblank_pixels = pixels
            .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        assert!(nonblank_pixels > 0, "the rendered glyph must affect pixels");
        assert!(
            nonblank_pixels < (READBACK_EXTENT * READBACK_EXTENT) as usize,
            "the glyph must not fill the entire target"
        );
    }

    #[test]
    fn custom_face_glyph_packet_renders_nonblank_gpu_pixels() {
        let face = FontFace::from_bytes(
            include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let node = UiNode::new(UiStyle::absolute(4.0, 4.0, 56.0, 40.0))
            .with_key(UiLocalKey::runtime("wgpu-custom-font", "label").unwrap())
            .with_text(UiText::new("Nara", 20.0).unwrap());
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("wgpu-custom-font", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(node),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            READBACK_EXTENT as f32,
            READBACK_EXTENT as f32,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let frame = UiCompiler::new(face.clone(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let packet = nara_ui_render::UiPresentationPlanner::with_font(face, Default::default())
            .unwrap()
            .plan(frame, target, &Default::default())
            .unwrap();
        let mut batches = collect_ui_quad_batches(
            &[packet],
            target,
            Some(WindowId::PRIMARY),
            readback_viewport(),
        )
        .unwrap();
        batches.retain(|batch| matches!(&batch.texture, WgpuQuadTexture::Glyph(_)));
        assert!(!batches.is_empty());
        let pixels = render_batches_to_rgba(&batches).unwrap();
        let nonblank = pixels
            .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        assert!(nonblank > 0);
        assert!(nonblank < (READBACK_EXTENT * READBACK_EXTENT) as usize);
    }

    #[test]
    fn image_target_ui_packets_match_only_their_exact_target() {
        let first = RenderTarget::Image(nara_render::RenderImage2d::allocate_handle().unwrap());
        let second = RenderTarget::Image(nara_render::RenderImage2d::allocate_handle().unwrap());
        let packet = text_packet_for_target(first);
        let viewport = readback_viewport();

        assert!(
            !collect_ui_quad_batches(
                &[Arc::clone(&packet)],
                first,
                Some(WindowId::PRIMARY),
                viewport,
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            collect_ui_quad_batches(&[packet], second, Some(WindowId::PRIMARY), viewport,)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn image_packet_renders_the_captured_snapshot_pixels() {
        let packet = image_packet();
        let viewport = readback_viewport();
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let mut batches =
            collect_ui_quad_batches(&[packet], target, Some(WindowId::PRIMARY), viewport).unwrap();
        batches.retain(|batch| matches!(&batch.texture, WgpuQuadTexture::Image(_)));
        assert!(
            !batches.is_empty(),
            "the UI packet should contain image work"
        );

        let pixels = render_batches_to_rgba(&batches).unwrap();
        assert!(
            pixels
                .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
                .any(|pixel| pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 64 && pixel[2] > 80)
        );
    }

    #[test]
    fn rounded_fill_and_border_render_through_the_shared_quad_pipeline() {
        let packet = rounded_packet();
        let viewport = readback_viewport();
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let batches =
            collect_ui_quad_batches(&[packet], target, Some(WindowId::PRIMARY), viewport).unwrap();

        let pixels = render_batches_to_rgba(&batches).unwrap();
        let solid_corner = readback_pixel(&pixels, 4, 4);
        let border = readback_pixel(&pixels, 18, 5);
        let solid_interior = readback_pixel(&pixels, 18, 18);
        let image_corner = readback_pixel(&pixels, 36, 4);
        let image_interior = readback_pixel(&pixels, 48, 16);
        assert!(
            solid_corner[3] < 32,
            "the rounded solid corner must remain transparent"
        );
        assert!(
            border[3] > 200 && border[0] > border[1],
            "the border band must use the authored border color"
        );
        assert!(
            solid_interior[3] > 200 && solid_interior[1] > solid_interior[0],
            "the solid interior must retain the authored fill color"
        );
        assert!(
            image_corner[3] < 32,
            "the rounded image corner must remain transparent"
        );
        assert!(
            image_interior[3] > 200
                && image_interior[0] > 200
                && image_interior[1] < 64
                && image_interior[2] > 80,
            "the image interior must retain the captured snapshot color"
        );
    }

    fn text_packet() -> Arc<UiPaintPacket> {
        text_packet_for_target(RenderTarget::Window(WindowId::PRIMARY))
    }

    fn text_packet_for_target(target: RenderTarget) -> Arc<UiPaintPacket> {
        let node = UiNode::new(UiStyle::absolute(4.0, 4.0, 56.0, 40.0))
            .with_key(UiLocalKey::runtime("wgpu-glyph-readback", "label").unwrap())
            .with_text(UiText::new("Nara", 20.0).unwrap());
        let frame = compile_frame("wgpu-glyph-readback", node);
        nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, target, &nara_ui_render::UiPreparedImages::default())
            .unwrap()
    }

    fn image_packet() -> Arc<UiPaintPacket> {
        let image_id = UiImageId::new("project://ui/readback.png").unwrap();
        let node = UiNode::new(UiStyle::absolute(8.0, 8.0, 32.0, 32.0))
            .with_key(UiLocalKey::runtime("wgpu-image-readback", "image").unwrap())
            .with_visual(UiVisual::image(
                image_id.clone(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            ));
        let frame = compile_frame("wgpu-image-readback", node);
        let mut prepared = nara_ui_render::UiPreparedImages::default();
        prepared.insert(image_id, captured_image_snapshot());
        nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, RenderTarget::Window(WindowId::PRIMARY), &prepared)
            .unwrap()
    }

    fn rounded_packet() -> Arc<UiPaintPacket> {
        let solid = UiNode::new(UiStyle::absolute(4.0, 4.0, 28.0, 28.0))
            .with_key(UiLocalKey::runtime("wgpu-rounded-readback", "solid").unwrap())
            .with_visual(
                UiVisual::solid(UiColor::rgba(0.0, 1.0, 0.0, 1.0))
                    .with_border(
                        UiBorder::new(3.0, UiBrush::Solid(UiColor::rgba(1.0, 0.0, 0.0, 1.0)))
                            .unwrap(),
                    )
                    .with_radii(UiCornerRadii::all(8.0).unwrap()),
            );
        let image_id = UiImageId::new("project://ui/rounded-readback.png").unwrap();
        let image = UiNode::new(UiStyle::absolute(36.0, 4.0, 24.0, 24.0))
            .with_key(UiLocalKey::runtime("wgpu-rounded-readback", "image").unwrap())
            .with_visual(
                UiVisual::image(image_id.clone(), UiColor::rgba(1.0, 1.0, 1.0, 1.0))
                    .with_radii(UiCornerRadii::all(8.0).unwrap()),
            );
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("wgpu-rounded-readback", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(solid)
                .with_child(image),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            READBACK_EXTENT as f32,
            READBACK_EXTENT as f32,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        let mut prepared = nara_ui_render::UiPreparedImages::default();
        prepared.insert(image_id, captured_image_snapshot());
        nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, RenderTarget::Window(WindowId::PRIMARY), &prepared)
            .unwrap()
    }

    fn compile_frame(namespace: &str, node: UiNode) -> Arc<nara_ui_kernel::UiFrame> {
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime(namespace, "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(node),
            )
            .build()
            .unwrap();
        let environment = UiEnvironment::new(
            UiSurfaceId::new(1).unwrap(),
            READBACK_EXTENT as f32,
            READBACK_EXTENT as f32,
            1.0,
            1,
            1,
            1,
            1,
        )
        .unwrap();
        UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap()
    }

    fn captured_image_snapshot() -> PreparedImageSnapshot {
        let image = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![240, 16, 120, 255],
        )
        .unwrap();
        let handle = nara_asset::allocate_anonymous_handle().unwrap();
        let mut images = Assets::<ImageAsset>::default();
        let mut states = AssetStates::default();
        images
            .commit_loaded(
                handle,
                image,
                &mut states,
                &mut AssetEvents::default(),
                None,
                None,
            )
            .unwrap();
        PreparedImageSnapshot::capture(handle, &images, &states).unwrap()
    }
}
