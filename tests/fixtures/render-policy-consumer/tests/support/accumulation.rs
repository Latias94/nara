//! Two independently replaceable temporal effects per View, through public policy APIs only.

use std::{num::NonZeroU64, sync::Arc};

use nara::core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, FrameStats, RenderFramePacket, RenderImagePacket,
    RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_image_render_frame_packet,
};
use nara_render_wgpu::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuEmptyHistoryReason,
    WgpuFeatureAllocation, WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePrepareContext, WgpuHistoryLineage, WgpuHistoryTexture,
    WgpuHistoryView, WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyFrameFacts,
    WgpuPolicyProgram, WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderFeature,
    WgpuRenderPolicy, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

pub const VIEW_WIDTH: u32 = 16;
pub const WIDTH: u32 = 2 * VIEW_WIDTH;
pub const HEIGHT: u32 = 16;
pub const HISTORY_SLOT: u64 = 0;
const PARAMETER_BYTES: u64 = 16;

pub struct HistoryColor;
pub struct FinalColor;

/// The assertion is captured with the domain snapshot, never obtained from a latest-slot lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Predecessor {
    Empty(WgpuEmptyHistoryReason),
    Frame(u64),
}

#[derive(Clone, Debug)]
pub struct Effect {
    pub ordinal: usize,
    pub tag: u32,
    pub declaration: WgpuFeatureDeclaration<Accumulation>,
}

impl Effect {
    pub fn new(ordinal: usize, tag: u32) -> Self {
        assert!(ordinal < 2 && tag < 8);
        // Declare exactly once on the original definition for this effect incarnation.
        let definition = WgpuFeatureDefinition::new(Accumulation { tag });
        Self {
            ordinal,
            tag,
            declaration: definition.declare(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ViewTopology {
    pub index: usize,
    pub id: RenderViewId,
    pub history_view: WgpuHistoryView,
    pub effects: [Effect; 2],
}

impl ViewTopology {
    pub fn region(&self) -> ViewportRect {
        ViewportRect::new(self.index as u32 * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT).unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct Topology {
    pub views: [ViewTopology; 2],
}

impl Topology {
    pub fn new() -> Self {
        let root = RenderViewId::try_acquire().expect("an independent View root lifetime");
        Self {
            views: std::array::from_fn(|view| ViewTopology {
                index: view,
                id: root.subview(view as u32),
                history_view: WgpuHistoryView::new(root.subview(view as u32)),
                effects: std::array::from_fn(|effect| {
                    Effect::new(effect, (2 * view + effect) as u32)
                }),
            }),
        }
    }

    pub fn replace(&mut self, view: usize, effect: usize, tag: u32) {
        self.views[view].effects[effect] = Effect::new(effect, tag);
    }

    pub fn capture(
        &self,
        lineage: &WgpuHistoryLineage,
        phase: u32,
        reverse: bool,
        predecessors: [Predecessor; 4],
    ) -> Arc<HistoryCapture> {
        assert!(phase < 64);
        let mut views = self.views.clone().map(|view| {
            let mut effects = view.effects.clone().map(|effect| CapturedEffect {
                predecessor: predecessors[2 * view.index + effect.ordinal],
                effect,
            });
            if reverse {
                effects.reverse();
            }
            CapturedView {
                index: view.index,
                id: view.id,
                region: view.region(),
                history_view: view.history_view,
                effects,
            }
        });
        if reverse {
            views.reverse();
        }
        Arc::new(HistoryCapture {
            lineage: lineage.clone(),
            phase,
            views,
            reverse_outputs: reverse,
        })
    }
}

#[derive(Clone, Debug)]
struct CapturedEffect {
    effect: Effect,
    predecessor: Predecessor,
}

#[derive(Clone, Debug)]
struct CapturedView {
    index: usize,
    id: RenderViewId,
    history_view: WgpuHistoryView,
    region: ViewportRect,
    effects: [CapturedEffect; 2],
}

#[derive(Debug)]
pub struct HistoryCapture {
    pub lineage: WgpuHistoryLineage,
    pub phase: u32,
    views: [CapturedView; 2],
    reverse_outputs: bool,
}

impl HistoryCapture {
    pub fn packet(&self, frame: u64, target: RenderImagePacket) -> RenderFramePacket {
        let mut views = ExtractedViews::default();
        for (order, view) in self.views.iter().enumerate() {
            views.push(ExtractedView {
                id: view.id,
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::Image(target.image()),
                viewport: view.region,
                mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, HEIGHT as f32, view.region)
                    .unwrap(),
                order: order as i32,
                load: if order == 0 {
                    ViewLoadOp::Clear(Color::BLACK)
                } else {
                    ViewLoadOp::Load
                },
            });
        }
        build_image_render_frame_packet(self.lineage.source_generation(), frame, &views, target)
            .expect("ordered View regions and explicit target initialization must validate")
            .expect("two Views produce one packet")
    }
}

pub struct HistoryPolicy {
    composite: WgpuFeatureDeclaration<Composite>,
}

impl HistoryPolicy {
    pub fn new() -> Self {
        Self {
            composite: WgpuFeatureDefinition::new(Composite).declare(),
        }
    }
}

pub struct HistoryBindings {
    composite: Arc<WgpuFeatureInstance<Composite>>,
    views: Vec<BoundView>,
}

struct BoundView {
    effects: Vec<BoundEffect>,
}

struct BoundEffect {
    instance: Arc<WgpuFeatureInstance<Accumulation>>,
    history: WgpuHistoryTexture<HistoryColor>,
}

impl WgpuRenderPolicy for HistoryPolicy {
    type Capture = HistoryCapture;
    type Bindings = HistoryBindings;

    fn history_lineage(&self, capture: &HistoryCapture) -> Option<WgpuHistoryLineage> {
        Some(capture.lineage.clone())
    }

    fn bind(
        &self,
        capture: &HistoryCapture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<HistoryBindings, WgpuRenderProgramError> {
        let mut views = Vec::with_capacity(2);
        for view in &capture.views {
            let mut effects = Vec::with_capacity(2);
            for captured in &view.effects {
                let declaration = &captured.effect.declaration;
                effects.push(BoundEffect {
                    instance: context.feature(declaration)?,
                    history: context.history_texture::<HistoryColor, _>(
                        &view.history_view,
                        declaration,
                        HISTORY_SLOT,
                        history_descriptor(),
                    )?,
                });
            }
            views.push(BoundView { effects });
        }
        Ok(HistoryBindings {
            composite: context.feature(&self.composite)?,
            views,
        })
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &HistoryCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        if packet.source_generation() != capture.lineage.source_generation()
            || packet.world_canvas().is_some()
            || packet.target().extent() != Extent2d::new(WIDTH, HEIGHT).unwrap()
            || packet.views().len() != 2
            || packet.views().iter().zip(&capture.views).enumerate().any(
                |(index, (actual, captured))| {
                    actual.id != captured.id
                        || actual.viewport != captured.region
                        || actual.load
                            != if index == 0 {
                                ViewLoadOp::Clear(Color::BLACK)
                            } else {
                                ViewLoadOp::Load
                            }
                },
            )
        {
            return Err(WgpuRenderProgramError::feature(
                "accumulation requires two original View regions on one equal-sized image target",
            ));
        }
        Ok(())
    }

    fn build(
        &self,
        bindings: &HistoryBindings,
        capture: &HistoryCapture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        self.validate_capture(context.packet(), capture)?;
        if !matches!(
            context.format(),
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(WgpuRenderProgramError::feature(
                "the independent history consumer requires an RGBA8 or BGRA8 destination",
            ));
        }
        let packet = context.packet();
        let mut builder = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            limits(),
        )?;
        let mut outputs = Vec::with_capacity(5);
        let mut histories = Vec::with_capacity(4);
        let mut final_image: Option<WgpuProgramTexture<FinalColor>> = None;

        for (order, (view, bound)) in capture.views.iter().zip(&bindings.views).enumerate() {
            let mut colors = Vec::with_capacity(2);
            for (captured, effect) in view.effects.iter().zip(&bound.effects) {
                let key = 100 + (view.index * 4 + captured.effect.ordinal * 2) as u64;
                let previous = context.previous_texture(&effect.history)?;
                let reason = context.empty_texture_reason(&effect.history)?;
                let actual = match (&previous, reason) {
                    (Some(output), None) => Predecessor::Frame(output.frame_index()),
                    (None, Some(reason)) => Predecessor::Empty(reason),
                    _ => {
                        return Err(WgpuRenderProgramError::feature(
                            "a pinned predecessor must be either an exact output or an explicit empty reason",
                        ));
                    }
                };
                if actual != captured.predecessor {
                    return Err(WgpuRenderProgramError::feature(format!(
                        "View {} effect {} expected {:?}, received {actual:?}",
                        view.index, captured.effect.ordinal, captured.predecessor,
                    )));
                }
                let previous = if let Some(output) = previous {
                    builder.import_texture(&output)?
                } else {
                    // This is a real initialized local texture, never a fabricated import.
                    let seed = builder.texture::<HistoryColor>(history_descriptor())?;
                    builder.initialize(
                        key,
                        &[seed.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
                    )?;
                    seed
                };
                let color = builder.texture::<HistoryColor>(history_descriptor())?;
                builder.operation(
                    key + 1,
                    &effect.instance,
                    Arc::new(AccumulationCapture {
                        previous: previous.clone(),
                        destination: color.clone(),
                        phase: capture.phase,
                    }),
                    &[previous.read(AccessRole::Sampled)],
                    &[color.clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)],
                )?;
                histories.push(effect.history.bind(&color));
                outputs.push(color.output());
                colors.push((captured.effect.ordinal, color));
            }

            let upper = &colors.iter().find(|(ordinal, _)| *ordinal == 0).unwrap().1;
            let lower = &colors.iter().find(|(ordinal, _)| *ordinal == 1).unwrap().1;
            let destination = builder.texture::<FinalColor>(final_descriptor(context.format()))?;
            let write = match (&final_image, packet.views()[order].load) {
                (None, ViewLoadOp::Clear(_)) => {
                    destination.clear(wgpu::Color::BLACK, AccessRole::Attachment)
                }
                (Some(previous), ViewLoadOp::Load) => {
                    destination.preserve(previous, AccessRole::Attachment)
                }
                _ => {
                    return Err(WgpuRenderProgramError::feature(
                        "View composition requires an ordered clear followed by a preserved load",
                    ));
                }
            };
            builder.operation(
                500 + order as u64,
                &bindings.composite,
                Arc::new(CompositeCapture {
                    upper: upper.clone(),
                    lower: lower.clone(),
                    destination: destination.clone(),
                    region: view.region,
                }),
                &[
                    upper.read(AccessRole::Sampled),
                    lower.read(AccessRole::Sampled),
                ],
                &[write],
            )?;
            final_image = Some(destination);
        }

        let final_image = final_image.expect("two captured Views have a complete final image");
        outputs.push(final_image.output());
        if capture.reverse_outputs {
            outputs.reverse();
        }
        // Binding order is deliberately independent of publication and operation order.
        histories.reverse();
        Ok(WgpuPolicyProgram::new(
            builder.finish(&outputs)?,
            &final_image,
            WgpuPolicyFrameFacts {
                frame: FrameStats {
                    draw_calls: 6,
                    sprites: 0,
                },
                ..WgpuPolicyFrameFacts::default()
            },
        )?
        .with_history(histories))
    }
}

pub fn limits() -> GraphLimits {
    GraphLimits {
        max_resources: 12,
        max_operations: 12,
        max_accesses: 40,
        max_outputs: 5,
        max_modeled_resource_bytes: 1024 * 1024,
        max_modeled_metadata_bytes: 8 * 1024 * 1024,
        ..GraphLimits::default()
    }
}

pub fn history_descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("independent-linear-history"),
            size: wgpu::Extent3d {
                width: VIEW_WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: TextureTransfer::Linear,
        alpha: TextureAlpha::Premultiplied,
    }
}

fn final_descriptor(format: wgpu::TextureFormat) -> WgpuProgramTextureDescriptor {
    let mut descriptor = history_descriptor();
    descriptor.native.label = Some("independent-history-final");
    descriptor.native.size.width = WIDTH;
    descriptor.native.format = format;
    descriptor.transfer = if format.has_srgb_suffix() {
        TextureTransfer::Srgb
    } else {
        TextureTransfer::Linear
    };
    descriptor.native.usage |= wgpu::TextureUsages::COPY_DST;
    descriptor.alpha = TextureAlpha::Opaque;
    descriptor
}

pub struct Accumulation {
    tag: u32,
}

pub struct AccumulationCapture {
    previous: WgpuProgramTexture<HistoryColor>,
    destination: WgpuProgramTexture<HistoryColor>,
    phase: u32,
}

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

pub struct AccumulationPrepared {
    destination: WgpuProgramTexture<HistoryColor>,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    _allocation: WgpuFeatureAllocation,
}

impl WgpuRenderFeature for Accumulation {
    type Capture = AccumulationCapture;
    type State = Pipeline;
    type Prepared = AccumulationPrepared;
    type Report = ();

    fn create_state(
        &self,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Pipeline, WgpuRenderProgramError> {
        Ok(pipeline(
            context.device(),
            wgpu::TextureFormat::Rgba32Float,
            "accumulate",
            &[
                texture_binding(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(PARAMETER_BYTES),
                    },
                    count: None,
                },
            ],
        ))
    }

    fn prepare(
        &self,
        state: &mut Pipeline,
        capture: &AccumulationCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<AccumulationPrepared, WgpuRenderProgramError> {
        let allocation = context.reserve_bytes(PARAMETER_BYTES)?;
        let uniform = context.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("independent-accumulation-parameters"),
            size: PARAMETER_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes: Vec<u8> = [capture.phase, self.tag, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        context.upload_buffer(&uniform, 0, &bytes)?;
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent-accumulation-inputs"),
                layout: &state.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            context.resources().texture(&capture.previous)?,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
        Ok(AccumulationPrepared {
            destination: capture.destination.clone(),
            pipeline: state.pipeline.clone(),
            group,
            _uniform: uniform,
            _allocation: allocation,
        })
    }

    fn encode(
        &self,
        prepared: &AccumulationPrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        draw(
            context,
            &prepared.destination,
            &prepared.pipeline,
            &prepared.group,
            ViewportRect::new(0, 0, VIEW_WIDTH, HEIGHT).unwrap(),
        )
    }
}

struct Composite;

#[derive(Clone)]
struct CompositeCapture {
    upper: WgpuProgramTexture<HistoryColor>,
    lower: WgpuProgramTexture<HistoryColor>,
    destination: WgpuProgramTexture<FinalColor>,
    region: ViewportRect,
}

struct CompositePrepared {
    capture: CompositeCapture,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

impl WgpuRenderFeature for Composite {
    type Capture = CompositeCapture;
    type State = Option<(wgpu::TextureFormat, Pipeline)>;
    type Prepared = CompositePrepared;
    type Report = ();

    fn create_state(
        &self,
        _: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(None)
    }

    fn prepare(
        &self,
        state: &mut Self::State,
        capture: &CompositeCapture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<CompositePrepared, WgpuRenderProgramError> {
        let format = capture.destination.descriptor().native.format;
        if state.as_ref().is_none_or(|(current, _)| *current != format) {
            *state = Some((
                format,
                pipeline(
                    context.device(),
                    format,
                    "composite",
                    &[texture_binding(0), texture_binding(2)],
                ),
            ));
        }
        let state = &state
            .as_ref()
            .expect("the original capture's output format was prepared")
            .1;
        let group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent-history-composition"),
                layout: &state.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            context.resources().texture(&capture.upper)?,
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            context.resources().texture(&capture.lower)?,
                        ),
                    },
                ],
            });
        Ok(CompositePrepared {
            capture: capture.clone(),
            pipeline: state.pipeline.clone(),
            group,
        })
    }

    fn encode(
        &self,
        prepared: &CompositePrepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        draw(
            context,
            &prepared.capture.destination,
            &prepared.pipeline,
            &prepared.group,
            prepared.capture.region,
        )
    }
}

fn texture_binding(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    fragment: &str,
    entries: &[wgpu::BindGroupLayoutEntry],
) -> Pipeline {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("independent-history-layout"),
        entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("independent-history-pipeline-layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("independent-history-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("accumulation.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("independent-history-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
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
    Pipeline { pipeline, layout }
}

fn draw<S: 'static>(
    context: &mut WgpuFeatureEncodeContext<'_>,
    destination: &WgpuProgramTexture<S>,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    region: ViewportRect,
) -> Result<(), WgpuRenderProgramError> {
    let (resources, encoder) = context.split();
    let target = resources.texture(destination)?;
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("independent-history-draw"),
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
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.set_viewport(
        region.physical_x as f32,
        region.physical_y as f32,
        region.physical_width as f32,
        region.physical_height as f32,
        0.0,
        1.0,
    );
    pass.set_scissor_rect(
        region.physical_x,
        region.physical_y,
        region.physical_width,
        region.physical_height,
    );
    pass.draw(0..3, 0..1);
    Ok(())
}
