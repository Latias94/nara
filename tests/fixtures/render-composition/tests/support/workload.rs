//! Shared/branching image and compute workload, declared independently of execution order.

use super::graph::{
    AccessRole, AlphaMode, BufferMeaning, Candidate, ColorTransfer, CompiledGraph, GraphError,
    ImportState, Initialization, Limits, OperationKey, Read, ResourceDesc, TextureMeaning, Version,
    Write,
};

pub(super) const WIDTH: u32 = 64;
pub(super) const HEIGHT: u32 = 64;
pub(super) const BLUR_HORIZONTAL: OperationKey = 70;
pub(super) const BLUR_VERTICAL: OperationKey = 10;
pub(super) const COMPLEMENT: OperationKey = 40;
pub(super) const HUD: OperationKey = 60;
pub(super) const LUMINANCE: OperationKey = 50;
pub(super) const NORMALIZE: OperationKey = 30;
pub(super) const HISTOGRAM: OperationKey = 20;
pub(super) const OPERATION_KEYS: [OperationKey; 7] = [
    BLUR_HORIZONTAL,
    BLUR_VERTICAL,
    COMPLEMENT,
    HUD,
    LUMINANCE,
    NORMALIZE,
    HISTOGRAM,
];

pub(super) struct Versions {
    pub original: Version,
    pub horizontal: Version,
    pub blurred: Version,
    pub complement: Version,
    pub composite: Version,
    pub histogram: Version,
    pub raw: Version,
    pub normalized: Version,
}

pub(super) struct Workload {
    pub graph: CompiledGraph,
    pub versions: Versions,
}

pub(super) fn build(order: &[OperationKey]) -> Result<Workload, GraphError> {
    let mut candidate = Candidate::new(Limits::default())?;
    let original = candidate.import_resource(texture_desc(1), ImportState::Initialized)?;
    let horizontal = candidate.reserve_resource(texture_desc(2))?;
    let blurred = candidate.reserve_resource(texture_desc(3))?;
    let complement = candidate.reserve_resource(texture_desc(4))?;
    let composite = candidate.reserve_resource(texture_desc(5))?;
    let histogram = candidate.reserve_resource(texture_desc(6))?;
    let raw = candidate.reserve_resource(buffer_desc(false))?;
    let normalized = candidate.reserve_resource(buffer_desc(true))?;
    let sampled = |version: &Version, semantic| Read {
        version: version.clone(),
        role: AccessRole::Sampled,
        expected: texture_desc(semantic),
    };
    let image_write = |version: &Version, role| Write {
        version: version.clone(),
        role,
        initialization: Initialization::Clear(wgpu::Color::TRANSPARENT),
    };
    for key in order {
        match *key {
            BLUR_HORIZONTAL => candidate.add_operation(
                *key,
                &[sampled(&original, 1)],
                &[image_write(&horizontal, AccessRole::StorageWrite)],
            )?,
            BLUR_VERTICAL => candidate.add_operation(
                *key,
                &[sampled(&horizontal, 2)],
                &[image_write(&blurred, AccessRole::StorageWrite)],
            )?,
            COMPLEMENT => candidate.add_operation(
                *key,
                &[sampled(&blurred, 3)],
                &[image_write(&complement, AccessRole::StorageWrite)],
            )?,
            HUD => candidate.add_operation(
                *key,
                &[sampled(&complement, 4)],
                &[image_write(&composite, AccessRole::StorageWrite)],
            )?,
            LUMINANCE => candidate.add_operation(
                *key,
                &[sampled(&original, 1)],
                &[Write {
                    version: raw.clone(),
                    role: AccessRole::StorageReadWrite,
                    initialization: Initialization::Zero,
                }],
            )?,
            NORMALIZE => candidate.add_operation(
                *key,
                &[],
                &[Write {
                    version: normalized.clone(),
                    role: AccessRole::StorageReadWrite,
                    initialization: Initialization::Preserve(raw.clone()),
                }],
            )?,
            HISTOGRAM => candidate.add_operation(
                *key,
                &[Read {
                    version: normalized.clone(),
                    role: AccessRole::Vertex,
                    expected: buffer_desc(true),
                }],
                &[image_write(&histogram, AccessRole::Attachment)],
            )?,
            _ => panic!("unknown private comparison operation"),
        }
    }
    candidate.set_output_group(&[
        original.clone(),
        horizontal.clone(),
        blurred.clone(),
        complement.clone(),
        composite.clone(),
        histogram.clone(),
        raw.clone(),
        normalized.clone(),
    ])?;
    let graph = candidate.compile()?;
    Ok(Workload {
        graph,
        versions: Versions {
            original,
            horizontal,
            blurred,
            complement,
            composite,
            histogram,
            raw,
            normalized,
        },
    })
}

pub(super) fn texture_desc(semantic: u64) -> ResourceDesc {
    ResourceDesc::Texture {
        native: wgpu::TextureDescriptor {
            label: Some("comparison-resource-version"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        range: wgpu::ImageSubresourceRange {
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
        },
        meaning: TextureMeaning {
            semantic,
            transfer: ColorTransfer::Linear,
            alpha: AlphaMode::Premultiplied,
        },
    }
}

pub(super) fn buffer_desc(vertex: bool) -> ResourceDesc {
    ResourceDesc::Buffer {
        native: wgpu::BufferDescriptor {
            label: Some("comparison-statistics-version"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | if vertex {
                    wgpu::BufferUsages::VERTEX
                } else {
                    wgpu::BufferUsages::empty()
                },
            mapped_at_creation: false,
        },
        meaning: BufferMeaning {
            semantic: if vertex { 11 } else { 10 },
        },
    }
}
