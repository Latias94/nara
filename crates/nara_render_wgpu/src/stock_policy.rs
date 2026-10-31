//! Stock image policy. Domain capture and wiring stay outside the generic program executor.

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
    WgpuPolicyBuildContext, WgpuPolicyFrameFacts, WgpuPolicyProgram, WgpuProgramTextureDescriptor,
    WgpuRenderPolicy, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};

struct SceneColor;
struct ResolvedSceneColor;
#[cfg(feature = "stock-ui")]
struct FinalComposite;

/// Immutable stock renderer definition, retaining the original native Feature definitions.
///
/// Sprite and UI drawing require explicit captured inputs. Compiling their Cargo features does
/// not populate a capture or install a domain's capture systems.
#[derive(Debug, Clone)]
pub struct WgpuStockPolicy {
    canvas: WgpuFeatureDeclaration<
        crate::canvas::CanvasCompositeFeature<SceneColor, ResolvedSceneColor>,
    >,
    #[cfg(feature = "stock-sprite")]
    world: WgpuFeatureDeclaration<crate::sprite::SpriteFeature<SceneColor>>,
    #[cfg(feature = "stock-ui")]
    ui: WgpuFeatureDeclaration<crate::ui::UiFeature<FinalComposite>>,
}

impl WgpuStockPolicy {
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

impl Default for WgpuStockPolicy {
    fn default() -> Self {
        Self::new()
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
pub struct WgpuStockPolicyBindings {
    canvas: Arc<
        WgpuFeatureInstance<crate::canvas::CanvasCompositeFeature<SceneColor, ResolvedSceneColor>>,
    >,
    #[cfg(feature = "stock-sprite")]
    world: Arc<WgpuFeatureInstance<crate::sprite::SpriteFeature<SceneColor>>>,
    #[cfg(feature = "stock-ui")]
    ui: Arc<WgpuFeatureInstance<crate::ui::UiFeature<FinalComposite>>>,
}

impl WgpuRenderPolicy for WgpuStockPolicy {
    type Capture = WgpuStockCapture;
    type Bindings = WgpuStockPolicyBindings;

    fn device_requirements(&self) -> WgpuDeviceRequirements {
        let requirements = self.canvas.device_requirements().clone();
        #[cfg(feature = "stock-sprite")]
        let requirements = requirements.union(self.world.device_requirements());
        #[cfg(feature = "stock-ui")]
        let requirements = requirements.union(self.ui.device_requirements());
        requirements
    }

    fn bind(
        &self,
        _capture: &Self::Capture,
        context: &mut WgpuPolicyBindingContext<'_>,
    ) -> Result<Self::Bindings, WgpuRenderProgramError> {
        Ok(WgpuStockPolicyBindings {
            canvas: context.feature(&self.canvas)?,
            #[cfg(feature = "stock-sprite")]
            world: context.feature(&self.world)?,
            #[cfg(feature = "stock-ui")]
            ui: context.feature(&self.ui)?,
        })
    }

    fn validate_capture(
        &self,
        packet: &RenderFramePacket,
        capture: &Self::Capture,
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

    fn build(
        &self,
        bindings: &Self::Bindings,
        capture: &Self::Capture,
        context: WgpuPolicyBuildContext<'_>,
    ) -> Result<WgpuPolicyProgram, WgpuRenderProgramError> {
        let packet = context.packet();
        self.validate_capture(packet, capture)?;
        let format = context.format();
        let mut facts = WgpuPolicyFrameFacts::default();
        let mut builder = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            GraphLimits::default(),
        )?;
        let extent = packet.target().extent();
        let descriptor = WgpuProgramTextureDescriptor {
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
            transfer: if format.is_srgb() {
                TextureTransfer::Srgb
            } else {
                TextureTransfer::Linear
            },
            alpha: TextureAlpha::Premultiplied,
        };
        let mut world_descriptor = descriptor.clone();
        if let Some(canvas) = packet.world_canvas() {
            world_descriptor.native.size.width = canvas.extent().width;
            world_descriptor.native.size.height = canvas.extent().height;
            world_descriptor.native.format = crate::canvas::WORLD_CANVAS_FORMAT;
            world_descriptor.transfer = TextureTransfer::Srgb;
        }
        let scene = builder.texture::<SceneColor>(world_descriptor)?;
        let Some(ViewLoadOp::Clear(clear)) = packet.views().first().map(|view| view.load) else {
            return Err(WgpuRenderProgramError::feature(
                "the first world View must clear its target",
            ));
        };
        #[cfg(feature = "stock-sprite")]
        if let Some(batches) = capture.sprite_batches.as_ref() {
            let sprite_capture = crate::sprite::SpriteCapture::for_views(
                Some(batches),
                packet.views(),
                scene.clone(),
            )?;
            add_draw_facts(&mut facts, &sprite_capture.batches)?;
            builder.operation(
                1,
                &bindings.world,
                Arc::new(sprite_capture),
                &[],
                &[scene.clear(crate::clear_color_to_wgpu(clear), AccessRole::Attachment)],
            )?;
        }
        #[cfg(feature = "stock-sprite")]
        let initialize_only = capture.sprite_batches.is_none();
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
                .last()
                .unwrap_or(clear);
            builder.initialize(
                1,
                &[scene.clear(crate::clear_color_to_wgpu(clear), AccessRole::Attachment)],
            )?;
        }
        let resolved = if let Some(frame) = packet.world_canvas() {
            facts.frame.draw_calls = facts.frame.draw_calls.saturating_add(1);
            let resolved = builder.texture::<ResolvedSceneColor>(descriptor.clone())?;
            builder.operation(
                2,
                &bindings.canvas,
                Arc::new(crate::canvas::CanvasCompositeCapture::new(
                    scene.clone(),
                    resolved.clone(),
                    frame,
                )),
                &[scene.read(AccessRole::Sampled)],
                &[resolved.clear(
                    crate::clear_color_to_wgpu(frame.bar_color()),
                    AccessRole::Attachment,
                )],
            )?;
            Some(resolved)
        } else {
            None
        };
        #[cfg(feature = "stock-ui")]
        if !capture.ui_packets.is_empty() {
            let final_color = builder.texture::<FinalComposite>(descriptor.clone())?;
            let ui_capture = Arc::new(crate::ui::UiCapture::new(
                &capture.ui_packets,
                capture.logical_target.unwrap_or(packet.views()[0].target),
                capture.primary_window_id,
                final_color.clone(),
                ViewportRect::from_extent(extent).expect("admitted target extent"),
            )?);
            add_draw_facts(&mut facts, &ui_capture.batches)?;
            let initialized = match &resolved {
                Some(resolved) => final_color.preserve(resolved, AccessRole::Attachment),
                None => final_color.preserve(&scene, AccessRole::Attachment),
            };
            let binding =
                crate::WgpuUiProgramBinding::new(3, &bindings.ui, Arc::clone(&ui_capture));
            builder.operation(3, &bindings.ui, ui_capture, &[], &[initialized])?;
            return WgpuPolicyProgram::new(
                builder.finish(&[final_color.output()])?,
                &final_color,
                facts,
            )?
            .with_ui(binding);
        }
        if let Some(resolved) = resolved {
            return WgpuPolicyProgram::new(builder.finish(&[resolved.output()])?, &resolved, facts);
        }
        WgpuPolicyProgram::new(builder.finish(&[scene.output()])?, &scene, facts)
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
