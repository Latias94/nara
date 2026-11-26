use std::{borrow::Cow, collections::HashMap};

use crate::gpu::{WgpuRenderError, device::validate_custom_pipeline, pass::texture_layout_entry};
use crate::material::{
    __STOCK_QUAD_SHADER_SOURCE, AlphaMode2d, Material2dDeclaration, Material2dGeneration,
};
use crate::sprite_render::batching::PreparedMaterial2d;

use super::{geometry::vertex_layout, view_uniform::VIEW_UNIFORM_BYTES};

const MAX_CACHED_PIPELINES: usize = 64;
const MAX_CACHED_UNIFORM_BINDINGS: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    format: wgpu::TextureFormat,
    alpha: AlphaMode2d,
    material: Option<([u8; 32], [u8; 32])>,
}

#[derive(Clone)]
pub(super) struct Pipeline {
    pub(super) native: wgpu::RenderPipeline,
    pub(super) material_layout: Option<wgpu::BindGroupLayout>,
}

#[derive(Default)]
pub(super) struct Pipelines {
    pub(super) view_layout: Option<wgpu::BindGroupLayout>,
    entries: HashMap<PipelineKey, (Pipeline, u64)>,
}

impl Pipelines {
    pub(super) fn get(
        &mut self,
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        alpha: AlphaMode2d,
        material: Option<&Material2dGeneration>,
        frame: u64,
    ) -> Result<Pipeline, WgpuRenderError> {
        let key = PipelineKey {
            format,
            alpha,
            material: material.map(|generation| {
                (
                    generation.shader().digest(),
                    generation.declaration().binding_digest(),
                )
            }),
        };
        if let Some((pipeline, used_at)) = self.entries.get_mut(&key) {
            *used_at = frame;
            return Ok(pipeline.clone());
        }
        let pipeline = if let Some(generation) = material {
            validate_device(generation.declaration(), &device.limits())?;
            let view_layout = self.view_layout.get_or_insert_with(|| {
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("nara_scene_view"),
                    entries: &[uniform_layout_entry(
                        VIEW_UNIFORM_BYTES,
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                    )],
                })
            });
            validate_custom_pipeline(device, || {
                create_pipeline(device, texture_layout, key, Some((generation, view_layout)))
            })?
        } else {
            create_pipeline(device, texture_layout, key, None)
        };
        if self.entries.len() == MAX_CACHED_PIPELINES
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used_at))| *used_at)
                .map(|(key, _)| *key)
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (pipeline.clone(), frame));
        Ok(pipeline)
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    texture_layout: &wgpu::BindGroupLayout,
    key: PipelineKey,
    material: Option<(&Material2dGeneration, &wgpu::BindGroupLayout)>,
) -> Pipeline {
    const STOCK_FRAGMENT: &str = "\nfn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint; }";
    let source = match material {
        Some((generation, _)) => Cow::Borrowed(generation.shader().source()),
        None => Cow::Owned([__STOCK_QUAD_SHADER_SOURCE, STOCK_FRAGMENT].concat()),
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nara_sprite_tile"),
        source: wgpu::ShaderSource::Wgsl(source),
    });
    let material_layout =
        material.map(|(generation, _)| material_layout(device, generation.declaration()));
    let layouts = [
        Some(texture_layout),
        material_layout.as_ref(),
        material.map(|(_, layout)| layout),
    ];
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_sprite_tile"),
        bind_group_layouts: &layouts[..if material.is_some() { 3 } else { 1 }],
        immediate_size: 0,
    });
    let native = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_sprite_tile"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(vertex_layout())],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(match key.alpha {
                AlphaMode2d::Opaque => "fs_opaque",
                AlphaMode2d::Blend => "fs_main",
            }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: key.format,
                blend: (key.alpha == AlphaMode2d::Blend)
                    .then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    Pipeline {
        native,
        material_layout,
    }
}

fn material_layout(
    device: &wgpu::Device,
    declaration: &Material2dDeclaration,
) -> wgpu::BindGroupLayout {
    let mut entries = vec![uniform_layout_entry(
        u64::from(declaration.parameters().byte_size()),
        wgpu::ShaderStages::FRAGMENT,
    )];
    entries.extend(
        declaration
            .textures()
            .iter()
            .map(|texture| texture_layout_entry(texture.binding())),
    );
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nara_material_2d"),
        entries: &entries,
    })
}

fn uniform_layout_entry(size: u64, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

#[derive(Default)]
pub(super) struct UniformBindings {
    entries: HashMap<UniformKey, (wgpu::BindGroup, u64)>,
}

#[derive(PartialEq, Eq, Hash)]
struct UniformKey {
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    offset: u64,
    size: Option<wgpu::BufferSize>,
    textures: Vec<(u32, wgpu::TextureView)>,
}

impl UniformBindings {
    pub(super) fn prune(&mut self, frame: u64) {
        self.entries
            .retain(|_, (_, used_at)| frame.saturating_sub(*used_at) <= 2);
    }

    pub(super) fn bind(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: wgpu::BufferBinding<'_>,
        textures: Vec<(u32, wgpu::TextureView)>,
        frame: u64,
    ) -> wgpu::BindGroup {
        let key = UniformKey {
            layout: layout.clone(),
            buffer: uniform.buffer.clone(),
            offset: uniform.offset,
            size: uniform.size,
            textures,
        };
        if let Some((group, used_at)) = self.entries.get_mut(&key) {
            *used_at = frame;
            return group.clone();
        }
        let mut entries = Vec::with_capacity(1 + key.textures.len());
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(uniform),
        });
        entries.extend(
            key.textures
                .iter()
                .map(|(binding, view)| wgpu::BindGroupEntry {
                    binding: *binding,
                    resource: wgpu::BindingResource::TextureView(view),
                }),
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nara_scene_uniforms"),
            layout,
            entries: &entries,
        });
        if self.entries.len() < MAX_CACHED_UNIFORM_BINDINGS {
            self.entries.insert(key, (group.clone(), frame));
        }
        group
    }
}

pub(super) fn validate_capture(material: &PreparedMaterial2d) -> Result<(), WgpuRenderError> {
    let declaration = material.binding.generation().declaration();
    let valid = material.binding.uniform_bytes().len()
        == declaration.parameters().byte_size() as usize
        && material.textures.len() == declaration.textures().len()
        && material
            .textures
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0)
        && declaration.textures().iter().all(|texture| {
            material
                .binding
                .textures()
                .get(texture.id())
                .is_some_and(|handle| {
                    material.textures.iter().any(|(binding, image)| {
                        *binding == texture.binding()
                            && image.identity().key() == crate::image::image_resource_key(*handle)
                    })
                })
        });
    if valid {
        Ok(())
    } else {
        Err(WgpuRenderError::Validation(format!(
            "material {:?} capture does not match its uniform and texture declarations",
            declaration.id(),
        )))
    }
}

fn validate_device(
    declaration: &Material2dDeclaration,
    limits: &wgpu::Limits,
) -> Result<(), WgpuRenderError> {
    let uniform = u64::from(declaration.parameters().byte_size()).max(VIEW_UNIFORM_BYTES);
    let max_binding = declaration
        .textures()
        .iter()
        .map(|texture| texture.binding())
        .max()
        .unwrap_or(0)
        .max(1);
    for (limit, required, available) in [
        (
            "max_uniform_buffer_binding_size",
            uniform,
            limits.max_uniform_buffer_binding_size,
        ),
        ("max_buffer_size", uniform, limits.max_buffer_size),
        (
            "max_sampled_textures_per_shader_stage",
            declaration.textures().len() as u64 + 1,
            u64::from(limits.max_sampled_textures_per_shader_stage),
        ),
        (
            "max_bindings_per_bind_group",
            u64::from(max_binding) + 1,
            u64::from(limits.max_bindings_per_bind_group),
        ),
        ("max_bind_groups", 3, u64::from(limits.max_bind_groups)),
        (
            "max_bind_groups_plus_vertex_buffers",
            4,
            u64::from(limits.max_bind_groups_plus_vertex_buffers),
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            2,
            u64::from(limits.max_uniform_buffers_per_shader_stage),
        ),
        (
            "max_samplers_per_shader_stage",
            1,
            u64::from(limits.max_samplers_per_shader_stage),
        ),
    ] {
        if required > available {
            return Err(WgpuRenderError::Validation(format!(
                "material {:?} requires {limit} >= {required}, device enables {available}",
                declaration.id(),
            )));
        }
    }
    Ok(())
}
