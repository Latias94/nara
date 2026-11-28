use std::sync::Arc;

use nara_ecs::World;

#[cfg(not(feature = "sprite-render"))]
use crate::ViewLoadOp;
use crate::{ClearColor, Color, Extent2d, RenderFramePacket, RenderFrameTarget};

use super::{
    GpuContext, PassAttachments, PostProcess2d, RenderPass, RenderPassContext, RenderTargetView,
    TextureCacheStats, WgpuImageOutput, WgpuNativeWorkStats, WgpuRenderError,
    canvas::CanvasPipelines,
    frame_buffer::FrameRingBuffer,
    post_process::{PostProcessInput, PostProcessPass},
    texture_cache::{TextureCache, TextureLease},
};

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

/// Immutable CPU geometry and resource snapshots for a single destination.
#[derive(Clone)]
pub struct RenderInput {
    packet: RenderFramePacket,
    clear: Color,
    post_process: PostProcess2d,
    #[cfg(feature = "sprite-render")]
    sprites: crate::sprite_render::SpriteBatches,
    #[cfg(feature = "hud-runtime")]
    hud: Option<crate::hud::HudFrame>,
}

impl RenderInput {
    /// Freezes the extracted frame after the game's render preparation schedules have run.
    pub fn capture(world: &World, packet: RenderFramePacket) -> Self {
        Self {
            #[cfg(feature = "hud-runtime")]
            hud: crate::hud::capture(world, &packet),
            clear: world
                .get_resource::<ClearColor>()
                .copied()
                .unwrap_or_default()
                .0,
            post_process: world
                .get_resource::<PostProcess2d>()
                .copied()
                .unwrap_or_default(),
            #[cfg(feature = "sprite-render")]
            sprites: world
                .get_resource::<crate::sprite_render::SpriteBatches>()
                .cloned()
                .unwrap_or_default(),
            packet,
        }
    }

    pub fn packet(&self) -> &RenderFramePacket {
        &self.packet
    }

    pub fn clear_color(&self) -> Color {
        self.clear
    }

    /// Returns the effects captured with this frame.
    #[must_use]
    pub const fn post_process(&self) -> PostProcess2d {
        self.post_process
    }

    #[cfg(feature = "sprite-render")]
    pub fn sprite_batches(&self) -> &[crate::sprite_render::batching::SpriteBatch] {
        self.sprites.as_slice()
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn hud(&self) -> Option<&crate::hud::HudFrame> {
        self.hud.as_ref()
    }

    /// Sets the exact HUD input for a destination that is rendered outside the UI runtime.
    #[cfg(feature = "hud-runtime")]
    pub fn with_hud(mut self, hud: crate::hud::HudFrame) -> Self {
        self.hud = Some(hud);
        self
    }
}

/// Facts returned after successfully submitting the frame.
pub struct RenderReport {
    pub submission: wgpu::SubmissionIndex,
    pub work: WgpuNativeWorkStats,
    /// The window driver publishes this only after the matching surface texture presents.
    #[cfg(feature = "hud-runtime")]
    pub hud: Result<Option<crate::hud::HudPresentation>, crate::hud::HudDrawError>,
}

/// Executes the scene, optional post processing, HUD, and editor overlay for both target kinds.
pub struct Renderer {
    context: GpuContext,
    error_cursor: u64,
    targets: TextureCache,
    uploads: FrameRingBuffer,
    canvas: CanvasPipelines,
    post_process: PostProcessPass,
    #[cfg(feature = "sprite-render")]
    scene: super::SpriteTilePass,
    #[cfg(feature = "hud-runtime")]
    hud: super::HudPass,
    editor_overlay: Option<Box<dyn RenderPass>>,
}

impl Renderer {
    pub fn new(context: GpuContext) -> Self {
        Self {
            context,
            error_cursor: 0,
            targets: TextureCache::default(),
            uploads: FrameRingBuffer::default(),
            canvas: CanvasPipelines::default(),
            post_process: PostProcessPass::default(),
            #[cfg(feature = "sprite-render")]
            scene: super::SpriteTilePass::default(),
            #[cfg(feature = "hud-runtime")]
            hud: super::HudPass::default(),
            editor_overlay: None,
        }
    }

    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    pub fn texture_cache_stats(&self) -> TextureCacheStats {
        self.targets.stats()
    }

    /// Installs the editor's pass after the scene and HUD. The pass loads and writes frame color.
    pub fn set_editor_overlay(
        &mut self,
        overlay: Option<Box<dyn RenderPass>>,
    ) -> Result<(), WgpuRenderError> {
        if overlay
            .as_ref()
            .is_some_and(|pass| pass.attachments() != PassAttachments::OVERLAY)
        {
            return Err(WgpuRenderError::InvalidTarget(
                "the editor overlay must read and write color",
            ));
        }
        self.editor_overlay = overlay;
        Ok(())
    }

    pub fn check_errors(&mut self) -> Result<(), WgpuRenderError> {
        self.poll_gpu().map(|_| ())
    }

    /// Services completion callbacks and returns any recorded device error without waiting.
    pub fn poll_gpu(&mut self) -> Result<wgpu::PollStatus, WgpuRenderError> {
        let polled = self.context.device().poll(wgpu::PollType::Poll);
        let result = self
            .context
            .check_signal(&mut self.error_cursor)
            .and_then(|()| polled.map_err(|error| WgpuRenderError::Device(error.to_string())));
        if result.is_err() {
            self.discard_caches();
        }
        result
    }

    fn discard_caches(&mut self) {
        self.targets = TextureCache::default();
        self.uploads = FrameRingBuffer::default();
        self.canvas = CanvasPipelines::default();
        self.post_process.clear_caches();
        #[cfg(feature = "sprite-render")]
        {
            self.scene = super::SpriteTilePass::default();
        }
        #[cfg(feature = "hud-runtime")]
        {
            self.hud = super::HudPass::default();
        }
        if let Some(overlay) = &mut self.editor_overlay {
            overlay.clear_caches();
        }
    }

    /// Draws into an already acquired attachment. The caller owns presenting it.
    pub fn render_to_view(
        &mut self,
        frame: &RenderInput,
        target: RenderTargetView<'_>,
    ) -> Result<RenderReport, WgpuRenderError> {
        self.validate_extent(frame, target.extent)?;
        self.check_errors()?;
        let mut work = WgpuNativeWorkStats::default();
        let scene = self.acquire_scene_targets(frame, false, &mut work)?;
        self.render(frame, target, work, scene, None)
    }

    /// Draws into a leased sRGB image for sampling on the same queue.
    /// Returns [`WgpuRenderError::TargetsBusy`] while every pooled target is retained by the GPU
    /// or a caller.
    pub fn render_to_image(
        &mut self,
        frame: &RenderInput,
    ) -> Result<(WgpuImageOutput, RenderReport), WgpuRenderError> {
        let RenderFrameTarget::Image(image) = frame.packet.target() else {
            return Err(WgpuRenderError::InvalidTarget(
                "offscreen rendering requires an image packet",
            ));
        };
        let extent = image.extent();
        self.validate_extent(frame, extent)?;
        self.check_errors()?;

        let mut work = WgpuNativeWorkStats::default();
        let scene = self.acquire_scene_targets(frame, true, &mut work)?;
        let lease = self.targets.acquire(
            self.context.device(),
            extent,
            WgpuImageOutput::FORMAT,
            self.context.loss_signal(),
            &mut work,
        )?;
        let rendered = self.render(frame, lease.target(), work, scene, Some(Arc::clone(&lease)));
        match rendered {
            Ok(report) => Ok((WgpuImageOutput(lease), report)),
            Err(error) => {
                lease.discard();
                Err(error)
            }
        }
    }

    fn validate_extent(
        &self,
        frame: &RenderInput,
        target: Extent2d,
    ) -> Result<(), WgpuRenderError> {
        if target.is_empty() || frame.packet.target().extent() != target {
            return Err(WgpuRenderError::InvalidTarget(
                "attachment extent differs from the extracted frame",
            ));
        }
        let extent_limit = self.context.device().limits().max_texture_dimension_2d;
        let too_large =
            |extent: Extent2d| extent.width > extent_limit || extent.height > extent_limit;
        if too_large(target)
            || frame
                .packet
                .world_canvas()
                .is_some_and(|canvas| too_large(canvas.extent()))
        {
            return Err(WgpuRenderError::Capacity("2D attachment size"));
        }
        Ok(())
    }

    fn acquire_scene_targets(
        &self,
        frame: &RenderInput,
        image_output: bool,
        work: &mut WgpuNativeWorkStats,
    ) -> Result<SceneTargets, WgpuRenderError> {
        let canvas = frame.packet.world_canvas();
        let effects = frame.post_process();
        let mut acquire = |extent, format| {
            self.targets.acquire(
                self.context.device(),
                extent,
                format,
                self.context.loss_signal(),
                work,
            )
        };
        if effects.is_active() {
            let has_bloom = effects.has_bloom();
            let count = if has_bloom { 3 } else { 1 };
            self.targets
                .check_available(count + usize::from(image_output))?;
            let extent = canvas.map_or(frame.packet.target().extent(), |canvas| canvas.extent());
            let scene = acquire(extent, PostProcessPass::FORMAT)?;
            let bloom = if has_bloom {
                let half = Extent2d {
                    width: extent.width.div_ceil(2),
                    height: extent.height.div_ceil(2),
                };
                Some([
                    acquire(half, PostProcessPass::FORMAT)?,
                    acquire(half, PostProcessPass::FORMAT)?,
                ])
            } else {
                None
            };
            Ok(SceneTargets::PostProcess { scene, bloom })
        } else if let Some(canvas) = canvas {
            self.targets
                .check_available(1 + usize::from(image_output))?;
            acquire(canvas.extent(), WgpuImageOutput::FORMAT).map(SceneTargets::Canvas)
        } else {
            Ok(SceneTargets::Direct)
        }
    }

    fn render(
        &mut self,
        frame: &RenderInput,
        target: RenderTargetView<'_>,
        mut work: WgpuNativeWorkStats,
        scene_targets: SceneTargets,
        output_lease: Option<Arc<TextureLease>>,
    ) -> Result<RenderReport, WgpuRenderError> {
        let device = self.context.device();
        let queue = self.context.queue();
        let (scene_target, post_process) = match &scene_targets {
            SceneTargets::Direct => (target, None),
            SceneTargets::Canvas(lease) => (lease.target(), None),
            SceneTargets::PostProcess { scene, bloom } => {
                let scene = scene.target();
                let bloom = bloom.as_ref().map(|[a, b]| [a.target(), b.target()]);
                (scene, Some(PostProcessInput { scene, bloom }))
            }
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("nara_frame"),
        });
        self.uploads.begin_frame();
        #[cfg(feature = "hud-runtime")]
        let mut hud = Ok(None);
        let mut context = RenderPassContext {
            device,
            queue,
            encoder: &mut encoder,
            target: scene_target,
            frame,
            work: &mut work,
            uploads: &mut self.uploads,
            post_process,
            #[cfg(feature = "hud-runtime")]
            hud: &mut hud,
        };

        let encoded = (|| {
            #[cfg(feature = "sprite-render")]
            self.scene.execute(&mut context)?;
            #[cfg(not(feature = "sprite-render"))]
            context.clear(
                frame
                    .packet
                    .views()
                    .iter()
                    .rev()
                    .find_map(|view| match view.load {
                        ViewLoadOp::Clear(color) => Some(color),
                        ViewLoadOp::Load => None,
                    })
                    .unwrap_or(frame.clear),
            );

            context.target = target;
            if post_process.is_some() {
                self.post_process.execute(&mut context)?;
            } else if let Some(canvas) = frame.packet.world_canvas() {
                self.canvas
                    .composite(&mut context, scene_target.view, canvas)?;
            }
            #[cfg(feature = "hud-runtime")]
            self.hud.execute(&mut context)?;
            if let Some(overlay) = &mut self.editor_overlay {
                overlay.execute(&mut context)?;
            }
            Ok::<_, WgpuRenderError>(())
        })();
        let rendered = (|| {
            if let Err(error) = encoded {
                drop(encoder);
                self.context.check_signal(&mut self.error_cursor)?;
                if work.issued_buffer_write_calls != 0 || work.issued_texture_write_calls != 0 {
                    // WGPU holds queue writes until a submit. Retire their staging even when
                    // a later pass rejects the frame. The failed encoder is already discarded.
                    queue.submit([]);
                    self.context.check_signal(&mut self.error_cursor)?;
                }
                return Err(error);
            }
            let commands = encoder.finish();
            self.context.check_signal(&mut self.error_cursor)?;
            let submission = queue.submit([commands]);
            // Hold each texture even when submission has reported an uncaptured error.
            if !matches!(scene_targets, SceneTargets::Direct) || output_lease.is_some() {
                let leases = (scene_targets.clone(), output_lease);
                queue.on_submitted_work_done(move || drop(leases));
            }
            self.context.check_signal(&mut self.error_cursor)?;
            Ok(RenderReport {
                submission,
                work,
                #[cfg(feature = "hud-runtime")]
                hud,
            })
        })();
        self.uploads.end_frame();
        if rendered.is_err() {
            scene_targets.discard();
        }
        if matches!(
            rendered,
            Err(WgpuRenderError::Device(_) | WgpuRenderError::DeviceLost)
        ) {
            self.discard_caches();
        }
        rendered
    }
}

#[derive(Clone)]
enum SceneTargets {
    Direct,
    Canvas(Arc<TextureLease>),
    PostProcess {
        scene: Arc<TextureLease>,
        bloom: Option<[Arc<TextureLease>; 2]>,
    },
}

impl SceneTargets {
    fn discard(&self) {
        match self {
            Self::Direct => {}
            Self::Canvas(lease) => lease.discard(),
            Self::PostProcess { scene, bloom } => {
                scene.discard();
                for lease in bloom.iter().flatten() {
                    lease.discard();
                }
            }
        }
    }
}
