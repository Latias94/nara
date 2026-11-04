use std::{collections::BTreeSet, marker::PhantomData, sync::Arc};

use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{RenderTarget, ViewportRect};
use nara_ui_render::{
    UiPacketFill, UiPacketPrimitive, UiPacketRect, UiPacketUv, UiPaintPacket, UiPaintPacketIdentity,
};
use thiserror::Error;

mod cost;
mod mesh;
#[cfg(test)]
pub(crate) mod mesh_tests;

pub use mesh::UiState;
use mesh::{MeshPrepared, MeshVertex, UiDraw};

use crate::{
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuRenderError, WgpuRenderFeature, WgpuRenderProgramError,
    quad::{
        WGPU_QUAD_SHAPE_PLAIN, WGPU_QUAD_SHAPE_ROUNDED_BORDER, WGPU_QUAD_SHAPE_ROUNDED_FILL,
        WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture, WgpuScissorRect,
    },
    stock_quad::StockQuadPrepared,
};

/// Replay packets can outnumber new permits. Each actual permit still needs exactly one
/// original packet/target fact, including a packet whose drawing was completely clipped.
pub(crate) fn validate_submission_packets(
    submissions: &[nara_ui_render::__private::UiSubmittedBackendSubmission],
    packets: &[WgpuLoweredUiPacketFacts],
) -> Result<(), WgpuRenderError> {
    for submission in submissions {
        let identity = submission.submission().identity();
        let mut matching = packets.iter().filter(|facts| {
            facts.packet == identity.packet()
                && facts.target == identity.target()
                && Arc::ptr_eq(&facts.original_packet, submission.submission().packet())
        });
        if matching.next().is_none() || matching.next().is_some() {
            return Err(WgpuRenderError::UiSubmissionRejected {
                message: format!(
                    "UI backend attempt {} requires one exact packet for its original logical target",
                    identity.admission_id().get(),
                ),
            });
        }
    }
    Ok(())
}

/// Stock UI drawing against the policy's exact semantic texture port.
pub struct UiFeature<S: 'static>(PhantomData<fn() -> S>);

impl<S: 'static> UiFeature<S> {
    /// Creates the UI adapter for a policy-owned semantic texture port.
    #[must_use]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S: 'static> Default for UiFeature<S> {
    fn default() -> Self {
        Self::new()
    }
}

/// Drawing and exact packet facts, independent of one-shot backend submission permits.
pub struct UiCapture<S: 'static> {
    pub(crate) target: WgpuProgramTexture<S>,
    pub(crate) viewport: ViewportRect,
    pub(crate) batches: Arc<[WgpuQuadBatch]>,
    pub(crate) draws: Arc<[UiDraw]>,
    pub(crate) mesh_vertices: Arc<[MeshVertex]>,
    pub(crate) mesh_indices: Arc<[u32]>,
    pub(crate) packet_facts: Arc<[WgpuLoweredUiPacketFacts]>,
}

impl<S: 'static> UiCapture<S> {
    /// Lowers packets matching the original logical target into this candidate's texture.
    /// Primary-window aliases resolve through the supplied original window identity.
    /// One operation admits at most 65,536 mesh vertices and 196,608 triangle indices across
    /// all matching packets, including repeated packets and geometry clipped by this viewport.
    pub fn new(
        packets: &[Arc<UiPaintPacket>],
        logical_target: RenderTarget,
        primary_window_id: Option<nara_window::WindowId>,
        target: WgpuProgramTexture<S>,
        viewport: ViewportRect,
    ) -> Result<Self, WgpuRenderProgramError> {
        let lowered = lower_ui_packets(packets, logical_target, primary_window_id, viewport)
            .map_err(|error| WgpuRenderError::UiPacketInvariant {
                message: error.to_string(),
            })?;
        Ok(Self {
            target,
            viewport,
            batches: lowered.batches.into(),
            draws: lowered.draws.into(),
            mesh_vertices: lowered.mesh_vertices.into(),
            mesh_indices: lowered.mesh_indices.into(),
            packet_facts: lowered.packet_facts.into(),
        })
    }

    /// Observes original packet identities and logical targets in draw order, including
    /// replayed packets and packets whose drawing was completely clipped.
    pub fn packets(
        &self,
    ) -> impl ExactSizeIterator<Item = (UiPaintPacketIdentity, RenderTarget)> + '_ {
        self.packet_facts
            .iter()
            .map(|facts| (facts.packet, facts.target))
    }
}

/// Opaque epoch-local drawing retained by the UI adapter through native completion.
pub struct UiPrepared<S: 'static> {
    target: WgpuProgramTexture<S>,
    viewport: ViewportRect,
    draw: StockQuadPrepared,
    mesh: Option<MeshPrepared>,
    draws: Arc<[UiDraw]>,
    mesh_count: u32,
    mesh_vertices: u64,
    mesh_triangles: u64,
    packet_facts: Arc<[WgpuLoweredUiPacketFacts]>,
}

/// Actual UI encoding observations, retaining the original capture's packet-facts allocation.
/// Presentation bindings validate that allocation's identity before using these observations.
#[derive(Clone, Debug, Default)]
pub struct UiReport {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) image_sprites: u32,
    pub(crate) meshes: u32,
    pub(crate) mesh_vertices: u64,
    pub(crate) mesh_triangles: u64,
    pub(crate) packet_facts: Arc<[WgpuLoweredUiPacketFacts]>,
}

impl UiReport {
    /// Number of encoded UI draw calls.
    #[must_use]
    pub const fn draw_calls(&self) -> u32 {
        self.draw_calls
    }

    /// Number of encoded quads counted as sprites by the shared drawing path.
    #[must_use]
    pub const fn sprites(&self) -> u32 {
        self.sprites
    }

    /// Number of encoded image quads counted as sprites by the shared drawing path.
    #[must_use]
    pub const fn image_sprites(&self) -> u32 {
        self.image_sprites
    }

    /// Number of encoded indexed triangle draws, excluding completely clipped meshes.
    #[must_use]
    pub const fn meshes(&self) -> u32 {
        self.meshes
    }

    /// Number of captured vertices submitted by mesh draws, counting repeated occurrences.
    #[must_use]
    pub const fn mesh_vertices(&self) -> u64 {
        self.mesh_vertices
    }

    /// Number of submitted indexed triangles; clipping may remove their raster coverage.
    #[must_use]
    pub const fn mesh_triangles(&self) -> u64 {
        self.mesh_triangles
    }

    /// Observes the capture's original packet identities and logical targets, including
    /// replayed packets and packets whose drawing was completely clipped.
    pub fn packets(
        &self,
    ) -> impl ExactSizeIterator<Item = (UiPaintPacketIdentity, RenderTarget)> + '_ {
        self.packet_facts
            .iter()
            .map(|facts| (facts.packet, facts.target))
    }
}

impl<S: 'static> WgpuRenderFeature for UiFeature<S> {
    type Capture = UiCapture<S>;
    type State = UiState;
    type Prepared = UiPrepared<S>;
    type Report = UiReport;

    fn preflight(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        context.collect_stock_images(&capture.batches)?;
        cost::preflight(capture, context)
    }

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(UiState::default())
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let draw = context
            .prepare_stock_draw(&capture.batches, capture.target.descriptor().native.format)?;
        let mesh = MeshPrepared::create(
            state,
            &capture.mesh_vertices,
            &capture.mesh_indices,
            capture.target.descriptor().native.format,
            context,
        )?;
        Ok(UiPrepared {
            target: capture.target.clone(),
            viewport: capture.viewport,
            draw,
            mesh,
            draws: Arc::clone(&capture.draws),
            mesh_count: capture.mesh_count(),
            mesh_vertices: crate::saturating_u64(capture.mesh_vertices.len()),
            mesh_triangles: crate::saturating_u64(capture.mesh_indices.len() / 3),
            packet_facts: Arc::clone(&capture.packet_facts),
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let target = context.resources().texture(&prepared.target)?;
        let mut pass = context
            .encoder()
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nara_ui_feature"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        let viewport = prepared.viewport;
        pass.set_viewport(
            viewport.physical_x as f32,
            viewport.physical_y as f32,
            viewport.physical_width as f32,
            viewport.physical_height as f32,
            0.0,
            1.0,
        );
        for draw in prepared.draws.iter() {
            match draw {
                UiDraw::Quads(range) => crate::quad::draw_quad_batch_buffers(
                    &mut pass,
                    &prepared.draw.pipelines,
                    &prepared.draw.buffers[range.clone()],
                    viewport,
                ),
                UiDraw::Mesh { indices, scissor } => {
                    let mesh = prepared
                        .mesh
                        .as_ref()
                        .ok_or(WgpuRenderProgramError::FeatureState)?;
                    mesh.encode(&mut pass, indices.clone(), *scissor);
                }
            }
        }
        drop(pass);
        Ok(UiReport {
            draw_calls: prepared.draw.draw_calls.saturating_add(prepared.mesh_count),
            sprites: prepared.draw.sprites,
            image_sprites: prepared.draw.image_sprites,
            meshes: prepared.mesh_count,
            mesh_vertices: prepared.mesh_vertices,
            mesh_triangles: prepared.mesh_triangles,
            packet_facts: Arc::clone(&prepared.packet_facts),
        })
    }
}

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
    let lowered = lower_ui_packets(packets, target, primary_window_id, viewport)?;
    lowered.require_quad_only()?;
    Ok(lowered.batches)
}

pub(crate) fn lower_ui_packets(
    packets: &[Arc<UiPaintPacket>],
    target: RenderTarget,
    primary_window_id: Option<nara_window::WindowId>,
    viewport: ViewportRect,
) -> Result<WgpuUiLowering, WgpuUiPacketError> {
    mesh::preflight_packets(
        packets
            .iter()
            .filter(|packet| ui_packet_targets_view(packet, target, primary_window_id)),
    )?;
    let mut lowered = WgpuUiLowering::default();
    let mut packet_facts = Vec::new();
    for packet in packets
        .iter()
        .filter(|packet| ui_packet_targets_view(packet, target, primary_window_id))
    {
        let mut facts = WgpuLoweredUiPacketFacts::new(packet);
        for primitive in packet.primitives() {
            append_primitive(&mut lowered, &mut facts, packet, primitive, viewport)?;
        }
        packet_facts.push(facts);
    }
    lowered.packet_facts = packet_facts;
    Ok(lowered)
}

#[derive(Debug, Default)]
pub(crate) struct WgpuUiLowering {
    pub(crate) batches: Vec<WgpuQuadBatch>,
    pub(crate) draws: Vec<UiDraw>,
    mesh_vertices: Vec<MeshVertex>,
    mesh_indices: Vec<u32>,
    merge_quad: bool,
    pub(crate) packet_facts: Vec<WgpuLoweredUiPacketFacts>,
}

impl WgpuUiLowering {
    /// Legacy numerical quad oracles must refuse geometry instead of silently dropping it.
    #[cfg(test)]
    pub(crate) fn require_quad_only(&self) -> Result<(), WgpuUiPacketError> {
        if self
            .packet_facts
            .iter()
            .any(|facts| facts.mesh_primitive_count != 0)
        {
            return Err(WgpuUiPacketError::LegacyMeshUnsupported);
        }
        Ok(())
    }
}

impl<S: 'static> UiCapture<S> {
    pub(crate) fn mesh_count(&self) -> u32 {
        u32::try_from(
            self.draws
                .iter()
                .filter(|draw| matches!(draw, UiDraw::Mesh { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WgpuLoweredUiPacketFacts {
    original_packet: Arc<UiPaintPacket>,
    pub(crate) packet: UiPaintPacketIdentity,
    pub(crate) target: RenderTarget,
    pub(crate) primitive_count: u64,
    pub(crate) image_count: u64,
    pub(crate) glyph_primitive_count: u64,
    pub(crate) glyph_page_count: u64,
    pub(crate) submitted_quad_count: u64,
    pub(crate) submitted_glyph_quad_count: u64,
    pub(crate) mesh_primitive_count: u64,
    pub(crate) submitted_mesh_count: u64,
    pub(crate) submitted_mesh_vertex_count: u64,
    pub(crate) submitted_mesh_triangle_count: u64,
    pub(crate) resources: BTreeSet<WgpuQuadMaterialKey>,
}

impl WgpuLoweredUiPacketFacts {
    fn new(packet: &Arc<UiPaintPacket>) -> Self {
        Self {
            original_packet: Arc::clone(packet),
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
            mesh_primitive_count: crate::saturating_u64(
                packet
                    .primitives()
                    .iter()
                    .filter(|primitive| matches!(primitive, UiPacketPrimitive::Mesh { .. }))
                    .count(),
            ),
            submitted_mesh_count: 0,
            submitted_mesh_vertex_count: 0,
            submitted_mesh_triangle_count: 0,
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
    lowered: &mut WgpuUiLowering,
    facts: &mut WgpuLoweredUiPacketFacts,
    packet: &UiPaintPacket,
    primitive: &UiPacketPrimitive,
    viewport: ViewportRect,
) -> Result<(), WgpuUiPacketError> {
    if let UiPacketPrimitive::Mesh { rect, clip, mesh } = primitive {
        lowered.merge_quad = false;
        if mesh::append_mesh(
            &mut lowered.mesh_vertices,
            &mut lowered.mesh_indices,
            &mut lowered.draws,
            mesh,
            *rect,
            *clip,
            viewport,
        )? {
            facts.submitted_mesh_count = facts.submitted_mesh_count.saturating_add(1);
            facts.submitted_mesh_vertex_count = facts
                .submitted_mesh_vertex_count
                .saturating_add(crate::saturating_u64(mesh.vertices().len()));
            facts.submitted_mesh_triangle_count = facts
                .submitted_mesh_triangle_count
                .saturating_add(crate::saturating_u64(mesh.indices().len() / 3));
        }
        return Ok(());
    }
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
                    append_quad(lowered, facts, texture, scissor, instance);
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
                append_quad(lowered, facts, WgpuQuadTexture::Fallback, scissor, instance);
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
                    lowered,
                    facts,
                    WgpuQuadTexture::Glyph(page.clone()),
                    scissor,
                    instance,
                );
            }
        }
        UiPacketPrimitive::Mesh { .. } => unreachable!("meshes are lowered before quad clipping"),
    }
    Ok(())
}

fn append_quad(
    lowered: &mut WgpuUiLowering,
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
    if lowered.merge_quad
        && let Some(batch) = lowered.batches.last_mut()
        && batch.material == material
        && batch.texture == texture
        && batch.scissor == scissor
    {
        batch.instances.push(instance);
        return;
    }
    lowered.batches.push(WgpuQuadBatch {
        material,
        texture,
        scissor,
        instances: vec![instance],
        counts_as_sprites: false,
        material_extension: None,
    });
    let end = lowered.batches.len();
    if let Some(UiDraw::Quads(range)) = lowered.draws.last_mut() {
        range.end = end;
    } else {
        lowered.draws.push(UiDraw::Quads(end - 1..end));
    }
    lowered.merge_quad = true;
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
    #[error("UI paint packet contains invalid mesh geometry: {reason}")]
    InvalidMesh { reason: &'static str },
    #[error("UI mesh geometry exceeds native indexed buffer capacity")]
    MeshCapacity,
    #[cfg(test)]
    #[error("the legacy quad-only oracle cannot execute a UI mesh; use the typed UI Feature")]
    LegacyMeshUnsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    use nara_asset::{AssetEvents, AssetStates, Assets};

    use nara_image::{
        ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot,
    };
    use nara_text::{FontFace, TextDiagnostic, TextEngine, TextLayoutRequest, bundled_font_face};
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiBorder, UiBrush, UiColor, UiCompiler, UiCornerRadii,
        UiEnvironment, UiImageId, UiLayer, UiLocalKey, UiNode, UiOverflow, UiPaintPrimitive,
        UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTextOverflow, UiTheme, UiVisual,
    };
    use nara_window::WindowId;

    use crate::readback_tests::{
        READBACK_BYTES_PER_PIXEL, READBACK_EXTENT, readback_pixel, readback_viewport,
        render_batches_to_rgba,
    };
    use crate::{
        AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
        WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder,
    };

    #[test]
    fn ui_capture_routes_logical_window_packets_and_retains_replayed_packet_facts() {
        type Composite = std::rc::Rc<()>;
        let _definition = WgpuFeatureDefinition::new(UiFeature::<Composite>::new());
        let mut builder = capture_builder();
        let target = builder.texture::<Composite>(capture_descriptor()).unwrap();
        let image = image_packet();
        let alias = text_packet_for_target(RenderTarget::PrimaryWindow);
        let other = text_packet_for_target(RenderTarget::Image(
            nara_render::RenderImage2d::allocate_handle().unwrap(),
        ));
        let expected_packets = [image.identity(), alias.identity(), image.identity()];
        let expected_image = image.images()[0].snapshot().identity().clone();
        let capture = UiCapture::new(
            &[
                Arc::clone(&image),
                other,
                Arc::clone(&alias),
                Arc::clone(&image),
            ],
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            target.clone(),
            readback_viewport(),
        )
        .unwrap();
        drop(image);
        drop(alias);

        assert_eq!(
            capture
                .packet_facts
                .iter()
                .map(|facts| facts.packet)
                .collect::<Vec<_>>(),
            expected_packets,
        );
        assert_eq!(
            capture.packet_facts[0].target,
            RenderTarget::Window(WindowId::PRIMARY),
        );
        assert_eq!(capture.packet_facts[1].target, RenderTarget::PrimaryWindow);
        assert!(
            capture
                .packet_facts
                .iter()
                .all(|facts| facts.submitted_quad_count > 0)
        );
        assert!(capture.batches.iter().any(|batch| {
            matches!(&batch.texture, WgpuQuadTexture::Image(snapshot) if snapshot.identity() == &expected_image)
        }));
        assert!(
            capture
                .batches
                .iter()
                .any(|batch| matches!(&batch.texture, WgpuQuadTexture::Glyph(_)))
        );
        builder
            .initialize(
                1,
                &[capture
                    .target
                    .clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
            )
            .unwrap();
        builder.finish(&[target.output()]).unwrap();
    }

    #[test]
    fn ui_capture_keeps_packet_facts_when_clipping_removes_all_draws() {
        let child = UiNode::new(UiStyle::absolute(0.0, 0.0, 8.0, 8.0))
            .with_key(UiLocalKey::runtime("wgpu-capture-clip", "fill").unwrap())
            .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)));
        let node =
            UiNode::new(UiStyle::absolute(16.0, 16.0, 8.0, 8.0).with_overflow(UiOverflow::Clip))
                .with_child(child);
        let frame = compile_frame("wgpu-capture-clip", node);
        let packet = nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, RenderTarget::PrimaryWindow, &Default::default())
            .unwrap();
        assert!(!packet.primitives().is_empty());
        let mut builder = capture_builder();
        let target = builder.texture::<()>(capture_descriptor()).unwrap();
        let viewport = ViewportRect::new(4, 8, 8, 8).unwrap();
        let capture = UiCapture::new(
            &[Arc::clone(&packet)],
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            target,
            viewport,
        )
        .unwrap();

        assert_eq!(capture.viewport, viewport);
        assert!(capture.batches.is_empty());
        assert_eq!(capture.packet_facts.len(), 1);
        let facts = &capture.packet_facts[0];
        assert_eq!(facts.packet, packet.identity());
        assert_eq!(facts.target, RenderTarget::PrimaryWindow);
        assert_eq!(facts.primitive_count, 1);
        assert_eq!(facts.submitted_quad_count, 0);
        assert!(facts.resources.is_empty());
    }

    #[test]
    fn submission_packets_reject_replanned_image_bindings_with_equal_identity() {
        let original = image_packet();
        let mut images = nara_ui_render::UiPreparedImages::default();
        images.insert(
            original.images()[0].id().clone(),
            captured_image_snapshot([16, 240, 16, 255]),
        );
        let replacement = nara_ui_render::UiPresentationPlanner::default()
            .plan(original.frame(), original.target(), &images)
            .unwrap();
        assert_eq!(original.identity(), replacement.identity());
        assert_eq!(original.target(), replacement.target());
        assert!(!Arc::ptr_eq(&original, &replacement));
        assert_eq!(
            original.images()[0].snapshot().resource().pixels(),
            [240, 16, 120, 255],
        );
        assert_eq!(
            replacement.images()[0].snapshot().resource().pixels(),
            [16, 240, 16, 255],
        );

        let mut builder = capture_builder();
        let target = builder.texture::<()>(capture_descriptor()).unwrap();
        let capture = UiCapture::new(
            std::slice::from_ref(&original),
            original.target(),
            Some(WindowId::PRIMARY),
            target.clone(),
            readback_viewport(),
        )
        .unwrap();
        let replacement_capture = UiCapture::new(
            std::slice::from_ref(&replacement),
            replacement.target(),
            Some(WindowId::PRIMARY),
            target.clone(),
            readback_viewport(),
        )
        .unwrap();
        assert_ne!(
            capture.packet_facts[0].resources,
            replacement_capture.packet_facts[0].resources,
        );
        for (capture, expected) in [
            (&capture, [240, 16, 120, 255]),
            (&replacement_capture, [16, 240, 16, 255]),
        ] {
            assert!(capture.batches.iter().any(|batch| {
                matches!(&batch.texture, WgpuQuadTexture::Image(snapshot)
                    if snapshot.resource().pixels() == expected)
            }));
        }

        let mut outcomes = nara_ui_render::__private::UiPresentationOutcomes::default();
        let prepared = crate::tests::admit_ui_attempt(
            &mut outcomes,
            crate::tests::prepared_ui_attempt(Arc::clone(&replacement), 1, 3),
        );
        let submissions = nara_ui_render::__private::bind_backend_submission_batch(
            vec![prepared],
            nara_ui_render::UiPresentationBackendGeneration::new(17, 3).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            validate_submission_packets(&submissions, &capture.packet_facts),
            Err(WgpuRenderError::UiSubmissionRejected { .. }),
        ));
        let report = UiReport {
            packet_facts: Arc::clone(&capture.packet_facts),
            ..Default::default()
        };
        assert!(matches!(
            validate_submission_packets(&submissions, &report.packet_facts),
            Err(WgpuRenderError::UiSubmissionRejected { .. }),
        ));
        assert!(Arc::ptr_eq(
            submissions[0].submission().packet(),
            &replacement,
        ));
        validate_submission_packets(&submissions, &replacement_capture.packet_facts).unwrap();

        let replay_capture = UiCapture::new(
            &[original, Arc::clone(&replacement)],
            replacement.target(),
            Some(WindowId::PRIMARY),
            target.clone(),
            readback_viewport(),
        )
        .unwrap();
        validate_submission_packets(&submissions, &replay_capture.packet_facts).unwrap();
        validate_submission_packets(&[], &replay_capture.packet_facts).unwrap();
        let duplicate_capture = UiCapture::new(
            &[Arc::clone(&replacement), replacement],
            replay_capture.packet_facts[0].target,
            Some(WindowId::PRIMARY),
            target,
            readback_viewport(),
        )
        .unwrap();
        assert!(matches!(
            validate_submission_packets(&submissions, &duplicate_capture.packet_facts),
            Err(WgpuRenderError::UiSubmissionRejected { .. }),
        ));
    }

    fn capture_builder() -> WgpuRenderProgramBuilder {
        WgpuRenderProgramBuilder::new(
            nara_render::RenderFrameSourceGeneration::try_acquire().unwrap(),
            1,
            GraphLimits::default(),
        )
        .unwrap()
    }

    fn capture_descriptor() -> WgpuProgramTextureDescriptor {
        WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("ui-capture"),
                size: wgpu::Extent3d {
                    width: READBACK_EXTENT,
                    height: READBACK_EXTENT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            },
            transfer: TextureTransfer::Linear,
            alpha: TextureAlpha::Premultiplied,
        }
    }

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
                UiPacketPrimitive::Rect { .. } | UiPacketPrimitive::Mesh { .. } => None,
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
    fn fallback_font_packet_retains_clipped_gpu_pixels_matching_single_face_rasters() {
        let primary = bundled_font_face().unwrap();
        let fallback = FontFace::from_bytes(
            include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let faces = vec![primary.clone(), fallback.clone()];
        let node = UiNode::new(UiStyle::absolute(4.0, 4.0, 24.0, 40.0))
            .with_key(UiLocalKey::runtime("wgpu-fallback-font", "label").unwrap())
            .with_text(
                UiText::new("A\u{2229}", 24.0)
                    .unwrap()
                    .with_brush(UiBrush::Solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)))
                    .with_overflow(UiTextOverflow::Clip),
            );
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("wgpu-fallback-font", "root").unwrap(),
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
        let frame = UiCompiler::with_fonts(faces.clone(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        let layout = frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                UiPaintPrimitive::Text { layout, .. } => Some(layout),
                UiPaintPrimitive::Rect { .. } | UiPaintPrimitive::Mesh { .. } => None,
            })
            .expect("the mixed-font frame must retain its shaped text");
        assert!(layout.diagnostics().is_empty());
        let keys = layout
            .glyphs()
            .iter()
            .map(|glyph| glyph.key())
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].face_id(), primary.id());
        assert_eq!(keys[1].face_id(), fallback.id());

        let mut primary_engine = TextEngine::new(primary.clone(), Default::default()).unwrap();
        let mut fallback_engine = TextEngine::new(fallback.clone(), Default::default()).unwrap();
        let primary_layout = primary_engine
            .layout(&TextLayoutRequest::new("A", 24.0).unwrap())
            .unwrap();
        let fallback_layout = fallback_engine
            .layout(&TextLayoutRequest::new("\u{2229}", 24.0).unwrap())
            .unwrap();
        assert_eq!(
            keys[0].glyph_id(),
            primary_layout.glyphs()[0].key().glyph_id(),
        );
        assert_eq!(
            keys[1].glyph_id(),
            fallback_layout.glyphs()[0].key().glyph_id(),
        );
        assert_ne!(keys[1].glyph_id(), 0);
        let rasters = [
            primary_engine.rasterize(keys[0]).unwrap(),
            fallback_engine.rasterize(keys[1]).unwrap(),
        ];
        drop(primary_engine);
        drop(fallback_engine);

        let primary_only_frame = UiCompiler::new(primary.clone(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        let missing_layout = primary_only_frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                UiPaintPrimitive::Text { layout, .. } => Some(layout),
                UiPaintPrimitive::Rect { .. } | UiPaintPrimitive::Mesh { .. } => None,
            })
            .expect("the primary-only control must retain its missing-glyph layout");
        assert_eq!(missing_layout.glyphs().len(), 2);
        assert_eq!(missing_layout.glyphs()[1].key().face_id(), primary.id());
        assert_eq!(missing_layout.glyphs()[1].key().glyph_id(), 0);
        assert_eq!(
            missing_layout.diagnostics(),
            &[TextDiagnostic::MissingGlyph { scalar: '\u{2229}' }],
        );
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let primary_only_packet =
            nara_ui_render::UiPresentationPlanner::with_font(primary, Default::default())
                .unwrap()
                .plan(primary_only_frame, target, &Default::default())
                .unwrap();
        let packet = nara_ui_render::UiPresentationPlanner::with_fonts(faces, Default::default())
            .unwrap()
            .plan(frame, target, &Default::default())
            .unwrap();
        let glyphs = packet
            .primitives()
            .iter()
            .filter_map(|primitive| match primitive {
                UiPacketPrimitive::Glyph { rect, clip, .. } => {
                    Some((*rect, clip.expect("text clipping must reach the packet")))
                }
                UiPacketPrimitive::Rect { .. } | UiPacketPrimitive::Mesh { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(glyphs.len(), 2);
        assert!(glyphs[0].0.max_x() <= glyphs[1].0.min_x());
        let text_clip = UiPacketRect::new(4, 4, 28, 44).unwrap();
        let mut expected_alpha = vec![0_u8; (READBACK_EXTENT * READBACK_EXTENT) as usize];
        let mut visible_ink = [0_usize; 2];
        let mut clipped_ink = [0_usize; 2];
        for (index, ((rect, clip), raster)) in glyphs.iter().zip(&rasters).enumerate() {
            assert_eq!(
                (rect.width(), rect.height()),
                (raster.width(), raster.height()),
            );
            assert!(rect.min_x() >= 0 && rect.min_y() >= 0);
            assert!(rect.max_x() <= READBACK_EXTENT as i32);
            assert!(rect.max_y() <= READBACK_EXTENT as i32);
            assert_eq!(rect.intersect(text_clip), Some(*clip));
            for row in 0..raster.height() {
                for column in 0..raster.width() {
                    let x = rect.min_x() + column as i32;
                    let y = rect.min_y() + row as i32;
                    let alpha = raster.mask()[(row * raster.width() + column) as usize];
                    if (clip.min_x()..clip.max_x()).contains(&x)
                        && (clip.min_y()..clip.max_y()).contains(&y)
                    {
                        expected_alpha[(y as u32 * READBACK_EXTENT + x as u32) as usize] = alpha;
                        visible_ink[index] += usize::from(alpha != 0);
                    } else {
                        clipped_ink[index] += usize::from(alpha != 0);
                    }
                }
            }
        }
        assert!(
            visible_ink[0] > 0,
            "the primary A must contribute visible ink"
        );
        assert!(
            visible_ink[1] > 0,
            "the fallback intersection must contribute visible ink",
        );
        assert!(
            clipped_ink[1] > 0,
            "clipping must remove actual fallback ink",
        );

        let batches = collect_ui_quad_batches(
            &[packet],
            target,
            Some(WindowId::PRIMARY),
            readback_viewport(),
        )
        .unwrap();
        let primary_only_batches = collect_ui_quad_batches(
            &[primary_only_packet],
            target,
            Some(WindowId::PRIMARY),
            readback_viewport(),
        )
        .unwrap();
        let pixels = render_batches_to_rgba(&batches).unwrap();
        let primary_only_pixels = render_batches_to_rgba(&primary_only_batches).unwrap();
        for (index, (pixel, expected)) in pixels
            .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
            .zip(expected_alpha)
            .enumerate()
        {
            assert!(
                pixel[3].abs_diff(expected) <= 1,
                "retained GPU alpha at ({}, {}) was {}, expected single-face coverage {}",
                index % READBACK_EXTENT as usize,
                index / READBACK_EXTENT as usize,
                pixel[3],
                expected,
            );
        }
        let fallback_clip = glyphs[1].1;
        assert!(
            (fallback_clip.min_y()..fallback_clip.max_y()).any(|y| {
                (fallback_clip.min_x()..fallback_clip.max_x()).any(|x| {
                    readback_pixel(&pixels, x as u32, y as u32)[3]
                        != readback_pixel(&primary_only_pixels, x as u32, y as u32)[3]
                })
            }),
            "fallback-region GPU pixels must differ from the primary-only missing glyph",
        );
    }

    #[test]
    fn reconfigured_editor_snapshot_retains_clipped_gpu_pixels_and_old_font_packets() {
        use nara_text::{TextEditor, TextWindowError};
        use nara_ui_kernel::UiPreparedText;

        use crate::{
            quad::{
                WgpuQuadPipelineDrawRef, create_material_bind_group_layout,
                create_quad_batch_buffers, create_quad_pipeline,
                create_quad_texture_bind_group_layout, draw_quad_batch_buffers,
            },
            readback_tests::ReadbackGpu,
            texture::WgpuSpriteTextureCache,
        };

        const SIZE: f32 = 24.0;
        const SCALE: f32 = 1.25;
        let primary = bundled_font_face().unwrap();
        let fallback = FontFace::from_bytes(
            include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
            0,
        )
        .unwrap();
        let faces = vec![primary.clone(), fallback.clone()];
        let mut original_engine = TextEngine::new(primary.clone(), Default::default()).unwrap();
        let mut replacement_engine =
            TextEngine::with_fonts(faces.clone(), Default::default()).unwrap();
        let mut editor = TextEditor::new(&mut original_engine, "A\u{2229}", SIZE).unwrap();
        let old_snapshot = editor.paint_snapshot();
        let selection = editor.selection();

        // These single-face engines supply independent glyph IDs and coverage, without using
        // the editing layout's atlas or the presentation planner's rasterizer.
        let mut primary_rasterizer = TextEngine::new(primary.clone(), Default::default()).unwrap();
        let mut fallback_rasterizer =
            TextEngine::new(fallback.clone(), Default::default()).unwrap();
        let primary_layout = primary_rasterizer
            .layout(&TextLayoutRequest::new("A", SIZE).unwrap())
            .unwrap();
        let fallback_layout = fallback_rasterizer
            .layout(&TextLayoutRequest::new("\u{2229}", SIZE).unwrap())
            .unwrap();
        assert_ne!(fallback_layout.glyphs()[0].key().glyph_id(), 0);

        let target = RenderTarget::Window(WindowId::PRIMARY);
        let gpu = ReadbackGpu::new();
        let texture_layout = create_quad_texture_bind_group_layout(&gpu.device).unwrap();
        let material_layout = create_material_bind_group_layout(&gpu.device).unwrap();
        let mut texture_cache = WgpuSpriteTextureCache::default();
        let mut frame_index = 0;
        // Every readback uses the original device/queue and texture cache, including old
        // packets replayed after the new fonts have drawn and their source owners are gone.
        let mut render = |packet: &Arc<UiPaintPacket>| {
            let batches = collect_ui_quad_batches(
                std::slice::from_ref(packet),
                target,
                Some(WindowId::PRIMARY),
                readback_viewport(),
            )
            .unwrap();
            frame_index += 1;
            texture_cache.begin_frame(frame_index);
            gpu.render_to_rgba(|device, _, format, output_view, encoder, preparation| {
                let pipelines = crate::required_quad_pipeline_keys(&batches, format)
                    .into_iter()
                    .map(|key| {
                        let pipeline =
                            create_quad_pipeline(device, key, &texture_layout, &material_layout)?;
                        Ok(WgpuQuadPipelineDrawRef {
                            alpha_mode: key.alpha_mode,
                            material: key.material,
                            pipeline: pipeline.pipeline,
                        })
                    })
                    .collect::<Result<Vec<_>, crate::WgpuRenderError>>()?;
                let buffers = create_quad_batch_buffers(
                    preparation,
                    &batches,
                    &texture_layout,
                    &material_layout,
                    &mut texture_cache,
                    frame_index,
                )
                .unwrap();
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("nara_reconfigured_editor_readback"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: output_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    draw_quad_batch_buffers(&mut pass, &pipelines, &buffers, readback_viewport());
                }
                Ok(())
            })
            .unwrap()
            .0
        };

        let mut packets = Vec::new();
        let mut readbacks = Vec::new();
        let mut old_page_pixels = Vec::new();
        for refreshed in [false, true] {
            if refreshed {
                let outcome = editor.reconfigure_fonts(&mut replacement_engine).unwrap();
                assert!(outcome.display_changed);
                assert!(!outcome.text_changed && !outcome.composition_changed);
                assert_eq!(editor.text(), "A\u{2229}");
                assert_eq!(editor.logical_size(), SIZE);
                assert_eq!(editor.selection(), selection);
                original_engine
                    .validate_paint_snapshot(&old_snapshot, SCALE)
                    .unwrap();
                assert_eq!(
                    replacement_engine.validate_paint_snapshot(&old_snapshot, SCALE),
                    Err(TextWindowError::FontSetMismatch),
                );
            }
            let snapshot = editor.paint_snapshot();
            assert_eq!(old_snapshot.same_source(&snapshot), !refreshed);
            if refreshed {
                assert_eq!(
                    original_engine.validate_paint_snapshot(&snapshot, SCALE),
                    Err(TextWindowError::FontSetMismatch),
                );
            }
            let fonts = if refreshed {
                faces.clone()
            } else {
                vec![primary.clone()]
            };
            let revision = if refreshed { 2 } else { 1 };
            let node = UiNode::new(UiStyle::absolute(4.25, 3.5, 24.0, 40.0))
                .with_key(UiLocalKey::runtime("wgpu-editor-font-refresh", "label").unwrap())
                .with_prepared_text(
                    UiPreparedText::new(snapshot)
                        .with_brush(UiBrush::Solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                );
            let authored = UiAuthoringSnapshotBuilder::new(revision)
                .with_root(
                    UiRoot::new(
                        UiRootKey::runtime("wgpu-editor-font-refresh", "root").unwrap(),
                        UiLayer::Base,
                        UiStyle::fill(),
                    )
                    .with_child(node),
                )
                .build()
                .unwrap();
            let environment = UiEnvironment::new(
                UiSurfaceId::new(1).unwrap(),
                READBACK_EXTENT as f32 / SCALE,
                READBACK_EXTENT as f32 / SCALE,
                SCALE,
                1,
                1,
                1,
                revision,
            )
            .unwrap();
            let frame = UiCompiler::with_fonts(fonts.clone(), Default::default())
                .unwrap()
                .compile(&authored, environment, UiTheme::default())
                .unwrap();
            let layout = frame
                .paint()
                .iter()
                .find_map(|primitive| match primitive {
                    UiPaintPrimitive::Text { layout, .. } => Some(layout),
                    UiPaintPrimitive::Rect { .. } | UiPaintPrimitive::Mesh { .. } => None,
                })
                .expect("the editor snapshot must reach text paint");
            let keys = layout
                .glyphs()
                .iter()
                .map(|glyph| glyph.key())
                .collect::<Vec<_>>();
            assert_eq!(keys.len(), 2);
            assert!(keys.iter().all(|key| key.physical_size() == SIZE * SCALE));
            assert_eq!(keys[0].face_id(), primary.id());
            assert_eq!(
                keys[0].glyph_id(),
                primary_layout.glyphs()[0].key().glyph_id()
            );
            if refreshed {
                assert!(layout.diagnostics().is_empty());
                assert_eq!(keys[1].face_id(), fallback.id());
                assert_eq!(
                    keys[1].glyph_id(),
                    fallback_layout.glyphs()[0].key().glyph_id()
                );
            } else {
                assert_eq!(keys[1].face_id(), primary.id());
                assert_eq!(keys[1].glyph_id(), 0);
                assert_eq!(
                    layout.diagnostics(),
                    &[TextDiagnostic::MissingGlyph { scalar: '\u{2229}' }],
                );
            }
            let rasters = [
                primary_rasterizer.rasterize(keys[0]).unwrap(),
                if refreshed {
                    fallback_rasterizer.rasterize(keys[1]).unwrap()
                } else {
                    primary_rasterizer.rasterize(keys[1]).unwrap()
                },
            ];
            let packet =
                nara_ui_render::UiPresentationPlanner::with_fonts(fonts, Default::default())
                    .unwrap()
                    .plan(frame, target, &Default::default())
                    .unwrap();
            let glyphs = packet
                .primitives()
                .iter()
                .map(|primitive| match primitive {
                    UiPacketPrimitive::Glyph { rect, clip, .. } => (
                        *rect,
                        clip.expect("prepared text must retain its content clip"),
                    ),
                    UiPacketPrimitive::Rect { .. } | UiPacketPrimitive::Mesh { .. } => {
                        panic!("the fixture draws only editor text")
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(glyphs.len(), 2);
            assert!(glyphs[0].0.max_x() <= glyphs[1].0.min_x());
            let text_clip = UiPacketRect::new(5, 4, 35, 54).unwrap();
            let mut expected_alpha = vec![0_u8; (READBACK_EXTENT * READBACK_EXTENT) as usize];
            let mut visible_ink = [0_usize; 2];
            let mut clipped_ink = [0_usize; 2];
            for (index, ((rect, clip), raster)) in glyphs.iter().zip(&rasters).enumerate() {
                assert_eq!(
                    (rect.width(), rect.height()),
                    (raster.width(), raster.height())
                );
                assert!(rect.min_x() >= 0 && rect.min_y() >= 0);
                assert!(rect.max_x() <= READBACK_EXTENT as i32);
                assert!(rect.max_y() <= READBACK_EXTENT as i32);
                assert_eq!(rect.intersect(text_clip), Some(*clip));
                for row in 0..raster.height() {
                    for column in 0..raster.width() {
                        let x = rect.min_x() + column as i32;
                        let y = rect.min_y() + row as i32;
                        let alpha = raster.mask()[(row * raster.width() + column) as usize];
                        if (clip.min_x()..clip.max_x()).contains(&x)
                            && (clip.min_y()..clip.max_y()).contains(&y)
                        {
                            expected_alpha[(y as u32 * READBACK_EXTENT + x as u32) as usize] =
                                alpha;
                            visible_ink[index] += usize::from(alpha != 0);
                        } else {
                            clipped_ink[index] += usize::from(alpha != 0);
                        }
                    }
                }
            }
            assert!(visible_ink[0] > 0 && visible_ink[1] > 0);
            if refreshed {
                assert!(
                    clipped_ink[1] > 0,
                    "clipping must remove actual fallback ink"
                );
            }
            let pixels = render(&packet);
            for (index, (pixel, expected)) in pixels
                .chunks_exact(READBACK_BYTES_PER_PIXEL as usize)
                .zip(expected_alpha)
                .enumerate()
            {
                assert!(
                    pixel[3].abs_diff(expected) <= 1,
                    "refreshed={refreshed}, GPU alpha at ({}, {}) was {}, expected single-face coverage {}",
                    index % READBACK_EXTENT as usize,
                    index / READBACK_EXTENT as usize,
                    pixel[3],
                    expected,
                );
            }
            if !refreshed {
                old_page_pixels = packet
                    .glyph_pages()
                    .iter()
                    .map(|page| page.pixels().to_vec())
                    .collect::<Vec<_>>();
            }
            packets.push(packet);
            readbacks.push(pixels);
        }

        let UiPacketPrimitive::Glyph {
            clip: Some(clip), ..
        } = &packets[1].primitives()[1]
        else {
            panic!("the refreshed packet must retain the clipped fallback glyph")
        };
        assert!(
            (clip.min_y()..clip.max_y()).any(|y| {
                (clip.min_x()..clip.max_x()).any(|x| {
                    readback_pixel(&readbacks[0], x as u32, y as u32)[3]
                        != readback_pixel(&readbacks[1], x as u32, y as u32)[3]
                })
            }),
            "the refreshed fallback region must differ from the old missing glyph",
        );
        assert_eq!(
            packets[0]
                .glyph_pages()
                .iter()
                .map(|page| page.pixels().to_vec())
                .collect::<Vec<_>>(),
            old_page_pixels,
        );
        assert_eq!(render(&packets[0]), readbacks[0]);

        drop(editor);
        drop(original_engine);
        drop(replacement_engine);
        drop(primary_rasterizer);
        drop(fallback_rasterizer);
        assert_eq!(old_snapshot.display_text(), "A\u{2229}");
        assert_eq!(render(&packets[1]), readbacks[1]);
        assert_eq!(render(&packets[0]), readbacks[0]);
        assert_eq!(
            packets[0]
                .glyph_pages()
                .iter()
                .map(|page| page.pixels().to_vec())
                .collect::<Vec<_>>(),
            old_page_pixels,
        );
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
        prepared.insert(image_id, captured_image_snapshot([240, 16, 120, 255]));
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
        prepared.insert(image_id, captured_image_snapshot([240, 16, 120, 255]));
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

    fn captured_image_snapshot(pixels: [u8; 4]) -> PreparedImageSnapshot {
        let image = ImageAsset::new(
            ImageExtent::new(1, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            pixels.to_vec(),
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
