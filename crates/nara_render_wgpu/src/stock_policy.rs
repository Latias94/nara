//! Stock image policy. Domain capture and wiring stay outside the generic program executor.

use std::sync::{Arc, atomic::AtomicBool};

#[cfg(feature = "ui-submitter")]
use nara_render::ViewportRect;
use nara_render::{RenderFramePacket, ViewLoadOp};

use crate::{
    AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
    WgpuProgramTextureDescriptor, WgpuRenderProducerId, WgpuRenderProgram,
    WgpuRenderProgramBuilder, WgpuRenderProgramError, frame::WgpuFramePayload,
    graph::ResourceVersion,
};

pub(crate) struct SceneColor;
pub(crate) struct ResolvedSceneColor;
#[cfg(feature = "ui-submitter")]
pub(crate) struct FinalComposite;

pub(crate) struct StockPolicyInstances {
    canvas: crate::WgpuFeatureInstance<
        crate::canvas::CanvasCompositeFeature<SceneColor, ResolvedSceneColor>,
    >,
    #[cfg(feature = "sprite-submitter")]
    world: crate::WgpuFeatureInstance<crate::sprite::SpriteFeature<SceneColor>>,
    #[cfg(feature = "ui-submitter")]
    ui: crate::WgpuFeatureInstance<crate::ui::UiFeature<FinalComposite>>,
}

impl StockPolicyInstances {
    pub(crate) fn new(
        backend_instance_id: u64,
        producer: WgpuRenderProducerId,
        closed: &Arc<AtomicBool>,
    ) -> Result<Self, WgpuRenderProgramError> {
        Ok(Self {
            canvas: crate::WgpuFeatureInstance::new(
                backend_instance_id,
                producer,
                Arc::clone(closed),
                &WgpuFeatureDefinition::new(crate::canvas::CanvasCompositeFeature::new()),
            )?,
            #[cfg(feature = "sprite-submitter")]
            world: crate::WgpuFeatureInstance::new(
                backend_instance_id,
                producer,
                Arc::clone(closed),
                &WgpuFeatureDefinition::new(crate::sprite::SpriteFeature::new()),
            )?,
            #[cfg(feature = "ui-submitter")]
            ui: crate::WgpuFeatureInstance::new(
                backend_instance_id,
                producer,
                Arc::clone(closed),
                &WgpuFeatureDefinition::new(crate::ui::UiFeature::new()),
            )?,
        })
    }

    pub(crate) fn build(
        &self,
        packet: &RenderFramePacket,
        payload: &WgpuFramePayload,
        format: wgpu::TextureFormat,
    ) -> Result<StockProgram, WgpuRenderProgramError> {
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
        #[cfg(feature = "sprite-submitter")]
        if let Some(batches) = payload.sprite_batches.as_ref() {
            let capture = crate::sprite::SpriteCapture::for_views(
                Some(batches),
                packet.views(),
                scene.clone(),
            )?;
            builder.operation(
                1,
                &self.world,
                Arc::new(capture),
                &[],
                &[scene.clear(crate::clear_color_to_wgpu(clear), AccessRole::Attachment)],
            )?;
        }
        #[cfg(feature = "sprite-submitter")]
        let initialize_only = payload.sprite_batches.is_none();
        #[cfg(not(feature = "sprite-submitter"))]
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
            let resolved = builder.texture::<ResolvedSceneColor>(descriptor.clone())?;
            builder.operation(
                2,
                &self.canvas,
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
        #[cfg(not(feature = "ui-submitter"))]
        let _ = payload;
        #[cfg(feature = "ui-submitter")]
        if !payload.ui_packets.is_empty() {
            let final_color = builder.texture::<FinalComposite>(descriptor.clone())?;
            let capture = crate::ui::UiCapture::new(
                &payload.ui_packets,
                payload
                    .ui_logical_target
                    .unwrap_or(packet.views()[0].target),
                payload.primary_window_id,
                final_color.clone(),
                ViewportRect::from_extent(extent).expect("admitted target extent"),
            )?;
            let ui_facts = Arc::clone(&capture.packet_facts);
            let initialized = match &resolved {
                Some(resolved) => final_color.preserve(resolved, AccessRole::Attachment),
                None => final_color.preserve(&scene, AccessRole::Attachment),
            };
            builder.operation(3, &self.ui, Arc::new(capture), &[], &[initialized])?;
            return Ok(StockProgram {
                program: builder.finish(&[final_color.output()])?,
                final_version: final_color.version.clone(),
                ui_facts,
            });
        }
        if let Some(resolved) = resolved {
            return Ok(StockProgram {
                program: builder.finish(&[resolved.output()])?,
                final_version: resolved.version.clone(),
                #[cfg(feature = "ui-submitter")]
                ui_facts: Arc::from([]),
            });
        }
        Ok(StockProgram {
            program: builder.finish(&[scene.output()])?,
            final_version: scene.version.clone(),
            #[cfg(feature = "ui-submitter")]
            ui_facts: Arc::from([]),
        })
    }
}

pub(crate) struct StockProgram {
    pub(crate) program: WgpuRenderProgram,
    pub(crate) final_version: ResourceVersion,
    #[cfg(feature = "ui-submitter")]
    pub(crate) ui_facts: Arc<[crate::ui::WgpuLoweredUiPacketFacts]>,
}
