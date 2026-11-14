use crate::texture::{
    WgpuCachedImageView, WgpuCachedTextureBinding, WgpuSpriteTextureCache, WgpuTextureError,
};
use crate::{
    allocation::checked_allocation,
    budget::MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE,
    material_textures::WgpuMaterialTextureSource,
    preparation::{NativePreparation, WgpuFeatureAllocation},
    program::WgpuOperationResources,
};
use bytemuck::{Pod, Zeroable};
use nara_image::PreparedImageSnapshot;
use nara_material::{
    AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dGeneration, SamplerDescriptor,
};
use nara_render::{RenderResourceKey, ViewportRect};
#[cfg(feature = "stock-ui")]
use nara_ui_render::{UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot};
use std::collections::BTreeMap;
use thiserror::Error;

const VERTICES_PER_QUAD: u32 = 6;
pub(crate) const WGPU_QUAD_SHAPE_PLAIN: u32 = 0;
#[cfg(feature = "stock-ui")]
pub(crate) const WGPU_QUAD_SHAPE_ROUNDED_FILL: u32 = 1;
#[cfg(feature = "stock-ui")]
pub(crate) const WGPU_QUAD_SHAPE_ROUNDED_BORDER: u32 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct WgpuQuadInstance {
    pub(crate) center: [f32; 2],
    pub(crate) x_axis: [f32; 2],
    pub(crate) y_axis: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) uv_min: [f32; 2],
    pub(crate) uv_size: [f32; 2],
    pub(crate) texture_mode: u32,
    pub(crate) shape_mode: u32,
    pub(crate) rect_size: [f32; 2],
    pub(crate) corner_radius: f32,
    pub(crate) border_width: f32,
}

const QUAD_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
    3 => Float32x4,
    4 => Float32x2,
    5 => Float32x2,
    6 => Uint32,
    7 => Uint32,
    8 => Float32x2,
    9 => Float32,
    10 => Float32
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WgpuQuadTextureKey {
    Fallback,
    Image(RenderResourceKey),
    #[cfg(feature = "stock-ui")]
    Glyph(UiGlyphAtlasPageKey),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WgpuQuadTexture {
    Fallback,
    Image(PreparedImageSnapshot),
    #[cfg(feature = "stock-ui")]
    Glyph(UiGlyphAtlasPageSnapshot),
}

impl WgpuQuadTexture {
    pub(crate) fn key(&self) -> WgpuQuadTextureKey {
        match self {
            Self::Fallback => WgpuQuadTextureKey::Fallback,
            Self::Image(snapshot) => WgpuQuadTextureKey::Image(snapshot.identity().key()),
            #[cfg(feature = "stock-ui")]
            Self::Glyph(page) => WgpuQuadTextureKey::Glyph(page.key()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WgpuQuadMaterialKey {
    pub(crate) texture: WgpuQuadTextureKey,
    pub(crate) sampler: SamplerDescriptor,
    pub(crate) alpha_mode: AlphaMode2d,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WgpuQuadBatch {
    pub(crate) material: WgpuQuadMaterialKey,
    pub(crate) texture: WgpuQuadTexture,
    pub(crate) scissor: Option<WgpuScissorRect>,
    pub(crate) instances: Vec<WgpuQuadInstance>,
    pub(crate) counts_as_sprites: bool,
    pub(crate) material_extension: Option<WgpuMaterial2d>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WgpuMaterial2d {
    pub(crate) binding: Material2dBinding,
    pub(crate) textures: Vec<(u32, WgpuMaterialTexture)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WgpuMaterialTexture {
    Image(PreparedImageSnapshot),
    #[cfg_attr(not(feature = "stock-sprite"), allow(dead_code))]
    Program(WgpuMaterialTextureSource),
}

impl From<PreparedImageSnapshot> for WgpuMaterialTexture {
    fn from(snapshot: PreparedImageSnapshot) -> Self {
        Self::Image(snapshot)
    }
}

impl WgpuMaterialTexture {
    pub(crate) fn image(&self) -> Option<&PreparedImageSnapshot> {
        match self {
            Self::Image(snapshot) => Some(snapshot),
            Self::Program(_) => None,
        }
    }
}

#[derive(Debug)]
pub(crate) enum WgpuMaterialTextureView {
    Image(WgpuCachedImageView),
    Program(wgpu::TextureView),
}

impl WgpuMaterialTextureView {
    fn view(&self) -> &wgpu::TextureView {
        match self {
            Self::Image(image) => &image.view,
            Self::Program(view) => view,
        }
    }
}

/// Retains the admitted source while comparing only actual pipeline inputs.
#[derive(Debug, Clone)]
pub(crate) struct WgpuMaterial2dPipelineKey {
    generation: Material2dGeneration,
    shader_digest: [u8; 32],
    binding_digest: [u8; 32],
}

impl WgpuMaterial2dPipelineKey {
    pub(crate) fn new(generation: &Material2dGeneration) -> Self {
        Self {
            generation: generation.clone(),
            shader_digest: generation.shader().digest(),
            binding_digest: generation.declaration().binding_digest(),
        }
    }

    pub(crate) fn declaration(&self) -> &Material2dDeclaration {
        self.generation.declaration()
    }

    pub(crate) fn pipeline_digests(&self) -> ([u8; 32], [u8; 32]) {
        (self.shader_digest, self.binding_digest)
    }
}

impl PartialEq for WgpuMaterial2dPipelineKey {
    fn eq(&self, other: &Self) -> bool {
        self.shader_digest == other.shader_digest && self.binding_digest == other.binding_digest
    }
}

impl Eq for WgpuMaterial2dPipelineKey {}

pub(crate) type WgpuMaterial2dLayouts = BTreeMap<[u8; 32], wgpu::BindGroupLayout>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuScissorRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct WgpuQuadPipeline {
    pub(crate) key: WgpuQuadPipelineKey,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WgpuQuadPipelineKey {
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: AlphaMode2d,
    pub(crate) material: Option<WgpuMaterial2dPipelineKey>,
}

#[derive(Debug, Clone)]
pub(crate) struct WgpuQuadPipelineDrawRef {
    pub(crate) alpha_mode: AlphaMode2d,
    pub(crate) material: Option<WgpuMaterial2dPipelineKey>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

#[derive(Debug)]
pub(crate) struct WgpuQuadBatchBuffer {
    buffer: wgpu::Buffer,
    _instance_allocation: WgpuFeatureAllocation,
    bind_group: WgpuCachedTextureBinding,
    material_bind_group: Option<wgpu::BindGroup>,
    _material_textures: Vec<WgpuMaterialTextureView>,
    _material_buffer: Option<wgpu::Buffer>,
    _material_allocation: Option<WgpuFeatureAllocation>,
    material: Option<WgpuMaterial2dPipelineKey>,
    alpha_mode: AlphaMode2d,
    scissor: Option<WgpuScissorRect>,
    instance_count: u32,
    #[cfg(test)]
    logical_bytes: u64,
}

pub(crate) fn create_material_bind_group_layout(
    device: &wgpu::Device,
    declaration: &Material2dDeclaration,
) -> Result<wgpu::BindGroupLayout, WgpuTextureError> {
    validate_material_device_limits(declaration, &device.limits())?;
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(u64::from(
                declaration.parameters().byte_size(),
            )),
        },
        count: None,
    }];
    entries.extend(
        declaration
            .textures()
            .iter()
            .map(|texture| wgpu::BindGroupLayoutEntry {
                binding: texture.binding(),
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }),
    );
    checked_allocation(device, "nara_material_2d_bind_group_layout", || {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara_material_2d_bind_group_layout"),
            entries: &entries,
        })
    })
}

#[cfg(test)]
impl WgpuQuadBatchBuffer {
    pub(crate) const fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuQuadDrawStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) image_sprites: u32,
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error(
    "quad instances require {required} bytes; render-scope instance-buffer maximum is {maximum}"
)]
pub(crate) struct WgpuQuadInstanceBudgetError {
    required: u64,
    maximum: u64,
}

pub(crate) fn create_quad_texture_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, WgpuTextureError> {
    checked_allocation(device, "nara_wgpu_quad_texture_bind_group_layout", || {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nara_wgpu_quad_texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    })
}

#[cfg(test)]
std::thread_local! {
    pub(crate) static QUAD_PIPELINE_ALLOCATION_FAILURE: std::cell::Cell<bool> = const {
        std::cell::Cell::new(false)
    };
    static QUAD_SHADER_SOURCE_OVERRIDE: std::cell::Cell<Option<([u8; 32], &'static str)>> =
        const { std::cell::Cell::new(None) };
}

/// Overrides only native module creation for one admitted artifact on the current test thread.
///
/// This exercises WGPU rejection, cache, OOM and device-loss handling without constructing an
/// invalid public generation or changing CPU admission, bindings or production Shader source.
#[cfg(test)]
pub(crate) struct QuadShaderSourceGuard;

#[cfg(test)]
impl QuadShaderSourceGuard {
    pub(crate) fn invalid(material: &WgpuMaterial2dPipelineKey) -> Self {
        QUAD_SHADER_SOURCE_OVERRIDE.with(|source| {
            assert!(
                source
                    .replace(Some((material.shader_digest, "this is not valid WGSL")))
                    .is_none()
            );
        });
        Self
    }
}

#[cfg(test)]
impl Drop for QuadShaderSourceGuard {
    fn drop(&mut self) {
        QUAD_SHADER_SOURCE_OVERRIDE.with(|source| source.set(None));
    }
}

fn finish_quad_pipeline_stage(
    allocation_scope: wgpu::ErrorScopeGuard,
    validation_scope: wgpu::ErrorScopeGuard,
    key: &WgpuQuadPipelineKey,
) -> Result<(), crate::WgpuRenderError> {
    let validation_error = pollster::block_on(validation_scope.pop());
    let allocation_error =
        pollster::block_on(allocation_scope.pop()).map(|error| error.to_string());
    #[cfg(test)]
    let allocation_error = allocation_error.or_else(|| {
        QUAD_PIPELINE_ALLOCATION_FAILURE.with(|failure| {
            failure
                .replace(false)
                .then(|| "injected pipeline allocation failure".to_owned())
        })
    });
    // Allocation failure can leave invalid handles that also produce validation errors.
    // It must remain retryable rather than poisoning this immutable material definition.
    if let Some(message) = allocation_error {
        return Err(crate::WgpuRenderError::UploadDeferred {
            message: format!("2D pipeline allocation: {message}"),
        });
    }
    if let Some(error) = validation_error {
        return Err(crate::WgpuRenderError::ContentRejected {
            message: format!(
                "2D material {:?}: {error}",
                key.material
                    .as_ref()
                    .map(|material| material.declaration().id())
            ),
        });
    }
    Ok(())
}

pub(crate) fn create_quad_pipeline(
    device: &wgpu::Device,
    key: WgpuQuadPipelineKey,
    texture_bind_group_layout: &wgpu::BindGroupLayout,
    material_bind_group_layout: Option<&wgpu::BindGroupLayout>,
) -> Result<WgpuQuadPipeline, crate::WgpuRenderError> {
    if let Some(material) = &key.material {
        validate_material_device_limits(material.declaration(), &device.limits())?;
    }
    let material_bind_group_layout = key
        .material
        .as_ref()
        .map(|material| {
            material_bind_group_layout.ok_or_else(|| crate::WgpuRenderError::ContentRejected {
                message: format!(
                    "2D material {:?} requires a material bind group layout",
                    material.declaration().id()
                ),
            })
        })
        .transpose()?;
    let source = match &key.material {
        Some(material) => std::borrow::Cow::Borrowed(material.generation.shader().source()),
        None => std::borrow::Cow::Owned(format!(
            "{}\nfn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {{ return sampled * tint; }}",
            nara_material::__STOCK_QUAD_SHADER_SOURCE,
        )),
    };
    #[cfg(test)]
    let source =
        QUAD_SHADER_SOURCE_OVERRIDE.with(|overridden| match (overridden.get(), &key.material) {
            (Some((digest, source)), Some(material)) if material.shader_digest == digest => {
                std::borrow::Cow::Borrowed(source)
            }
            _ => source,
        });
    let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nara_wgpu_quad_shader"),
        source: wgpu::ShaderSource::Wgsl(source),
    });
    finish_quad_pipeline_stage(allocation_scope, validation_scope, &key)?;
    let mut layouts = vec![Some(texture_bind_group_layout)];
    if let Some(material_bind_group_layout) = material_bind_group_layout {
        layouts.push(Some(material_bind_group_layout));
    }
    let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_wgpu_quad_pipeline_layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    finish_quad_pipeline_stage(allocation_scope, validation_scope, &key)?;
    let vertex_buffer_layout = quad_instance_buffer_layout();
    let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_wgpu_quad_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_buffer_layout)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(match key.alpha_mode {
                AlphaMode2d::Opaque => "fs_opaque",
                AlphaMode2d::Blend => "fs_main",
            }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(quad_color_target_state(key.format, key.alpha_mode))],
        }),
        multiview_mask: None,
        cache: None,
    });

    finish_quad_pipeline_stage(allocation_scope, validation_scope, &key)?;
    Ok(WgpuQuadPipeline { key, pipeline })
}

pub(crate) fn create_quad_batch_buffers(
    preparation: &mut NativePreparation<'_>,
    batches: &[WgpuQuadBatch],
    texture_layout: &wgpu::BindGroupLayout,
    material_layouts: &WgpuMaterial2dLayouts,
    texture_cache: &mut WgpuSpriteTextureCache,
    frame_index: u64,
    resources: Option<&WgpuOperationResources<'_>>,
) -> Result<Vec<WgpuQuadBatchBuffer>, WgpuTextureError> {
    let missing_material_layout = || WgpuTextureError::ResourceRejected {
        label: "nara_material_2d_bind_group",
        message: "material bind group layout was not prepared".to_owned(),
    };
    for material in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .filter_map(|batch| batch.material_extension.as_ref())
    {
        let declaration = material.binding.generation().declaration();
        validate_material_device_limits(declaration, &preparation.device().limits())?;
        if !material_layouts.contains_key(&declaration.binding_digest()) {
            return Err(missing_material_layout());
        }
        validate_material_capture(material)?;
        validate_material_texture_resources(material, resources)?;
    }
    batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| {
            let bind_group = texture_cache.bind_group(
                preparation,
                texture_layout,
                batch.material,
                &batch.texture,
                frame_index,
            )?;
            let (material_bind_group, material_textures, material_buffer, material_allocation) =
                if let Some(material) = &batch.material_extension {
                    let declaration = material.binding.generation().declaration();
                    let material_layout = material_layouts
                        .get(&declaration.binding_digest())
                        .ok_or_else(missing_material_layout)?;
                    let views = material
                        .textures
                        .iter()
                        .map(|(_, texture)| match texture {
                            WgpuMaterialTexture::Image(snapshot) => texture_cache
                                .prepared_image_view(preparation, snapshot, frame_index)
                                .map(WgpuMaterialTextureView::Image),
                            WgpuMaterialTexture::Program(source) => {
                                material_program_view(resources, source)
                                    .map(|view| WgpuMaterialTextureView::Program(view.clone()))
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let (parameters, allocation) = preparation.create_buffer_with_data(
                        "nara_material_2d_parameters",
                        material.binding.uniform_bytes(),
                        wgpu::BufferUsages::UNIFORM,
                    )?;
                    let mut entries = vec![wgpu::BindGroupEntry {
                        binding: 0,
                        resource: parameters.as_entire_binding(),
                    }];
                    entries.extend(material.textures.iter().zip(&views).map(
                        |((binding, _), view)| wgpu::BindGroupEntry {
                            binding: *binding,
                            resource: wgpu::BindingResource::TextureView(view.view()),
                        },
                    ));
                    let device = preparation.device();
                    let binding =
                        checked_allocation(device, "nara_material_2d_bind_group", || {
                            device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("nara_material_2d_bind_group"),
                                layout: material_layout,
                                entries: &entries,
                            })
                        })?;
                    (Some(binding), views, Some(parameters), Some(allocation))
                } else {
                    (None, Vec::new(), None, None)
                };
            let instance_count = saturating_u32(batch.instances.len());
            #[cfg(test)]
            let logical_bytes = quad_instance_byte_len(batch.instances.len());
            let (buffer, instance_allocation) = preparation.create_buffer_with_data(
                "nara_wgpu_quad_instances",
                bytemuck::cast_slice(&batch.instances),
                wgpu::BufferUsages::VERTEX,
            )?;

            Ok(WgpuQuadBatchBuffer {
                buffer,
                _instance_allocation: instance_allocation,
                bind_group,
                material_bind_group,
                _material_textures: material_textures,
                _material_buffer: material_buffer,
                _material_allocation: material_allocation,
                material: batch
                    .material_extension
                    .as_ref()
                    .map(|material| WgpuMaterial2dPipelineKey::new(material.binding.generation())),
                alpha_mode: batch.material.alpha_mode,
                scissor: batch.scissor,
                instance_count,
                #[cfg(test)]
                logical_bytes,
            })
        })
        .collect()
}

/// Verifies the complete resource projection before any stock allocation or upload.
pub(crate) fn validate_material_capture(material: &WgpuMaterial2d) -> Result<(), WgpuTextureError> {
    let declaration = material.binding.generation().declaration();
    let valid = material.binding.uniform_bytes().len()
        == declaration.parameters().byte_size() as usize
        && material.textures.len() == declaration.textures().len()
        && material
            .textures
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
        && declaration.textures().iter().all(|texture| {
            material.textures.iter().any(|(binding, source)| {
                *binding == texture.binding()
                    && match source {
                        WgpuMaterialTexture::Image(snapshot) => material
                            .binding
                            .textures()
                            .get(texture.id())
                            .is_some_and(|handle| {
                                snapshot.identity().key() == nara_image::image_resource_key(*handle)
                            }),
                        WgpuMaterialTexture::Program(_) => true,
                    }
            })
        });
    if !valid {
        return Err(WgpuTextureError::ResourceRejected {
            label: "nara_material_2d_capture",
            message: "material uniform and captured textures must match the admitted declaration"
                .to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn validate_material_texture_resources(
    material: &WgpuMaterial2d,
    resources: Option<&WgpuOperationResources<'_>>,
) -> Result<(), WgpuTextureError> {
    for (_, texture) in &material.textures {
        if let WgpuMaterialTexture::Program(source) = texture {
            material_program_view(resources, source)?;
        }
    }
    Ok(())
}

fn material_program_view<'a>(
    resources: Option<&WgpuOperationResources<'a>>,
    source: &WgpuMaterialTextureSource,
) -> Result<&'a wgpu::TextureView, WgpuTextureError> {
    let resources = resources.ok_or_else(|| WgpuTextureError::ResourceRejected {
        label: "nara_material_2d_program_texture",
        message: "program material textures require the original operation resources".to_owned(),
    })?;
    resources
        .material_texture(&source.read())
        .map_err(|error| WgpuTextureError::ResourceRejected {
            label: "nara_material_2d_program_texture",
            message: error.to_string(),
        })
}

/// Compares actual stock pipeline requirements with the enabled device's upper bounds.
pub(crate) fn validate_material_device_limits(
    declaration: &Material2dDeclaration,
    limits: &wgpu::Limits,
) -> Result<(), WgpuTextureError> {
    let uniform_size = u64::from(declaration.parameters().byte_size());
    let texture_count = declaration.textures().len() as u64 + 1;
    let binding_index = declaration
        .textures()
        .iter()
        .map(|texture| u64::from(texture.binding()))
        .max()
        .unwrap_or(0)
        .max(1);
    for (name, required, maximum) in [
        (
            "max_uniform_buffer_binding_size",
            uniform_size,
            limits.max_uniform_buffer_binding_size,
        ),
        ("max_buffer_size", uniform_size, limits.max_buffer_size),
        (
            "max_sampled_textures_per_shader_stage",
            texture_count,
            u64::from(limits.max_sampled_textures_per_shader_stage),
        ),
        (
            "max_bindings_per_bind_group",
            binding_index + 1,
            u64::from(limits.max_bindings_per_bind_group),
        ),
        ("max_bind_groups", 2, u64::from(limits.max_bind_groups)),
        (
            "max_bind_groups_plus_vertex_buffers",
            3,
            u64::from(limits.max_bind_groups_plus_vertex_buffers),
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            1,
            u64::from(limits.max_uniform_buffers_per_shader_stage),
        ),
        (
            "max_samplers_per_shader_stage",
            1,
            u64::from(limits.max_samplers_per_shader_stage),
        ),
    ] {
        if required > maximum {
            return Err(WgpuTextureError::ResourceRejected {
                label: "nara_material_2d_device_limits",
                message: format!(
                    "material {:?} requires {required} for {name}; active device enables {maximum}",
                    declaration.id(),
                ),
            });
        }
    }
    Ok(())
}

pub(crate) fn draw_quad_batch_buffers<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    pipelines: &'pass [WgpuQuadPipelineDrawRef],
    buffers: &'pass [WgpuQuadBatchBuffer],
    viewport: ViewportRect,
) {
    let mut bound_pipeline = None;
    for buffer in buffers {
        let Some(pipeline) = pipelines
            .iter()
            .find(|pipeline| {
                pipeline.alpha_mode == buffer.alpha_mode && pipeline.material == buffer.material
            })
            .map(|pipeline| &pipeline.pipeline)
        else {
            continue;
        };
        if bound_pipeline != Some((buffer.alpha_mode, &buffer.material)) {
            render_pass.set_pipeline(pipeline);
            bound_pipeline = Some((buffer.alpha_mode, &buffer.material));
        }
        let scissor = buffer.scissor.unwrap_or(WgpuScissorRect {
            x: viewport.physical_x,
            y: viewport.physical_y,
            width: viewport.physical_width,
            height: viewport.physical_height,
        });
        render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        render_pass.set_bind_group(0, &buffer.bind_group.bind_group, &[]);
        if let Some(material) = &buffer.material_bind_group {
            render_pass.set_bind_group(1, material, &[]);
        }
        render_pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        render_pass.draw(0..VERTICES_PER_QUAD, 0..buffer.instance_count);
    }
}

pub(crate) fn quad_batch_draw_stats(batches: &[WgpuQuadBatch]) -> WgpuQuadDrawStats {
    let mut stats = WgpuQuadDrawStats::default();
    for batch in batches {
        let instances = saturating_u32(batch.instances.len());
        if instances == 0 {
            continue;
        }
        stats.draw_calls = stats.draw_calls.saturating_add(1);
        if batch.counts_as_sprites {
            stats.sprites = stats.sprites.saturating_add(instances);
            if matches!(batch.material.texture, WgpuQuadTextureKey::Image(_)) {
                stats.image_sprites = stats.image_sprites.saturating_add(instances);
            }
        }
    }
    stats
}

pub(crate) fn admit_quad_instance_buffers(
    batches: &[WgpuQuadBatch],
) -> Result<u64, WgpuQuadInstanceBudgetError> {
    let required = batches.iter().try_fold(0_u64, |total, batch| {
        total.checked_add(quad_instance_byte_len(batch.instances.len()))
    });
    admit_quad_instance_bytes(required.unwrap_or(u64::MAX))
}

fn admit_quad_instance_bytes(required: u64) -> Result<u64, WgpuQuadInstanceBudgetError> {
    if required > MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE {
        return Err(WgpuQuadInstanceBudgetError {
            required,
            maximum: MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE,
        });
    }
    Ok(required)
}

fn quad_instance_buffer_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<WgpuQuadInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &QUAD_INSTANCE_ATTRIBUTES,
    }
}

fn quad_color_target_state(
    format: wgpu::TextureFormat,
    alpha_mode: AlphaMode2d,
) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: match alpha_mode {
            AlphaMode2d::Opaque => None,
            AlphaMode2d::Blend => Some(wgpu::BlendState::ALPHA_BLENDING),
        },
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn quad_instance_byte_len(instance_count: usize) -> wgpu::BufferAddress {
    u64::try_from(instance_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(std::mem::size_of::<WgpuQuadInstance>() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_instance_layout_matches_shader_contract() {
        let layout = quad_instance_buffer_layout();
        assert_eq!(
            layout.array_stride,
            std::mem::size_of::<WgpuQuadInstance>() as wgpu::BufferAddress
        );
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        assert_eq!(layout.attributes.len(), 11);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[5].offset, 48);
        assert_eq!(layout.attributes[6].offset, 56);
        assert_eq!(layout.attributes[7].offset, 60);
        assert_eq!(layout.attributes[8].offset, 64);
        assert_eq!(layout.attributes[9].offset, 72);
        assert_eq!(layout.attributes[10].offset, 76);
        assert_eq!(quad_instance_byte_len(3), 3 * layout.array_stride);
    }

    #[test]
    fn color_target_respects_alpha_mode() {
        assert_eq!(
            quad_color_target_state(wgpu::TextureFormat::Bgra8UnormSrgb, AlphaMode2d::Opaque).blend,
            None
        );
        assert_eq!(
            quad_color_target_state(wgpu::TextureFormat::Bgra8UnormSrgb, AlphaMode2d::Blend).blend,
            Some(wgpu::BlendState::ALPHA_BLENDING)
        );
    }

    #[test]
    fn instance_buffer_budget_is_checked_without_allocating_the_payload() {
        assert_eq!(
            admit_quad_instance_bytes(MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE),
            Ok(MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE)
        );
        assert_eq!(
            admit_quad_instance_bytes(MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE + 1),
            Err(WgpuQuadInstanceBudgetError {
                required: MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE + 1,
                maximum: MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE,
            })
        );
    }

    #[test]
    fn batch_storage_retains_instance_material_and_texture_charges_after_cache_retirement() {
        use crate::{output::OutputBudget, submission::WgpuSubmissionReservation};
        use std::sync::Arc;

        let (handle, [image, _]) = crate::readback_tests::image_versions_with_handle(1, 1);
        crate::readback_tests::render_to_rgba(|device, queue, _, _, _, _| {
            let texture_layout = create_quad_texture_bind_group_layout(device)?;
            let mut cache = WgpuSpriteTextureCache::default();
            let batch = WgpuQuadBatch {
                material: WgpuQuadMaterialKey {
                    texture: WgpuQuadTextureKey::Fallback,
                    sampler: SamplerDescriptor::NEAREST_CLAMP,
                    alpha_mode: AlphaMode2d::Opaque,
                },
                texture: WgpuQuadTexture::Fallback,
                scissor: None,
                instances: vec![WgpuQuadInstance::zeroed(); 2],
                counts_as_sprites: true,
                material_extension: Some(WgpuMaterial2d {
                    binding: crate::readback_tests::material_binding(
                        &crate::readback_tests::material_generation(
                            "test.native-material-custody",
                            AlphaMode2d::Opaque,
                            crate::readback_tests::TEXTURE_MATERIAL_FRAGMENT,
                        ),
                        [1.0; 4],
                        handle,
                    ),
                    textures: vec![(1, image.into())],
                }),
            };
            let layouts =
                crate::readback_tests::material_layouts(device, std::slice::from_ref(&batch))?;
            let expected = quad_instance_byte_len(2) + 16 + 4 + 4;
            let budget = Arc::new(OutputBudget::new(expected));
            let mut reservation = WgpuSubmissionReservation::new(device, queue, ());
            let buffers = create_quad_batch_buffers(
                &mut NativePreparation::new(
                    device,
                    queue,
                    &budget,
                    &mut reservation,
                    &mut Default::default(),
                ),
                &[batch],
                &texture_layout,
                &layouts,
                &mut cache,
                1,
                None,
            )?;
            assert_eq!(buffers.len(), 1);
            assert_eq!(buffers[0].buffer.size(), quad_instance_byte_len(2));
            assert_eq!(buffers[0]._material_buffer.as_ref().unwrap().size(), 16);
            assert_eq!(budget.retained(), expected);
            drop(cache);
            let index = queue.submit([]);
            let prefix = reservation.submitted(index.clone());
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(index),
                    timeout: Some(std::time::Duration::from_secs(10)),
                })
                .unwrap();
            assert!(prefix.poll().unwrap());
            assert_eq!(
                budget.retained(),
                expected,
                "the actual prepared storage still owns its guards"
            );
            drop(buffers);
            assert_eq!(budget.retained(), 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn draw_stats_distinguish_image_sprites_from_fallback_quads() {
        let fallback = WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Fallback,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
            },
            texture: WgpuQuadTexture::Fallback,
            scissor: None,
            instances: vec![WgpuQuadInstance::zeroed(); 2],
            counts_as_sprites: true,
            material_extension: None,
        };
        let image = WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Image(RenderResourceKey::new(
                    nara_asset::allocate_anonymous_handle::<()>().unwrap().id(),
                    nara_render::RenderResourceKind::IMAGE_2D,
                )),
                ..fallback.material
            },
            instances: vec![WgpuQuadInstance::zeroed(); 3],
            ..fallback.clone()
        };
        let ui = WgpuQuadBatch {
            counts_as_sprites: false,
            ..image.clone()
        };

        let stats = quad_batch_draw_stats(&[fallback, image, ui]);

        assert_eq!(stats.draw_calls, 3);
        assert_eq!(stats.sprites, 5);
        assert_eq!(stats.image_sprites, 3);
    }
}
