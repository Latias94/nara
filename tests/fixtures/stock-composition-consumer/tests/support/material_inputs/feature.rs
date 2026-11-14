use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_render_wgpu::{
    WgpuFeatureAllocation, WgpuFeatureEncodeContext, WgpuFeaturePreflightContext,
    WgpuFeaturePrepareContext, WgpuProgramTexture, WgpuRenderFeature, WgpuRenderProgramError,
};

use super::{Palette, descriptor};

pub struct PaletteWriter {
    pub live: Arc<AtomicUsize>,
}

pub struct PaletteCapture {
    pub destination: WgpuProgramTexture<Palette>,
    pub colors: [[f32; 4]; 4],
}

pub struct NativeLifetime(Arc<AtomicUsize>);

impl NativeLifetime {
    fn new(live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(live))
    }
}

impl Drop for NativeLifetime {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct PalettePrepared {
    destination: WgpuProgramTexture<Palette>,
    pipeline: wgpu::RenderPipeline,
    bindings: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
    _lifetime: NativeLifetime,
}

impl WgpuRenderFeature for PaletteWriter {
    type Capture = PaletteCapture;
    type State = NativeLifetime;
    type Prepared = PalettePrepared;
    type Report = ();

    fn preflight(
        &self,
        capture: &PaletteCapture,
        _: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        if capture.destination.descriptor()
            != &descriptor(capture.destination.descriptor().native.format)
            || capture
                .colors
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
        {
            return Err(WgpuRenderProgramError::feature(
                "finite 2x2 palette required",
            ));
        }
        Ok(())
    }

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<NativeLifetime, WgpuRenderProgramError> {
        Ok(NativeLifetime::new(&self.live))
    }

    fn prepare(
        &self,
        _: &mut NativeLifetime,
        capture: &PaletteCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<PalettePrepared, WgpuRenderProgramError> {
        context.resources().texture(&capture.destination)?;
        let allocation = context.reserve_bytes(64)?;
        let device = context.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("external-material-palette-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("external-material-palette-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("external-material-palette-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("palette.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("external-material-palette-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: capture.destination.descriptor().native.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("external-material-palette-uniform"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let bytes = capture.colors.map(|color| color.map(f32::to_le_bytes));
        uniform
            .get_mapped_range_mut(..)
            .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
            .copy_from_slice(bytes.as_flattened().as_flattened());
        uniform.unmap();
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("external-material-palette-bindings"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Ok(PalettePrepared {
            destination: capture.destination.clone(),
            pipeline,
            bindings,
            _uniform: uniform,
            _allocation: allocation,
            _lifetime: NativeLifetime::new(&self.live),
        })
    }

    fn encode(
        &self,
        prepared: &PalettePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        let (resources, encoder) = context.split();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("external-material-palette-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: resources.texture(&prepared.destination)?,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&prepared.pipeline);
        pass.set_bind_group(0, &prepared.bindings, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}
