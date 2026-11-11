//! Stock image policy. Domain capture and wiring stay outside the generic program executor.

mod adapter;
pub use adapter::WgpuStockCaptureAdapter;

use std::sync::Arc;

use nara_render::{RenderFramePacket, ViewLoadOp};
#[cfg(feature = "stock-ui")]
use nara_render::{RenderTarget, ViewportRect};
#[cfg(feature = "stock-sprite")]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(feature = "stock-ui")]
use nara_ui_render::UiPaintPacket;
#[cfg(feature = "stock-ui")]
use nara_window::WindowId;

use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuDeviceRequirements,
    WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureInstance, WgpuPolicyBindingContext,
    WgpuPolicyBuildContext, WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramOutput,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuRenderPolicy, WgpuRenderProgramBuilder,
    WgpuRenderProgramError,
};

pub(crate) fn premultiplied_clear_color(color: nara_render::Color) -> wgpu::Color {
    let mut clear = crate::clear_color_to_wgpu(color);
    clear.r *= clear.a;
    clear.g *= clear.a;
    clear.b *= clear.a;
    clear
}

/// Ordered world drawing in its actual raster extent, before optional Canvas resolution.
#[derive(Debug)]
pub struct WgpuWorldColor;
/// Scene color resolved through the captured Canvas mapping to the physical target extent.
#[derive(Debug)]
pub struct WgpuSceneColor;
#[cfg(feature = "stock-ui")]
/// Full-resolution Scene and UI composite, distinct from its selected pre-UI Scene version.
#[derive(Debug)]
pub struct WgpuCompositeColor;

/// Immutable stock renderer definition, retaining the original native Feature definitions.
///
/// Sprite and UI drawing require explicit captured inputs. Compiling their Cargo features does
/// not populate a capture or install a domain's capture systems.
#[derive(Debug, Clone)]
pub struct WgpuStockStages {
    canvas: WgpuFeatureDeclaration<
        crate::canvas::CanvasCompositeFeature<WgpuWorldColor, WgpuSceneColor>,
    >,
    #[cfg(feature = "stock-sprite")]
    world: WgpuFeatureDeclaration<crate::sprite::SpriteFeature<WgpuWorldColor>>,
    #[cfg(feature = "stock-ui")]
    ui: WgpuFeatureDeclaration<crate::ui::UiFeature<WgpuCompositeColor>>,
}

impl WgpuStockStages {
    #[must_use]
    pub fn new() -> Self {
        Self {
            canvas: WgpuFeatureDefinition::new(crate::canvas::CanvasCompositeFeature::new())
                .declare(),
            #[cfg(feature = "stock-sprite")]
            world: WgpuFeatureDefinition::new(crate::sprite::SpriteFeature::new()).declare(),
            #[cfg(feature = "stock-ui")]
            ui: WgpuFeatureDefinition::new(crate::ui::UiFeature::new()).declare(),
        }
    }
}

impl Default for WgpuStockStages {
    fn default() -> Self {
        Self::new()
    }
}

/// Default composition of the same stock stages available to external renderer policies.
#[derive(Debug, Default, Clone)]
pub struct WgpuStockPolicy {
    stages: WgpuStockStages,
}

impl WgpuStockPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Replayable stock inputs retaining their original domain snapshots and logical UI routing.
///
/// UI packets are immutable drawing inputs; one-shot presentation permissions remain with their
/// concrete completion owner. An empty capture draws only the captured Views' clears and any
/// explicitly configured WorldCanvas composite.
#[derive(Debug, Default, Clone)]
pub struct WgpuStockCapture {
    #[cfg(feature = "stock-sprite")]
    pub(crate) sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui_packets: Vec<Arc<UiPaintPacket>>,
    #[cfg(feature = "stock-ui")]
    pub(crate) logical_target: Option<RenderTarget>,
    #[cfg(feature = "stock-ui")]
    pub(crate) primary_window_id: Option<WindowId>,
}

impl WgpuStockCapture {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Retains the exact extracted batches, including their View order and logical targets.
    #[cfg(feature = "stock-sprite")]
    #[doc(hidden)]
    #[must_use]
    pub fn with_sprites(mut self, batches: SpriteBatches) -> Self {
        self.sprite_batches = Some(batches);
        self
    }

    #[cfg(feature = "stock-sprite")]
    #[doc(hidden)]
    #[must_use]
    pub fn sprite_batches(&self) -> Option<&SpriteBatches> {
        self.sprite_batches.as_ref()
    }

    /// Retains exact packets and routing facts without resolving a primary-window alias.
    ///
    /// `logical_target` defaults to the captured Views' target when omitted. Image adaptation
    /// changes only the execution target and never changes this logical routing. The caller
    /// supplies the actual primary window when primary-window aliases need resolution.
    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn with_ui(
        mut self,
        packets: Vec<Arc<UiPaintPacket>>,
        logical_target: Option<RenderTarget>,
        primary_window_id: Option<WindowId>,
    ) -> Self {
        self.ui_packets = packets;
        self.logical_target = logical_target;
        self.primary_window_id = primary_window_id;
        self
    }

    /// Original replayable drawing packets, separate from backend completion permissions.
    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn ui_packets(&self) -> &[Arc<UiPaintPacket>] {
        &self.ui_packets
    }
}

/// One immutable candidate's actual owners of the stock policy's original declarations.
pub struct WgpuStockBindings {
    stages: u64,
    canvas: Arc<
        WgpuFeatureInstance<crate::canvas::CanvasCompositeFeature<WgpuWorldColor, WgpuSceneColor>>,
    >,
    #[cfg(feature = "stock-sprite")]
    world: Arc<WgpuFeatureInstance<crate::sprite::SpriteFeature<WgpuWorldColor>>>,
    #[cfg(feature = "stock-ui")]
    ui: Arc<WgpuFeatureInstance<crate::ui::UiFeature<WgpuCompositeColor>>>,
}

impl WgpuStockStages {
    #[must_use]
    pub fn device_requirements(&self) -> WgpuDeviceRequirements {
        let requirements = self.canvas.device_requirements().clone();
        #[cfg(feature = "stock-sprite")]
        let requirements = requirements.union(self.world.device_requirements());
        #[cfg(feature = "stock-ui")]
        let requirements = requirements.union(self.ui.device_requirements());
        requirements
    }

    pub fn bind(
        &self,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<WgpuStockBindings, WgpuRenderProgramError> {
        Ok(WgpuStockBindings {
            stages: self.canvas.id(),
            canvas: context.feature(&self.canvas)?,
            #[cfg(feature = "stock-sprite")]
            world: context.feature(&self.world)?,
            #[cfg(feature = "stock-ui")]
            ui: context.feature(&self.ui)?,
        })
    }

    /// Validates original logical View, Sprite and UI routing without native work.
    pub fn validate_capture(
        packet: &RenderFramePacket,
        capture: &WgpuStockCapture,
    ) -> Result<(), WgpuRenderProgramError> {
        let views = packet.views();
        let Some(first) = views.first() else {
            return Err(WgpuRenderProgramError::feature(
                "invalid stock world View count",
            ));
        };
        if views.len() > nara_render::MAX_VIEWS_PER_TARGET {
            return Err(WgpuRenderProgramError::feature(
                "invalid stock world View count",
            ));
        }
        if !matches!(first.load, ViewLoadOp::Clear(_)) {
            return Err(WgpuRenderProgramError::feature(
                "the first world View must clear its target",
            ));
        }
        #[cfg(not(any(feature = "stock-sprite", feature = "stock-ui")))]
        let _ = capture;
        #[cfg(feature = "stock-sprite")]
        if capture.sprite_batches.as_ref().is_some_and(|batches| {
            batches.as_slice().iter().any(|batch| {
                views.get(batch.view_index).is_none_or(|view| {
                    batch.target != view.target || batch.view_order != view.order
                })
            })
        }) {
            return Err(WgpuRenderProgramError::feature(
                "a Sprite batch does not belong to the captured View and logical target",
            ));
        }
        #[cfg(feature = "stock-ui")]
        if !capture.ui_packets.is_empty() {
            let logical_target = capture.logical_target.unwrap_or(first.target);
            if views.iter().any(|view| {
                !ui_targets_match(logical_target, view.target, capture.primary_window_id)
            }) || capture.ui_packets.iter().any(|packet| {
                !ui_targets_match(packet.target(), logical_target, capture.primary_window_id)
            }) {
                return Err(WgpuRenderProgramError::feature(
                    "UI packets and their logical target must belong to the captured Views",
                ));
            }
        }
        Ok(())
    }

    /// Starts one finite program using the original immutable stock inputs and bindings.
    /// Operation keys remain caller-selected and obey the graph's ordinary uniqueness rule.
    pub fn begin<'a>(
        &self,
        bindings: &'a WgpuStockBindings,
        capture: &'a WgpuStockCapture,
        context: WgpuPolicyBuildContext<'a>,
        limits: GraphLimits,
    ) -> Result<WgpuStockProgramBuilder<'a>, WgpuRenderProgramError> {
        if bindings.stages != self.canvas.id() {
            return Err(WgpuRenderProgramError::feature(
                "stock construction requires its original stage bindings",
            ));
        }
        let packet = context.packet();
        Self::validate_capture(packet, capture)?;
        let graph = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            limits,
        )?;
        Ok(WgpuStockProgramBuilder {
            graph,
            bindings,
            #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
            capture,
            context,
            facts: WgpuPolicyFrameFacts::default(),
            max_outputs: limits.max_outputs,
        })
    }
}

/// Shared stock lowering around caller-authored typed Feature operations.
///
/// World drawing, optional Canvas resolution and full-resolution UI are explicit operations.
/// Borrowing the ordinary graph builder permits branching and effects in either pixel space;
/// it does not grant upload, submission or presentation authority.
pub struct WgpuStockProgramBuilder<'a> {
    graph: WgpuRenderProgramBuilder,
    bindings: &'a WgpuStockBindings,
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    capture: &'a WgpuStockCapture,
    context: WgpuPolicyBuildContext<'a>,
    facts: WgpuPolicyFrameFacts,
    max_outputs: usize,
}

impl WgpuStockProgramBuilder<'_> {
    pub fn graph(&mut self) -> &mut WgpuRenderProgramBuilder {
        &mut self.graph
    }

    pub fn facts_mut(&mut self) -> &mut WgpuPolicyFrameFacts {
        &mut self.facts
    }

    /// Actual target-sized descriptor; world drawing uses the Canvas extent when configured.
    #[must_use]
    pub fn target_descriptor(&self) -> WgpuProgramTextureDescriptor {
        let packet = self.context.packet();
        let format = self.context.format();
        let extent = packet.target().extent();
        WgpuProgramTextureDescriptor {
            native: wgpu::TextureDescriptor {
                label: Some("nara_stock_scene_color"),
                size: wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            transfer: if format.has_srgb_suffix() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        }
    }

    /// Writes ordered Sprite Views once, or their exact clear-only result.
    pub fn world(
        &mut self,
        operation: u64,
    ) -> Result<WgpuProgramTexture<WgpuWorldColor>, WgpuRenderProgramError> {
        let packet = self.context.packet();
        let mut world_descriptor = self.target_descriptor();
        if let Some(canvas) = packet.world_canvas() {
            world_descriptor.native.size.width = canvas.extent().width;
            world_descriptor.native.size.height = canvas.extent().height;
            world_descriptor.native.format = crate::canvas::WORLD_CANVAS_FORMAT;
            world_descriptor.transfer = TextureTransfer::Srgb;
        }
        let scene = self.graph.texture::<WgpuWorldColor>(world_descriptor)?;
        let Some(ViewLoadOp::Clear(clear)) = packet.views().first().map(|view| view.load) else {
            return Err(WgpuRenderProgramError::feature(
                "the first world View must clear its target",
            ));
        };
        #[cfg(feature = "stock-sprite")]
        if let Some(batches) = self.capture.sprite_batches.as_ref() {
            let sprite_capture = crate::sprite::SpriteCapture::for_views(
                Some(batches),
                packet.views(),
                scene.clone(),
            )?;
            add_draw_facts(&mut self.facts, &sprite_capture.batches)?;
            self.graph.operation(
                operation,
                &self.bindings.world,
                Arc::new(sprite_capture),
                &[],
                &[scene.clear(premultiplied_clear_color(clear), AccessRole::Attachment)],
            )?;
        }
        #[cfg(feature = "stock-sprite")]
        let initialize_only = self.capture.sprite_batches.is_none();
        #[cfg(not(feature = "stock-sprite"))]
        let initialize_only = true;
        if initialize_only {
            // With no world drawing, the last whole-attachment clear is the complete result.
            let clear = packet
                .views()
                .iter()
                .filter_map(|view| match view.load {
                    ViewLoadOp::Clear(color) => Some(color),
                    ViewLoadOp::Load => None,
                })
                .next_back()
                .unwrap_or(clear);
            self.graph.initialize(
                operation,
                &[scene.clear(premultiplied_clear_color(clear), AccessRole::Attachment)],
            )?;
        }
        Ok(scene)
    }

    /// Resolves a world-raster version using the original Canvas mapping.
    /// Absence of a Canvas is explicit: finish directly from the world version instead.
    pub fn resolve_canvas(
        &mut self,
        operation: u64,
        scene: &WgpuProgramTexture<WgpuWorldColor>,
    ) -> Result<WgpuProgramTexture<WgpuSceneColor>, WgpuRenderProgramError> {
        let frame = self.context.packet().world_canvas().ok_or_else(|| {
            WgpuRenderProgramError::feature("Canvas resolution requires a captured WorldCanvas")
        })?;
        let resolved = self
            .graph
            .texture::<WgpuSceneColor>(self.target_descriptor())?;
        let capture =
            crate::canvas::CanvasCompositeCapture::new(scene.clone(), resolved.clone(), frame);
        capture.validate()?;
        self.graph.operation(
            operation,
            &self.bindings.canvas,
            Arc::new(capture),
            &[scene.read(AccessRole::Sampled)],
            &[resolved.clear(
                premultiplied_clear_color(frame.bar_color()),
                AccessRole::Attachment,
            )],
        )?;
        self.facts.frame.draw_calls = self.facts.frame.draw_calls.saturating_add(1);
        Ok(resolved)
    }

    /// Composes the captured HUD over the exact selected Scene version and closes the group.
    /// Additional outputs retain their own original versions, such as a before-effects Scene.
    /// The exact final version is added only if absent; repeated caller entries still reject.
    #[cfg_attr(not(feature = "stock-ui"), allow(unused_mut))]
    pub fn finish<S: 'static>(
        mut self,
        ui_operation: u64,
        scene: &WgpuProgramTexture<S>,
        additional_outputs: &[WgpuProgramOutput],
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        #[cfg(feature = "stock-ui")]
        let has_ui = !self.capture.ui_packets.is_empty();
        #[cfg(not(feature = "stock-ui"))]
        let has_ui = false;
        let append_final = has_ui || !additional_outputs.contains(&scene.output());
        let count = additional_outputs
            .len()
            .checked_add(usize::from(append_final))
            .filter(|count| *count <= self.max_outputs)
            .ok_or(crate::GraphError::LimitExceeded(crate::LimitKind::Outputs))?;
        let descriptor = self.target_descriptor();
        if scene.descriptor() != &descriptor {
            // Labels and additional usage bits are not color/shape semantics.
            let mut actual = scene.descriptor().clone();
            actual.native.label = descriptor.native.label;
            actual.native.usage = descriptor.native.usage;
            if actual != descriptor {
                return Err(WgpuRenderProgramError::feature(
                    "the selected pre-UI Scene must match the actual target extent and color interpretation",
                ));
            }
        }
        #[cfg(not(feature = "stock-ui"))]
        let _ = ui_operation;
        #[cfg(feature = "stock-ui")]
        if !self.capture.ui_packets.is_empty() {
            let extent = self.context.packet().target().extent();
            let final_color = self.graph.texture::<WgpuCompositeColor>(descriptor)?;
            let ui_capture = Arc::new(crate::ui::UiCapture::new(
                &self.capture.ui_packets,
                self.capture
                    .logical_target
                    .unwrap_or(self.context.packet().views()[0].target),
                self.capture.primary_window_id,
                final_color.clone(),
                ViewportRect::from_extent(extent).expect("admitted target extent"),
            )?);
            add_draw_facts(&mut self.facts, &ui_capture.batches)?;
            self.facts.frame.draw_calls = self
                .facts
                .frame
                .draw_calls
                .saturating_add(ui_capture.mesh_count());
            let initialized = final_color.preserve(scene, AccessRole::Attachment);
            let binding = crate::WgpuUiProgramBinding::new(
                ui_operation,
                &self.bindings.ui,
                Arc::clone(&ui_capture),
            );
            self.graph.operation(
                ui_operation,
                &self.bindings.ui,
                ui_capture,
                &[],
                &[initialized],
            )?;
            let final_output = final_color.output();
            let outputs =
                (0..count).map(|index| additional_outputs.get(index).unwrap_or(&final_output));
            return WgpuPolicyProgram::new(
                self.graph.finish_output_refs(outputs)?,
                &final_color,
                self.facts,
            )?
            .with_ui(binding);
        }
        let final_output = scene.output();
        let outputs =
            (0..count).map(|index| additional_outputs.get(index).unwrap_or(&final_output));
        WgpuPolicyProgram::new(self.graph.finish_output_refs(outputs)?, scene, self.facts)
    }
}

impl WgpuRenderPolicy for WgpuStockPolicy {
    type Capture = WgpuStockCapture;
    type Bindings = WgpuStockBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        self.stages.device_requirements()
    }

    fn bind(
        &self,
        _capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        self.stages.bind(context)
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &Self::Capture,
    ) -> Result<(), WgpuRenderProgramError> {
        WgpuStockStages::validate_capture(packet, capture)
    }

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let mut program = self
            .stages
            .begin(bindings, capture, context, GraphLimits::default())?;
        let world = program.world(1)?;
        if context.packet().world_canvas().is_some() {
            let scene = program.resolve_canvas(2, &world)?;
            program.finish(3, &scene, &[])
        } else {
            program.finish(3, &world, &[])
        }
    }
}

#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
fn add_draw_facts(
    facts: &mut WgpuPolicyFrameFacts,
    batches: &[crate::quad::WgpuQuadBatch],
) -> Result<(), WgpuRenderProgramError> {
    let stats = crate::quad::quad_batch_draw_stats(batches);
    facts.frame.draw_calls = facts.frame.draw_calls.saturating_add(stats.draw_calls);
    facts.frame.sprites = facts.frame.sprites.saturating_add(stats.sprites);
    facts.image_sprites = facts.image_sprites.saturating_add(stats.image_sprites);
    let bytes = crate::quad::admit_quad_instance_buffers(batches)
        .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?;
    facts.instance_buffer_bytes =
        facts
            .instance_buffer_bytes
            .checked_add(bytes)
            .ok_or_else(|| {
                WgpuRenderProgramError::feature("stock instance bytes cannot be represented")
            })?;
    Ok(())
}

#[cfg(feature = "stock-ui")]
fn ui_targets_match(
    packet: RenderTarget,
    view: RenderTarget,
    primary_window_id: Option<WindowId>,
) -> bool {
    match (packet, view) {
        (RenderTarget::PrimaryWindow, RenderTarget::Window(window))
        | (RenderTarget::Window(window), RenderTarget::PrimaryWindow) => {
            primary_window_id == Some(window)
        }
        _ => packet == view,
    }
}
