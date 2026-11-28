use std::{collections::HashMap, mem::size_of};

use nara_ecs::Resource;

use crate::{Color, Extent2d};

use super::{
    PassAttachments, RenderPass, RenderPassContext, RenderTargetView, WgpuRenderError,
    canvas::CanvasViewport, pass::begin_color_pass, premultiplied_clear_color,
};

/// Optional world effects captured from the ECS world for each render destination.
///
/// The default and zero-strength effects keep the original scene path. Effects run before the HUD
/// and editor overlay. With a `WorldCanvas`, they affect the world raster inside the fitted area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Resource)]
pub struct PostProcess2d {
    pub bloom: Option<Bloom2d>,
    pub vignette: Option<Vignette2d>,
    pub screen_fade: Option<ScreenFade2d>,
}

impl PostProcess2d {
    /// Returns whether the renderer needs a separate scene attachment for these effects.
    #[must_use]
    pub fn is_active(self) -> bool {
        self.has_bloom()
            || self
                .vignette
                .is_some_and(|vignette| unit(vignette.intensity) > 0.0)
            || self.screen_fade.is_some_and(|fade| fade.opacity() > 0.0)
    }

    pub(super) fn has_bloom(self) -> bool {
        self.bloom
            .is_some_and(|bloom| nonnegative(bloom.intensity) > 0.0)
    }
}

/// Blurs the bright parts of the world and adds the light back within the scene's coverage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom2d {
    /// Extraction threshold on the largest premultiplied linear RGB channel.
    pub threshold: f32,
    /// Nonnegative multiplier for the extracted light. Zero disables bloom.
    pub intensity: f32,
    /// Blur radius in world raster pixels. Downsampling still applies at zero radius.
    pub radius: f32,
}

/// Darkens the edges of the world while preserving its alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vignette2d {
    /// Edge darkening from zero to one. Zero disables the vignette.
    pub intensity: f32,
    /// Width of the transition toward the center, from zero to one.
    pub smoothness: f32,
}

/// Blends a solid color over the world after bloom and vignette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenFade2d {
    /// Straight linear RGBA in the range zero to one.
    pub color: Color,
    /// Opacity multiplier from zero to one. The effective opacity also includes `color.a`.
    pub alpha: f32,
}

impl ScreenFade2d {
    fn opacity(self) -> f32 {
        unit(self.alpha) * unit(self.color.a)
    }
}

fn unit(value: f32) -> f32 {
    nonnegative(value).min(1.0)
}

fn nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
pub(super) struct PostProcessInput<'a> {
    pub(super) scene: RenderTargetView<'a>,
    pub(super) bloom: Option<[RenderTargetView<'a>; 2]>,
}

#[derive(Default)]
pub(super) struct PostProcessPass {
    native: Option<NativePostProcess>,
}

impl PostProcessPass {
    pub(super) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
}

impl RenderPass for PostProcessPass {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::POST_PROCESS
    }

    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let Some(input) = context.post_process else {
            return Ok(());
        };
        let target = context.target;
        let canvas = context.frame.packet().world_canvas();
        let viewport = canvas.map(CanvasViewport::new);
        let rect = viewport.map_or(
            [
                0.0,
                0.0,
                target.extent.width as f32,
                target.extent.height as f32,
            ],
            |viewport| viewport.edges,
        );
        let clear = canvas.map_or(wgpu::Color::TRANSPARENT, |canvas| {
            premultiplied_clear_color(canvas.bar_color())
        });
        let settings = Settings::new(
            context.frame.post_process(),
            input.scene.extent,
            rect,
            target.format,
        );
        let uniform = context.upload_buffer(
            "nara_post_process_settings",
            wgpu::BufferUsages::UNIFORM,
            bytemuck::bytes_of(&settings),
        )?;
        let native = self.native.get_or_insert_with(|| NativePostProcess {
            shader: context
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("nara_post_process"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("post_process.wgsl").into()),
                }),
            bloom: None,
            composites: HashMap::new(),
        });
        let shader = &native.shader;
        let bloom = if let Some(scratch) = input.bloom {
            let pipelines = native
                .bloom
                .get_or_insert_with(|| BloomPipelines::new(context.device, shader));
            pipelines.record(context, input.scene.view, scratch, &uniform);
            Some((&pipelines.sampler, scratch[0].view))
        } else {
            None
        };
        let composite = native
            .composites
            .entry((target.format, bloom.is_some()))
            .or_insert_with(|| {
                let entry = if bloom.is_some() {
                    "fs_composite_bloom"
                } else {
                    "fs_composite"
                };
                FullscreenPipeline::new(
                    context.device,
                    shader,
                    target.format,
                    "nara_post_composite",
                    entry,
                )
            });

        let [source, settings] = source_bindings(input.scene.view, &uniform);
        if let Some((sampler, bloom)) = bloom {
            composite.record(
                context,
                target.view,
                &[
                    source,
                    settings,
                    sampler_binding(sampler),
                    texture_binding(3, bloom),
                ],
                clear,
                viewport,
            );
        } else {
            composite.record(context, target.view, &[source, settings], clear, viewport);
        }
        Ok(())
    }

    fn clear_caches(&mut self) {
        self.native = None;
    }
}

const HALF_FLOAT_MAX: f32 = 65_504.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Settings {
    physical_rect: [f32; 4],
    fade: [f32; 4],
    bloom_threshold: f32,
    bloom_intensity: f32,
    bloom_radius: f32,
    vignette_intensity: f32,
    vignette_smoothness: f32,
    rgb_limit: f32,
    padding: [f32; 2],
}

impl Settings {
    fn new(
        effects: PostProcess2d,
        scene: Extent2d,
        physical_rect: [f32; 4],
        format: wgpu::TextureFormat,
    ) -> Self {
        let fade = effects.screen_fade.map_or([0.0; 4], |fade| {
            let a = fade.opacity();
            [
                unit(fade.color.r) * a,
                unit(fade.color.g) * a,
                unit(fade.color.b) * a,
                a,
            ]
        });
        let bloom = effects.bloom;
        let max_radius = scene.width.max(scene.height) as f32;
        let vignette = effects.vignette;
        // Normalized RGBA outputs carry premultiplied display values. Float targets also carry
        // the headroom of the linear scene raster.
        let rgb_limit = match format.remove_srgb_suffix() {
            wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Rgb10a2Unorm
            | wgpu::TextureFormat::Rgba16Unorm => 1.0,
            _ => HALF_FLOAT_MAX,
        };
        Self {
            physical_rect,
            fade,
            bloom_threshold: bloom.map_or(0.0, |bloom| nonnegative(bloom.threshold)),
            bloom_intensity: bloom.map_or(0.0, |bloom| {
                nonnegative(bloom.intensity).min(HALF_FLOAT_MAX)
            }),
            bloom_radius: bloom.map_or(0.0, |bloom| nonnegative(bloom.radius).min(max_radius)),
            vignette_intensity: vignette.map_or(0.0, |vignette| unit(vignette.intensity)),
            vignette_smoothness: vignette.map_or(0.0, |vignette| unit(vignette.smoothness)),
            rgb_limit,
            padding: [0.0; 2],
        }
    }
}

struct NativePostProcess {
    shader: wgpu::ShaderModule,
    bloom: Option<BloomPipelines>,
    composites: HashMap<(wgpu::TextureFormat, bool), FullscreenPipeline>,
}

struct BloomPipelines {
    sampler: wgpu::Sampler,
    extract: FullscreenPipeline,
    horizontal: FullscreenPipeline,
    vertical: FullscreenPipeline,
}

impl BloomPipelines {
    fn new(device: &wgpu::Device, shader: &wgpu::ShaderModule) -> Self {
        let pipeline = |label, entry| {
            FullscreenPipeline::new(device, shader, PostProcessPass::FORMAT, label, entry)
        };
        Self {
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("nara_bloom_linear_sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            extract: pipeline("nara_bloom_extract", "fs_extract"),
            horizontal: pipeline("nara_bloom_horizontal", "fs_horizontal"),
            vertical: pipeline("nara_bloom_vertical", "fs_vertical"),
        }
    }

    fn record(
        &self,
        context: &mut RenderPassContext<'_>,
        scene: &wgpu::TextureView,
        [first, second]: [RenderTargetView<'_>; 2],
        uniform: &wgpu::Buffer,
    ) {
        self.extract.record(
            context,
            first.view,
            &source_bindings(scene, uniform),
            wgpu::Color::TRANSPARENT,
            None,
        );
        for (pipeline, source, target) in [
            (&self.horizontal, first, second),
            (&self.vertical, second, first),
        ] {
            let [source, settings] = source_bindings(source.view, uniform);
            pipeline.record(
                context,
                target.view,
                &[source, settings, sampler_binding(&self.sampler)],
                wgpu::Color::TRANSPARENT,
                None,
            );
        }
    }
}

struct FullscreenPipeline {
    label: &'static str,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl FullscreenPipeline {
    fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
        label: &'static str,
        fragment: &'static str,
    ) -> Self {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: None,
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        Self {
            label,
            pipeline,
            layout,
        }
    }

    fn record(
        &self,
        context: &mut RenderPassContext<'_>,
        target: &wgpu::TextureView,
        entries: &[wgpu::BindGroupEntry<'_>],
        clear: wgpu::Color,
        viewport: Option<CanvasViewport>,
    ) {
        let binding = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(self.label),
                layout: &self.layout,
                entries,
            });
        let mut pass = begin_color_pass(
            context.encoder,
            target,
            self.label,
            wgpu::LoadOp::Clear(clear),
        );
        if viewport.is_none_or(|viewport| viewport.apply(&mut pass)) {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &binding, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

fn source_bindings<'a>(
    source: &'a wgpu::TextureView,
    uniform: &'a wgpu::Buffer,
) -> [wgpu::BindGroupEntry<'a>; 2] {
    [
        texture_binding(0, source),
        wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: uniform,
                offset: 0,
                size: wgpu::BufferSize::new(size_of::<Settings>() as u64),
            }),
        },
    ]
}

fn texture_binding(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn sampler_binding(sampler: &wgpu::Sampler) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding: 2,
        resource: wgpu::BindingResource::Sampler(sampler),
    }
}
