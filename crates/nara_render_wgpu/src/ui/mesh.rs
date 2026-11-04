//! Private indexed geometry inside the original UI attachment transaction.

use std::{mem::size_of, ops::Range, sync::Arc};

use bytemuck::{Pod, Zeroable};
use nara_render::ViewportRect;
use nara_ui_render::{UiPacketMesh, UiPacketPrimitive, UiPacketRect, UiPaintPacket};

use super::{WgpuUiPacketError, primitive_scissor};
use crate::{
    WgpuFeatureAllocation, WgpuFeaturePrepareContext, WgpuRenderProgramError,
    allocation::checked_resource, quad::WgpuScissorRect,
};

const MAX_MESH_VERTICES_PER_OPERATION: usize = 65_536;
const MAX_MESH_INDICES_PER_OPERATION: usize = 196_608;

pub(super) fn preflight_packets<'a>(
    packets: impl Iterator<Item = &'a Arc<UiPaintPacket>>,
) -> Result<(), WgpuUiPacketError> {
    let mut vertices = 0_usize;
    let mut indices = 0_usize;
    for primitive in packets.flat_map(|packet| packet.primitives()) {
        if let UiPacketPrimitive::Mesh { mesh, .. } = primitive {
            vertices = vertices
                .checked_add(mesh.vertices().len())
                .filter(|count| *count <= MAX_MESH_VERTICES_PER_OPERATION)
                .ok_or(WgpuUiPacketError::MeshCapacity)?;
            indices = indices
                .checked_add(mesh.indices().len())
                .filter(|count| *count <= MAX_MESH_INDICES_PER_OPERATION)
                .ok_or(WgpuUiPacketError::MeshCapacity)?;
        }
    }
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct MeshVertex {
    pub(super) position: [f32; 2],
    pub(super) color: [f32; 4],
}

/// Every range addresses one original preparation's nonempty quad buffers or mesh indices.
/// A mesh is a hard batching barrier, even when adjacent quads share a material and clip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UiDraw {
    Quads(Range<usize>),
    Mesh {
        indices: Range<u32>,
        scissor: WgpuScissorRect,
    },
}

pub(super) fn append_mesh(
    vertices: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    draws: &mut Vec<UiDraw>,
    mesh: &UiPacketMesh,
    rect: UiPacketRect,
    clip: Option<UiPacketRect>,
    viewport: ViewportRect,
) -> Result<bool, WgpuUiPacketError> {
    validate_mesh(mesh)?;
    let clip = match clip {
        Some(clip) => rect.intersect(clip),
        None => Some(rect),
    };
    let Some(Some(scissor)) = clip.and_then(|clip| primitive_scissor(Some(clip), viewport)) else {
        return Ok(false);
    };
    if mesh.indices().is_empty() {
        return Ok(false);
    }
    let base = u32::try_from(vertices.len()).map_err(|_| WgpuUiPacketError::MeshCapacity)?;
    let vertex_end = vertices
        .len()
        .checked_add(mesh.vertices().len())
        .filter(|count| u32::try_from(*count).is_ok())
        .ok_or(WgpuUiPacketError::MeshCapacity)?;
    let start = u32::try_from(indices.len()).map_err(|_| WgpuUiPacketError::MeshCapacity)?;
    let end = indices
        .len()
        .checked_add(mesh.indices().len())
        .and_then(|count| u32::try_from(count).ok())
        .ok_or(WgpuUiPacketError::MeshCapacity)?;
    vertices
        .try_reserve(mesh.vertices().len())
        .map_err(|_| WgpuUiPacketError::MeshCapacity)?;
    indices
        .try_reserve(mesh.indices().len())
        .map_err(|_| WgpuUiPacketError::MeshCapacity)?;
    draws
        .try_reserve(1)
        .map_err(|_| WgpuUiPacketError::MeshCapacity)?;
    for vertex in mesh.vertices() {
        let [physical_x, physical_y] = vertex.position();
        let position = nara_render::viewport_physical_to_clip(
            viewport,
            [
                physical_x + viewport.physical_x as f32,
                physical_y + viewport.physical_y as f32,
            ]
            .into(),
        )
        .ok_or(WgpuUiPacketError::InvalidMesh {
            reason: "the physical vertex cannot be projected into its original viewport",
        })?;
        if !position.is_finite() {
            return Err(WgpuUiPacketError::InvalidMesh {
                reason: "the projected vertex is not finite",
            });
        }
        vertices.push(MeshVertex {
            position: position.to_array(),
            color: vertex.color().components(),
        });
    }
    debug_assert_eq!(vertices.len(), vertex_end);
    for index in mesh.indices() {
        indices.push(
            base.checked_add(*index)
                .ok_or(WgpuUiPacketError::MeshCapacity)?,
        );
    }
    draws.push(UiDraw::Mesh {
        indices: start..end,
        scissor,
    });
    Ok(true)
}

fn validate_mesh(mesh: &UiPacketMesh) -> Result<(), WgpuUiPacketError> {
    if !mesh.indices().len().is_multiple_of(3)
        || mesh
            .indices()
            .iter()
            .any(|index| *index as usize >= mesh.vertices().len())
    {
        return Err(WgpuUiPacketError::InvalidMesh {
            reason: "triangle indices must reference the captured vertices",
        });
    }
    for vertex in mesh.vertices() {
        let [physical_x, physical_y] = vertex.position();
        if !physical_x.is_finite()
            || !physical_y.is_finite()
            || vertex
                .color()
                .components()
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
        {
            return Err(WgpuUiPacketError::InvalidMesh {
                reason: "vertices must have finite physical positions and straight-alpha colors",
            });
        }
    }
    Ok(())
}

pub(super) struct MeshPipeline {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
}

impl MeshPipeline {
    fn create(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<Self, WgpuRenderProgramError> {
        let shader = checked_resource(device, "nara_ui_mesh_shader", || {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("nara_ui_mesh_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
            })
        })?;
        let layout = checked_resource(device, "nara_ui_mesh_pipeline_layout", || {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("nara_ui_mesh_pipeline_layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
        })?;
        let pipeline = checked_resource(device, "nara_ui_mesh_pipeline", || {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("nara_ui_mesh_pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<MeshVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })?;
        Ok(Self { format, pipeline })
    }
}

/// Native mesh state belongs to the original Feature instance and device epoch.
#[derive(Default)]
pub struct UiState {
    pipeline: Option<Arc<MeshPipeline>>,
}

pub(super) struct MeshPrepared {
    pipeline: Arc<MeshPipeline>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    _vertex_allocation: WgpuFeatureAllocation,
    _index_allocation: WgpuFeatureAllocation,
}

impl MeshPrepared {
    pub(super) fn create(
        state: &mut UiState,
        vertices: &[MeshVertex],
        indices: &[u32],
        format: wgpu::TextureFormat,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Option<Self>, WgpuRenderProgramError> {
        if indices.is_empty() {
            return Ok(None);
        }
        if state
            .pipeline
            .as_ref()
            .is_none_or(|pipeline| pipeline.format != format)
        {
            state.pipeline = Some(Arc::new(MeshPipeline::create(context.device(), format)?));
        }
        let pipeline = Arc::clone(
            state
                .pipeline
                .as_ref()
                .ok_or(WgpuRenderProgramError::FeatureState)?,
        );
        let (vertices, vertex_allocation) = context.create_buffer_with_data(
            "nara_ui_mesh_vertices",
            bytemuck::cast_slice(vertices),
            wgpu::BufferUsages::VERTEX,
        )?;
        let (indices, index_allocation) = context.create_buffer_with_data(
            "nara_ui_mesh_indices",
            bytemuck::cast_slice(indices),
            wgpu::BufferUsages::INDEX,
        )?;
        Ok(Some(Self {
            pipeline,
            vertices,
            indices,
            _vertex_allocation: vertex_allocation,
            _index_allocation: index_allocation,
        }))
    }

    pub(super) fn encode<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        indices: Range<u32>,
        scissor: WgpuScissorRect,
    ) {
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(indices, 0, 0..1);
    }
}
